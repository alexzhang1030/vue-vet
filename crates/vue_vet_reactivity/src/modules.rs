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
    // `span_source` is excluded: style-only SFC edits change the wrapper file
    // without invalidating script body IR when `source` + `source_offset` match.
    self.id == other.id
      && self.source == other.source
      && self.language == other.language
      && self.kind == other.kind
      && self.source_offset == other.source_offset
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

  /// Borrow the attached module semantic IR, when present.
  #[must_use]
  pub fn module_summary(&self) -> Option<Arc<ModuleSummary>> {
    self.module_summary.as_ref().map(Arc::clone)
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
  /// Retain export/seed fixed-point snapshots on `state` for later incremental
  /// scans. One-shot [`trace_modules_with_options`] forces this off so cold
  /// `trace_*` benches do not pay archive costs that are immediately discarded.
  pub persist_linking_cache: bool,
}

impl Default for TraceModulesOptions {
  fn default() -> Self {
    Self {
      max_workers: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
      reuse_current_pool: false,
      persist_linking_cache: true,
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

/// Synthetic [`ModuleLink`] specifier prefix for bare Nuxt auto-import calls.
///
/// Kept in sync with `vue_vet_project::conventions::NUXT_IMPORTS_SPECIFIER_PREFIX`.
const NUXT_IMPORTS_SPECIFIER_PREFIX: &str = "#nuxt-imports:";
/// Exclusive end for [`BTreeMap::range`] over `#nuxt-imports:…` keys (`';` follows `:`).
const NUXT_IMPORTS_RANGE_END: &str = "#nuxt-imports;";

#[derive(Clone, Debug, Eq, PartialEq)]
enum ExportState {
  /// Imported local is itself a reactive binding (`import { count } from './x'`).
  Known(ReactiveBindingKind),
  /// Calling the export returns a statically keyed object bag.
  Composable(BTreeMap<String, ReactiveBindingKind>),
  /// Calling the export returns a scalar reactive value (`return ref(0)` / `(): Ref<T>`).
  Factory(ReactiveBindingKind),
  /// Declared `() => PlainObject` (no Ref fields) — needs body evidence for Reactive factory.
  DeclaredPlainObjectFactory,
  /// Body unwraps a state ref (e.g. `return useState(...).value`) — needs plain-object declaration.
  BodyUnwrappedState,
  Ambiguous,
}

/// Under-approx classification of a composable/factory function return.
#[derive(Clone, Debug, Eq, PartialEq)]
enum ComposableReturn {
  Object(BTreeMap<String, ReactiveBindingKind>),
  Factory(ReactiveBindingKind),
  /// Body unwraps a state ref (`return useState(...).value`, unresolved / `#imports`).
  UnwrappedState,
}

/// Declared TypeScript return surface for factory/composable exports.
#[derive(Clone, Debug, Eq, PartialEq)]
enum DeclaredReturn {
  Factory(ReactiveBindingKind),
  Composable(BTreeMap<String, ReactiveBindingKind>),
  /// Object-shaped type with ≥1 property and no Ref-like fields.
  PlainObject,
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

impl ModuleSummary {
  /// Specifiers this module imports or re-exports (for external follow).
  #[must_use]
  pub fn follow_specifiers(&self) -> Vec<String> {
    let mut specifiers = BTreeSet::new();
    for import in &self.imports {
      specifiers.insert(import.source.clone());
    }
    for export in &self.exports {
      match export {
        ExportSummary::Reexport { source, .. } | ExportSummary::Star { source } => {
          specifiers.insert(source.clone());
        }
        ExportSummary::Local { .. } => {}
      }
    }
    specifiers.into_iter().collect()
  }

  /// Whether any export local is a finished Factory/Composable/Known seed.
  #[must_use]
  pub fn has_reactivity_export_seeds(&self) -> bool {
    self.locals.values().any(|state| {
      matches!(state, ExportState::Factory(_) | ExportState::Composable(_) | ExportState::Known(_))
    })
  }

  /// Whether a companion implementation file may still complete provisional seeds.
  #[must_use]
  pub fn needs_implementation_merge(&self) -> bool {
    self.locals.values().any(|state| {
      matches!(state, ExportState::DeclaredPlainObjectFactory | ExportState::BodyUnwrappedState)
    }) || !self.has_reactivity_export_seeds()
  }

  /// Replace locals after merging a declaration file with its implementation body.
  #[must_use]
  fn with_locals(mut self, locals: BTreeMap<String, ExportState>) -> Self {
    self.locals = locals;
    self
  }
}

/// Merge `.d.ts` declaration locals with companion implementation locals.
///
/// `DeclaredPlainObjectFactory` + `BodyUnwrappedState` → `Factory(Reactive)`.
#[must_use]
pub fn merge_declaration_implementation_summary(
  declaration: ModuleSummary,
  implementation: &ModuleSummary,
) -> ModuleSummary {
  let mut merged = declaration.locals.clone();
  for (name, impl_state) in &implementation.locals {
    match (merged.get(name), impl_state) {
      (Some(ExportState::DeclaredPlainObjectFactory), ExportState::BodyUnwrappedState)
      | (Some(ExportState::BodyUnwrappedState), ExportState::DeclaredPlainObjectFactory) => {
        merged.insert(name.clone(), ExportState::Factory(ReactiveBindingKind::Reactive));
      }
      (Some(ExportState::DeclaredPlainObjectFactory), ExportState::Factory(kind))
        if *kind == ReactiveBindingKind::Reactive =>
      {
        merged.insert(name.clone(), ExportState::Factory(ReactiveBindingKind::Reactive));
      }
      (
        None | Some(ExportState::DeclaredPlainObjectFactory | ExportState::BodyUnwrappedState),
        state,
      ) if matches!(
        state,
        ExportState::Factory(_) | ExportState::Composable(_) | ExportState::Known(_)
      ) =>
      {
        merged.insert(name.clone(), state.clone());
      }
      (None, ExportState::BodyUnwrappedState | ExportState::DeclaredPlainObjectFactory) => {
        merged.insert(name.clone(), impl_state.clone());
      }
      _ => {}
    }
  }
  declaration.with_locals(merged)
}

/// Parse a standalone module and attach its [`ModuleSummary`] (external seed path).
///
/// # Errors
///
/// Returns parse/semantic errors for invalid sources or unsupported languages.
pub fn prepare_standalone_module_source(
  id: impl Into<ModuleId>,
  source: impl Into<Arc<str>>,
  language: impl Into<String>,
) -> Result<ModuleSource, TraceModulesError> {
  let module = ModuleSource::standalone(id, source, language, ScriptKind::Script);
  let phase = analyze_module_phase_one(&module)?;
  Ok(module.with_module_summary(phase.facts.summary))
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
  summary: Arc<ModuleSummary>,
  plan: ModuleSeedPlan,
  reactivity: ModuleReactivity,
}

/// Cross-scan cache for export resolution and seed plans.
///
/// Linking surface excludes [`ModuleSummary::local_graph`]: a leaf body edit that
/// does not change imports/exports/provides/injects reuses the prior fixed point.
/// When only a subset of surfaces change, seed plans are recomputed for the
/// export/inject closure — not every module.
///
/// Summaries are retained as [`Arc`] handles so warm reuse can use
/// [`Arc::ptr_eq`] instead of cloning imports/exports/locals on every scan.
#[derive(Clone, Debug)]
struct CachedLinkingSnapshot {
  links: Vec<ModuleLink>,
  /// Phase-one summaries keyed for linking-surface equality (not `local_graph`).
  summaries: BTreeMap<ModuleId, Arc<ModuleSummary>>,
  exports: Arc<BTreeMap<ModuleId, BTreeMap<String, ExportState>>>,
  provide_index: Arc<BTreeMap<super::InjectionKey, Vec<super::ProvideOffer>>>,
  plans: Arc<BTreeMap<ModuleId, ModuleSeedPlan>>,
}

/// Linking-relevant fields only — never clones; prefers [`Arc::ptr_eq`].
fn linking_surface_eq(left: &Arc<ModuleSummary>, right: &Arc<ModuleSummary>) -> bool {
  Arc::ptr_eq(left, right)
    || (left.imports == right.imports
      && left.exports == right.exports
      && left.locals == right.locals
      && left.provides == right.provides
      && left.injects == right.injects)
}

fn linking_cache_reusable(
  owned_links: &[ModuleLink],
  facts_by_id: &BTreeMap<ModuleId, ModuleExportFacts>,
  cached: &CachedLinkingSnapshot,
) -> bool {
  if cached.links != owned_links || cached.summaries.len() != facts_by_id.len() {
    return false;
  }
  facts_by_id.iter().all(|(id, facts)| {
    cached.summaries.get(id).is_some_and(|prev| linking_surface_eq(&facts.summary, prev))
  })
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
  mut options: TraceModulesOptions,
) -> Result<Vec<ModuleReactivity>, TraceModulesError> {
  // Fresh state is dropped on return — never archive linking snapshots.
  options.persist_linking_cache = false;
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
    return trace_modules_incremental_in_current_pool(&unique, links, state, report, options);
  }

  let Ok(pool) = rayon::ThreadPoolBuilder::new()
    .num_threads(options.max_workers.max(1).min(unique.len()))
    .build()
  else {
    report.issues.push(TraceModulesError::WorkerDisconnected);
    return report;
  };
  pool.install(|| trace_modules_incremental_in_current_pool(&unique, links, state, report, options))
}

fn trace_modules_incremental_in_current_pool(
  unique: &[&ModuleSource],
  links: &[ModuleLink],
  state: &mut ModuleTraceState,
  mut report: TraceModulesReport,
  options: TraceModulesOptions,
) -> TraceModulesReport {
  // Borrow entries for phase-one reuse — never clone ModuleSource into a side map.
  let phase_one = {
    let entries = &state.entries;
    unique
      .par_iter()
      .map(|module| {
        analyze_module_phase_one_cached(
          module,
          entries.get(&module.id).map(|entry| (&entry.source, &entry.summary)),
        )
      })
      .collect::<Vec<Result<ModulePhaseOne, TraceModulesError>>>()
  };
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

  let mut pending_linking_archive = None;
  let work = if options.persist_linking_cache {
    if state.linking.is_none() {
      // First persistent scan: build plans once (like oneshot), archive after phase two.
      let (work, archive) = build_cold_persistent_seed_work(
        unique,
        links,
        &resolved_links,
        &facts_by_id,
        &mut local_graphs,
        &mut report,
      );
      pending_linking_archive = Some(archive);
      work
    } else {
      build_persistent_seed_work(
        unique,
        links,
        &resolved_links,
        &facts_by_id,
        &mut local_graphs,
        state,
        &mut report,
      )
    }
  } else {
    // One-shot cold path: no link sort / seed-plan archive.
    build_oneshot_seed_work(unique, &resolved_links, &facts_by_id, &mut local_graphs, &mut report)
  };

  let persist = options.persist_linking_cache;
  // Split reused vs dirty before Rayon — independent leaf edits must not
  // schedule 999 immediate-reuse workers.
  let mut reused = Vec::new();
  let mut dirty_work = Vec::new();
  for (module, local_graph, plan, summary) in work {
    if let Some(cached) = state.entries.get(&module.id)
      && cached.source == *module
      && cached.plan == plan
    {
      reused.push(cached.reactivity.clone());
      continue;
    }
    dirty_work.push((module, local_graph, plan, summary));
  }
  report.stats.reused_graphs += reused.len();
  for reactivity in reused {
    report.modules.push(reactivity);
  }

  let outcomes = dirty_work
    .into_par_iter()
    .map(|(module, mut local_graph, plan, summary)| {
      let seeded = !plan.is_empty();
      match trace_module_phase_two(module, Arc::clone(&local_graph), &plan) {
        Ok(reactivity) => PhaseTwoOutcome::Traced {
          source: persist.then(|| module.clone()),
          summary,
          plan: persist.then_some(plan),
          reactivity,
          seeded,
        },
        Err(error) => {
          Arc::make_mut(&mut local_graph).set_module_id(module.id.clone());
          PhaseTwoOutcome::Partial {
            source: persist.then(|| module.clone()),
            summary,
            plan: persist.then_some(plan),
            reactivity: ModuleReactivity { id: module.id.clone(), graph: local_graph },
            error,
          }
        }
      }
    })
    .collect::<Vec<_>>();

  let mut keep: BTreeSet<ModuleId> =
    report.modules.iter().map(|module| module.id.clone()).collect();
  for outcome in outcomes {
    match outcome {
      PhaseTwoOutcome::Traced { source, summary, plan, reactivity, seeded } => {
        report.stats.seeded_reparses += usize::from(seeded);
        keep.insert(reactivity.id.clone());
        if let (Some(source), Some(summary), Some(plan)) = (source, summary, plan) {
          state.entries.insert(
            reactivity.id.clone(),
            CachedModuleTrace { source, summary, plan, reactivity: reactivity.clone() },
          );
        }
        report.modules.push(reactivity);
      }
      PhaseTwoOutcome::Partial { source, summary, plan, reactivity, error } => {
        report.issues.push(error);
        keep.insert(reactivity.id.clone());
        if let (Some(source), Some(summary), Some(plan)) = (source, summary, plan) {
          state.entries.insert(
            reactivity.id.clone(),
            CachedModuleTrace { source, summary, plan, reactivity: reactivity.clone() },
          );
        }
        report.modules.push(reactivity);
      }
    }
  }
  if persist {
    state.entries.retain(|module_id, _| keep.contains(module_id));
    if let Some(archive) = pending_linking_archive {
      let plans = state
        .entries
        .iter()
        .map(|(id, entry)| (id.clone(), entry.plan.clone()))
        .collect::<BTreeMap<_, _>>();
      state.linking = Some(CachedLinkingSnapshot {
        links: archive.links,
        summaries: archive.summaries,
        exports: archive.exports,
        provide_index: archive.provide_index,
        plans: Arc::new(plans),
      });
    }
  }
  report.modules.sort_by(|left, right| left.id.cmp(&right.id));
  report.issues.sort_by(|left, right| {
    (left.module_id(), left.to_string()).cmp(&(right.module_id(), right.to_string()))
  });
  report
}

type SeedWorkItem<'a> =
  (&'a ModuleSource, Arc<ReactivityGraph>, ModuleSeedPlan, Option<Arc<ModuleSummary>>);

struct PendingLinkingArchive {
  links: Vec<ModuleLink>,
  summaries: BTreeMap<ModuleId, Arc<ModuleSummary>>,
  exports: Arc<BTreeMap<ModuleId, BTreeMap<String, ExportState>>>,
  provide_index: Arc<BTreeMap<super::InjectionKey, Vec<super::ProvideOffer>>>,
}

fn build_cold_persistent_seed_work<'a>(
  unique: &[&'a ModuleSource],
  links: &[ModuleLink],
  resolved_links: &BTreeMap<(ModuleId, String), ModuleId>,
  facts_by_id: &BTreeMap<ModuleId, ModuleExportFacts>,
  local_graphs: &mut BTreeMap<ModuleId, Arc<ReactivityGraph>>,
  report: &mut TraceModulesReport,
) -> (Vec<SeedWorkItem<'a>>, PendingLinkingArchive) {
  let mut owned_links = links.to_vec();
  owned_links.sort_by(|left, right| {
    (&left.from, &left.specifier, &left.to).cmp(&(&right.from, &right.specifier, &right.to))
  });
  owned_links.dedup();
  let link_index = link_index(resolved_links);
  let exports = Arc::new(resolve_exports(facts_by_id, &link_index));
  let provide_index = Arc::new(global_provide_index(facts_by_id));
  report.stats.export_resolve_ran = true;
  let work = unique
    .iter()
    .filter_map(|module| {
      let facts = facts_by_id.get(&module.id)?;
      let local_graph = local_graphs.remove(&module.id)?;
      let plan = ModuleSeedPlan {
        imports: seed_plan_for(facts, &exports, &link_index),
        injects: inject_seed_plan(facts, &provide_index),
      };
      Some((*module, local_graph, plan, Some(Arc::clone(&facts.summary))))
    })
    .collect::<Vec<_>>();
  report.stats.seed_plans_recomputed = work.len();
  let summaries =
    facts_by_id.iter().map(|(id, facts)| (id.clone(), Arc::clone(&facts.summary))).collect();
  (work, PendingLinkingArchive { links: owned_links, summaries, exports, provide_index })
}

fn build_oneshot_seed_work<'a>(
  unique: &[&'a ModuleSource],
  resolved_links: &BTreeMap<(ModuleId, String), ModuleId>,
  facts_by_id: &BTreeMap<ModuleId, ModuleExportFacts>,
  local_graphs: &mut BTreeMap<ModuleId, Arc<ReactivityGraph>>,
  report: &mut TraceModulesReport,
) -> Vec<SeedWorkItem<'a>> {
  let link_index = link_index(resolved_links);
  let exports = resolve_exports(facts_by_id, &link_index);
  let provide_index = global_provide_index(facts_by_id);
  report.stats.export_resolve_ran = true;
  let work = unique
    .iter()
    .filter_map(|module| {
      let facts = facts_by_id.get(&module.id)?;
      let local_graph = local_graphs.remove(&module.id)?;
      let plan = ModuleSeedPlan {
        imports: seed_plan_for(facts, &exports, &link_index),
        injects: inject_seed_plan(facts, &provide_index),
      };
      Some((*module, local_graph, plan, None))
    })
    .collect::<Vec<_>>();
  report.stats.seed_plans_recomputed = work.len();
  work
}

fn build_persistent_seed_work<'a>(
  unique: &[&'a ModuleSource],
  links: &[ModuleLink],
  resolved_links: &BTreeMap<(ModuleId, String), ModuleId>,
  facts_by_id: &BTreeMap<ModuleId, ModuleExportFacts>,
  local_graphs: &mut BTreeMap<ModuleId, Arc<ReactivityGraph>>,
  state: &mut ModuleTraceState,
  report: &mut TraceModulesReport,
) -> Vec<SeedWorkItem<'a>> {
  let mut owned_links = links.to_vec();
  owned_links.sort_by(|left, right| {
    (&left.from, &left.specifier, &left.to).cmp(&(&right.from, &right.specifier, &right.to))
  });
  owned_links.dedup();

  let plans = if let Some(cached) = state
    .linking
    .as_ref()
    .filter(|cached| linking_cache_reusable(&owned_links, facts_by_id, cached))
  {
    report.stats.seed_plans_recomputed = 0;
    report.stats.export_resolve_ran = false;
    Arc::clone(&cached.plans)
  } else {
    // Caller guarantees `state.linking` is already populated (warm invalidate path).
    report.stats.export_resolve_ran = true;
    let link_index = link_index(resolved_links);
    let exports = Arc::new(resolve_exports(facts_by_id, &link_index));
    let provide_index = Arc::new(global_provide_index(facts_by_id));
    let dirty_seed = modules_needing_seed_recompute(
      state.linking.as_ref(),
      &exports,
      &provide_index,
      &owned_links,
      facts_by_id,
    );
    let mut next_plans =
      state.linking.as_ref().map(|cached| (*cached.plans).clone()).unwrap_or_default();
    next_plans.retain(|id, _| facts_by_id.contains_key(id));
    let mut recomputed = 0_usize;
    for id in &dirty_seed {
      let Some(facts) = facts_by_id.get(id) else {
        continue;
      };
      next_plans.insert(
        id.clone(),
        ModuleSeedPlan {
          imports: seed_plan_for(facts, &exports, &link_index),
          injects: inject_seed_plan(facts, &provide_index),
        },
      );
      recomputed += 1;
    }
    for module in unique {
      if next_plans.contains_key(&module.id) {
        continue;
      }
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
      recomputed += 1;
    }
    report.stats.seed_plans_recomputed = recomputed;
    let plans = Arc::new(next_plans);
    let summaries =
      facts_by_id.iter().map(|(id, facts)| (id.clone(), Arc::clone(&facts.summary))).collect();
    state.linking = Some(CachedLinkingSnapshot {
      links: owned_links,
      summaries,
      exports,
      provide_index,
      plans: Arc::clone(&plans),
    });
    plans
  };

  unique
    .iter()
    .filter_map(|module| {
      let facts = facts_by_id.get(&module.id)?;
      let local_graph = local_graphs.remove(&module.id)?;
      let plan = plans.get(&module.id)?.clone();
      Some((*module, local_graph, plan, Some(Arc::clone(&facts.summary))))
    })
    .collect()
}

/// Modules whose seed plans must be refreshed after a linking-surface change.
fn modules_needing_seed_recompute(
  previous: Option<&CachedLinkingSnapshot>,
  exports: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
  provide_index: &BTreeMap<super::InjectionKey, Vec<super::ProvideOffer>>,
  links: &[ModuleLink],
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
) -> BTreeSet<ModuleId> {
  let Some(prev) = previous else {
    return facts.keys().cloned().collect();
  };
  if prev.links != links {
    return facts.keys().cloned().collect();
  }

  let mut dirty = BTreeSet::new();
  for (id, module_facts) in facts {
    match prev.summaries.get(id) {
      Some(prev_summary) if linking_surface_eq(&module_facts.summary, prev_summary) => {}
      _ => {
        dirty.insert(id.clone());
      }
    }
  }

  let mut changed_exports = BTreeSet::new();
  for (id, map) in exports {
    if prev.exports.get(id) != Some(map) {
      changed_exports.insert(id.clone());
    }
  }
  for id in prev.exports.keys() {
    if !exports.contains_key(id) {
      changed_exports.insert(id.clone());
    }
  }
  for link in links {
    if changed_exports.contains(&link.to) {
      dirty.insert(link.from.clone());
    }
  }

  if prev.provide_index.as_ref() != provide_index {
    for (id, module_facts) in facts {
      if module_facts.summary.injects.is_empty() {
        continue;
      }
      let old = inject_seed_plan(module_facts, prev.provide_index.as_ref());
      let new = inject_seed_plan(module_facts, provide_index);
      if old != new {
        dirty.insert(id.clone());
      }
    }
  }

  dirty
}

enum PhaseTwoOutcome {
  Traced {
    source: Option<ModuleSource>,
    summary: Option<Arc<ModuleSummary>>,
    plan: Option<ModuleSeedPlan>,
    reactivity: ModuleReactivity,
    seeded: bool,
  },
  Partial {
    source: Option<ModuleSource>,
    summary: Option<Arc<ModuleSummary>>,
    plan: Option<ModuleSeedPlan>,
    reactivity: ModuleReactivity,
    error: TraceModulesError,
  },
}

struct ModulePhaseOne {
  facts: ModuleExportFacts,
  local_graph: Arc<ReactivityGraph>,
}

fn analyze_module_phase_one_cached(
  module: &ModuleSource,
  cached: Option<(&ModuleSource, &Arc<ModuleSummary>)>,
) -> Result<ModulePhaseOne, TraceModulesError> {
  if let Some(summary) = &module.module_summary {
    return Ok(phase_one_from_summary(module, summary));
  }
  if let Some((source, summary)) = cached
    && source == module
  {
    return Ok(phase_one_from_summary(module, summary));
  }
  analyze_module_phase_one(module)
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
    "d.ts" | "dts" => Ok(SourceType::d_ts()),
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

  // Lazy: modules with no function/composable candidates must not pay a full
  // return-statement index walk (cold `trace_1k_*` synthetic modules).
  let mut returns_by_function = None;

  // `function useX() { return { field } }` / `return ref(0)` / `(): Ref<T>`
  for node in semantic.nodes() {
    let AstKind::Function(function) = node.kind() else {
      continue;
    };
    let Some(identifier) = &function.id else {
      continue;
    };
    let index = returns_by_function.get_or_insert_with(|| build_returns_by_function(semantic));
    let function_id = function.node_id.get();
    if let Some(state) = composable_export_state(
      semantic,
      function_id,
      shape_graph,
      script_offset,
      index,
      function_return_type_kind(function),
      || declared_return_for_function(semantic, function),
    ) {
      locals.insert(identifier.name.to_string(), state);
    }
  }

  // `const useX = () => ({ … })` / `export declare const useX: () => T`
  for node in semantic.nodes() {
    let AstKind::VariableDeclarator(declarator) = node.kind() else {
      continue;
    };
    let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
      continue;
    };
    let state = match &declarator.init {
      Some(Expression::ArrowFunctionExpression(arrow)) => {
        let index = returns_by_function.get_or_insert_with(|| build_returns_by_function(semantic));
        composable_export_state(
          semantic,
          arrow.node_id.get(),
          shape_graph,
          script_offset,
          index,
          arrow_return_type_kind(arrow),
          || {
            declared_return_for_arrow(semantic, arrow)
              .or_else(|| declared_return_from_declarator_annotation(semantic, declarator))
          },
        )
      }
      Some(Expression::FunctionExpression(function)) => {
        let index = returns_by_function.get_or_insert_with(|| build_returns_by_function(semantic));
        composable_export_state(
          semantic,
          function.node_id.get(),
          shape_graph,
          script_offset,
          index,
          function_return_type_kind(function),
          || {
            declared_return_for_function(semantic, function)
              .or_else(|| declared_return_from_declarator_annotation(semantic, declarator))
          },
        )
      }
      // `export declare const useX: () => T` — no init; only then pay for annotations.
      None => combine_composable_export(
        None,
        declared_return_from_declarator_annotation(semantic, declarator),
      ),
      // Keep the CallExpression/`ref()` cold path tiny: never build the return
      // index or declared shapes until we see a real function init.
      Some(_) => continue,
    };
    if let Some(state) = state {
      locals.insert(identifier.name.to_string(), state);
    }
  }
  locals
}

fn composable_export_state(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  shape_graph: &ReactivityGraph,
  script_offset: usize,
  returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
  declared_return_kind: Option<ReactiveBindingKind>,
  declared_return: impl FnOnce() -> Option<DeclaredReturn>,
) -> Option<ExportState> {
  match composable_return_with_index(
    semantic,
    function_id,
    shape_graph,
    script_offset,
    returns_by_function,
  ) {
    Some(ComposableReturn::Object(shape)) => Some(ExportState::Composable(shape)),
    Some(ComposableReturn::Factory(kind)) => Some(ExportState::Factory(kind)),
    Some(ComposableReturn::UnwrappedState) => match declared_return() {
      Some(DeclaredReturn::PlainObject) => {
        Some(ExportState::Factory(ReactiveBindingKind::Reactive))
      }
      _ => Some(ExportState::BodyUnwrappedState),
    },
    None => {
      if let Some(kind) = declared_return_kind {
        return Some(ExportState::Factory(kind));
      }
      combine_composable_export(None, declared_return())
    }
  }
}

fn combine_composable_export(
  body: Option<ComposableReturn>,
  declared: Option<DeclaredReturn>,
) -> Option<ExportState> {
  match (body, declared) {
    (Some(ComposableReturn::Object(shape)), _)
    | (None, Some(DeclaredReturn::Composable(shape))) => Some(ExportState::Composable(shape)),
    (Some(ComposableReturn::Factory(kind)), _) | (None, Some(DeclaredReturn::Factory(kind))) => {
      Some(ExportState::Factory(kind))
    }
    (Some(ComposableReturn::UnwrappedState), Some(DeclaredReturn::PlainObject)) => {
      Some(ExportState::Factory(ReactiveBindingKind::Reactive))
    }
    (Some(ComposableReturn::UnwrappedState), _) => Some(ExportState::BodyUnwrappedState),
    (None, Some(DeclaredReturn::PlainObject)) => Some(ExportState::DeclaredPlainObjectFactory),
    (None, None) => None,
  }
}

/// One-pass index: owning function/arrow → return statement node ids.
///
/// Built once per semantic so composable shape extraction is O(returns) total
/// instead of O(functions × nodes).
#[must_use]
pub fn build_returns_by_function(
  semantic: &oxc_semantic::Semantic<'_>,
) -> BTreeMap<NodeId, Vec<NodeId>> {
  let mut returns_by_function: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
  for (return_id, node) in semantic.nodes().iter_enumerated() {
    let AstKind::ReturnStatement(_) = node.kind() else {
      continue;
    };
    let Some(owner) = semantic.nodes().ancestor_ids(return_id).find(|ancestor_id| {
      matches!(
        semantic.nodes().kind(*ancestor_id),
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
      )
    }) else {
      continue;
    };
    returns_by_function.entry(owner).or_default().push(return_id);
  }
  returns_by_function
}

/// Object shape returned by a composable function / arrow (under-approx).
///
/// `script_offset` must match the offset used when materializing `graph.bindings`
/// spans (0 for standalone modules, Vize `loc.start` for SFC script bodies).
/// Prefer [`composable_return_shape_with_index`] when indexing many functions.
#[must_use]
pub fn composable_return_shape(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  graph: &ReactivityGraph,
  script_offset: usize,
) -> BTreeMap<String, ReactiveBindingKind> {
  let returns_by_function = build_returns_by_function(semantic);
  composable_return_shape_with_index(
    semantic,
    function_id,
    graph,
    script_offset,
    &returns_by_function,
  )
}

/// [`composable_return_shape`] using a prebuilt [`build_returns_by_function`] index.
#[must_use]
pub fn composable_return_shape_with_index(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  graph: &ReactivityGraph,
  script_offset: usize,
  returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
) -> BTreeMap<String, ReactiveBindingKind> {
  match composable_return_with_index(
    semantic,
    function_id,
    graph,
    script_offset,
    returns_by_function,
  ) {
    Some(ComposableReturn::Object(shape)) => shape,
    Some(ComposableReturn::Factory(_) | ComposableReturn::UnwrappedState) | None => BTreeMap::new(),
  }
}

#[expect(
  clippy::struct_excessive_bools,
  reason = "return-kind accumulator tracks independent under-approx signals"
)]
struct ReturnKindAccum {
  shape: BTreeMap<String, ReactiveBindingKind>,
  ambiguous: BTreeSet<String>,
  factory_kind: Option<ReactiveBindingKind>,
  factory_conflict: bool,
  saw_object_return: bool,
  saw_scalar_return: bool,
  /// `return <call>(...).value` — provisional until paired with a plain object declaration.
  saw_unwrapped_state: bool,
}

