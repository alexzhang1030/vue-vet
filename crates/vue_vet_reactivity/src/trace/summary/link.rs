use std::{
  collections::{BTreeMap, BTreeSet, btree_map::Entry},
  sync::Arc,
};

use oxc_allocator::Allocator;
use oxc_ast::{
  AstKind,
  ast::{BindingPattern, Expression},
};
use oxc_parser::Parser;
use oxc_semantic::{Semantic, SemanticBuilder};
use oxc_span::Span;
use rayon::prelude::*;
use vue_vet_core::{ModuleId, ReactiveBindingFact, ReactivityGraph};

use super::super::kinds::{collect_binding_identifiers, collect_imported_bindings, source_span};
use super::super::{
  ProvideOffer, TraceSeeds, collect_inject_sites, provide_offer_index, resolve_inject_offer,
  trace_reactivity_seeded,
};
use super::{
  DestructuredCallBinding, ExportState, ExportSummary, ImportSummary, InstanceCallBinding,
  ModuleExportFacts, ModuleLink, ModulePhaseOne, ModuleReactivity, ModuleSource, ModuleSummary,
  NUXT_IMPORTS_RANGE_END, NUXT_IMPORTS_SPECIFIER_PREFIX, OptionsCallbackSlots, TraceModulesError,
  TypedCallbackParamSlots, ValueBag, ValueBagEntry, analyze_module_phase_one_cached,
  collect_imports, export_lattice, join_errors, phase_one_from_summary, source_type,
};

/// Concurrency limit for cross-module tracing.
#[derive(Clone, Debug)]
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
  /// Plugin-supplied named API bag contracts (Nuxt / vue-i18n / …). Empty by
  /// default — the analysis boundary installs [`vue_vet_plugins`] defaults.
  pub named_api_bags: Vec<crate::NamedApiBag>,
  /// When `Some`, `modules` may be a dirty subset. Cached entries whose ids
  /// are in this set stay in linking. Entries not in this set are dropped.
  /// `None` means `modules` is the universe unless
  /// [`Self::retain_cached_modules`] is set.
  ///
  /// Prefer [`Self::retain_cached_modules`] + [`Self::drop_module_ids`] so a
  /// warm scan does not allocate a cloned live-id set. Explicit ids win when
  /// both are set. Subset mode requires [`Self::persist_linking_cache`]. The
  /// linker still computes the seed-dirty set — do not invent a second
  /// export-closure.
  pub live_module_ids: Option<BTreeSet<ModuleId>>,
  /// When set with [`Self::persist_linking_cache`], the live universe is
  /// `(state.entries ∪ input) − drop_module_ids`. The report lists this pass
  /// only; callers read unchanged graphs from [`ModuleTraceState`].
  pub retain_cached_modules: bool,
  /// Deleted module ids. Ignored unless [`Self::retain_cached_modules`] is set
  /// and [`Self::live_module_ids`] is `None`.
  pub drop_module_ids: BTreeSet<ModuleId>,
}

impl Default for TraceModulesOptions {
  fn default() -> Self {
    Self {
      max_workers: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
      reuse_current_pool: false,
      persist_linking_cache: true,
      named_api_bags: Vec::new(),
      live_module_ids: None,
      retain_cached_modules: false,
      drop_module_ids: BTreeSet::new(),
    }
  }
}

/// Per-import resolution for one consumer module (`import.local` → export state).
/// Spans are applied on the worker that still holds the parse.
type ImportSeedPlan = BTreeMap<String, ExportState>;

/// Cross-module seeds delivered after the barrier (imports + unique inject keys).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ModuleSeedPlan {
  imports: ImportSeedPlan,
  /// inject local → offer (scalar kind and/or composable bag shape).
  injects: BTreeMap<String, ProvideOffer>,
  /// Import / bare auto-import local → options-object callback bag shapes.
  options_callback_slots: BTreeMap<String, OptionsCallbackSlots>,
  /// Import / bare auto-import local → typed function-callback Ref formals.
  typed_callback_param_slots: BTreeMap<String, TypedCallbackParamSlots>,
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

impl ModuleTraceState {
  /// Cached source for `id` when a prior incremental scan retained it.
  ///
  /// Callers assembling a workspace module list should reuse this handle when
  /// it still equals the freshly prepared [`ModuleSource`] (script body +
  /// offset). That avoids reconstructing source text on an independent leaf
  /// edit.
  #[must_use]
  pub fn cached_source(&self, id: &ModuleId) -> Option<&ModuleSource> {
    self.entries.get(id).map(|entry| entry.source.as_ref())
  }

  /// Cached final graph for `id` when a prior incremental scan retained it.
  #[must_use]
  pub fn cached_reactivity(&self, id: &ModuleId) -> Option<&ModuleReactivity> {
    self.entries.get(id).map(|entry| &entry.reactivity)
  }

  /// Cached module ids in deterministic map order.
  pub fn cached_module_ids(&self) -> impl Iterator<Item = &ModuleId> {
    self.entries.keys()
  }

  /// Cached `(id, graph)` pairs in deterministic map order.
  pub fn iter_cached_reactivity(&self) -> impl Iterator<Item = (&ModuleId, &ModuleReactivity)> {
    self.entries.iter().map(|(id, entry)| (id, &entry.reactivity))
  }

  /// Whether a prior persistent scan retained at least one module.
  #[must_use]
  pub fn has_cached_modules(&self) -> bool {
    !self.entries.is_empty()
  }
}

#[derive(Clone, Debug)]
struct CachedModuleTrace {
  source: Arc<ModuleSource>,
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
  provide_index: Arc<BTreeMap<super::super::InjectionKey, Vec<super::super::ProvideOffer>>>,
  plans: Arc<BTreeMap<ModuleId, ModuleSeedPlan>>,
}

