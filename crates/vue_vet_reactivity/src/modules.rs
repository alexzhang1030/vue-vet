use std::{
  collections::{BTreeMap, BTreeSet, btree_map::Entry},
  sync::Arc,
};

use oxc_allocator::Allocator;
use oxc_ast::{
  AstKind,
  ast::{
    BindingPattern, Declaration, ExportDefaultDeclarationKind, Expression,
    ImportDeclarationSpecifier, ObjectPropertyKind,
  },
};
use oxc_parser::Parser;
use oxc_semantic::{NodeId, Semantic, SemanticBuilder};
use oxc_span::{SourceType, Span};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vue_vet_core::{
  ModuleId, ReactiveBindingFact, ReactiveBindingKind, ReactivityGraph, ScriptKind,
};

use super::{
  ProvideOffer, TraceSeeds, collect_binding_identifiers, collect_imported_bindings,
  collect_inject_sites, collect_provide_sites, module_export_name, provide_offer_index,
  reactive_binding_kind, reference_resolves_to_binding, resolve_inject_offer, resolved_vue_callee,
  source_span, trace_reactivity_seeded,
};
use oxc_ast::ast::Argument;

/// One script surface to analyze — standalone JS/TS or an extracted SFC block.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModuleSource {
  /// Stable module identity used in [`ModuleLink`] and result ordering.
  pub id: ModuleId,
  /// Text parsed by Oxc (extracted `<script>` body for SFCs).
  pub source: Arc<str>,
  /// Language hint (`js`, `ts`, `jsx`, `tsx`, …).
  pub language: String,
  pub kind: ScriptKind,
  /// Byte offset of [`Self::source`] within [`Self::span_source`].
  #[serde(default)]
  pub source_offset: usize,
  /// Full original file used for absolute line/column (SFC source). When empty,
  /// spans are computed against [`Self::source`] (standalone modules).
  #[serde(default)]
  pub span_source: Arc<str>,
  /// Module semantic IR extracted by the Oxc adapter during its first parse.
  #[serde(skip)]
  module_summary: Option<std::sync::Arc<ModuleSummary>>,
}

impl PartialEq for ModuleSource {
  fn eq(&self, other: &Self) -> bool {
    self.id == other.id
      && self.source == other.source
      && self.language == other.language
      && self.kind == other.kind
      && self.source_offset == other.source_offset
      && self.span_source == other.span_source
  }
}

impl Eq for ModuleSource {}

impl ModuleSource {
  /// Standalone JS/TS module (offset 0, spans against `source`).
  #[must_use]
  pub fn standalone(
    id: impl Into<ModuleId>,
    source: impl Into<Arc<str>>,
    language: impl Into<String>,
    kind: ScriptKind,
  ) -> Self {
    Self {
      id: id.into(),
      source: source.into(),
      language: language.into(),
      kind,
      source_offset: 0,
      span_source: Arc::from(""),
      module_summary: None,
    }
  }

  /// Extracted SFC script block with absolute span mapping into the original file.
  #[must_use]
  pub fn sfc_script(
    id: impl Into<ModuleId>,
    script_source: impl Into<Arc<str>>,
    language: impl Into<String>,
    kind: ScriptKind,
    source_offset: usize,
    sfc_source: impl Into<Arc<str>>,
  ) -> Self {
    Self {
      id: id.into(),
      source: script_source.into(),
      language: language.into(),
      kind,
      source_offset,
      span_source: sfc_source.into(),
      module_summary: None,
    }
  }

  /// Attach module semantic IR produced from the same Oxc parse as script facts.
  #[must_use]
  pub fn with_module_summary(mut self, module_summary: impl Into<Arc<ModuleSummary>>) -> Self {
    self.module_summary = Some(module_summary.into());
    self
  }

  /// Compatibility alias for [`Self::with_module_summary`].
  #[must_use]
  pub fn with_prepared_trace(self, prepared_trace: PreparedModuleTrace) -> Self {
    self.with_module_summary(prepared_trace)
  }

  fn span_origin(&self) -> &str {
    if self.span_source.is_empty() { self.source.as_ref() } else { self.span_source.as_ref() }
  }
}

/// Already-resolved import edge between two [`ModuleSource::id`] values.
///
/// This crate does not open the filesystem or resolve bare specifiers; the
/// caller (for example Vue Vet's project graph) must supply concrete targets.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleLink {
  pub from: ModuleId,
  pub specifier: String,
  pub to: ModuleId,
}

/// Per-module reactivity graph produced by [`trace_modules`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleReactivity {
  pub id: ModuleId,
  pub graph: std::sync::Arc<ReactivityGraph>,
}

/// Failures while parsing, linking, or tracing a module set.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TraceModulesError {
  #[error("duplicate reactivity module id `{0}`")]
  DuplicateModule(ModuleId),
  #[error("module `{module}` uses unsupported language `{language}`")]
  UnsupportedLanguage { module: ModuleId, language: String },
  #[error("could not parse reactivity module `{module}`: {message}")]
  Parse { module: ModuleId, message: String },
  #[error("could not build semantics for reactivity module `{module}`: {message}")]
  Semantic { module: ModuleId, message: String },
  #[error("reactivity module link {from} -> {to} references an unknown module")]
  UnknownLink { from: ModuleId, to: ModuleId },
  #[error("reactivity module `{from}` resolves `{specifier}` to multiple targets")]
  AmbiguousLink { from: ModuleId, specifier: String },
  #[error("reactivity module worker pool could not complete tracing")]
  WorkerDisconnected,
}

impl TraceModulesError {
  /// Module most directly responsible for this issue, when one exists.
  #[must_use]
  pub const fn module_id(&self) -> Option<&ModuleId> {
    match self {
      Self::DuplicateModule(module)
      | Self::UnsupportedLanguage { module, .. }
      | Self::Parse { module, .. }
      | Self::Semantic { module, .. } => Some(module),
      Self::UnknownLink { from, .. } | Self::AmbiguousLink { from, .. } => Some(from),
      Self::WorkerDisconnected => None,
    }
  }
}