impl ReturnKindAccum {
  fn consider(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
    graph: &ReactivityGraph,
    imported_bindings: &BTreeMap<String, (String, String)>,
    param_names: &BTreeSet<String>,
    script_offset: usize,
  ) {
    let expression = match expression {
      Expression::ParenthesizedExpression(paren) => &paren.expression,
      other => other,
    };
    if matches!(expression, Expression::ObjectExpression(_)) {
      self.saw_object_return = true;
      merge_return_object_into_shape(
        semantic,
        expression,
        graph,
        imported_bindings,
        param_names,
        script_offset,
        &mut self.shape,
        &mut self.ambiguous,
      );
      return;
    }
    if is_unwrapped_call_return(semantic, expression, imported_bindings) {
      self.saw_scalar_return = true;
      self.saw_unwrapped_state = true;
      return;
    }
    self.saw_scalar_return = true;
    let Some(kind) = reactive_return_kind(
      semantic,
      expression,
      graph,
      imported_bindings,
      param_names,
      script_offset,
    ) else {
      self.factory_conflict = true;
      return;
    };
    match self.factory_kind {
      None => self.factory_kind = Some(kind),
      Some(existing) if existing == kind => {}
      Some(_) => self.factory_conflict = true,
    }
  }

  fn finish(self) -> Option<ComposableReturn> {
    if self.saw_object_return && self.saw_scalar_return {
      return None;
    }
    if self.saw_object_return && !self.shape.is_empty() {
      return Some(ComposableReturn::Object(self.shape));
    }
    if self.saw_scalar_return && !self.factory_conflict {
      if let Some(kind) = self.factory_kind {
        return Some(ComposableReturn::Factory(kind));
      }
      if self.saw_unwrapped_state {
        return Some(ComposableReturn::UnwrappedState);
      }
    }
    None
  }
}