/// Linking-relevant fields only — never clones; prefers [`Arc::ptr_eq`].
fn linking_surface_eq(left: &Arc<ModuleSummary>, right: &Arc<ModuleSummary>) -> bool {
  Arc::ptr_eq(left, right)
    || (left.imports == right.imports
      && left.exports == right.exports
      && left.locals == right.locals
      && left.options_callback_slots == right.options_callback_slots
      && left.typed_callback_param_slots == right.typed_callback_param_slots
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
  /// Module ids whose seed plans were freshly computed this pass.
  /// Empty when the linking cache reused every plan.
  pub seed_plan_dirty: BTreeSet<ModuleId>,
}

impl ModuleSeedPlan {
  fn is_empty(&self) -> bool {
    self.imports.is_empty()
      && self.injects.is_empty()
      && self.options_callback_slots.is_empty()
      && self.typed_callback_param_slots.is_empty()
  }
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
  let report = trace_modules_incremental_with_options(modules, links, &options, &mut state);
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
  options: &TraceModulesOptions,
  state: &mut ModuleTraceState,
) -> TraceModulesReport {
  let unique = modules.iter().collect::<Vec<_>>();
  trace_modules_incremental_from_refs(&unique, links, options, state)
}

/// Same as [`trace_modules_incremental_with_options`] with borrowed sources.
///
/// Warm callers pass only the source-dirty slice and set
/// [`TraceModulesOptions::retain_cached_modules`] so unchanged modules stay
/// in `state` without a `ModuleSource` clone.
#[must_use]
pub fn trace_modules_incremental_from_refs(
  modules: &[&ModuleSource],
  links: &[ModuleLink],
  options: &TraceModulesOptions,
  state: &mut ModuleTraceState,
) -> TraceModulesReport {
  let mut report = TraceModulesReport::default();
  let mut seen = BTreeSet::new();
  let unique = modules
    .iter()
    .copied()
    .filter(|module| {
      if seen.insert(&module.id) {
        true
      } else {
        report.issues.push(TraceModulesError::DuplicateModule(module.id.clone()));
        false
      }
    })
    .collect::<Vec<_>>();
  if unique.is_empty() && !subset_cache_emit(options, state) {
    state.entries.clear();
    state.linking = None;
    return report;
  }

  if unique.is_empty() || options.reuse_current_pool {
    return trace_modules_incremental_in_current_pool(&unique, links, state, report, options);
  }

  let Ok(pool) = rayon::ThreadPoolBuilder::new()
    .num_threads(options.max_workers.max(1).min(unique.len().max(1)))
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
  options: &TraceModulesOptions,
) -> TraceModulesReport {
  let scope = live_scope(options);
  if unique.is_empty() && options.persist_linking_cache && state.linking.is_none() {
    report.stats.reused_graphs += count_cached_silent(scope, unique, &report, state);
    retain_live_entries(scope, unique, &BTreeSet::new(), state);
    return report;
  }
  // Attached / cached summaries stay sequential. Rayon only the modules that
  // still need a parse — independent leaf edits must not schedule N workers.
  let phase_one = phase_one_outcomes(unique, state, &options.named_api_bags);
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

  if persist_subset(options) {
    merge_cached_live_modules(scope, state, &mut facts_by_id, &mut local_graphs);
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
      report.seed_plan_dirty =
        work.iter().map(|(module, _, _, _)| module.source().id.clone()).collect();
      work
    } else {
      let mut work = build_persistent_seed_work(
        unique,
        links,
        &resolved_links,
        &facts_by_id,
        &mut local_graphs,
        state,
        &mut report,
      );
      if persist_subset(options)
        && let Some(plans) = state.linking.as_ref().map(|cached| Arc::clone(&cached.plans))
      {
        work.extend(pull_cached_seed_work(
          unique,
          scope,
          &report.seed_plan_dirty,
          state,
          &facts_by_id,
          &mut local_graphs,
          &plans,
        ));
      }
      work
    }
  } else {
    // One-shot cold path: no link sort / seed-plan archive.
    let work = build_oneshot_seed_work(
      unique,
      &resolved_links,
      &facts_by_id,
      &mut local_graphs,
      &mut report,
    );
    report.seed_plan_dirty =
      work.iter().map(|(module, _, _, _)| module.source().id.clone()).collect();
    work
  };

  let persist = options.persist_linking_cache;
  let subset = persist_subset(options);
  // Split reused vs dirty before Rayon — independent leaf edits must not
  // schedule 999 immediate-reuse workers. Subset reports are this-pass
  // traces only; unchanged graphs stay in `state`.
  let mut reused = Vec::new();
  let mut dirty_work = Vec::new();
  for (module, local_graph, plan, summary) in work {
    let source = module.source();
    if let Some(cached) = state.entries.get(&source.id)
      && cached.source.as_ref() == source
      && cached.plan == plan
    {
      report.stats.reused_graphs += 1;
      if !subset {
        reused.push(cached.reactivity.clone());
      }
      continue;
    }
    dirty_work.push((module, local_graph, plan, summary));
  }
  for reactivity in reused {
    report.modules.push(reactivity);
  }

  let outcomes = dirty_work
    .into_par_iter()
    .map(|(module, local_graph, plan, summary)| {
      trace_dirty_module(module, local_graph, plan, summary, persist, &options.named_api_bags)
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
    if subset {
      report.stats.reused_graphs += count_cached_silent(scope, unique, &report, state);
      retain_live_entries(scope, unique, &keep, state);
    } else {
      state.entries.retain(|module_id, _| keep.contains(module_id));
    }
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

fn phase_one_outcomes(
  unique: &[&ModuleSource],
  state: &ModuleTraceState,
  named_api_bags: &[crate::NamedApiBag],
) -> Vec<Result<ModulePhaseOne, TraceModulesError>> {
  let config = crate::TraceConfig { named_api_bags };
  let need_parse = unique
    .iter()
    .copied()
    .filter(|module| {
      module.module_summary().is_none()
        && !state.entries.get(&module.id).is_some_and(|entry| entry.source.as_ref() == *module)
    })
    .collect::<Vec<_>>();
  let mut parsed = need_parse
    .par_iter()
    .map(|module| {
      (
        module.id.clone(),
        analyze_module_phase_one_cached(
          module,
          state.entries.get(&module.id).map(|entry| (entry.source.as_ref(), &entry.summary)),
          &config,
        ),
      )
    })
    .collect::<BTreeMap<_, _>>();
  unique
    .iter()
    .map(|module| {
      reused_phase_one(module, state).map_or_else(
        || parsed.remove(&module.id).unwrap_or(Err(TraceModulesError::WorkerDisconnected)),
        Ok,
      )
    })
    .collect()
}

fn reused_phase_one(module: &ModuleSource, state: &ModuleTraceState) -> Option<ModulePhaseOne> {
  if let Some(summary) = module.module_summary() {
    return Some(phase_one_from_summary(module, &summary));
  }
  let entry = state.entries.get(&module.id)?;
  (entry.source.as_ref() == module).then(|| phase_one_from_summary(module, &entry.summary))
}

/// Phase-two work source. Input modules are borrowed; seed-dirty modules
/// missing from this pass share the cached `Arc` (no `ModuleSource` clone).
enum WorkSource<'a> {
  Input(&'a ModuleSource),
  Cached(Arc<ModuleSource>),
}

impl WorkSource<'_> {
  fn source(&self) -> &ModuleSource {
    match self {
      Self::Input(module) => module,
      Self::Cached(module) => module,
    }
  }

  fn into_persist_source(self, persist: bool) -> Option<Arc<ModuleSource>> {
    if !persist {
      return None;
    }
    match self {
      Self::Input(module) => Some(Arc::new(module.clone())),
      Self::Cached(module) => Some(module),
    }
  }
}

type SeedWorkItem<'a> =
  (WorkSource<'a>, Arc<ReactivityGraph>, ModuleSeedPlan, Option<Arc<ModuleSummary>>);

#[derive(Clone, Copy)]
enum LiveScope<'a> {
  InputOnly,
  Explicit(&'a BTreeSet<ModuleId>),
  Retain { drop: &'a BTreeSet<ModuleId> },
}

const fn live_scope(options: &TraceModulesOptions) -> LiveScope<'_> {
  if let Some(live) = options.live_module_ids.as_ref() {
    return LiveScope::Explicit(live);
  }
  if options.retain_cached_modules {
    return LiveScope::Retain { drop: &options.drop_module_ids };
  }
  LiveScope::InputOnly
}

const fn persist_subset(options: &TraceModulesOptions) -> bool {
  options.persist_linking_cache && !matches!(live_scope(options), LiveScope::InputOnly)
}

fn subset_cache_emit(options: &TraceModulesOptions, state: &ModuleTraceState) -> bool {
  persist_subset(options) && state.has_cached_modules()
}

fn merge_cached(
  id: &ModuleId,
  entry: &CachedModuleTrace,
  facts_by_id: &mut BTreeMap<ModuleId, ModuleExportFacts>,
  local_graphs: &mut BTreeMap<ModuleId, Arc<ReactivityGraph>>,
) {
  if facts_by_id.contains_key(id) {
    return;
  }
  let analysis = phase_one_from_summary(entry.source.as_ref(), &entry.summary);
  facts_by_id.insert(id.clone(), analysis.facts);
  local_graphs.insert(id.clone(), analysis.local_graph);
}

fn merge_cached_live_modules(
  scope: LiveScope<'_>,
  state: &ModuleTraceState,
  facts_by_id: &mut BTreeMap<ModuleId, ModuleExportFacts>,
  local_graphs: &mut BTreeMap<ModuleId, Arc<ReactivityGraph>>,
) {
  match scope {
    LiveScope::InputOnly => {}
    LiveScope::Explicit(live) => {
      for id in live {
        let Some(entry) = state.entries.get(id) else {
          continue;
        };
        merge_cached(id, entry, facts_by_id, local_graphs);
      }
    }
    LiveScope::Retain { drop } => {
      for (id, entry) in &state.entries {
        if drop.contains(id) {
          continue;
        }
        merge_cached(id, entry, facts_by_id, local_graphs);
      }
    }
  }
}

fn count_cached_silent(
  scope: LiveScope<'_>,
  unique: &[&ModuleSource],
  report: &TraceModulesReport,
  state: &ModuleTraceState,
) -> usize {
  let input_ids: BTreeSet<&ModuleId> = unique.iter().map(|module| &module.id).collect();
  let present: BTreeSet<&ModuleId> = report.modules.iter().map(|module| &module.id).collect();
  match scope {
    LiveScope::InputOnly => 0,
    LiveScope::Explicit(live) => live
      .iter()
      .filter(|id| {
        !input_ids.contains(id) && !present.contains(id) && state.entries.contains_key(*id)
      })
      .count(),
    LiveScope::Retain { drop } => state
      .entries
      .keys()
      .filter(|id| !drop.contains(*id) && !input_ids.contains(*id) && !present.contains(*id))
      .count(),
  }
}

fn retain_live_entries(
  scope: LiveScope<'_>,
  unique: &[&ModuleSource],
  keep: &BTreeSet<ModuleId>,
  state: &mut ModuleTraceState,
) {
  let input_ids: BTreeSet<&ModuleId> = unique.iter().map(|module| &module.id).collect();
  state.entries.retain(|module_id, _| match scope {
    LiveScope::InputOnly => keep.contains(module_id),
    LiveScope::Explicit(live) => {
      live.contains(module_id) && (!input_ids.contains(module_id) || keep.contains(module_id))
    }
    LiveScope::Retain { drop } => {
      !drop.contains(module_id) && (!input_ids.contains(module_id) || keep.contains(module_id))
    }
  });
}

fn pull_cached_seed_work<'a>(
  unique: &[&ModuleSource],
  scope: LiveScope<'_>,
  dirty_seed: &BTreeSet<ModuleId>,
  state: &ModuleTraceState,
  facts_by_id: &BTreeMap<ModuleId, ModuleExportFacts>,
  local_graphs: &mut BTreeMap<ModuleId, Arc<ReactivityGraph>>,
  plans: &BTreeMap<ModuleId, ModuleSeedPlan>,
) -> Vec<SeedWorkItem<'a>> {
  let input_ids: BTreeSet<&ModuleId> = unique.iter().map(|module| &module.id).collect();
  dirty_seed
    .iter()
    .filter_map(|id| {
      if input_ids.contains(id) || !id_is_live(scope, id, state) {
        return None;
      }
      let source = state.entries.get(id).map(|entry| Arc::clone(&entry.source))?;
      let facts = facts_by_id.get(id)?;
      let local_graph = local_graphs.remove(id)?;
      let plan = plans.get(id)?.clone();
      Some((WorkSource::Cached(source), local_graph, plan, Some(Arc::clone(&facts.summary))))
    })
    .collect()
}

fn id_is_live(scope: LiveScope<'_>, id: &ModuleId, state: &ModuleTraceState) -> bool {
  match scope {
    LiveScope::InputOnly => false,
    LiveScope::Explicit(live) => live.contains(id),
    LiveScope::Retain { drop } => !drop.contains(id) && state.entries.contains_key(id),
  }
}

struct PendingLinkingArchive {
  links: Vec<ModuleLink>,
  summaries: BTreeMap<ModuleId, Arc<ModuleSummary>>,
  exports: Arc<BTreeMap<ModuleId, BTreeMap<String, ExportState>>>,
  provide_index: Arc<BTreeMap<super::super::InjectionKey, Vec<super::super::ProvideOffer>>>,
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
  let options_exports = resolve_options_callback_exports(facts_by_id, &link_index);
  let typed_callback_exports = resolve_typed_callback_param_exports(facts_by_id, &link_index);
  let provide_index = Arc::new(global_provide_index(facts_by_id));
  report.stats.export_resolve_ran = true;
  let work = unique
    .iter()
    .filter_map(|module| {
      let facts = facts_by_id.get(&module.id)?;
      let local_graph = local_graphs.remove(&module.id)?;
      let (imports, options_callback_slots, typed_callback_param_slots) =
        seed_plan_for(facts, &exports, &options_exports, &typed_callback_exports, &link_index);
      let plan = ModuleSeedPlan {
        imports,
        injects: inject_seed_plan(facts, &provide_index),
        options_callback_slots,
        typed_callback_param_slots,
      };
      Some((WorkSource::Input(module), local_graph, plan, Some(Arc::clone(&facts.summary))))
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
  let options_exports = resolve_options_callback_exports(facts_by_id, &link_index);
  let typed_callback_exports = resolve_typed_callback_param_exports(facts_by_id, &link_index);
  let provide_index = global_provide_index(facts_by_id);
  report.stats.export_resolve_ran = true;
  let work = unique
    .iter()
    .filter_map(|module| {
      let facts = facts_by_id.get(&module.id)?;
      let local_graph = local_graphs.remove(&module.id)?;
      let (imports, options_callback_slots, typed_callback_param_slots) =
        seed_plan_for(facts, &exports, &options_exports, &typed_callback_exports, &link_index);
      let plan = ModuleSeedPlan {
        imports,
        injects: inject_seed_plan(facts, &provide_index),
        options_callback_slots,
        typed_callback_param_slots,
      };
      Some((WorkSource::Input(module), local_graph, plan, None))
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
    report.seed_plan_dirty.clear();
    Arc::clone(&cached.plans)
  } else {
    // Caller guarantees `state.linking` is already populated (warm invalidate path).
    report.stats.export_resolve_ran = true;
    let link_index = link_index(resolved_links);
    let exports = Arc::new(resolve_exports(facts_by_id, &link_index));
    let options_exports = resolve_options_callback_exports(facts_by_id, &link_index);
    let typed_callback_exports = resolve_typed_callback_param_exports(facts_by_id, &link_index);
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
    let mut dirty_ids = BTreeSet::new();
    for id in &dirty_seed {
      let Some(facts) = facts_by_id.get(id) else {
        continue;
      };
      let (imports, options_callback_slots, typed_callback_param_slots) =
        seed_plan_for(facts, &exports, &options_exports, &typed_callback_exports, &link_index);
      next_plans.insert(
        id.clone(),
        ModuleSeedPlan {
          imports,
          injects: inject_seed_plan(facts, &provide_index),
          options_callback_slots,
          typed_callback_param_slots,
        },
      );
      dirty_ids.insert(id.clone());
      recomputed += 1;
    }
    for module in unique {
      if next_plans.contains_key(&module.id) {
        continue;
      }
      let Some(facts) = facts_by_id.get(&module.id) else {
        continue;
      };
      let (imports, options_callback_slots, typed_callback_param_slots) =
        seed_plan_for(facts, &exports, &options_exports, &typed_callback_exports, &link_index);
      next_plans.insert(
        module.id.clone(),
        ModuleSeedPlan {
          imports,
          injects: inject_seed_plan(facts, &provide_index),
          options_callback_slots,
          typed_callback_param_slots,
        },
      );
      dirty_ids.insert(module.id.clone());
      recomputed += 1;
    }
    report.stats.seed_plans_recomputed = recomputed;
    report.seed_plan_dirty = dirty_ids;
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
      Some((WorkSource::Input(module), local_graph, plan, Some(Arc::clone(&facts.summary))))
    })
    .collect()
}

/// Modules whose seed plans must be refreshed after a linking-surface change.
fn modules_needing_seed_recompute(
  previous: Option<&CachedLinkingSnapshot>,
  exports: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
  provide_index: &BTreeMap<super::super::InjectionKey, Vec<super::super::ProvideOffer>>,
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
    source: Option<Arc<ModuleSource>>,
    summary: Option<Arc<ModuleSummary>>,
    plan: Option<ModuleSeedPlan>,
    reactivity: ModuleReactivity,
    seeded: bool,
  },
  Partial {
    source: Option<Arc<ModuleSource>>,
    summary: Option<Arc<ModuleSummary>>,
    plan: Option<ModuleSeedPlan>,
    reactivity: ModuleReactivity,
    error: TraceModulesError,
  },
}

fn trace_dirty_module(
  module: WorkSource<'_>,
  mut local_graph: Arc<ReactivityGraph>,
  plan: ModuleSeedPlan,
  summary: Option<Arc<ModuleSummary>>,
  persist: bool,
  named_api_bags: &[crate::NamedApiBag],
) -> PhaseTwoOutcome {
  let seeded = !plan.is_empty();
  let id = module.source().id.clone();
  match trace_module_phase_two(module.source(), Arc::clone(&local_graph), &plan, named_api_bags) {
    Ok(reactivity) => PhaseTwoOutcome::Traced {
      source: module.into_persist_source(persist),
      summary,
      plan: persist.then_some(plan),
      reactivity,
      seeded,
    },
    Err(error) => {
      Arc::make_mut(&mut local_graph).set_module_id(id.clone());
      PhaseTwoOutcome::Partial {
        source: module.into_persist_source(persist),
        summary,
        plan: persist.then_some(plan),
        reactivity: ModuleReactivity { id, graph: local_graph },
        error,
      }
    }
  }
}

fn trace_module_phase_two(
  module: &ModuleSource,
  mut local_graph: Arc<ReactivityGraph>,
  plan: &ModuleSeedPlan,
  named_api_bags: &[crate::NamedApiBag],
) -> Result<ModuleReactivity, TraceModulesError> {
  if plan.is_empty() {
    Arc::make_mut(&mut local_graph).set_module_id(module.id.clone());
    return Ok(ModuleReactivity { id: module.id.clone(), graph: local_graph });
  }

  let allocator = Allocator::default();
  let source_type = source_type(module)?;
  let parsed = Parser::new(&allocator, module.source.as_ref(), source_type).parse();
  if !parsed.diagnostics.is_empty() {
    return Err(TraceModulesError::Parse {
      module: module.id.clone(),
      message: join_errors(parsed.diagnostics.as_slice()),
    });
  }
  let built = SemanticBuilder::new()
    .with_build_nodes(true)
    .with_check_syntax_error(true)
    .build(&parsed.program);
  if !built.diagnostics.is_empty() {
    return Err(TraceModulesError::Semantic {
      module: module.id.clone(),
      message: join_errors(built.diagnostics.as_slice()),
    });
  }
  let semantic = built.semantic;
  let seeds = materialize_seeds(module, &semantic, plan);
  let config = crate::TraceConfig { named_api_bags };
  let mut graph = trace_reactivity_seeded(
    &semantic,
    module.span_origin(),
    module.source_offset,
    module.kind,
    &seeds,
    &config,
  );
  graph.set_module_id(module.id.clone());
  Ok(ModuleReactivity { id: module.id.clone(), graph: Arc::new(graph) })
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

  // Cheap path: seed finished locals once (same as pre-forward linking).
  // `import { x as y }; export { y }` barrels resolve via import links in the
  // fixed-point below — only locals with finished state are inserted here.
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

  // Working copies only for modules that still need ForwardReturn / value-bag /
  // generic-method instantiate refine.
  let mut working_locals: BTreeMap<ModuleId, BTreeMap<String, ExportState>> = BTreeMap::new();
  for (id, module_facts) in facts {
    if module_facts.summary.locals.values().any(|state| {
      matches!(
        state,
        ExportState::ForwardReturn(_)
          | ExportState::ValueFactory(_)
          | ExportState::ValueBag(_)
          | ExportState::ValueFactoryCall(_)
          | ExportState::GenericMethodInstantiate { .. }
      ) || matches!(
        state,
        ExportState::Composable(shape) if shape.has_pending_value_bag_fields()
      )
    }) {
      working_locals.insert(id.clone(), module_facts.summary.locals.clone());
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
    let needs_reexport =
      module_facts.summary.exports.iter().any(|export| {
        matches!(export, ExportSummary::Reexport { .. } | ExportSummary::Star { .. })
      });
    // `import { d as x }; export { x }` — Local export name is not in `locals`.
    let needs_import_local_export = module_facts.summary.exports.iter().any(|export| {
      matches!(
        export,
        ExportSummary::Local { local, .. }
          if !module_facts.summary.locals.contains_key(local)
            && module_facts.summary.imports.iter().any(|import| import.local == *local)
      )
    });
    if needs_reexport || working_locals.contains_key(id) || needs_import_local_export {
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
    let mut refined_forward = false;

    // Resolve ForwardReturn / refine value bags against imports + working locals.
    if let Some(locals) = working_locals.get_mut(id) {
      let names: Vec<String> = locals.keys().cloned().collect();
      for name in names {
        let Some(state) = locals.get(&name).cloned() else {
          continue;
        };
        if !matches!(
          state,
          ExportState::ForwardReturn(_)
            | ExportState::ValueFactory(_)
            | ExportState::ValueBag(_)
            | ExportState::ValueFactoryCall(_)
            | ExportState::Composable(_)
            | ExportState::GenericMethodInstantiate { .. }
        ) {
          continue;
        }
        let refined = refine_export_state(state, id, locals, facts, links, &resolved);
        if locals.get(&name) != Some(&refined) {
          refined_forward = true;
          locals.insert(name, refined);
          changed = true;
        }
      }
      for export in &module_facts.summary.exports {
        let ExportSummary::Local { local, exported } = export else {
          continue;
        };
        if let Some(state) = locals.get(local)
          && let Some(publish) =
            publishable_export_state(state, id, locals, facts, links, &resolved)
        {
          changed |= insert_export(&mut resolved, id, exported, publish);
        }
      }
    }

    for export in &module_facts.summary.exports {
      match export {
        ExportSummary::Local { local, exported } => {
          // Barrel: `import { d as defineTypedComponent }; export { defineTypedComponent }`.
          if module_facts.summary.locals.contains_key(local) {
            continue;
          }
          if let Some(state) =
            resolve_name_export_state(id, local, &BTreeMap::new(), facts, links, &resolved, 0)
          {
            changed |= insert_export(&mut resolved, id, exported, state);
          }
        }
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
    if let Some(users) = reverse_users.get(id) {
      for consumer in users {
        if queued.insert(consumer) {
          queue.push_back(consumer);
        }
      }
    }
    // Only re-enter when a forward/value-bag refine may unlock more locals.
    if refined_forward && working_locals.contains_key(id) && queued.insert(id) {
      queue.push_back(id);
    }
  }

  resolved
}

fn refine_export_state(
  state: ExportState,
  module_id: &ModuleId,
  locals: &BTreeMap<String, ExportState>,
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
  resolved: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
) -> ExportState {
  match state {
    ExportState::ForwardReturn(callee) => export_lattice::refine_forward_return(
      resolve_name_export_state(module_id, &callee, locals, facts, links, resolved, 0),
      callee,
    ),
    ExportState::ValueFactory(bag) => {
      ExportState::ValueFactory(refine_value_bag(bag, module_id, locals, facts, links, resolved))
    }
    ExportState::ValueBag(bag) => {
      ExportState::ValueBag(refine_value_bag(bag, module_id, locals, facts, links, resolved))
    }
    ExportState::Composable(shape) => ExportState::Composable(refine_composable_shape(
      shape, module_id, locals, facts, links, resolved,
    )),
    // Keep the call marker so each export publish re-snapshots the callee bag
    // (avoid sticky MethodForward clones after the factory later refines).
    ExportState::ValueFactoryCall(callee) => ExportState::ValueFactoryCall(callee),
    ExportState::GenericMethodInstantiate { callee, property, type_arg_shapes } => {
      let callee_state =
        resolve_name_export_state(module_id, &callee, locals, facts, links, resolved, 0);
      let keep = ExportState::GenericMethodInstantiate {
        callee,
        property: property.clone(),
        type_arg_shapes: type_arg_shapes.clone(),
      };
      export_lattice::refine_generic_method_instantiate(
        callee_state.as_ref(),
        &property,
        &type_arg_shapes,
        keep,
      )
    }
    other => other,
  }
}

fn refine_composable_shape(
  shape: super::ComposableShape,
  module_id: &ModuleId,
  locals: &BTreeMap<String, ExportState>,
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
  resolved: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
) -> super::ComposableShape {
  export_lattice::refine_composable_pending(shape, |root| {
    resolve_name_export_state(module_id, root, locals, facts, links, resolved, 0)
  })
}

/// Materialize [`ExportState::ValueFactoryCall`] against current resolved exports.
fn publishable_export_state(
  state: &ExportState,
  module_id: &ModuleId,
  locals: &BTreeMap<String, ExportState>,
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
  resolved: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
) -> Option<ExportState> {
  let materialized = match state {
    ExportState::ValueFactoryCall(callee) => {
      let callee_state =
        resolve_name_export_state(module_id, callee, locals, facts, links, resolved, 0);
      export_lattice::value_factory_call_bag(callee_state.as_ref()).map(|bag| {
        ExportState::ValueBag(refine_value_bag(
          bag.clone(),
          module_id,
          locals,
          facts,
          links,
          resolved,
        ))
      })
    }
    _ => None,
  };
  export_lattice::as_publishable(state, materialized)
}

fn refine_value_bag(
  bag: super::ValueBag,
  module_id: &ModuleId,
  locals: &BTreeMap<String, ExportState>,
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
  resolved: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
) -> super::ValueBag {
  export_lattice::refine_value_bag(bag, |name| {
    resolve_name_export_state(module_id, name, locals, facts, links, resolved, 0)
  })
}

fn resolve_name_export_state(
  module_id: &ModuleId,
  name: &str,
  locals: &BTreeMap<String, ExportState>,
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
  resolved: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
  depth: u8,
) -> Option<ExportState> {
  // Pure order: locals → ES import → bare `#nuxt-imports:{name}` (PCR Name resolve).
  let imports: Vec<export_lattice::ImportBindingView<'_>> = facts
    .get(module_id)
    .map(|module_facts| {
      module_facts
        .summary
        .imports
        .iter()
        .map(|import| export_lattice::ImportBindingView {
          local: import.local.as_str(),
          source: import.source.as_str(),
          imported: import.imported.as_str(),
        })
        .collect()
    })
    .unwrap_or_default();
  export_lattice::resolve_name_export_state(
    name,
    locals,
    &imports,
    |specifier| links.get(&(module_id, specifier)).copied().cloned(),
    |target, export_name| resolved.get(target)?.get(export_name).cloned(),
    depth,
  )
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
  // Provisional / unresolved halves never cross the seed barrier alone.
  if !export_lattice::is_seedable(&state) {
    return false;
  }
  // Publish value bags even when some MethodForward entries remain. Waiting for
  // *every* forward (e.g. `useMutation` next to resolved `useQuery`) blocked the
  // whole factory export; unresolved methods stay quiet at `resolve_path`.
  let Some(module_exports) = resolved.get_mut(module) else {
    return false;
  };
  match module_exports.entry(exported.into()) {
    Entry::Vacant(entry) => {
      entry.insert(state);
      true
    }
    Entry::Occupied(mut entry) => match export_lattice::merge_published(entry.get(), &state) {
      export_lattice::PublishMerge::Unchanged => false,
      export_lattice::PublishMerge::Replace => {
        entry.insert(state);
        true
      }
      export_lattice::PublishMerge::Ambiguous => {
        entry.insert(ExportState::Ambiguous);
        true
      }
    },
  }
}

/// Propagate options-callback slots through `export { x } from` / `export *` barrels.
///
/// Independent of [`resolve_exports`]: a `declare function` may publish callback bags
/// without a seedable return [`ExportState`].
fn resolve_options_callback_exports(
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
) -> BTreeMap<ModuleId, BTreeMap<String, OptionsCallbackSlots>> {
  use std::collections::VecDeque;

  // Empty-slot graphs (typical synthetic / re-export benches) must not pay a
  // full barrel fixpoint that queues every `export { … } from`.
  if !facts
    .values()
    .any(|module| module.summary.options_callback_slots.values().any(|slots| !slots.is_empty()))
  {
    return BTreeMap::new();
  }

  let mut resolved: BTreeMap<ModuleId, BTreeMap<String, OptionsCallbackSlots>> = BTreeMap::new();

  for (id, module_facts) in facts {
    for (name, slots) in &module_facts.summary.options_callback_slots {
      if slots.is_empty() {
        continue;
      }
      resolved.entry(id.clone()).or_default().insert(name.clone(), slots.clone());
    }
    for export in &module_facts.summary.exports {
      let ExportSummary::Local { local, exported } = export else {
        continue;
      };
      if local == exported {
        continue;
      }
      if let Some(slots) = module_facts.summary.options_callback_slots.get(local)
        && !slots.is_empty()
      {
        resolved.entry(id.clone()).or_default().insert(exported.clone(), slots.clone());
      }
    }
  }

  let mut reverse_users: BTreeMap<&ModuleId, Vec<&ModuleId>> = BTreeMap::new();
  for ((from, _), to) in links {
    reverse_users.entry(*to).or_default().push(*from);
  }

  let mut queue = VecDeque::new();
  let mut queued = BTreeSet::new();
  for (id, module_facts) in facts {
    let needs_reexport =
      module_facts.summary.exports.iter().any(|export| {
        matches!(export, ExportSummary::Reexport { .. } | ExportSummary::Star { .. })
      });
    let needs_import_local_export = module_facts.summary.exports.iter().any(|export| {
      matches!(
        export,
        ExportSummary::Local { local, .. }
          if !module_facts.summary.locals.contains_key(local)
            && !module_facts.summary.options_callback_slots.contains_key(local)
            && module_facts.summary.imports.iter().any(|import| import.local == *local)
      )
    });
    if needs_reexport || needs_import_local_export {
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
        ExportSummary::Local { local, exported } => {
          if module_facts.summary.options_callback_slots.contains_key(local)
            || module_facts.summary.locals.contains_key(local)
          {
            continue;
          }
          let Some(import) =
            module_facts.summary.imports.iter().find(|import| import.local == *local)
          else {
            continue;
          };
          let Some(target) = links.get(&(id, import.source.as_str())).copied() else {
            continue;
          };
          let Some(slots) =
            resolved.get(target).and_then(|exports| exports.get(&import.imported)).cloned()
          else {
            continue;
          };
          changed |= insert_options_callback_export(&mut resolved, id, exported, slots);
        }
        ExportSummary::Reexport { source, imported, exported } => {
          let Some(target) = links.get(&(id, source.as_str())).copied() else {
            continue;
          };
          let Some(slots) = resolved.get(target).and_then(|exports| exports.get(imported)).cloned()
          else {
            continue;
          };
          changed |= insert_options_callback_export(&mut resolved, id, exported, slots);
        }
        ExportSummary::Star { source } => {
          let Some(target) = links.get(&(id, source.as_str())).copied() else {
            continue;
          };
          let Some(target_slots) = resolved.get(target).cloned() else {
            continue;
          };
          for (exported, slots) in target_slots {
            if exported != "default" {
              changed |= insert_options_callback_export(&mut resolved, id, &exported, slots);
            }
          }
        }
      }
    }
    if !changed {
      continue;
    }
    if let Some(users) = reverse_users.get(id) {
      for consumer in users {
        if queued.insert(consumer) {
          queue.push_back(consumer);
        }
      }
    }
  }

  resolved
}

fn insert_options_callback_export(
  resolved: &mut BTreeMap<ModuleId, BTreeMap<String, OptionsCallbackSlots>>,
  module: &ModuleId,
  exported: &str,
  slots: OptionsCallbackSlots,
) -> bool {
  if slots.is_empty() {
    return false;
  }
  // Barrel-only modules are not pre-seeded; create on first insert.
  match resolved.entry(module.clone()).or_default().entry(exported.into()) {
    Entry::Vacant(entry) => {
      entry.insert(slots);
      true
    }
    Entry::Occupied(mut entry) if entry.get() != &slots => {
      entry.insert(slots);
      true
    }
    Entry::Occupied(_) => false,
  }
}

/// Propagate typed function-callback Ref formals through barrels (same as options slots).
fn resolve_typed_callback_param_exports(
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
) -> BTreeMap<ModuleId, BTreeMap<String, TypedCallbackParamSlots>> {
  use std::collections::VecDeque;

  if !facts
    .values()
    .any(|module| module.summary.typed_callback_param_slots.values().any(|slots| !slots.is_empty()))
  {
    return BTreeMap::new();
  }

  let mut resolved: BTreeMap<ModuleId, BTreeMap<String, TypedCallbackParamSlots>> = BTreeMap::new();

  for (id, module_facts) in facts {
    for (name, slots) in &module_facts.summary.typed_callback_param_slots {
      if slots.is_empty() {
        continue;
      }
      resolved.entry(id.clone()).or_default().insert(name.clone(), slots.clone());
    }
    for export in &module_facts.summary.exports {
      let ExportSummary::Local { local, exported } = export else {
        continue;
      };
      if local == exported {
        continue;
      }
      if let Some(slots) = module_facts.summary.typed_callback_param_slots.get(local)
        && !slots.is_empty()
      {
        resolved.entry(id.clone()).or_default().insert(exported.clone(), slots.clone());
      }
    }
  }

  let mut reverse_users: BTreeMap<&ModuleId, Vec<&ModuleId>> = BTreeMap::new();
  for ((from, _), to) in links {
    reverse_users.entry(*to).or_default().push(*from);
  }

  let mut queue = VecDeque::new();
  let mut queued = BTreeSet::new();
  for (id, module_facts) in facts {
    let needs_reexport =
      module_facts.summary.exports.iter().any(|export| {
        matches!(export, ExportSummary::Reexport { .. } | ExportSummary::Star { .. })
      });
    let needs_import_local_export = module_facts.summary.exports.iter().any(|export| {
      matches!(
        export,
        ExportSummary::Local { local, .. }
          if !module_facts.summary.locals.contains_key(local)
            && !module_facts.summary.typed_callback_param_slots.contains_key(local)
            && module_facts.summary.imports.iter().any(|import| import.local == *local)
      )
    });
    if needs_reexport || needs_import_local_export {
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
        ExportSummary::Local { local, exported } => {
          if module_facts.summary.typed_callback_param_slots.contains_key(local)
            || module_facts.summary.locals.contains_key(local)
          {
            continue;
          }
          let Some(import) =
            module_facts.summary.imports.iter().find(|import| import.local == *local)
          else {
            continue;
          };
          let Some(target) = links.get(&(id, import.source.as_str())).copied() else {
            continue;
          };
          let Some(slots) =
            resolved.get(target).and_then(|exports| exports.get(&import.imported)).cloned()
          else {
            continue;
          };
          changed |= insert_typed_callback_param_export(&mut resolved, id, exported, slots);
        }
        ExportSummary::Reexport { source, imported, exported } => {
          let Some(target) = links.get(&(id, source.as_str())).copied() else {
            continue;
          };
          let Some(slots) = resolved.get(target).and_then(|exports| exports.get(imported)).cloned()
          else {
            continue;
          };
          changed |= insert_typed_callback_param_export(&mut resolved, id, exported, slots);
        }
        ExportSummary::Star { source } => {
          let Some(target) = links.get(&(id, source.as_str())).copied() else {
            continue;
          };
          let Some(target_slots) = resolved.get(target).cloned() else {
            continue;
          };
          for (exported, slots) in target_slots {
            if exported != "default" {
              changed |= insert_typed_callback_param_export(&mut resolved, id, &exported, slots);
            }
          }
        }
      }
    }
    if !changed {
      continue;
    }
    if let Some(users) = reverse_users.get(id) {
      for consumer in users {
        if queued.insert(consumer) {
          queue.push_back(consumer);
        }
      }
    }
  }

  resolved
}

fn insert_typed_callback_param_export(
  resolved: &mut BTreeMap<ModuleId, BTreeMap<String, TypedCallbackParamSlots>>,
  module: &ModuleId,
  exported: &str,
  slots: TypedCallbackParamSlots,
) -> bool {
  if slots.is_empty() {
    return false;
  }
  match resolved.entry(module.clone()).or_default().entry(exported.into()) {
    Entry::Vacant(entry) => {
      entry.insert(slots);
      true
    }
    Entry::Occupied(mut entry) if entry.get() != &slots => {
      entry.insert(slots);
      true
    }
    Entry::Occupied(_) => false,
  }
}

/// Coordinator-side: which of this module's import locals resolve to reactive exports,
/// plus callback-param slots (independent of return-shape seedability).
fn seed_plan_for(
  facts: &ModuleExportFacts,
  exports: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
  options_exports: &BTreeMap<ModuleId, BTreeMap<String, OptionsCallbackSlots>>,
  typed_callback_exports: &BTreeMap<ModuleId, BTreeMap<String, TypedCallbackParamSlots>>,
  links: &BTreeMap<(&ModuleId, &str), &ModuleId>,
) -> (
  ImportSeedPlan,
  BTreeMap<String, OptionsCallbackSlots>,
  BTreeMap<String, TypedCallbackParamSlots>,
) {
  use std::ops::Bound;

  let mut plan = ImportSeedPlan::new();
  let mut options_callback_slots = BTreeMap::new();
  let mut typed_callback_param_slots = BTreeMap::new();
  for import in &facts.summary.imports {
    if import.imported == "*" {
      continue;
    }
    let Some(target) = links.get(&(&facts.id, import.source.as_str())).copied() else {
      continue;
    };
    if let Some(slots) = options_exports
      .get(target)
      .and_then(|module| module.get(&import.imported))
      .filter(|slots| !slots.is_empty())
    {
      options_callback_slots.insert(import.local.clone(), slots.clone());
    }
    if let Some(slots) = typed_callback_exports
      .get(target)
      .and_then(|module| module.get(&import.imported))
      .filter(|slots| !slots.is_empty())
    {
      typed_callback_param_slots.insert(import.local.clone(), slots.clone());
    }
    let Some(state) =
      exports.get(target).and_then(|module_exports| module_exports.get(&import.imported))
    else {
      continue;
    };
    if !export_lattice::is_seedable(state) {
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
    if name.is_empty() {
      continue;
    }
    if !options_callback_slots.contains_key(name)
      && let Some(slots) = options_exports
        .get(*target)
        .and_then(|module| module.get(name))
        .filter(|slots| !slots.is_empty())
    {
      options_callback_slots.insert(name.to_owned(), slots.clone());
    }
    if !typed_callback_param_slots.contains_key(name)
      && let Some(slots) = typed_callback_exports
        .get(*target)
        .and_then(|module| module.get(name))
        .filter(|slots| !slots.is_empty())
    {
      typed_callback_param_slots.insert(name.to_owned(), slots.clone());
    }
    if plan.contains_key(name) {
      continue;
    }
    let Some(state) = exports.get(*target).and_then(|module_exports| module_exports.get(name))
    else {
      continue;
    };
    if !export_lattice::is_seedable(state) {
      continue;
    }
    plan.insert(name.to_owned(), state.clone());
  }
  (plan, options_callback_slots, typed_callback_param_slots)
}

/// Project-wide provide index (no App Tree): key → offers from every known site.
fn global_provide_index(
  facts: &BTreeMap<ModuleId, ModuleExportFacts>,
) -> BTreeMap<super::super::InjectionKey, Vec<ProvideOffer>> {
  let mut all = Vec::new();
  for module in facts.values() {
    all.extend(module.summary.provides.iter().cloned());
  }
  provide_offer_index(&all)
}

/// Unique inject seeds for one consumer (multi-provide keys stay quiet).
fn inject_seed_plan(
  facts: &ModuleExportFacts,
  provide_index: &BTreeMap<super::super::InjectionKey, Vec<ProvideOffer>>,
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
        // Prefer the import binding span; bare Nuxt/Vite auto-imports of exported
        // `const currentUser = computed(...)` have no import — use the first
        // unresolved identifier reference as the span.
        let span = if let Some(import) = imports.iter().find(|import| import.local == *local) {
          source_span(span_source, span_base, import.span)
        } else if let Some(reference_span) = first_bare_identifier_span(semantic, local) {
          source_span(span_source, span_base, reference_span)
        } else {
          continue;
        };
        if seeds.bindings.iter().any(|binding| binding.name == *local) {
          continue;
        }
        seeds.bindings.push(ReactiveBindingFact {
          name: local.clone(),
          kind: *kind,
          initialized_with_null: false,
          span,
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
          let Some(kind) = shape.kind_for_destructure(&call.property) else {
            continue;
          };
          seeds.bindings.push(ReactiveBindingFact {
            name: call.local.clone(),
            kind,
            initialized_with_null: false,
            span: source_span(span_source, span_base, call.span),
          });
        }
        let imported_instances = instance_calls.iter().filter(|call| call.imported_local == *local);
        let bare_instances =
          bare_instance_calls.iter().filter(|call| call.imported_local == *local);
        for call in imported_instances.chain(bare_instances) {
          seeds.composable_instances.insert(call.local.clone(), shape.fields.clone());
        }
      }
      ExportState::ValueFactory(bag) => {
        // `const api = createApi()` — calling a value factory yields a value bag.
        let imported_instances = instance_calls.iter().filter(|call| call.imported_local == *local);
        let bare_instances =
          bare_instance_calls.iter().filter(|call| call.imported_local == *local);
        for call in imported_instances.chain(bare_instances) {
          seed_value_bag_binding(&mut seeds, &call.local, bag);
        }
      }
      ExportState::ValueBag(bag) => {
        seed_value_bag_binding(&mut seeds, local, bag);
      }
      ExportState::ComponentFactory => {
        // Import local is a defineComponent setup wrapper — seed props at call sites.
        seeds.component_factories.insert(local.clone());
      }
      // Provisional / non-seedable (`!is_seedable`) — never invent consumer seeds.
      // New seedable variants without an arm also fall here (fail closed).
      _ => {}
    }
  }
  // Member-call destructures against seeded value bags (`api.maps.useX()`).
  let bags = seeds.value_bags.clone();
  seed_member_calls_from_value_bags(semantic, &bags, span_source, span_base, &mut seeds);
  // `defineFormProps({ setup({ values }) {…} })` — seed options-object callback bags.
  super::seed_options_callback_params_at_calls(
    semantic,
    &plan.options_callback_slots,
    span_source,
    span_base,
    &mut seeds.bindings,
  );
  // `useX(init, (state: ComputedRef<T>) => …)` — seed typed function-callback formals.
  super::seed_typed_callback_params_at_calls(
    semantic,
    &plan.typed_callback_param_slots,
    span_source,
    span_base,
    &mut seeds.bindings,
  );
  // Inject locals: re-read sites for exact spans; offers from the coordinator plan.
  if !plan.injects.is_empty() {
    let imported_bindings = collect_imported_bindings(semantic);
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

/// First unresolved `IdentifierReference` for `name` (bare auto-import value use).
fn first_bare_identifier_span(semantic: &oxc_semantic::Semantic<'_>, name: &str) -> Option<Span> {
  let mut best: Option<Span> = None;
  for node in semantic.nodes() {
    let AstKind::IdentifierReference(identifier) = node.kind() else {
      continue;
    };
    if identifier.name.as_str() != name {
      continue;
    }
    let Some(reference_id) = identifier.reference_id.get() else {
      continue;
    };
    if semantic.scoping().get_reference(reference_id).symbol_id().is_some() {
      continue;
    }
    let span = identifier.span;
    if best.is_none_or(|current| span.start < current.start) {
      best = Some(span);
    }
  }
  best
}

/// `const x = useX()` where `useX` is unresolved and present in the seed plan (bare auto-import).
///
/// Also covers `const x = cond ? ref(false) : useX()` when both arms are ref-like
/// (Vue primitive or seed-plan Factory/Known).
fn collect_bare_instance_calls(
  semantic: &oxc_semantic::Semantic<'_>,
  plan: &ImportSeedPlan,
) -> Vec<InstanceCallBinding> {
  let mut calls = Vec::new();
  let mut seen_locals = BTreeSet::new();
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
    let call_id = call.node_id.get();
    let parent = semantic.nodes().parent_kind(call_id);
    let (declarator, needs_arm_check) = match parent {
      AstKind::VariableDeclarator(declarator) => (declarator, false),
      AstKind::ConditionalExpression(_) => {
        // call → conditional → declarator
        let cond_id = semantic.nodes().parent_id(call_id);
        match semantic.nodes().parent_kind(cond_id) {
          AstKind::VariableDeclarator(declarator) => (declarator, true),
          _ => continue,
        }
      }
      _ => continue,
    };
    let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
      continue;
    };
    if needs_arm_check {
      let Some(Expression::ConditionalExpression(cond)) = &declarator.init else {
        continue;
      };
      if !conditional_arms_ref_like_with_plan(cond, plan) {
        continue;
      }
    }
    let local = identifier.name.to_string();
    if !seen_locals.insert(local.clone()) {
      continue;
    }
    calls.push(InstanceCallBinding {
      imported_local: callee.name.to_string(),
      local,
      span: identifier.span,
    });
  }
  calls.sort_by_key(|call| call.span.start);
  calls
}

/// Both ternary arms are ref-like: Vue `ref`/`computed`/… or seed-plan Factory/Known.
fn conditional_arms_ref_like_with_plan(
  cond: &oxc_ast::ast::ConditionalExpression<'_>,
  plan: &ImportSeedPlan,
) -> bool {
  arm_is_ref_like_with_plan(&cond.consequent, plan)
    && arm_is_ref_like_with_plan(&cond.alternate, plan)
}

fn arm_is_ref_like_with_plan(
  expression: &oxc_ast::ast::Expression<'_>,
  plan: &ImportSeedPlan,
) -> bool {
  let mut current = expression;
  for _ in 0..4 {
    match current {
      Expression::ParenthesizedExpression(paren) => current = &paren.expression,
      Expression::TSAsExpression(assertion) => current = &assertion.expression,
      Expression::TSTypeAssertion(assertion) => current = &assertion.expression,
      Expression::TSNonNullExpression(non_null) => current = &non_null.expression,
      Expression::CallExpression(call) => {
        let Some(callee) = call.callee.get_identifier_reference() else {
          return false;
        };
        let name = callee.name.as_str();
        // Vue primitive allowlist (bare or imported).
        if matches!(
          name,
          "ref"
            | "shallowRef"
            | "computed"
            | "customRef"
            | "toRef"
            | "useTemplateRef"
            | "defineModel"
        ) {
          return true;
        }
        return plan.get(name).and_then(export_lattice::ref_like_kind_from_export).is_some();
      }
      _ => return false,
    }
  }
  false
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

fn seed_value_bag_binding(seeds: &mut TraceSeeds, local: &str, bag: &ValueBag) {
  seeds.value_bags.insert(local.to_owned(), bag.clone());
}

fn seed_member_calls_from_value_bags(
  semantic: &Semantic<'_>,
  bags: &BTreeMap<String, ValueBag>,
  span_source: &str,
  span_base: usize,
  seeds: &mut TraceSeeds,
) {
  if bags.is_empty() {
    return;
  }
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some((root, path)) = super::static_member_call_path(&call.callee) else {
      continue;
    };
    let Some(bag) = bags.get(&root) else {
      continue;
    };
    let Some(entry) = bag.resolve_path(&path) else {
      continue;
    };
    let AstKind::VariableDeclarator(declarator) = semantic.nodes().parent_kind(call.node_id.get())
    else {
      continue;
    };
    match (entry, &declarator.id) {
      (ValueBagEntry::Method(shape), BindingPattern::ObjectPattern(pattern)) => {
        for property in &pattern.properties {
          let Some(exported) = property.key.static_name() else {
            continue;
          };
          let Some(kind) = shape.kind_for_destructure(exported.as_ref()) else {
            continue;
          };
          let mut identifiers = Vec::new();
          collect_binding_identifiers(&property.value, &mut identifiers);
          for (local, span) in identifiers {
            if seeds.bindings.iter().any(|binding| binding.name == local) {
              continue;
            }
            seeds.bindings.push(ReactiveBindingFact {
              name: local,
              kind,
              initialized_with_null: false,
              span: source_span(span_source, span_base, span),
            });
          }
        }
      }
      (ValueBagEntry::Method(shape), BindingPattern::BindingIdentifier(identifier)) => {
        seeds.composable_instances.insert(identifier.name.to_string(), shape.fields.clone());
      }
      (ValueBagEntry::MethodFactory(kind), BindingPattern::BindingIdentifier(identifier)) => {
        if seeds.bindings.iter().any(|binding| binding.name == identifier.name.as_str()) {
          continue;
        }
        seeds.bindings.push(ReactiveBindingFact {
          name: identifier.name.to_string(),
          kind: *kind,
          initialized_with_null: false,
          span: source_span(span_source, span_base, identifier.span),
        });
      }
      _ => {}
    }
  }
}
