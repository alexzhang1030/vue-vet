//! Post-trace layers: SFC template joins + static prop-flow edges.

use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

use vue_vet_core::ModuleId;
use vue_vet_reactivity::{ModuleReactivity, PropFlowSite, join_prop_flows};

use crate::model::{EdgeKind, GraphEdge, ProjectFile};
use crate::resolve::normalized_path;
use crate::state::{LayeredInputKey, ModuleLayerKey, ProjectGraphState};

/// Join template reads and prop-flow edges onto base module graphs, with warm reuse.
///
/// `this_pass` is the tracer report for this scan (dirty / retraced only when
/// the caller used subset retain). Unchanged bases are read from
/// `state.module_trace` so a warm leaf edit does not clone the live universe.
pub fn apply_template_prop_layers(
  state: &mut ProjectGraphState,
  ordered: &[&ProjectFile],
  edges: &[GraphEdge],
  this_pass: Vec<ModuleReactivity>,
  workspace_ids: &BTreeSet<ModuleId>,
) -> Arc<[ModuleReactivity]> {
  let facts_by_path = ordered
    .iter()
    .map(|file| (normalized_path(file.path.as_path()), Arc::clone(&file.facts)))
    .collect::<BTreeMap<_, _>>();
  let mut this_pass_by_id =
    this_pass.into_iter().map(|module| (module.id.clone(), module)).collect::<BTreeMap<_, _>>();
  let layered_key = LayeredInputKey {
    modules: workspace_ids
      .iter()
      .filter_map(|id| {
        base_module(id, &this_pass_by_id, state)
          .map(|module| module_layer_key(module, &facts_by_path))
      })
      .collect(),
    prop_edges: edges
      .iter()
      .filter(|edge| matches!(edge.kind, EdgeKind::ComponentUsage | EdgeKind::AutoComponent))
      .map(|edge| (edge.from.clone(), edge.to.clone(), edge.evidence.offset))
      .collect(),
  };

  if state.layered.key.as_ref() == Some(&layered_key) {
    state.last_stats.layered_graphs_rebuilt = false;
    return Arc::clone(&state.layered.modules);
  }

  state.last_stats.layered_graphs_rebuilt = true;
  if Arc::strong_count(&state.layered) > 1 {
    state.last_stats.partition_cow_clones += 1;
  }

  let prop_edges_unchanged =
    state.layered.key.as_ref().is_some_and(|prev| prev.prop_edges == layered_key.prop_edges);
  let prev_keys: BTreeMap<_, _> = state
    .layered
    .key
    .as_ref()
    .map(|prev| {
      prev.modules.iter().map(|key| (key.id.clone(), (key.base_ptr, key.facts_ptr))).collect()
    })
    .unwrap_or_default();
  let prev_modules = Arc::clone(&state.layered.modules);

  let mut rebuild = layered_key
    .modules
    .iter()
    .filter(|key| {
      !prev_keys.get(&key.id).is_some_and(|(base_ptr, facts_ptr)| {
        *base_ptr == key.base_ptr && *facts_ptr == key.facts_ptr
      })
    })
    .map(|key| key.id.clone())
    .collect::<BTreeSet<_>>();
  let expand_prop_children = !prop_edges_unchanged
    || layered_key
      .prop_edges
      .iter()
      .any(|(from, _, _)| rebuild.iter().any(|id| module_edge_key(id) == graph_edge_key(from)));
  if expand_prop_children {
    for (_, to, _) in &layered_key.prop_edges {
      for key in &layered_key.modules {
        if module_edge_key(&key.id) == graph_edge_key(to) {
          rebuild.insert(key.id.clone());
        }
      }
    }
  }

  let prev_by_id: BTreeMap<_, _> = prev_modules.iter().map(|module| (&module.id, module)).collect();
  let mut module_reactivity = Vec::with_capacity(workspace_ids.len());
  for id in workspace_ids {
    if !rebuild.contains(id)
      && let Some(prev) = prev_by_id.get(id)
    {
      module_reactivity.push((*prev).clone());
      continue;
    }
    let Some(mut module) =
      this_pass_by_id.remove(id).or_else(|| state.module_trace.cached_reactivity(id).cloned())
    else {
      continue;
    };
    if let Some(facts) = facts_by_path.get(module.id.as_str()) {
      Arc::make_mut(&mut module.graph).join_template_reads(&facts.template);
    }
    module_reactivity.push(module);
  }
  let graph_snapshots = module_reactivity
    .iter()
    .map(|module| (module.id.clone(), Arc::clone(&module.graph)))
    .collect::<BTreeMap<_, _>>();
  let prop_sites = edges
    .iter()
    .filter(|edge| matches!(edge.kind, EdgeKind::ComponentUsage | EdgeKind::AutoComponent))
    .filter_map(|edge| {
      // Graph edges use `file:{path}` node ids; module graphs / templates use bare paths.
      let parent_path = edge.from.strip_prefix("file:").unwrap_or(edge.from.as_str());
      let child_path = edge.to.strip_prefix("file:").unwrap_or(edge.to.as_str());
      let parent_facts = facts_by_path.get(parent_path)?;
      let parent_graph = graph_snapshots.get(parent_path)?;
      Some(PropFlowSite {
        element_span: edge.evidence.clone(),
        parent_template: &parent_facts.template,
        parent_graph,
        child_module: child_path,
      })
    })
    .collect::<Vec<_>>();
  if should_join_prop_flows(&prop_sites, prop_edges_unchanged, &rebuild) {
    join_prop_flows(&mut module_reactivity, &prop_sites);
  }
  let modules = Arc::<[ModuleReactivity]>::from(module_reactivity);
  let layered = Arc::make_mut(&mut state.layered);
  layered.key = Some(layered_key);
  layered.modules = Arc::clone(&modules);
  modules
}

fn base_module<'a>(
  id: &ModuleId,
  this_pass: &'a BTreeMap<ModuleId, ModuleReactivity>,
  state: &'a ProjectGraphState,
) -> Option<&'a ModuleReactivity> {
  this_pass.get(id).or_else(|| state.module_trace.cached_reactivity(id))
}

fn module_layer_key(
  module: &ModuleReactivity,
  facts_by_path: &BTreeMap<String, Arc<vue_vet_core::SfcFacts>>,
) -> ModuleLayerKey {
  ModuleLayerKey {
    id: module.id.clone(),
    base_ptr: Arc::as_ptr(&module.graph) as usize,
    facts_ptr: facts_by_path.get(module.id.as_str()).map_or(0, |facts| Arc::as_ptr(facts) as usize),
  }
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
  rebuild: &BTreeSet<ModuleId>,
) -> bool {
  !prop_sites.is_empty()
    && (!prop_edges_unchanged
      || prop_sites
        .iter()
        .any(|site| rebuild.iter().any(|id| module_edge_key(id) == site.child_module)))
}