/// Object bag or scalar factory return for a function/arrow (under-approx).
fn composable_return_with_index(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  graph: &ReactivityGraph,
  script_offset: usize,
  returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
) -> Option<ComposableReturn> {
  let imported_bindings = collect_imported_bindings(semantic);
  let param_names = function_param_names(semantic, function_id);
  let mut accum = ReturnKindAccum {
    shape: BTreeMap::new(),
    ambiguous: BTreeSet::new(),
    factory_kind: None,
    factory_conflict: false,
    saw_object_return: false,
    saw_scalar_return: false,
    saw_unwrapped_state: false,
  };

  // `() => ({ field: ref(0) })` / `() => ref(0)` expression body — no ReturnStatement.
  if let AstKind::ArrowFunctionExpression(arrow) = semantic.nodes().kind(function_id)
    && arrow.expression
    && let Some(statement) = arrow.body.statements.first()
    && let oxc_ast::ast::Statement::ExpressionStatement(expression) = statement
  {
    accum.consider(
      semantic,
      &expression.expression,
      graph,
      &imported_bindings,
      &param_names,
      script_offset,
    );
  }

  if let Some(return_ids) = returns_by_function.get(&function_id) {
    for &return_id in return_ids {
      let AstKind::ReturnStatement(statement) = semantic.nodes().kind(return_id) else {
        continue;
      };
      let Some(argument) = &statement.argument else {
        accum.factory_conflict = true;
        continue;
      };
      accum.consider(semantic, argument, graph, &imported_bindings, &param_names, script_offset);
    }
  }

  accum.finish()
}

