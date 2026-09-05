//! Post-trace layers: SFC template joins + static prop-flow edges.

use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

use vue_vet_core::ModuleId;
use vue_vet_reactivity::{ModuleReactivity, PropFlowSite, join_prop_flows};

use crate::model::{EdgeKind, GraphEdge, ProjectFile};
use crate::state::{LayeredInputKey, ModuleLayerKey, ProjectGraphState};

/// Join template reads and prop-flow edges onto base module graphs, with warm reuse.
///
/// `this_pass` is the tracer report for this scan (dirty / retraced only when
/// the caller used subset retain). Unchanged bases are read from
/// `state.module_trace`. A matching key returns the cached `Arc` slice with no
/// `ModuleReactivity` clone. A leaf edit patches the stored key in place so
/// the other N-1 `ModuleId`s are not cloned.
pub fn apply_template_prop_layers(
  state: &mut ProjectGraphState,
  ordered: &[&ProjectFile],
  edges: &[GraphEdge],
  this_pass: Vec<ModuleReactivity>,
  workspace_ids: &BTreeSet<&ModuleId>,
) -> Arc<[Arc<ModuleReactivity>]> {
  if layered_inputs_match(state, ordered, edges, &this_pass, workspace_ids) {
    state.last_stats.layered_graphs_rebuilt = false;
    return Arc::clone(&state.layered.modules);
  }

  let facts_ptr = facts_ptrs(ordered);
  let mut this_pass_by_id =
    this_pass.into_iter().map(|module| (module.id.clone(), module)).collect::<BTreeMap<_, _>>();

  state.last_stats.layered_graphs_rebuilt = true;
  if Arc::strong_count(&state.layered) > 1 {
    state.last_stats.partition_cow_clones += 1;
  }

  let prop_edges_unchanged =
    state.layered.key.as_ref().is_some_and(|prev| prop_edges_match(&prev.prop_edges, edges));
  let ids_unchanged =
    state.layered.key.as_ref().is_some_and(|prev| workspace_ids_match(prev, workspace_ids));

  let mut rebuild = workspace_ids
    .iter()
    .copied()
    .filter(|id| {
      let Some(base) = base_module(id, &this_pass_by_id, state) else {
        return true;
      };
      let new_base = Arc::as_ptr(&base.graph) as usize;
      let new_facts = facts_ptr_for(id, &facts_ptr);
      !state
        .layered
        .key
        .as_ref()
        .is_some_and(|prev| previous_layer_matches(&prev.modules, id, new_base, new_facts))
    })
    .collect::<BTreeSet<_>>();
  let expand_prop_children = !prop_edges_unchanged
    || state.layered.key.as_ref().is_some_and(|key| {
      key
        .prop_edges
        .iter()
        .any(|(from, _, _)| rebuild.iter().any(|id| module_edge_key(id) == graph_edge_key(from)))
    });
  if expand_prop_children && let Some(key) = state.layered.key.as_ref() {
    for (_, to, _) in &key.prop_edges {
      for id in workspace_ids.iter().copied() {
        if module_edge_key(id) == graph_edge_key(to) {
          rebuild.insert(id);
        }
      }
    }
  }

  let prev_modules = Arc::clone(&state.layered.modules);
  let patched_ptrs = (ids_unchanged && prop_edges_unchanged)
    .then(|| {
      state.layered.key.as_ref().map(|key| {
        key
          .modules
          .iter()
          .map(|module_key| {
            base_module(&module_key.id, &this_pass_by_id, state)
              .map_or((module_key.base_ptr, module_key.facts_ptr), |base| {
                (Arc::as_ptr(&base.graph) as usize, facts_ptr_for(&module_key.id, &facts_ptr))
              })
          })
          .collect::<Vec<_>>()
      })
    })
    .flatten();
  let rebuilt_key = (!ids_unchanged || !prop_edges_unchanged).then(|| LayeredInputKey {
    modules: workspace_ids
      .iter()
      .copied()
      .filter_map(|id| {
        base_module(id, &this_pass_by_id, state).map(|module| ModuleLayerKey {
          id: id.clone(),
          base_ptr: Arc::as_ptr(&module.graph) as usize,
          facts_ptr: facts_ptr_for(id, &facts_ptr),
        })
      })
      .collect(),
    prop_edges: prop_usage_edges(edges)
      .map(|(from, to, offset)| (from.to_owned(), to.to_owned(), offset))
      .collect(),
  });
  let layered = Arc::make_mut(&mut state.layered);
  if let Some(key) = rebuilt_key {
    debug_assert!(
      layer_keys_sorted(&key.modules),
      "LayeredInputKey.modules follows BTreeSet<&ModuleId> order"
    );
    layered.key = Some(key);
  } else if let (Some(key), Some(ptrs)) = (layered.key.as_mut(), patched_ptrs) {
    for (module_key, (base_ptr, facts_ptr)) in key.modules.iter_mut().zip(ptrs) {
      module_key.base_ptr = base_ptr;
      module_key.facts_ptr = facts_ptr;
    }
  }

  let prev_by_id: BTreeMap<_, _> =
    prev_modules.iter().map(|module| (&module.id, Arc::clone(module))).collect();
  let mut module_reactivity = Vec::with_capacity(workspace_ids.len());
  for id in workspace_ids.iter().copied() {
    if !rebuild.contains(id)
      && let Some(prev) = prev_by_id.get(id)
    {
      module_reactivity.push(Arc::clone(prev));
      continue;
    }
    let Some(mut module) =
      this_pass_by_id.remove(id).or_else(|| state.module_trace.cached_reactivity(id).cloned())
    else {
      continue;
    };
    if let Some(facts) = ordered
      .iter()
      .find(|file| file.path.as_str() == id.as_str().strip_suffix("#script").unwrap_or(id.as_str()))
    {
      Arc::make_mut(&mut module.graph).join_template_reads(&facts.facts.template);
    }
    module_reactivity.push(Arc::new(module));
  }
  let graph_snapshots = module_reactivity
    .iter()
    .map(|module| (module.id.as_str().to_owned(), Arc::clone(&module.graph)))
    .collect::<BTreeMap<_, _>>();
  let prop_sites = edges
    .iter()
    .filter(|edge| matches!(edge.kind, EdgeKind::ComponentUsage | EdgeKind::AutoComponent))
    .filter_map(|edge| {
      let parent_path = edge.from.strip_prefix("file:").unwrap_or(edge.from.as_str());
      let child_path = edge.to.strip_prefix("file:").unwrap_or(edge.to.as_str());
      let parent_facts = ordered.iter().find(|file| file.path.as_str() == parent_path)?;
      let parent_graph = graph_snapshots.get(parent_path)?;
      Some(PropFlowSite {
        element_span: edge.evidence,
        parent_template: &parent_facts.facts.template,
        parent_graph,
        child_module: child_path,
      })
    })
    .collect::<Vec<_>>();
  if should_join_prop_flows(&prop_sites, prop_edges_unchanged, &rebuild) {
    join_prop_flows(&mut module_reactivity, &prop_sites);
  }
  let modules = Arc::<[Arc<ModuleReactivity>]>::from(module_reactivity);
  let layered = Arc::make_mut(&mut state.layered);
  layered.modules = Arc::clone(&modules);
  modules
}