/// Concurrency limit for cross-module tracing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraceModulesOptions {
  /// Maximum native workers used by either tracing phase.
  ///
  /// Honored when [`Self::reuse_current_pool`] is `false` by installing a
  /// dedicated Rayon pool. Session analysis sets `reuse_current_pool` so the
  /// outer `--threads N` pool is shared instead of nesting a second pool.
  pub max_workers: usize,
  /// Use the already-installed Rayon pool (or the global pool) without creating
  /// a nested worker pool. Public callers should leave this `false`.
  pub reuse_current_pool: bool,
}

impl Default for TraceModulesOptions {
  fn default() -> Self {
    Self {
      max_workers: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
      reuse_current_pool: false,
    }
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ImportSummary {
  local: String,
  imported: String,
  source: String,
  span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ExportSummary {
  Local { local: String, exported: String },
  Reexport { source: String, imported: String, exported: String },
  Star { source: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DestructuredCallBinding {
  imported_local: String,
  property: String,
  local: String,
  span: Span,
}

/// `const bag = useFoo()` — whole-object composable call used via member access.
#[derive(Clone, Debug, Eq, PartialEq)]
struct InstanceCallBinding {
  imported_local: String,
  local: String,
  span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ExportState {
  Known(ReactiveBindingKind),
  Composable(BTreeMap<String, ReactiveBindingKind>),
  Ambiguous,
}

/// Export-resolution payload only — no source body, no owned reactivity graph.
/// Shares [`ModuleSummary`] across the seed barrier instead of cloning its vectors.
#[derive(Clone, Debug, Eq, PartialEq)]
struct ModuleExportFacts {
  id: ModuleId,
  summary: Arc<ModuleSummary>,
}

/// Stable module semantic IR extracted from an existing Oxc semantic.
///
/// Cross-file linking consumes this summary instead of parser ASTs. It is
/// intentionally not disk-serializable: callers retain it only for the current
/// analysis lifecycle, and Oxc nodes never cross the adapter boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleSummary {
  imports: Vec<ImportSummary>,
  exports: Vec<ExportSummary>,
  locals: BTreeMap<String, ExportState>,
  provides: Vec<super::ProvideSite>,
  injects: Vec<super::InjectSite>,
  local_graph: std::sync::Arc<ReactivityGraph>,
}

/// Compatibility alias for [`ModuleSummary`].
pub type PreparedModuleTrace = ModuleSummary;

/// Per-import resolution for one consumer module (`import.local` → export state).
/// Spans are applied on the worker that still holds the parse.
type ImportSeedPlan = BTreeMap<String, ExportState>;

/// Cross-module seeds delivered after the barrier (imports + unique inject keys).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ModuleSeedPlan {
  imports: ImportSeedPlan,
  /// inject local → offer (scalar kind and/or composable bag shape).
  injects: BTreeMap<String, ProvideOffer>,
}

/// Reusable state for cross-module linking in a long-lived project session.
///
/// Entries contain only Vue Vet-owned sources, seed plans, and final graphs.
/// No Oxc allocator, AST, or semantic object crosses this boundary.
#[derive(Clone, Debug, Default)]
pub struct ModuleTraceState {
  entries: BTreeMap<ModuleId, CachedModuleTrace>,
  /// Last export/provide/seed fixed-point inputs and outputs.
  linking: Option<CachedLinkingSnapshot>,
}

#[derive(Clone, Debug)]
struct CachedModuleTrace {
  source: ModuleSource,
  plan: ModuleSeedPlan,
  reactivity: ModuleReactivity,
}

/// Cross-scan cache for export resolution and seed plans.
///
/// Linking surface excludes [`ModuleSummary::local_graph`]: a leaf body edit that
/// does not change imports/exports/provides/injects reuses the prior fixed point.
#[derive(Clone, Debug)]
struct CachedLinkingSnapshot {
  links: Vec<ModuleLink>,
  /// Per-module linking surface (imports/exports/locals/provides/injects).
  surfaces: BTreeMap<ModuleId, LinkingSurface>,
  plans: Arc<BTreeMap<ModuleId, ModuleSeedPlan>>,
}

/// Export/seed inputs that participate in the cross-module fixed point.
#[derive(Clone, Debug, Eq, PartialEq)]
struct LinkingSurface {
  imports: Vec<ImportSummary>,
  exports: Vec<ExportSummary>,
  locals: BTreeMap<String, ExportState>,
  provides: Vec<super::ProvideSite>,
  injects: Vec<super::InjectSite>,
}

impl LinkingSurface {
  fn from_summary(summary: &ModuleSummary) -> Self {
    Self {
      imports: summary.imports.clone(),
      exports: summary.exports.clone(),
      locals: summary.locals.clone(),
      provides: summary.provides.clone(),
      injects: summary.injects.clone(),
    }
  }
}

/// Work counters used by incremental tests and performance instrumentation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TraceModulesStats {
  pub phase_one_succeeded: usize,
  pub phase_one_failed: usize,
  pub seeded_reparses: usize,
  pub reused_graphs: usize,
  /// Modules whose seed plans were freshly computed (not taken from linking cache).
  pub seed_plans_recomputed: usize,
  /// Whether `resolve_exports` / provide indexing ran this pass.
  pub export_resolve_ran: bool,
}

/// Partial, deterministic result from a module-linking pass.
#[derive(Debug, Default)]
pub struct TraceModulesReport {
  pub modules: Vec<ModuleReactivity>,
  pub issues: Vec<TraceModulesError>,
  pub stats: TraceModulesStats,
}

impl ModuleSeedPlan {
  fn is_empty(&self) -> bool {
    self.imports.is_empty() && self.injects.is_empty()
  }
}

/// Extract module semantic IR from a semantic already built by Oxc.
///
/// The returned value contains only Vue Vet-owned facts and spans. It can be
/// attached to [`ModuleSource::with_module_summary`] so project tracing does not
/// parse the module again unless cross-module seeds require materialization.
#[must_use]
pub fn prepare_module_summary(
  semantic: &Semantic<'_>,
  span_source: &str,
  source_offset: usize,
  kind: ScriptKind,
  local_graph: impl Into<Arc<ReactivityGraph>>,
) -> ModuleSummary {
  let local_graph = local_graph.into();
  let imports = collect_imports(semantic);
  let exports = collect_exports(semantic);
  let shape_graph = ReactivityGraph {
    bindings: super::collect_reactive_bindings(
      semantic,
      &super::collect_imported_bindings(semantic),
      span_source,
      source_offset,
      kind,
      true,
    ),
    ..ReactivityGraph::default()
  };
  let locals = collect_local_values(semantic, &local_graph, &shape_graph, source_offset);
  let imported_bindings = super::collect_imported_bindings(semantic);
  let provides = collect_provide_sites(
    semantic,
    &imported_bindings,
    &local_graph.bindings,
    &local_graph.composable_instances,
    &BTreeMap::new(),
    kind,
  );
  let injects = collect_inject_sites(semantic, &imported_bindings, &local_graph.bindings, kind);
  ModuleSummary { imports, exports, locals, provides, injects, local_graph }
}

/// Compatibility alias for [`prepare_module_summary`].
#[must_use]
pub fn prepare_module_trace(
  semantic: &Semantic<'_>,
  span_source: &str,
  source_offset: usize,
  kind: ScriptKind,
  local_graph: impl Into<Arc<ReactivityGraph>>,
) -> PreparedModuleTrace {
  prepare_module_summary(semantic, span_source, source_offset, kind, local_graph)
}

/// Traces local and linked reactivity across a resolved module graph.
///
/// Work is bounded by [`TraceModulesOptions::max_workers`]. Phase 1 parses every
/// module and retains only serializable export facts plus the local graph. The
/// coordinator resolves cross-module seeds. Phase 2 reuses local graphs for
/// modules without seeds and reparses only modules that need seed materialization.
///
/// # Errors
///
/// Returns an error when a module cannot be parsed or analyzed, module identifiers
/// are duplicated, or a supplied resolved link is unknown or ambiguous.
pub fn trace_modules(
  modules: &[ModuleSource],
  links: &[ModuleLink],
) -> Result<Vec<ModuleReactivity>, TraceModulesError> {
  trace_modules_with_options(modules, links, TraceModulesOptions::default())
}

/// Traces local and linked reactivity with an explicit worker bound.
///
/// # Errors
///
/// Returns an error when a module cannot be parsed or analyzed, module identifiers
/// are duplicated, a supplied resolved link is invalid, or the worker pool fails.
pub fn trace_modules_with_options(
  modules: &[ModuleSource],
  links: &[ModuleLink],
  options: TraceModulesOptions,
) -> Result<Vec<ModuleReactivity>, TraceModulesError> {
  let mut state = ModuleTraceState::default();
  let report = trace_modules_incremental_with_options(modules, links, options, &mut state);
  if let Some(error) = report.issues.into_iter().next() { Err(error) } else { Ok(report.modules) }
}

/// Trace a module set while retaining healthy cross-module results and reusing
/// unchanged seeded graphs from `state`.
///
/// When [`TraceModulesOptions::reuse_current_pool`] is `false`, installs a
/// dedicated Rayon pool sized to [`TraceModulesOptions::max_workers`]. Session
/// analysis should set `reuse_current_pool: true` after installing its own pool.
#[must_use]
pub fn trace_modules_incremental_with_options(
  modules: &[ModuleSource],
  links: &[ModuleLink],
  options: TraceModulesOptions,
  state: &mut ModuleTraceState,
) -> TraceModulesReport {
  let mut report = TraceModulesReport::default();
  let mut seen = BTreeSet::new();
  let unique = modules
    .iter()
    .filter(|module| {
      if seen.insert(module.id.clone()) {
        true
      } else {
        report.issues.push(TraceModulesError::DuplicateModule(module.id.clone()));
        false
      }
    })
    .collect::<Vec<_>>();
  if unique.is_empty() {
    state.entries.clear();
    state.linking = None;
    return report;
  }

  if options.reuse_current_pool {
    return trace_modules_incremental_in_current_pool(&unique, links, state, report);
  }

  let Ok(pool) = rayon::ThreadPoolBuilder::new()
    .num_threads(options.max_workers.max(1).min(unique.len()))
    .build()
  else {
    report.issues.push(TraceModulesError::WorkerDisconnected);
    return report;
  };
  pool.install(|| trace_modules_incremental_in_current_pool(&unique, links, state, report))
}

fn trace_modules_incremental_in_current_pool(
  unique: &[&ModuleSource],
  links: &[ModuleLink],
  state: &mut ModuleTraceState,
  mut report: TraceModulesReport,
) -> TraceModulesReport {
  let phase_one = unique
    .par_iter()
    .map(|module| analyze_module_phase_one(module))
    .collect::<Vec<Result<ModulePhaseOne, TraceModulesError>>>();
  let mut facts_by_id = BTreeMap::new();
  let mut local_graphs = BTreeMap::new();
  for (module, outcome) in unique.iter().zip(phase_one) {
    match outcome {
      Ok(analysis) => {
        report.stats.phase_one_succeeded += 1;
        facts_by_id.insert(module.id.clone(), analysis.facts);
        local_graphs.insert(module.id.clone(), analysis.local_graph);
      }
      Err(error) => {
        report.stats.phase_one_failed += 1;
        report.issues.push(error);
      }
    }
  }

  let (resolved_links, mut link_issues) = resolved_links_partial(&facts_by_id, links);
  report.issues.append(&mut link_issues);
  let mut owned_links = links.to_vec();
  owned_links.sort_by(|left, right| {
    (&left.from, &left.specifier, &left.to).cmp(&(&right.from, &right.specifier, &right.to))
  });
  owned_links.dedup();

  let surfaces = facts_by_id
    .iter()
    .map(|(id, facts)| (id.clone(), LinkingSurface::from_summary(&facts.summary)))
    .collect::<BTreeMap<_, _>>();
  let plans = if let Some(cached) = state
    .linking
    .as_ref()
    .filter(|cached| cached.links == owned_links && cached.surfaces == surfaces)
  {
    report.stats.seed_plans_recomputed = 0;
    report.stats.export_resolve_ran = false;
    Arc::clone(&cached.plans)
  } else {
    report.stats.export_resolve_ran = true;
    let link_index = link_index(&resolved_links);
    let exports = resolve_exports(&facts_by_id, &link_index);
    let provide_index = global_provide_index(&facts_by_id);
    let mut next_plans = BTreeMap::new();
    for module in unique {
      let Some(facts) = facts_by_id.get(&module.id) else {
        continue;
      };
      next_plans.insert(
        module.id.clone(),
        ModuleSeedPlan {
          imports: seed_plan_for(facts, &exports, &link_index),
          injects: inject_seed_plan(facts, &provide_index),
        },
      );
    }
    let previous_plans = state.linking.as_ref().map(|cached| Arc::clone(&cached.plans));
    report.stats.seed_plans_recomputed = next_plans
      .iter()
      .filter(|(id, plan)| previous_plans.as_ref().is_none_or(|prev| prev.get(*id) != Some(plan)))
      .count();
    let plans = Arc::new(next_plans);
    state.linking =
      Some(CachedLinkingSnapshot { links: owned_links, surfaces, plans: Arc::clone(&plans) });
    plans
  };

  let work = unique
    .iter()
    .filter_map(|module| {
      let local_graph = local_graphs.remove(&module.id)?;
      let plan = plans.get(&module.id)?.clone();
      Some((*module, local_graph, plan))
    })
    .collect::<Vec<_>>();

  let outcomes = work
    .into_par_iter()
    .map(|(module, mut local_graph, plan)| {
      if let Some(cached) = state.entries.get(&module.id)
        && cached.source == *module
        && cached.plan == plan
      {
        return PhaseTwoOutcome::Reused { reactivity: cached.reactivity.clone() };
      }
      let seeded = !plan.is_empty();
      match trace_module_phase_two(module, Arc::clone(&local_graph), &plan) {
        Ok(reactivity) => {
          PhaseTwoOutcome::Traced { source: module.clone(), plan, reactivity, seeded }
        }
        Err(error) => {
          Arc::make_mut(&mut local_graph).set_module_id(module.id.clone());
          PhaseTwoOutcome::Partial {
            source: module.clone(),
            plan,
            reactivity: ModuleReactivity { id: module.id.clone(), graph: local_graph },
            error,
          }
        }
      }
    })
    .collect::<Vec<_>>();

  let mut keep = BTreeSet::new();
  for outcome in outcomes {
    match outcome {
      PhaseTwoOutcome::Reused { reactivity } => {
        report.stats.reused_graphs += 1;
        keep.insert(reactivity.id.clone());
        report.modules.push(reactivity);
      }
      PhaseTwoOutcome::Traced { source, plan, reactivity, seeded } => {
        report.stats.seeded_reparses += usize::from(seeded);
        keep.insert(reactivity.id.clone());
        state.entries.insert(
          reactivity.id.clone(),
          CachedModuleTrace { source, plan, reactivity: reactivity.clone() },
        );
        report.modules.push(reactivity);
      }
      PhaseTwoOutcome::Partial { source, plan, reactivity, error } => {
        report.issues.push(error);
        keep.insert(reactivity.id.clone());
        state.entries.insert(
          reactivity.id.clone(),
          CachedModuleTrace { source, plan, reactivity: reactivity.clone() },
        );
        report.modules.push(reactivity);
      }
    }
  }
  state.entries.retain(|module_id, _| keep.contains(module_id));
  report.modules.sort_by(|left, right| left.id.cmp(&right.id));
  report.issues.sort_by(|left, right| {
    (left.module_id(), left.to_string()).cmp(&(right.module_id(), right.to_string()))
  });
  report
}

enum PhaseTwoOutcome {
  Reused {
    reactivity: ModuleReactivity,
  },
  Traced {
    source: ModuleSource,
    plan: ModuleSeedPlan,
    reactivity: ModuleReactivity,
    seeded: bool,
  },
  Partial {
    source: ModuleSource,
    plan: ModuleSeedPlan,
    reactivity: ModuleReactivity,
    error: TraceModulesError,
  },
}

struct ModulePhaseOne {
  facts: ModuleExportFacts,
  local_graph: Arc<ReactivityGraph>,
}

fn analyze_module_phase_one(module: &ModuleSource) -> Result<ModulePhaseOne, TraceModulesError> {
  if let Some(summary) = &module.module_summary {
    return Ok(phase_one_from_summary(module, summary));
  }

  let allocator = Allocator::default();
  let source_type = source_type(module)?;
  let parsed = Parser::new(&allocator, module.source.as_ref(), source_type).parse();
  if !parsed.errors.is_empty() {
    return Err(TraceModulesError::Parse {
      module: module.id.clone(),
      message: join_errors(&parsed.errors),
    });
  }
  let built = SemanticBuilder::new().with_check_syntax_error(true).build(&parsed.program);
  if !built.errors.is_empty() {
    return Err(TraceModulesError::Semantic {
      module: module.id.clone(),
      message: join_errors(&built.errors),
    });
  }
  let semantic = built.semantic;

  let empty = TraceSeeds::default();
  let local_graph = Arc::new(trace_reactivity_seeded(
    &semantic,
    module.span_origin(),
    module.source_offset,
    module.kind,
    &empty,
  ));
  let summary = Arc::new(prepare_module_summary(
    &semantic,
    module.span_origin(),
    module.source_offset,
    module.kind,
    Arc::clone(&local_graph),
  ));
  Ok(phase_one_from_summary(module, &summary))
}

fn phase_one_from_summary(module: &ModuleSource, summary: &Arc<ModuleSummary>) -> ModulePhaseOne {
  ModulePhaseOne {
    facts: ModuleExportFacts { id: module.id.clone(), summary: Arc::clone(summary) },
    local_graph: Arc::clone(&summary.local_graph),
  }
}

fn trace_module_phase_two(
  module: &ModuleSource,
  mut local_graph: Arc<ReactivityGraph>,
  plan: &ModuleSeedPlan,
) -> Result<ModuleReactivity, TraceModulesError> {
  if plan.is_empty() {
    Arc::make_mut(&mut local_graph).set_module_id(module.id.clone());
    return Ok(ModuleReactivity { id: module.id.clone(), graph: local_graph });
  }

  let allocator = Allocator::default();
  let source_type = source_type(module)?;
  let parsed = Parser::new(&allocator, module.source.as_ref(), source_type).parse();
  if !parsed.errors.is_empty() {
    return Err(TraceModulesError::Parse {
      module: module.id.clone(),
      message: join_errors(&parsed.errors),
    });
  }
  let built = SemanticBuilder::new().with_check_syntax_error(true).build(&parsed.program);
  if !built.errors.is_empty() {
    return Err(TraceModulesError::Semantic {
      module: module.id.clone(),
      message: join_errors(&built.errors),
    });
  }
  let semantic = built.semantic;
  let seeds = materialize_seeds(module, &semantic, plan);
  let mut graph = trace_reactivity_seeded(
    &semantic,
    module.span_origin(),
    module.source_offset,
    module.kind,
    &seeds,
  );
  graph.set_module_id(module.id.clone());
  Ok(ModuleReactivity { id: module.id.clone(), graph: Arc::new(graph) })
}

fn source_type(module: &ModuleSource) -> Result<SourceType, TraceModulesError> {
  match module.language.as_str() {
    "js" | "javascript" => Ok(SourceType::mjs()),
    "jsx" => Ok(SourceType::jsx()),
    "ts" | "typescript" => Ok(SourceType::ts()),
    "tsx" => Ok(SourceType::tsx()),
    language => Err(TraceModulesError::UnsupportedLanguage {
      module: module.id.clone(),
      language: language.into(),
    }),
  }
}

fn collect_imports(semantic: &oxc_semantic::Semantic<'_>) -> Vec<ImportSummary> {
  let mut imports = Vec::new();
  for node in semantic.nodes() {
    let AstKind::ImportDeclaration(declaration) = node.kind() else {
      continue;
    };
    let Some(specifiers) = &declaration.specifiers else {
      continue;
    };
    let source = declaration.source.value.to_string();
    for specifier in specifiers {
      let (local, imported, span) = match specifier {
        ImportDeclarationSpecifier::ImportSpecifier(specifier) => (
          specifier.local.name.to_string(),
          module_export_name(&specifier.imported),
          specifier.local.span,
        ),
        ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
          (specifier.local.name.to_string(), "default".into(), specifier.local.span)
        }
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
          (specifier.local.name.to_string(), "*".into(), specifier.local.span)
        }
      };
      imports.push(ImportSummary { local, imported, source: source.clone(), span });
    }
  }
  imports.sort_by_key(|import| import.span.start);
  imports
}

fn collect_local_values(
  semantic: &oxc_semantic::Semantic<'_>,
  public_graph: &ReactivityGraph,
  shape_graph: &ReactivityGraph,
  script_offset: usize,
) -> BTreeMap<String, ExportState> {
  let mut locals = public_graph
    .bindings
    .iter()
    .map(|binding| (binding.name.clone(), ExportState::Known(binding.kind)))
    .collect::<BTreeMap<_, _>>();

  // `function useX() { return { field } }` (incl. `export default function useX`)
  for node in semantic.nodes() {
    let AstKind::Function(function) = node.kind() else {
      continue;
    };
    let Some(identifier) = &function.id else {
      continue;
    };
    let shape =
      composable_return_shape(semantic, function.node_id.get(), shape_graph, script_offset);
    if !shape.is_empty() {
      locals.insert(identifier.name.to_string(), ExportState::Composable(shape));
    }
  }

  // `const useX = () => ({ … })` / `export const useX = function () { … }`
  for node in semantic.nodes() {
    let AstKind::VariableDeclarator(declarator) = node.kind() else {
      continue;
    };
    let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
      continue;
    };
    let Some(init) = &declarator.init else {
      continue;
    };
    let function_id = match init {
      Expression::ArrowFunctionExpression(arrow) => arrow.node_id.get(),
      Expression::FunctionExpression(function) => function.node_id.get(),
      _ => continue,
    };
    let shape = composable_return_shape(semantic, function_id, shape_graph, script_offset);
    if shape.is_empty() {
      continue;
    }
    locals.insert(identifier.name.to_string(), ExportState::Composable(shape));
  }
  locals
}