/// Declared TypeScript return type on a function (`.d.ts` / annotated source).
#[must_use]
pub fn function_return_type_kind(
  function: &oxc_ast::ast::Function<'_>,
) -> Option<ReactiveBindingKind> {
  function
    .return_type
    .as_ref()
    .and_then(|annotation| ts_type_reactive_kind(&annotation.type_annotation))
}

/// Declared object-bag return shape on a function (`.d.ts` / annotated source).
///
/// Kept out of line so the `const x = ref(0)` module-export cold path does not
/// pay for TypeScript shape machinery in instruction cache.
#[must_use]
#[inline(never)]
pub fn function_return_type_shape(
  semantic: &oxc_semantic::Semantic<'_>,
  function: &oxc_ast::ast::Function<'_>,
) -> BTreeMap<String, ReactiveBindingKind> {
  let Some(annotation) = function.return_type.as_ref() else {
    return BTreeMap::new();
  };
  let mut index = None;
  ts_type_composable_shape(semantic, &annotation.type_annotation, 0, &mut index)
}

/// Declared TypeScript return type on an arrow function.
#[must_use]
pub fn arrow_return_type_kind(
  arrow: &oxc_ast::ast::ArrowFunctionExpression<'_>,
) -> Option<ReactiveBindingKind> {
  arrow
    .return_type
    .as_ref()
    .and_then(|annotation| ts_type_reactive_kind(&annotation.type_annotation))
}

