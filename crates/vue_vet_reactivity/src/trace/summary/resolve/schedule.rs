//! Seed-plan scheduling: decide which modules receive which [`ModuleSeedPlan`]
//! on cold, warm-invalidate, cached-hit, and subset (dirty-only) passes.

use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

use vue_vet_core::{ModuleId, ReactivityGraph};

use super::super::super::{InjectionKey, ProvideOffer, provide_offer_index, resolve_inject_offer};
use super::super::{
  ExportState, ModuleExportFacts, ModuleLink, ModuleSource, ModuleSummary, NUXT_IMPORTS_RANGE_END,
  NUXT_IMPORTS_SPECIFIER_PREFIX, OptionsCallbackSlots, TypedCallbackParamSlots, export_lattice,
};
use super::cache::{
  CachedLinkingSnapshot, LiveScope, id_is_live, linking_cache_reusable,
  modules_needing_seed_recompute,
};
use super::worklist::{
  link_index, resolve_exports, resolve_options_callback_exports,
  resolve_typed_callback_param_exports,
};
use super::{ImportSeedPlan, ModuleSeedPlan, ModuleTraceState, TraceModulesReport};

/// Phase-two work source. Input modules are borrowed; callers that already
/// hold `Arc<ModuleSource>` persist that handle. Seed-dirty modules missing
/// from this pass share the cached `Arc` (no `ModuleSource` clone).
pub(super) enum WorkSource<'a> {
  Input(&'a ModuleSource),
  Shared(Arc<ModuleSource>),
  Cached(Arc<ModuleSource>),
}

impl WorkSource<'_> {
  pub(super) fn source(&self) -> &ModuleSource {
    match self {
      Self::Input(module) => module,
      Self::Shared(module) | Self::Cached(module) => module,
    }
  }

  pub(super) fn into_persist_source(self, persist: bool) -> Option<Arc<ModuleSource>> {
    if !persist {
      return None;
    }
    match self {
      Self::Input(module) => Some(Arc::new(module.clone())),
      Self::Shared(module) | Self::Cached(module) => Some(module),
    }
  }
}

fn input_work_source<'a>(
  module: &'a ModuleSource,
  persist_arcs: Option<&BTreeMap<&ModuleId, Arc<ModuleSource>>>,
) -> WorkSource<'a> {
  persist_arcs
    .and_then(|arcs| arcs.get(&module.id).map(|arc| WorkSource::Shared(Arc::clone(arc))))
    .unwrap_or(WorkSource::Input(module))
}

#[derive(Clone, Copy)]
pub(super) struct IncrementalInputs<'a> {
  pub(super) unique: &'a [&'a ModuleSource],
  pub(super) persist_arcs: Option<&'a BTreeMap<&'a ModuleId, Arc<ModuleSource>>>,
}

impl<'a> IncrementalInputs<'a> {
  fn work(self, module: &'a ModuleSource) -> WorkSource<'a> {
    input_work_source(module, self.persist_arcs)
  }
}

pub(super) type SeedWorkItem<'a> =
  (WorkSource<'a>, Arc<ReactivityGraph>, ModuleSeedPlan, Option<Arc<ModuleSummary>>);

pub(super) fn pull_cached_seed_work<'a>(
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

pub(super) struct PendingLinkingArchive {
  pub(super) links: Vec<ModuleLink>,
  pub(super) summaries: BTreeMap<ModuleId, Arc<ModuleSummary>>,
  pub(super) exports: Arc<BTreeMap<ModuleId, BTreeMap<String, ExportState>>>,
  pub(super) provide_index: Arc<BTreeMap<InjectionKey, Vec<ProvideOffer>>>,
}

pub(super) fn build_cold_persistent_seed_work<'a>(
  inputs: IncrementalInputs<'a>,
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
  let work = inputs
    .unique
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
      Some((inputs.work(module), local_graph, plan, Some(Arc::clone(&facts.summary))))
    })
    .collect::<Vec<_>>();
  report.stats.seed_plans_recomputed = work.len();
  let summaries =
    facts_by_id.iter().map(|(id, facts)| (id.clone(), Arc::clone(&facts.summary))).collect();
  (work, PendingLinkingArchive { links: owned_links, summaries, exports, provide_index })
}

pub(super) fn build_oneshot_seed_work<'a>(
  inputs: IncrementalInputs<'a>,
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
  let work = inputs
    .unique
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
      Some((inputs.work(module), local_graph, plan, Some(Arc::clone(&facts.summary))))
    })
    .collect::<Vec<_>>();
  report.stats.seed_plans_recomputed = work.len();
  work
}

pub(super) fn build_work_from_cached_plans<'a>(
  inputs: IncrementalInputs<'a>,
  state: &ModuleTraceState,
  facts_by_id: &BTreeMap<ModuleId, ModuleExportFacts>,
  local_graphs: &mut BTreeMap<ModuleId, Arc<ReactivityGraph>>,
  report: &mut TraceModulesReport,
) -> Vec<SeedWorkItem<'a>> {
  report.stats.seed_plans_recomputed = 0;
  report.stats.export_resolve_ran = false;
  report.seed_plan_dirty.clear();
  let Some(plans) = state.linking.as_ref().map(|cached| Arc::clone(&cached.plans)) else {
    return Vec::new();
  };
  inputs
    .unique
    .iter()
    .filter_map(|module| {
      let facts = facts_by_id.get(&module.id)?;
      let local_graph = local_graphs.remove(&module.id)?;
      let plan = plans.get(&module.id)?.clone();
      Some((inputs.work(module), local_graph, plan, Some(Arc::clone(&facts.summary))))
    })
    .collect()
}

pub(super) fn build_persistent_seed_work<'a>(
  inputs: IncrementalInputs<'a>,
  owned_links: &[ModuleLink],
  resolved_links: &BTreeMap<(ModuleId, String), ModuleId>,
  facts_by_id: &BTreeMap<ModuleId, ModuleExportFacts>,
  local_graphs: &mut BTreeMap<ModuleId, Arc<ReactivityGraph>>,
  state: &mut ModuleTraceState,
  report: &mut TraceModulesReport,
) -> Vec<SeedWorkItem<'a>> {
  let plans = if let Some(cached) =
    state.linking.as_ref().filter(|cached| linking_cache_reusable(owned_links, facts_by_id, cached))
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
      owned_links,
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
    for module in inputs.unique {
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
      links: owned_links.to_vec(),
      summaries,
      exports,
      provide_index,
      plans: Arc::clone(&plans),
    });
    plans
  };

  inputs
    .unique
    .iter()
    .filter_map(|module| {
      let facts = facts_by_id.get(&module.id)?;
      let local_graph = local_graphs.remove(&module.id)?;
      let plan = plans.get(&module.id)?.clone();
      Some((inputs.work(module), local_graph, plan, Some(Arc::clone(&facts.summary))))
    })
    .collect()
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
) -> BTreeMap<InjectionKey, Vec<ProvideOffer>> {
  let mut all = Vec::new();
  for module in facts.values() {
    all.extend(module.summary.provides.iter().cloned());
  }
  provide_offer_index(&all)
}

/// Unique inject seeds for one consumer (multi-provide keys stay quiet).
pub(super) fn inject_seed_plan(
  facts: &ModuleExportFacts,
  provide_index: &BTreeMap<InjectionKey, Vec<ProvideOffer>>,
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