/// Object shape returned by a composable function / arrow (under-approx).
///
/// `script_offset` must match the offset used when materializing `graph.bindings`
/// spans (0 for standalone modules, Vize `loc.start` for SFC script bodies).
pub fn composable_return_shape(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  graph: &ReactivityGraph,
  script_offset: usize,
) -> BTreeMap<String, ReactiveBindingKind> {
  let imported_bindings = collect_imported_bindings(semantic);
  let param_names = function_param_names(semantic, function_id);
  let mut shape = BTreeMap::new();
  let mut ambiguous = BTreeSet::new();

  // `() => ({ field: ref(0) })` expression body — no ReturnStatement node.
  if let AstKind::ArrowFunctionExpression(arrow) = semantic.nodes().kind(function_id)
    && arrow.expression
    && let Some(statement) = arrow.body.statements.first()
    && let oxc_ast::ast::Statement::ExpressionStatement(expression) = statement
  {
    merge_return_object_into_shape(
      semantic,
      &expression.expression,
      graph,
      &imported_bindings,
      &param_names,
      script_offset,
      &mut shape,
      &mut ambiguous,
    );
  }

  for (return_id, node) in semantic.nodes().iter_enumerated() {
    let AstKind::ReturnStatement(statement) = node.kind() else {
      continue;
    };
    let owner = semantic.nodes().ancestor_ids(return_id).find(|ancestor_id| {
      matches!(
        semantic.nodes().kind(*ancestor_id),
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
      )
    });
    if owner != Some(function_id) {
      continue;
    }
    let Some(argument) = &statement.argument else {
      continue;
    };
    merge_return_object_into_shape(
      semantic,
      argument,
      graph,
      &imported_bindings,
      &param_names,
      script_offset,
      &mut shape,
      &mut ambiguous,
    );
  }
  shape
}