/// Declared object-bag return shape on an arrow function.
#[must_use]
#[inline(never)]
pub fn arrow_return_type_shape(
  semantic: &oxc_semantic::Semantic<'_>,
  arrow: &oxc_ast::ast::ArrowFunctionExpression<'_>,
) -> BTreeMap<String, ReactiveBindingKind> {
  let Some(annotation) = arrow.return_type.as_ref() else {
    return BTreeMap::new();
  };
  let mut index = None;
  ts_type_composable_shape(semantic, &annotation.type_annotation, 0, &mut index)
}

/// Scalar factory kind from return expressions (`return ref(0)`), when consistent.
#[must_use]
pub fn composable_factory_kind_with_index(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  graph: &ReactivityGraph,
  script_offset: usize,
  returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
) -> Option<ReactiveBindingKind> {
  match composable_return_with_index(
    semantic,
    function_id,
    graph,
    script_offset,
    returns_by_function,
  ) {
    Some(ComposableReturn::Factory(kind)) => Some(kind),
    Some(ComposableReturn::Object(_) | ComposableReturn::UnwrappedState) | None => None,
  }
}

/// `return <call>(...).value` where callee is unresolved or imported from `#imports`.
///
/// Name-agnostic: pairs with a declared plain-object return to yield `Factory(Reactive)`.
fn is_unwrapped_call_return(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> bool {
  let Expression::StaticMemberExpression(member) = expression else {
    return false;
  };
  if member.property.name.as_str() != "value" {
    return false;
  }
  let Expression::CallExpression(call) = &member.object else {
    return false;
  };
  let Some(callee) = call.callee.get_identifier_reference() else {
    return false;
  };
  if let Some((source, _)) = imported_bindings.get(callee.name.as_str()) {
    return source == "#imports";
  }
  let Some(reference_id) = callee.reference_id.get() else {
    return false;
  };
  semantic.scoping().get_reference(reference_id).symbol_id().is_none()
}

fn declared_return_for_function(
  semantic: &oxc_semantic::Semantic<'_>,
  function: &oxc_ast::ast::Function<'_>,
) -> Option<DeclaredReturn> {
  if let Some(kind) = function_return_type_kind(function) {
    return Some(DeclaredReturn::Factory(kind));
  }
  let annotation = function.return_type.as_ref()?;
  classify_declared_return_type(semantic, &annotation.type_annotation)
}

fn declared_return_for_arrow(
  semantic: &oxc_semantic::Semantic<'_>,
  arrow: &oxc_ast::ast::ArrowFunctionExpression<'_>,
) -> Option<DeclaredReturn> {
  if let Some(kind) = arrow_return_type_kind(arrow) {
    return Some(DeclaredReturn::Factory(kind));
  }
  let annotation = arrow.return_type.as_ref()?;
  classify_declared_return_type(semantic, &annotation.type_annotation)
}

/// `export declare const useX: () => T` — function type on the declarator.
#[inline(never)]
fn declared_return_from_declarator_annotation(
  semantic: &oxc_semantic::Semantic<'_>,
  declarator: &oxc_ast::ast::VariableDeclarator<'_>,
) -> Option<DeclaredReturn> {
  use oxc_ast::ast::TSType;
  let annotation = declarator.type_annotation.as_ref()?;
  let ts_type = match &annotation.type_annotation {
    TSType::TSParenthesizedType(paren) => &paren.type_annotation,
    other => other,
  };
  let TSType::TSFunctionType(function_type) = ts_type else {
    return None;
  };
  classify_declared_return_type(semantic, &function_type.return_type.type_annotation)
}

#[inline(never)]
fn classify_declared_return_type(
  semantic: &oxc_semantic::Semantic<'_>,
  ts_type: &oxc_ast::ast::TSType<'_>,
) -> Option<DeclaredReturn> {
  if let Some(kind) = ts_type_reactive_kind(ts_type) {
    return Some(DeclaredReturn::Factory(kind));
  }
  let mut index = None;
  let shape = ts_type_composable_shape(semantic, ts_type, 0, &mut index);
  if !shape.is_empty() {
    return Some(DeclaredReturn::Composable(shape));
  }
  if ts_type_is_plain_object_shaped(semantic, ts_type, 0, &mut index) {
    return Some(DeclaredReturn::PlainObject);
  }
  None
}

/// Object-shaped type: ≥1 property and no Ref-like field types (under-approx).
#[inline(never)]
fn ts_type_is_plain_object_shaped<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  ts_type: &'a oxc_ast::ast::TSType<'a>,
  depth: u8,
  index: &mut Option<TypeDeclIndex<'a>>,
) -> bool {
  use oxc_ast::ast::{TSType, TSTypeName, TSTypeOperatorOperator};
  if depth > 4 {
    return false;
  }
  if ts_type_reactive_kind(ts_type).is_some() {
    return false;
  }
  match ts_type {
    TSType::TSParenthesizedType(paren) => {
      ts_type_is_plain_object_shaped(semantic, &paren.type_annotation, depth, index)
    }
    TSType::TSTypeOperatorType(operator)
      if operator.operator == TSTypeOperatorOperator::Readonly =>
    {
      ts_type_is_plain_object_shaped(semantic, &operator.type_annotation, depth, index)
    }
    TSType::TSTypeLiteral(literal) => signatures_are_plain_object_shaped(&literal.members),
    TSType::TSTypeReference(reference) => {
      let Some(name) = (match &reference.type_name {
        TSTypeName::IdentifierReference(identifier) => Some(identifier.name.as_str()),
        TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => None,
      }) else {
        return false;
      };
      let alias = {
        let decls = index.get_or_insert_with(|| TypeDeclIndex::build(semantic));
        if let Some(members) = decls.interfaces.get(name).copied() {
          return signatures_are_plain_object_shaped(members);
        }
        decls.aliases.get(name).copied()
      };
      let Some(alias) = alias else {
        return false;
      };
      ts_type_is_plain_object_shaped(semantic, alias, depth.saturating_add(1), index)
    }
    _ => false,
  }
}

