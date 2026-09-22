//! Session-retained linking state: cached module traces, the linking snapshot,
//! and live-scope retention rules for subset (dirty-only) scans.

use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

use vue_vet_core::{ModuleId, ReactivityGraph};

use super::super::super::{InjectionKey, ProvideOffer};
use super::super::{
  ExportState, ModuleExportFacts, ModuleLink, ModuleReactivity, ModuleSource, ModuleSummary,
};
use super::schedule::inject_seed_plan;
use super::{ModuleSeedPlan, TraceModulesOptions, TraceModulesReport, phase_one_from_summary};

/// Reusable state for cross-module linking in a long-lived project session.
///
/// Entries contain only Vue Vet-owned sources, seed plans, and final graphs.
/// No Oxc allocator, AST, or semantic object crosses this boundary.
#[derive(Clone, Debug, Default)]
pub struct ModuleTraceState {
  pub(super) entries: BTreeMap<ModuleId, CachedModuleTrace>,
  /// Last export/provide/seed fixed-point inputs and outputs.
  pub(super) linking: Option<CachedLinkingSnapshot>,
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

  /// Whether a prior persistent scan retained at least one module.
  #[must_use]
  pub fn has_cached_modules(&self) -> bool {
    !self.entries.is_empty()
  }
}

#[derive(Clone, Debug)]
pub(super) struct CachedModuleTrace {
  pub(super) source: Arc<ModuleSource>,
  pub(super) summary: Arc<ModuleSummary>,
  pub(super) plan: ModuleSeedPlan,
  pub(super) reactivity: ModuleReactivity,
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
pub(super) struct CachedLinkingSnapshot {
  pub(super) links: Vec<ModuleLink>,
  /// Phase-one summaries keyed for linking-surface equality (not `local_graph`).
  pub(super) summaries: BTreeMap<ModuleId, Arc<ModuleSummary>>,
  pub(super) exports: Arc<BTreeMap<ModuleId, BTreeMap<String, ExportState>>>,
  pub(super) provide_index: Arc<BTreeMap<InjectionKey, Vec<ProvideOffer>>>,
  pub(super) plans: Arc<BTreeMap<ModuleId, ModuleSeedPlan>>,
}

/// Linking-relevant fields only — never clones; prefers [`Arc::ptr_eq`].
///
/// `called_locals` is a phase-two skip index, not a linking key. A body edit
/// that starts calling an already-imported factory must reuse the cached plan
/// and reparse from source+plan dirtiness, not from a linking-cache miss.
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

pub(super) fn linking_cache_reusable(
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

pub(super) fn sorted_dedup_links(links: &[ModuleLink]) -> Vec<ModuleLink> {
  let mut owned = links.to_vec();
  owned.sort_by(|left, right| {
    (&left.from, &left.specifier, &left.to).cmp(&(&right.from, &right.specifier, &right.to))
  });
  owned.dedup();
  owned
}

/// Linking reuse against the live universe without merging cached graphs.
///
/// `this_pass` is the dirty subset. Cached modules are compared in place from
/// `state.entries` so a leaf edit does not rebuild N `facts_by_id` rows.
pub(super) fn linking_live_surfaces_match(
  owned_links: &[ModuleLink],
  this_pass: &BTreeMap<ModuleId, ModuleExportFacts>,
  scope: LiveScope<'_>,
  state: &ModuleTraceState,
  cached: &CachedLinkingSnapshot,
) -> bool {
  if cached.links != owned_links {
    return false;
  }
  let mut seen = 0_usize;
  for (id, facts) in this_pass {
    let Some(prev) = cached.summaries.get(id) else {
      return false;
    };
    if !linking_surface_eq(&facts.summary, prev) {
      return false;
    }
    seen += 1;
  }
  match scope {
    LiveScope::InputOnly => {}
    LiveScope::Explicit(live) => {
      for id in live {
        if this_pass.contains_key(id) {
          continue;
        }
        let Some(entry) = state.entries.get(id) else {
          continue;
        };
        let Some(prev) = cached.summaries.get(id) else {
          return false;
        };
        if !linking_surface_eq(&entry.summary, prev) {
          return false;
        }
        seen += 1;
      }
    }
    LiveScope::Retain { drop } => {
      for (id, entry) in &state.entries {
        if drop.contains(id) || this_pass.contains_key(id) {
          continue;
        }
        let Some(prev) = cached.summaries.get(id) else {
          return false;
        };
        if !linking_surface_eq(&entry.summary, prev) {
          return false;
        }
        seen += 1;
      }
    }
  }
  seen == cached.summaries.len()
}

#[derive(Clone, Copy)]
pub(super) enum LiveScope<'a> {
  InputOnly,
  Explicit(&'a BTreeSet<ModuleId>),
  Retain { drop: &'a BTreeSet<ModuleId> },
}

pub(super) const fn live_scope(options: &TraceModulesOptions) -> LiveScope<'_> {
  if let Some(live) = options.live_module_ids.as_ref() {
    return LiveScope::Explicit(live);
  }
  if options.retain_cached_modules {
    return LiveScope::Retain { drop: &options.drop_module_ids };
  }
  LiveScope::InputOnly
}

pub(super) const fn persist_subset(options: &TraceModulesOptions) -> bool {
  options.persist_linking_cache && !matches!(live_scope(options), LiveScope::InputOnly)
}

pub(super) fn subset_cache_emit(options: &TraceModulesOptions, state: &ModuleTraceState) -> bool {
  persist_subset(options) && state.has_cached_modules()
}

fn merge_cached(
  id: &ModuleId,
  entry: &CachedModuleTrace,
  facts_by_id: &mut BTreeMap<ModuleId, ModuleExportFacts>,
  local_graphs: &mut BTreeMap<ModuleId, Arc<ReactivityGraph>>,
) -> bool {
  if facts_by_id.contains_key(id) {
    return false;
  }
  let analysis = phase_one_from_summary(entry.source.as_ref(), &entry.summary);
  facts_by_id.insert(id.clone(), analysis.facts);
  local_graphs.insert(id.clone(), analysis.local_graph);
  true
}

pub(super) fn merge_cached_live_modules(
  scope: LiveScope<'_>,
  state: &ModuleTraceState,
  facts_by_id: &mut BTreeMap<ModuleId, ModuleExportFacts>,
  local_graphs: &mut BTreeMap<ModuleId, Arc<ReactivityGraph>>,
) -> usize {
  let mut merged = 0_usize;
  match scope {
    LiveScope::InputOnly => {}
    LiveScope::Explicit(live) => {
      for id in live {
        let Some(entry) = state.entries.get(id) else {
          continue;
        };
        merged += usize::from(merge_cached(id, entry, facts_by_id, local_graphs));
      }
    }
    LiveScope::Retain { drop } => {
      for (id, entry) in &state.entries {
        if drop.contains(id) {
          continue;
        }
        merged += usize::from(merge_cached(id, entry, facts_by_id, local_graphs));
      }
    }
  }
  merged
}

pub(super) fn count_cached_silent(
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

pub(super) fn retain_live_entries(
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

pub(super) fn id_is_live(scope: LiveScope<'_>, id: &ModuleId, state: &ModuleTraceState) -> bool {
  match scope {
    LiveScope::InputOnly => false,
    LiveScope::Explicit(live) => live.contains(id),
    LiveScope::Retain { drop } => !drop.contains(id) && state.entries.contains_key(id),
  }
}

/// Modules whose seed plans must be refreshed after a linking-surface change.
pub(super) fn modules_needing_seed_recompute(
  previous: Option<&CachedLinkingSnapshot>,
  exports: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
  provide_index: &BTreeMap<InjectionKey, Vec<ProvideOffer>>,
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