#[expect(
  clippy::too_many_arguments,
  reason = "shape merge is a pure helper; packing args would obscure the call sites"
)]
fn merge_return_object_into_shape(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  graph: &ReactivityGraph,
  imported_bindings: &BTreeMap<String, (String, String)>,
  param_names: &BTreeSet<String>,
  script_offset: usize,
  shape: &mut BTreeMap<String, ReactiveBindingKind>,
  ambiguous: &mut BTreeSet<String>,
) {
  // `() => ({ field })` wraps the object in parentheses.
  let expression = match expression {
    Expression::ParenthesizedExpression(paren) => &paren.expression,
    other => other,
  };
  // `return toRefs(param)` — every static key is ToRef when the argument is a parameter.
  if let Expression::CallExpression(call) = expression
    && resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Script)
      .is_some_and(|callee| callee == "toRefs")
    && call
      .arguments
      .first()
      .and_then(Argument::as_expression)
      .and_then(Expression::get_identifier_reference)
      .is_some_and(|identifier| param_names.contains(identifier.name.as_str()))
  {
    // Without an object shape we cannot invent keys; leave quiet.
    return;
  }
  let Expression::ObjectExpression(object) = expression else {
    return;
  };
  for property in &object.properties {
    let ObjectPropertyKind::ObjectProperty(property) = property else {
      continue;
    };
    let Some(exported) = property.key.static_name() else {
      continue;
    };
    let Some(kind) = reactive_return_kind(
      semantic,
      &property.value,
      graph,
      imported_bindings,
      param_names,
      script_offset,
    ) else {
      continue;
    };
    let exported = exported.into_owned();
    if ambiguous.contains(&exported) {
      continue;
    }
    match shape.entry(exported.clone()) {
      Entry::Vacant(entry) => {
        entry.insert(kind);
      }
      Entry::Occupied(entry) if *entry.get() == kind => {}
      Entry::Occupied(entry) => {
        entry.remove();
        ambiguous.insert(exported);
      }
    }
  }
}