fn layered_inputs_match(
  state: &ProjectGraphState,
  ordered: &[&ProjectFile],
  edges: &[GraphEdge],
  this_pass: &[ModuleReactivity],
  workspace_ids: &BTreeSet<&ModuleId>,
) -> bool {
  let Some(cached) = state.layered.key.as_ref() else {
    return false;
  };
  if !workspace_ids_match(cached, workspace_ids) || !prop_edges_match(&cached.prop_edges, edges) {
    return false;
  }
  let facts_ptr = facts_ptrs(ordered);
  cached.modules.iter().all(|key| {
    let Some(base) = this_pass
      .iter()
      .find(|module| module.id == key.id)
      .or_else(|| state.module_trace.cached_reactivity(&key.id))
    else {
      return false;
    };
    Arc::as_ptr(&base.graph) as usize == key.base_ptr
      && facts_ptr_for(&key.id, &facts_ptr) == key.facts_ptr
  })
}

fn workspace_ids_match(cached: &LayeredInputKey, workspace_ids: &BTreeSet<&ModuleId>) -> bool {
  cached.modules.len() == workspace_ids.len()
    && cached.modules.iter().map(|key| &key.id).eq(workspace_ids.iter().copied())
}

/// `LayeredInputKey.modules` is built from `BTreeSet<&ModuleId>` and stays sorted
/// by `id`. Warm scans patch `base_ptr` / `facts_ptr` in place, preserving order.
fn previous_layer_matches(
  modules: &[ModuleLayerKey],
  id: &ModuleId,
  base_ptr: usize,
  facts_ptr: usize,
) -> bool {
  let Ok(index) = modules.binary_search_by(|key| key.id.cmp(id)) else {
    return false;
  };
  modules.get(index).is_some_and(|key| key.base_ptr == base_ptr && key.facts_ptr == facts_ptr)
}

