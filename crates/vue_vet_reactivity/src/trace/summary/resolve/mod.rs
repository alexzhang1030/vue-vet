//! Cross-module linking: phase-one facts → export fixed point → seed plans → phase two.
//!
//! - [`worklist`] — export / callback-slot barrel fixed points over resolved links.
//! - [`schedule`] — which modules get which seed plan this pass (cold, warm, cached).
//! - [`cache`] — [`ModuleTraceState`] entries, linking snapshot, live-scope retention.
//! - [`seeds`] — worker-side seed materialization from the live parse.

mod cache;
mod schedule;
mod seeds;
mod worklist;

use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

use rayon::prelude::*;
use vue_vet_core::{ModuleId, ReactivityGraph};

use super::super::{ProvideOffer, trace_reactivity_seeded};
use super::{
  ExportState, ModuleLink, ModulePhaseOne, ModuleReactivity, ModuleSource, ModuleSummary,
  OptionsCallbackSlots, TraceModulesError, TypedCallbackParamSlots,
  analyze_module_phase_one_cached, phase_one_from_summary,
};
pub use cache::ModuleTraceState;
use cache::{
  CachedLinkingSnapshot, CachedModuleTrace, count_cached_silent, linking_live_surfaces_match,
  live_scope, merge_cached_live_modules, persist_subset, retain_live_entries, sorted_dedup_links,
  subset_cache_emit,
};
use schedule::{
  IncrementalInputs, WorkSource, build_fresh_seed_work, build_persistent_seed_work,
  build_work_from_cached_plans, pull_cached_seed_work,
};
use seeds::materialize_seeds;
use worklist::resolved_links_partial;

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
  /// Cached modules copied into this-pass `facts_by_id` for a linking miss.
  /// Zero on a linking-cache hit — do not merge the live universe just to
  /// compare surfaces.
  pub cached_modules_merged: usize,
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
/// modules whose seed plan cannot materialize and reparses only modules that
/// need seed materialization.
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
/// in `state` without a `ModuleSource` clone. Prefer
/// [`trace_modules_incremental_from_arcs`] when the caller already holds
/// `Arc<ModuleSource>` so persist is a refcount.
#[must_use]
pub fn trace_modules_incremental_from_refs(
  modules: &[&ModuleSource],
  links: &[ModuleLink],
  options: &TraceModulesOptions,
  state: &mut ModuleTraceState,
) -> TraceModulesReport {
  run_incremental_trace(modules, links, options, state, None)
}

/// Same as [`trace_modules_incremental_from_refs`] when the caller already
/// holds `Arc<ModuleSource>`. Persist stores that Arc instead of
/// `Arc::new(module.clone())`.
#[must_use]
pub fn trace_modules_incremental_from_arcs(
  modules: &[Arc<ModuleSource>],
  links: &[ModuleLink],
  options: &TraceModulesOptions,
  state: &mut ModuleTraceState,
) -> TraceModulesReport {
  let unique = modules.iter().map(AsRef::as_ref).collect::<Vec<_>>();
  let persist_arcs =
    modules.iter().map(|module| (&module.id, Arc::clone(module))).collect::<BTreeMap<_, _>>();
  run_incremental_trace(&unique, links, options, state, Some(&persist_arcs))
}

fn run_incremental_trace(
  modules: &[&ModuleSource],
  links: &[ModuleLink],
  options: &TraceModulesOptions,
  state: &mut ModuleTraceState,
  persist_arcs: Option<&BTreeMap<&ModuleId, Arc<ModuleSource>>>,
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

  let inputs = IncrementalInputs { unique: &unique, persist_arcs };
  if unique.is_empty() || options.reuse_current_pool {
    return trace_modules_incremental_in_current_pool(inputs, links, state, report, options);
  }

  let Ok(pool) = rayon::ThreadPoolBuilder::new()
    .num_threads(options.max_workers.max(1).min(unique.len().max(1)))
    .build()
  else {
    report.issues.push(TraceModulesError::WorkerDisconnected);
    return report;
  };
  pool.install(|| trace_modules_incremental_in_current_pool(inputs, links, state, report, options))
}