fn function_param_names(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
) -> BTreeSet<String> {
  let mut names = BTreeSet::new();
  let parameters = match semantic.nodes().kind(function_id) {
    AstKind::Function(function) => function.params.items.as_slice(),
    AstKind::ArrowFunctionExpression(callback) => callback.params.items.as_slice(),
    _ => return names,
  };
  for parameter in parameters {
    let mut identifiers = Vec::new();
    collect_binding_identifiers(&parameter.pattern, &mut identifiers);
    for (name, _) in identifiers {
      names.insert(name);
    }
  }
  names
}

fn reactive_return_kind(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  graph: &ReactivityGraph,
  imported_bindings: &BTreeMap<String, (String, String)>,
  param_names: &BTreeSet<String>,
  script_offset: usize,
) -> Option<ReactiveBindingKind> {
  if let Some(reference) = expression.get_identifier_reference() {
    if param_names.contains(reference.name.as_str()) {
      // Parametric pass-through: treat as reactive object/ref surface.
      return Some(ReactiveBindingKind::Reactive);
    }
    return graph
      .bindings
      .iter()
      .find(|binding| {
        binding.name == reference.name.as_str()
          && reference_resolves_to_binding(semantic, reference, binding, script_offset)
      })
      .map(|binding| binding.kind);
  }

  let Expression::CallExpression(call) = expression else {
    return None;
  };
  let callee = resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Script)?;
  if matches!(callee.as_str(), "toRef" | "toRefs") {
    // Parametric when first argument is a function parameter.
    if call
      .arguments
      .first()
      .and_then(Argument::as_expression)
      .and_then(Expression::get_identifier_reference)
      .is_some_and(|identifier| param_names.contains(identifier.name.as_str()))
    {
      return Some(ReactiveBindingKind::ToRef);
    }
  }
  reactive_binding_kind(&callee)
}