fn layer_keys_sorted(modules: &[ModuleLayerKey]) -> bool {
  modules.iter().zip(modules.iter().skip(1)).all(|(left, right)| left.id < right.id)
}

fn facts_ptrs<'a>(ordered: &[&'a ProjectFile]) -> BTreeMap<&'a str, usize> {
  ordered.iter().map(|file| (file.path.as_str(), Arc::as_ptr(&file.facts) as usize)).collect()
}

fn facts_ptr_for(id: &ModuleId, facts_ptr: &BTreeMap<&str, usize>) -> usize {
  facts_ptr.get(id.as_str().strip_suffix("#script").unwrap_or(id.as_str())).copied().unwrap_or(0)
}

fn prop_usage_edges(edges: &[GraphEdge]) -> impl Iterator<Item = (&str, &str, usize)> {
  edges
    .iter()
    .filter(|edge| matches!(edge.kind, EdgeKind::ComponentUsage | EdgeKind::AutoComponent))
    .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.evidence.offset))
}

fn prop_edges_match(cached: &[(String, String, usize)], edges: &[GraphEdge]) -> bool {
  cached
    .iter()
    .map(|(from, to, offset)| (from.as_str(), to.as_str(), *offset))
    .eq(prop_usage_edges(edges))
}

fn base_module<'a>(
  id: &ModuleId,
  this_pass: &'a BTreeMap<ModuleId, ModuleReactivity>,
  state: &'a ProjectGraphState,
) -> Option<&'a ModuleReactivity> {
  this_pass.get(id).or_else(|| state.module_trace.cached_reactivity(id))
}

fn module_edge_key(id: &ModuleId) -> &str {
  id.as_str().strip_suffix("#script").unwrap_or(id.as_str())
}

fn graph_edge_key(node: &str) -> &str {
  node.strip_prefix("file:").unwrap_or(node)
}

fn should_join_prop_flows(
  prop_sites: &[PropFlowSite<'_>],
  prop_edges_unchanged: bool,
  rebuild: &BTreeSet<&ModuleId>,
) -> bool {
  !prop_sites.is_empty()
    && (!prop_edges_unchanged
      || prop_sites
        .iter()
        .any(|site| rebuild.iter().any(|id| module_edge_key(id) == site.child_module)))
}

#[cfg(test)]
mod tests {
  use super::{ModuleLayerKey, layer_keys_sorted, previous_layer_matches};
  use vue_vet_core::ModuleId;

  fn key(id: &str, base_ptr: usize, facts_ptr: usize) -> ModuleLayerKey {
    ModuleLayerKey { id: ModuleId::from(id), base_ptr, facts_ptr }
  }

  #[test]
  fn previous_layer_matches_uses_sorted_ids() {
    let modules = vec![key("src/a.ts", 1, 2), key("src/a.ts#script", 3, 4), key("src/b.ts", 5, 6)];
    assert!(layer_keys_sorted(&modules));
    assert!(previous_layer_matches(&modules, &ModuleId::from("src/b.ts"), 5, 6));
    assert!(!previous_layer_matches(&modules, &ModuleId::from("src/b.ts"), 0, 6));
    assert!(!previous_layer_matches(&modules, &ModuleId::from("src/b.ts"), 5, 0));
    assert!(!previous_layer_matches(&modules, &ModuleId::from("src/c.ts"), 1, 2));
    assert!(!previous_layer_matches(&[], &ModuleId::from("src/a.ts"), 1, 2));
    assert!(previous_layer_matches(&modules, &ModuleId::from("src/a.ts#script"), 3, 4));
  }
}