fn signatures_are_plain_object_shaped(members: &[oxc_ast::ast::TSSignature<'_>]) -> bool {
  use oxc_ast::ast::TSSignature;
  let mut property_count = 0_usize;
  for member in members {
    let TSSignature::TSPropertySignature(property) = member else {
      continue;
    };
    property_count = property_count.saturating_add(1);
    let Some(annotation) = &property.type_annotation else {
      continue;
    };
    if ts_type_reactive_kind(&annotation.type_annotation).is_some() {
      return false;
    }
  }
  property_count > 0
}

/// Map a TypeScript return type surface to a reactive binding kind (under-approx).
///
/// Only recognizes Vue ref-like type names (`Ref`, `ComputedRef`, …). Full checker
/// inference and utility wrappers stay quiet.
fn ts_type_reactive_kind(ts_type: &oxc_ast::ast::TSType<'_>) -> Option<ReactiveBindingKind> {
  use oxc_ast::ast::{TSType, TSTypeName, TSTypeOperatorOperator};
  match ts_type {
    TSType::TSParenthesizedType(paren) => ts_type_reactive_kind(&paren.type_annotation),
    TSType::TSTypeOperatorType(operator)
      if operator.operator == TSTypeOperatorOperator::Readonly =>
    {
      ts_type_reactive_kind(&operator.type_annotation).map(|kind| match kind {
        ReactiveBindingKind::Ref => ReactiveBindingKind::Readonly,
        ReactiveBindingKind::ShallowRef => ReactiveBindingKind::ShallowReadonly,
        other => other,
      })
    }
    TSType::TSTypeReference(reference) => {
      let name = match &reference.type_name {
        TSTypeName::IdentifierReference(identifier) => identifier.name.as_str(),
        // `vue.Ref` / `import('vue').ShallowRef` rightmost name (qualified only).
        TSTypeName::QualifiedName(qualified) => qualified.right.name.as_str(),
        TSTypeName::ThisExpression(_) => return None,
      };
      match name {
        "Ref" => Some(ReactiveBindingKind::Ref),
        "ShallowRef" => Some(ReactiveBindingKind::ShallowRef),
        "ComputedRef" | "WritableComputedRef" => Some(ReactiveBindingKind::Computed),
        "CustomRef" => Some(ReactiveBindingKind::CustomRef),
        "ToRef" => Some(ReactiveBindingKind::ToRef),
        "Readonly" => {
          // `Readonly<Ref<T>>` — peel one type argument when present.
          let arg = reference.type_arguments.as_ref()?.params.first()?;
          ts_type_reactive_kind(arg).map(|kind| match kind {
            ReactiveBindingKind::Ref => ReactiveBindingKind::Readonly,
            ReactiveBindingKind::ShallowRef => ReactiveBindingKind::ShallowReadonly,
            other => other,
          })
        }
        _ => None,
      }
    }
    _ => None,
  }
}

/// Same-file `interface` / `type` declarations, built once per shape query.
struct TypeDeclIndex<'a> {
  interfaces: BTreeMap<&'a str, &'a [oxc_ast::ast::TSSignature<'a>]>,
  aliases: BTreeMap<&'a str, &'a oxc_ast::ast::TSType<'a>>,
}

impl<'a> TypeDeclIndex<'a> {
  fn build(semantic: &'a oxc_semantic::Semantic<'a>) -> Self {
    let mut interfaces = BTreeMap::new();
    let mut aliases = BTreeMap::new();
    for node in semantic.nodes() {
      match node.kind() {
        AstKind::TSInterfaceDeclaration(interface) => {
          interfaces.insert(interface.id.name.as_str(), interface.body.body.as_slice());
        }
        AstKind::TSTypeAliasDeclaration(alias) => {
          aliases.insert(alias.id.name.as_str(), &alias.type_annotation);
        }
        _ => {}
      }
    }
    Self { interfaces, aliases }
  }
}

/// Object-bag shape from a TypeScript return type (under-approx).
///
/// Recognizes inline `{ width: Ref<number> }`, same-file `interface` / `type`
/// aliases, and peels a single `readonly` operator. Non-reactive fields
/// (`stop: () => void`) stay out of the shape. Depth-bounded alias follow.
fn ts_type_composable_shape<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  ts_type: &'a oxc_ast::ast::TSType<'a>,
  depth: u8,
  index: &mut Option<TypeDeclIndex<'a>>,
) -> BTreeMap<String, ReactiveBindingKind> {
  use oxc_ast::ast::{TSType, TSTypeName, TSTypeOperatorOperator};
  if depth > 4 {
    return BTreeMap::new();
  }
  // Scalar Ref returns are Factory, not bags.
  if ts_type_reactive_kind(ts_type).is_some() {
    return BTreeMap::new();
  }
  match ts_type {
    TSType::TSParenthesizedType(paren) => {
      ts_type_composable_shape(semantic, &paren.type_annotation, depth, index)
    }
    TSType::TSTypeOperatorType(operator)
      if operator.operator == TSTypeOperatorOperator::Readonly =>
    {
      ts_type_composable_shape(semantic, &operator.type_annotation, depth, index)
    }
    TSType::TSTypeLiteral(literal) => shape_from_ts_signatures(&literal.members),
    TSType::TSTypeReference(reference) => {
      let Some(name) = (match &reference.type_name {
        TSTypeName::IdentifierReference(identifier) => Some(identifier.name.as_str()),
        TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => None,
      }) else {
        return BTreeMap::new();
      };
      // Resolve through a one-shot index; drop borrows before recursing into aliases.
      let alias = {
        let decls = index.get_or_insert_with(|| TypeDeclIndex::build(semantic));
        if let Some(members) = decls.interfaces.get(name).copied() {
          return shape_from_ts_signatures(members);
        }
        decls.aliases.get(name).copied()
      };
      let Some(alias) = alias else {
        return BTreeMap::new();
      };
      ts_type_composable_shape(semantic, alias, depth.saturating_add(1), index)
    }
    _ => BTreeMap::new(),
  }
}