fn resolve_imported_callee<'a>(
  semantic: &oxc_semantic::Semantic<'_>,
  callee: &oxc_ast::ast::IdentifierReference<'_>,
  imports: &'a [ImportSummary],
) -> Option<&'a ImportSummary> {
  imports.iter().find(|import| {
    if import.local != callee.name.as_str() {
      return false;
    }
    let Some(reference_id) = callee.reference_id.get() else {
      return false;
    };
    semantic
      .scoping()
      .get_reference(reference_id)
      .symbol_id()
      .is_some_and(|symbol_id| semantic.scoping().symbol_span(symbol_id) == import.span)
  })
}

fn collect_destructured_calls(
  semantic: &oxc_semantic::Semantic<'_>,
  imports: &[ImportSummary],
) -> Vec<DestructuredCallBinding> {
  let mut calls = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = call.callee.get_identifier_reference() else {
      continue;
    };
    let Some(import) = resolve_imported_callee(semantic, callee, imports) else {
      continue;
    };
    let AstKind::VariableDeclarator(declarator) = semantic.nodes().parent_kind(call.node_id.get())
    else {
      continue;
    };
    let BindingPattern::ObjectPattern(pattern) = &declarator.id else {
      continue;
    };
    for property in &pattern.properties {
      let Some(exported) = property.key.static_name() else {
        continue;
      };
      let mut identifiers = Vec::new();
      collect_binding_identifiers(&property.value, &mut identifiers);
      for (local, span) in identifiers {
        calls.push(DestructuredCallBinding {
          imported_local: import.local.clone(),
          property: exported.to_string(),
          local,
          span,
        });
      }
    }
  }
  calls.sort_by_key(|call| call.span.start);
  calls
}