fn trace_modules_incremental_in_current_pool(
  inputs: IncrementalInputs<'_>,
  links: &[ModuleLink],
  state: &mut ModuleTraceState,
  mut report: TraceModulesReport,
  options: &TraceModulesOptions,
) -> TraceModulesReport {
  let unique = inputs.unique;
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

  let owned_links = sorted_dedup_links(links);
  let linking_hit = options.persist_linking_cache
    && state.linking.as_ref().is_some_and(|cached| {
      linking_live_surfaces_match(&owned_links, &facts_by_id, scope, state, cached)
    });

  // Linking-cache hit: compare surfaces against `state.entries`. Do not copy
  // the live universe into `facts_by_id` / `local_graphs` just to check.
  if persist_subset(options) && !linking_hit {
    report.stats.cached_modules_merged =
      merge_cached_live_modules(scope, state, &mut facts_by_id, &mut local_graphs);
  }

  let mut pending_linking_archive = None;
  let work = if linking_hit {
    build_work_from_cached_plans(inputs, state, &facts_by_id, &mut local_graphs, &mut report)
  } else {
    let (resolved_links, mut link_issues) = resolved_links_partial(&facts_by_id, links);
    report.issues.append(&mut link_issues);
    if options.persist_linking_cache && state.linking.is_some() {
      let mut work = build_persistent_seed_work(
        inputs,
        &owned_links,
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
    } else {
      // Cold pass: build plans once. A first persistent scan archives the
      // resolution after phase two; one-shot scans drop it.
      let (work, resolution) = build_fresh_seed_work(
        inputs,
        &resolved_links,
        &facts_by_id,
        &mut local_graphs,
        &mut report,
      );
      if options.persist_linking_cache {
        pending_linking_archive = Some(resolution.into_archive(owned_links, &facts_by_id));
      }
      work
    }
  };

  let persist = options.persist_linking_cache;
  let subset = persist_subset(options);
  // Split reused vs dirty before Rayon — independent leaf edits must not
  // schedule 999 immediate-reuse workers. Subset reports are this-pass
  // traces only; unchanged graphs stay in `state`.
  let mut reused = Vec::new();
  let mut reused_ids = BTreeSet::new();
  let mut dirty_work = Vec::new();
  for (module, local_graph, plan, summary) in work {
    let source = module.source();
    if let Some(cached) = state.entries.get(&source.id)
      && cached.source.as_ref() == source
      && cached.plan == plan
    {
      report.stats.reused_graphs += 1;
      reused_ids.insert(source.id.clone());
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

  // Empty plans and unused call-site-only plans reuse `local_graph`. Do not
  // Rayon-schedule them: the worker would only `set_module_id`.
  let mut local_reuse = Vec::new();
  let mut reparse_work = Vec::new();
  for (module, local_graph, plan, summary) in dirty_work {
    if seed_plan_needs_reparse(&plan, summary.as_deref()) {
      reparse_work.push((module, local_graph, plan, summary));
    } else {
      local_reuse.push((module, local_graph, plan, summary));
    }
  }

  let mut outcomes = local_reuse
    .into_iter()
    .map(|(module, local_graph, plan, summary)| {
      finish_unseeded_module(module, local_graph, plan, summary, persist)
    })
    .collect::<Vec<_>>();
  outcomes.extend(
    reparse_work
      .into_par_iter()
      .map(|(module, local_graph, plan, summary)| {
        trace_dirty_module(module, local_graph, plan, summary, persist, &options.named_api_bags)
      })
      .collect::<Vec<_>>(),
  );

  let mut keep = reused_ids;
  keep.extend(report.modules.iter().map(|module| module.id.clone()));
  for outcome in outcomes {
    if let Some(error) = outcome.error {
      report.issues.push(error);
    } else {
      report.stats.seeded_reparses += usize::from(outcome.seeded);
    }
    let reactivity = outcome.reactivity;
    keep.insert(reactivity.id.clone());
    if let (Some(source), Some(summary), Some(plan)) =
      (outcome.source, outcome.summary, outcome.plan)
    {
      state.entries.insert(
        reactivity.id.clone(),
        CachedModuleTrace { source, summary, plan, reactivity: reactivity.clone() },
      );
    }
    report.modules.push(reactivity);
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

struct PhaseTwoOutcome {
  source: Option<Arc<ModuleSource>>,
  summary: Option<Arc<ModuleSummary>>,
  plan: Option<ModuleSeedPlan>,
  reactivity: ModuleReactivity,
  seeded: bool,
  error: Option<TraceModulesError>,
}

/// Whether materialize would produce seeds. `Known` / `ValueBag` /
/// `ComponentFactory` / inject always can. `Factory` / `Composable` /
/// `ValueFactory` / callback slots can only when phase-one `called_locals`
/// contains the name. Missing summary stays conservative (reparse).
fn seed_plan_needs_reparse(plan: &ModuleSeedPlan, summary: Option<&ModuleSummary>) -> bool {
  if plan.is_empty() {
    return false;
  }
  let Some(summary) = summary else {
    return true;
  };
  if !plan.injects.is_empty() {
    return true;
  }
  for (local, state) in &plan.imports {
    match state {
      ExportState::Known(_) | ExportState::ValueBag(_) | ExportState::ComponentFactory => {
        return true;
      }
      ExportState::Factory(_) | ExportState::Composable(_) | ExportState::ValueFactory(_) => {
        if summary.called_locals.contains(local) {
          return true;
        }
      }
      ExportState::ValueFactoryCall(_)
      | ExportState::GenericMethodInstantiate { .. }
      | ExportState::ForwardReturn(_)
      | ExportState::DeclaredPlainObjectFactory
      | ExportState::BodyUnwrappedState
      | ExportState::Ambiguous => {}
    }
  }
  plan.options_callback_slots.keys().any(|name| summary.called_locals.contains(name))
    || plan.typed_callback_param_slots.keys().any(|name| summary.called_locals.contains(name))
}

fn finish_unseeded_module(
  module: WorkSource<'_>,
  mut local_graph: Arc<ReactivityGraph>,
  plan: ModuleSeedPlan,
  summary: Option<Arc<ModuleSummary>>,
  persist: bool,
) -> PhaseTwoOutcome {
  let id = module.source().id.clone();
  Arc::make_mut(&mut local_graph).set_module_id(id.clone());
  PhaseTwoOutcome {
    source: module.into_persist_source(persist),
    summary,
    plan: persist.then_some(plan),
    reactivity: ModuleReactivity { id, graph: local_graph },
    seeded: false,
    error: None,
  }
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
    Ok(reactivity) => PhaseTwoOutcome {
      source: module.into_persist_source(persist),
      summary,
      plan: persist.then_some(plan),
      reactivity,
      seeded,
      error: None,
    },
    Err(error) => {
      Arc::make_mut(&mut local_graph).set_module_id(id.clone());
      PhaseTwoOutcome {
        source: module.into_persist_source(persist),
        summary,
        plan: persist.then_some(plan),
        reactivity: ModuleReactivity { id, graph: local_graph },
        seeded: false,
        error: Some(error),
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

  super::with_module_semantic(module, |semantic| {
    let seeds = materialize_seeds(module, semantic, plan);
    let config = crate::TraceConfig { named_api_bags };
    let mut graph = trace_reactivity_seeded(
      semantic,
      module.span_origin(),
      module.source_offset,
      module.kind,
      &seeds,
      &config,
    );
    graph.set_module_id(module.id.clone());
    Ok(ModuleReactivity { id: module.id.clone(), graph: Arc::new(graph) })
  })
}