fn shape_from_ts_signatures(
  members: &[oxc_ast::ast::TSSignature<'_>],
) -> BTreeMap<String, ReactiveBindingKind> {
  use oxc_ast::ast::TSSignature;
  let mut shape = BTreeMap::new();
  for member in members {
    let TSSignature::TSPropertySignature(property) = member else {
      continue;
    };
    let Some(exported) = property.key.static_name() else {
      continue;
    };
    let Some(annotation) = &property.type_annotation else {
      continue;
    };
    let Some(kind) = ts_type_reactive_kind(&annotation.type_annotation) else {
      continue;
    };
    shape.insert(exported.into_owned(), kind);
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
  // Provisional declaration/body halves never cross the seed barrier alone.
  if matches!(state, ExportState::DeclaredPlainObjectFactory | ExportState::BodyUnwrappedState) {
    return false;
  }
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

const fn is_seedable_export_state(state: &ExportState) -> bool {
  matches!(state, ExportState::Known(_) | ExportState::Factory(_) | ExportState::Composable(_))
}

/// Coordinator-side: which of this module's import locals resolve to reactive exports.
fn seed_plan_for(
  facts: &ModuleExportFacts,
  exports: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
) -> ImportSeedPlan {
  use std::ops::Bound;

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
    if !is_seedable_export_state(state) {
      continue;
    }
    // Only the resolved export state crosses the barrier (not source text / graphs).
    plan.insert(import.local.clone(), state.clone());
  }
  // Bare Nuxt auto-imports: `#nuxt-imports:{name}` — range-scan only this module's keys
  // (full-map filter would be O(modules × links) on long re-export chains).
  for ((_from, specifier), target) in links.range((
    Bound::Included((&facts.id, NUXT_IMPORTS_SPECIFIER_PREFIX)),
    Bound::Excluded((&facts.id, NUXT_IMPORTS_RANGE_END)),
  )) {
    let Some(name) = specifier.strip_prefix(NUXT_IMPORTS_SPECIFIER_PREFIX) else {
      continue;
    };
    if name.is_empty() || plan.contains_key(name) {
      continue;
    }
    let Some(state) = exports.get(*target).and_then(|module_exports| module_exports.get(name))
    else {
      continue;
    };
    if !is_seedable_export_state(state) {
      continue;
    }
    plan.insert(name.to_owned(), state.clone());
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
  let bare_instance_calls = collect_bare_instance_calls(semantic, &plan.imports);
  let bare_destructured_calls = collect_bare_destructured_calls(semantic, &plan.imports);
  let span_source = module.span_origin();
  let span_base = module.source_offset;
  let mut seeds = TraceSeeds::default();
  for (local, state) in &plan.imports {
    match state {
      ExportState::Known(kind) => {
        // Known seeds still require an import binding span.
        let Some(import) = imports.iter().find(|import| import.local == *local) else {
          continue;
        };
        seeds.bindings.push(ReactiveBindingFact {
          name: local.clone(),
          kind: *kind,
          initialized_with_null: false,
          span: source_span(span_source, span_base, import.span),
        });
      }
      ExportState::Factory(kind) => {
        let imported_calls = instance_calls.iter().filter(|call| call.imported_local == *local);
        let bare_calls = bare_instance_calls.iter().filter(|call| call.imported_local == *local);
        for call in imported_calls.chain(bare_calls) {
          if seeds.bindings.iter().any(|binding| binding.name == call.local) {
            continue;
          }
          seeds.bindings.push(ReactiveBindingFact {
            name: call.local.clone(),
            kind: *kind,
            initialized_with_null: false,
            span: source_span(span_source, span_base, call.span),
          });
        }
      }
      ExportState::Composable(shape) => {
        let imported_destructure =
          destructured_calls.iter().filter(|call| call.imported_local == *local);
        let bare_destructure =
          bare_destructured_calls.iter().filter(|call| call.imported_local == *local);
        for call in imported_destructure.chain(bare_destructure) {
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
        let imported_instances = instance_calls.iter().filter(|call| call.imported_local == *local);
        let bare_instances =
          bare_instance_calls.iter().filter(|call| call.imported_local == *local);
        for call in imported_instances.chain(bare_instances) {
          seeds.composable_instances.insert(call.local.clone(), shape.clone());
        }
      }
      ExportState::DeclaredPlainObjectFactory
      | ExportState::BodyUnwrappedState
      | ExportState::Ambiguous => {}
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

/// `const x = useX()` where `useX` is unresolved and present in the seed plan (bare auto-import).
fn collect_bare_instance_calls(
  semantic: &oxc_semantic::Semantic<'_>,
  plan: &ImportSeedPlan,
) -> Vec<InstanceCallBinding> {
  let mut calls = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = call.callee.get_identifier_reference() else {
      continue;
    };
    if !plan.contains_key(callee.name.as_str()) {
      continue;
    }
    let Some(reference_id) = callee.reference_id.get() else {
      continue;
    };
    if semantic.scoping().get_reference(reference_id).symbol_id().is_some() {
      continue;
    }
    let AstKind::VariableDeclarator(declarator) = semantic.nodes().parent_kind(call.node_id.get())
    else {
      continue;
    };
    let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
      continue;
    };
    calls.push(InstanceCallBinding {
      imported_local: callee.name.to_string(),
      local: identifier.name.to_string(),
      span: identifier.span,
    });
  }
  calls.sort_by_key(|call| call.span.start);
  calls
}

/// `const { field } = useX()` for bare unresolved auto-import callees in the seed plan.
fn collect_bare_destructured_calls(
  semantic: &oxc_semantic::Semantic<'_>,
  plan: &ImportSeedPlan,
) -> Vec<DestructuredCallBinding> {
  let mut calls = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = call.callee.get_identifier_reference() else {
      continue;
    };
    if !plan.contains_key(callee.name.as_str()) {
      continue;
    }
    let Some(reference_id) = callee.reference_id.get() else {
      continue;
    };
    if semantic.scoping().get_reference(reference_id).symbol_id().is_some() {
      continue;
    }
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
          imported_local: callee.name.to_string(),
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

fn join_errors(errors: &[impl ToString]) -> String {
  errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
}