fn collect_instance_calls(
  semantic: &oxc_semantic::Semantic<'_>,
  imports: &[ImportSummary],
) -> Vec<InstanceCallBinding> {
  let mut calls = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = call.callee.get_identifier_reference() else {
      continue;
    };
    let Some(import) = resolve_imported_callee(semantic, callee, imports) else {
      continue;
    };
    let AstKind::VariableDeclarator(declarator) = semantic.nodes().parent_kind(call.node_id.get())
    else {
      continue;
    };
    let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
      continue;
    };
    calls.push(InstanceCallBinding {
      imported_local: import.local.clone(),
      local: identifier.name.to_string(),
      span: identifier.span,
    });
  }
  calls.sort_by_key(|call| call.span.start);
  calls
}

fn collect_exports(semantic: &oxc_semantic::Semantic<'_>) -> Vec<ExportSummary> {
  let mut exports = Vec::new();
  for node in semantic.nodes() {
    match node.kind() {
      AstKind::ExportNamedDeclaration(declaration) => {
        match &declaration.declaration {
          Some(Declaration::VariableDeclaration(variable)) => {
            for declarator in &variable.declarations {
              let mut identifiers = Vec::new();
              collect_binding_identifiers(&declarator.id, &mut identifiers);
              for (local, _) in identifiers {
                exports.push(ExportSummary::Local { exported: local.clone(), local });
              }
            }
          }
          Some(Declaration::FunctionDeclaration(function)) => {
            if let Some(identifier) = &function.id {
              let local = identifier.name.to_string();
              exports.push(ExportSummary::Local { exported: local.clone(), local });
            }
          }
          _ => {}
        }
        for specifier in &declaration.specifiers {
          let local = module_export_name(&specifier.local);
          let exported = module_export_name(&specifier.exported);
          if let Some(source) = &declaration.source {
            exports.push(ExportSummary::Reexport {
              source: source.value.to_string(),
              imported: local,
              exported,
            });
          } else {
            exports.push(ExportSummary::Local { local, exported });
          }
        }
      }
      AstKind::ExportDefaultDeclaration(declaration) => match &declaration.declaration {
        ExportDefaultDeclarationKind::Identifier(identifier) => {
          exports.push(ExportSummary::Local {
            local: identifier.name.to_string(),
            exported: "default".into(),
          });
        }
        ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
          // `export default function useX() { … }` — local name is the function id.
          if let Some(identifier) = &function.id {
            exports.push(ExportSummary::Local {
              local: identifier.name.to_string(),
              exported: "default".into(),
            });
          }
        }
        _ => {}
      },
      AstKind::ExportAllDeclaration(declaration) if declaration.exported.is_none() => {
        exports.push(ExportSummary::Star { source: declaration.source.value.to_string() });
      }
      _ => {}
    }
  }
  exports
}

fn resolved_links_partial(
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &[ModuleLink],
) -> (BTreeMap<(ModuleId, String), ModuleId>, Vec<TraceModulesError>) {
  let mut resolved = BTreeMap::new();
  let mut ambiguous = BTreeSet::new();
  let mut issues = Vec::new();
  for link in links {
    if !facts.contains_key(&link.from) || !facts.contains_key(&link.to) {
      issues.push(TraceModulesError::UnknownLink { from: link.from.clone(), to: link.to.clone() });
      continue;
    }
    let key = (link.from.clone(), link.specifier.clone());
    if ambiguous.contains(&key) {
      continue;
    }
    match resolved.entry(key) {
      Entry::Vacant(entry) => {
        entry.insert(link.to.clone());
      }
      Entry::Occupied(entry) if entry.get() == &link.to => {}
      Entry::Occupied(entry) => {
        let key = entry.key().clone();
        entry.remove();
        ambiguous.insert(key);
        issues.push(TraceModulesError::AmbiguousLink {
          from: link.from.clone(),
          specifier: link.specifier.clone(),
        });
      }
    }
  }
  (resolved, issues)
}

fn resolve_exports(
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
) -> BTreeMap<ModuleId, BTreeMap<String, ExportState>> {
  use std::collections::VecDeque;

  let mut resolved =
    facts.keys().map(|id| (id.clone(), BTreeMap::new())).collect::<BTreeMap<_, _>>();

  for (id, module_facts) in facts {
    for export in &module_facts.summary.exports {
      let ExportSummary::Local { local, exported } = export else {
        continue;
      };
      if let Some(state) = module_facts.summary.locals.get(local) {
        insert_export(&mut resolved, id, exported, state.clone());
      }
    }
  }

  // target module → consumers that import/re-export from it
  let mut reverse_users: BTreeMap<&ModuleId, Vec<&ModuleId>> = BTreeMap::new();
  for ((from, _), to) in links {
    reverse_users.entry(*to).or_default().push(*from);
  }

  let mut queue = VecDeque::new();
  let mut queued = BTreeSet::new();
  for (id, module_facts) in facts {
    if module_facts
      .summary
      .exports
      .iter()
      .any(|export| matches!(export, ExportSummary::Reexport { .. } | ExportSummary::Star { .. }))
    {
      queue.push_back(id);
      queued.insert(id);
    }
  }

  while let Some(id) = queue.pop_front() {
    queued.remove(id);
    let Some(module_facts) = facts.get(id) else {
      continue;
    };
    let mut changed = false;
    for export in &module_facts.summary.exports {
      match export {
        ExportSummary::Local { .. } => {}
        ExportSummary::Reexport { source, imported, exported } => {
          let Some(target) = links.get(&(id, source.as_str())).copied() else {
            continue;
          };
          let Some(state) = resolved.get(target).and_then(|exports| exports.get(imported)).cloned()
          else {
            continue;
          };
          changed |= insert_export(&mut resolved, id, exported, state);
        }
        ExportSummary::Star { source } => {
          let Some(target) = links.get(&(id, source.as_str())).copied() else {
            continue;
          };
          let Some(target_exports) = resolved.get(target).cloned() else {
            continue;
          };
          for (exported, state) in target_exports {
            if exported != "default" {
              changed |= insert_export(&mut resolved, id, &exported, state);
            }
          }
        }
      }
    }
    if !changed {
      continue;
    }
    let Some(users) = reverse_users.get(id) else {
      continue;
    };
    for consumer in users {
      if queued.insert(consumer) {
        queue.push_back(consumer);
      }
    }
  }

  resolved
}

/// Borrowed index over owned resolved links — avoids re-allocating key pairs on lookup.
fn link_index(
  links: &BTreeMap<(ModuleId, String), ModuleId>,
) -> BTreeMap<(&ModuleId, &str), &ModuleId> {
  links.iter().map(|((from, specifier), to)| ((from, specifier.as_str()), to)).collect()
}

fn insert_export(
  resolved: &mut BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
  module: &ModuleId,
  exported: &str,
  state: ExportState,
) -> bool {
  let Some(module_exports) = resolved.get_mut(module) else {
    return false;
  };
  match module_exports.entry(exported.into()) {
    Entry::Vacant(entry) => {
      entry.insert(state);
      true
    }
    Entry::Occupied(mut entry)
      if entry.get() != &state && entry.get() != &ExportState::Ambiguous =>
    {
      entry.insert(ExportState::Ambiguous);
      true
    }
    Entry::Occupied(_) => false,
  }
}

/// Coordinator-side: which of this module's import locals resolve to reactive exports.
fn seed_plan_for(
  facts: &ModuleExportFacts,
  exports: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
) -> ImportSeedPlan {
  let mut plan = ImportSeedPlan::new();
  for import in &facts.summary.imports {
    if import.imported == "*" {
      continue;
    }
    let Some(target) = links.get(&(&facts.id, import.source.as_str())).copied() else {
      continue;
    };
    let Some(state) =
      exports.get(target).and_then(|module_exports| module_exports.get(&import.imported))
    else {
      continue;
    };
    // Only the resolved export state crosses the barrier (not source text / graphs).
    plan.insert(import.local.clone(), state.clone());
  }
  plan
}

/// Project-wide provide index (no App Tree): key → offers from every known site.
fn global_provide_index(
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
) -> BTreeMap<super::InjectionKey, Vec<ProvideOffer>> {
  let mut all = Vec::new();
  for module in facts.values() {
    all.extend(module.summary.provides.iter().cloned());
  }
  provide_offer_index(&all)
}

/// Unique inject seeds for one consumer (multi-provide keys stay quiet).
fn inject_seed_plan(
  facts: &ModuleExportFacts,
  provide_index: &BTreeMap<super::InjectionKey, Vec<ProvideOffer>>,
) -> BTreeMap<String, ProvideOffer> {
  let mut plan = BTreeMap::new();
  for inject in &facts.summary.injects {
    let Some(offer) = resolve_inject_offer(provide_index, inject) else {
      continue;
    };
    plan.insert(inject.local.clone(), offer);
  }
  plan
}

/// Worker-side: attach SFC-absolute spans from the live parse (no second parse).
fn materialize_seeds(
  module: &ModuleSource,
  semantic: &Semantic<'_>,
  plan: &ModuleSeedPlan,
) -> TraceSeeds {
  if plan.is_empty() {
    return TraceSeeds::default();
  }
  let imports = collect_imports(semantic);
  let destructured_calls = collect_destructured_calls(semantic, &imports);
  let instance_calls = collect_instance_calls(semantic, &imports);
  let span_source = module.span_origin();
  let span_base = module.source_offset;
  let mut seeds = TraceSeeds::default();
  for import in &imports {
    let Some(state) = plan.imports.get(&import.local) else {
      continue;
    };
    match state {
      ExportState::Known(kind) => seeds.bindings.push(ReactiveBindingFact {
        name: import.local.clone(),
        kind: *kind,
        initialized_with_null: false,
        span: source_span(span_source, span_base, import.span),
      }),
      ExportState::Composable(shape) => {
        for call in destructured_calls.iter().filter(|call| call.imported_local == import.local) {
          let Some(kind) = shape.get(&call.property) else {
            continue;
          };
          seeds.bindings.push(ReactiveBindingFact {
            name: call.local.clone(),
            kind: *kind,
            initialized_with_null: false,
            span: source_span(span_source, span_base, call.span),
          });
        }
        for call in instance_calls.iter().filter(|call| call.imported_local == import.local) {
          // Only record the instance bag for `bag.field.value` resolution.
          seeds.composable_instances.insert(call.local.clone(), shape.clone());
        }
      }
      ExportState::Ambiguous => {}
    }
  }
  // Inject locals: re-read sites for exact spans; offers from the coordinator plan.
  if !plan.injects.is_empty() {
    let imported_bindings = super::collect_imported_bindings(semantic);
    let injects = collect_inject_sites(semantic, &imported_bindings, &[], module.kind);
    for inject in injects {
      let Some(offer) = plan.injects.get(&inject.local) else {
        continue;
      };
      if let Some(kind) = offer.kind
        && !seeds.bindings.iter().any(|binding| binding.name == inject.local)
      {
        seeds.bindings.push(ReactiveBindingFact {
          name: inject.local.clone(),
          kind,
          initialized_with_null: false,
          span: source_span(span_source, span_base, inject.span),
        });
      }
      if let Some(shape) = &offer.instance_shape {
        seeds.composable_instances.entry(inject.local).or_insert_with(|| shape.clone());
      }
    }
  }
  seeds
}

fn join_errors(errors: &[impl ToString]) -> String {
  errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
}
