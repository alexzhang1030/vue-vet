//! Post-trace layers: SFC template joins + static prop-flow edges.

use std::{collections::BTreeMap, sync::Arc};

use vue_vet_reactivity::{ModuleReactivity, PropFlowSite, join_prop_flows};

use crate::model::{EdgeKind, GraphEdge, ProjectFile};
use crate::resolve::normalized_path;
use crate::state::{LayeredInputKey, ModuleLayerKey, ProjectGraphState};

/// Join template reads and prop-flow edges onto base module graphs, with warm reuse.
pub fn apply_template_prop_layers(
  state: &mut ProjectGraphState,
  ordered: &[&ProjectFile],
  edges: &[GraphEdge],
  base_reactivity: Vec<ModuleReactivity>,
) -> Vec<ModuleReactivity> {
  let facts_by_path = ordered
    .iter()
    .map(|file| (normalized_path(file.path.as_path()), Arc::clone(&file.facts)))
    .collect::<BTreeMap<_, _>>();
  let layered_key = LayeredInputKey {
    modules: base_reactivity
      .iter()
      .map(|module| ModuleLayerKey {
        id: module.id.clone(),
        base_ptr: Arc::as_ptr(&module.graph) as usize,
        facts_ptr: facts_by_path
          .get(module.id.as_str())
          .map_or(0, |facts| Arc::as_ptr(facts) as usize),
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
    return state.layered.modules.as_ref().to_vec();
  }

  state.last_stats.layered_graphs_rebuilt = true;
  if Arc::strong_count(&state.layered) > 1 {
    state.last_stats.partition_cow_clones += 1;
  }
  let layered = Arc::make_mut(&mut state.layered);
  let mut module_reactivity = base_reactivity;
  // Re-apply SFC template joins onto module graphs so cross-file seeds and
  // template reads share one fact surface. Spans stay SFC-absolute when the
  // module carried `source_offset` + `span_source`.
  for module in &mut module_reactivity {
    if let Some(facts) = facts_by_path.get(module.id.as_str()) {
      Arc::make_mut(&mut module.graph).join_template_reads(&facts.template);
    }
  }
  // Static parent `:prop="binding"` → child `props.prop` edges (under-approx).
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
  join_prop_flows(&mut module_reactivity, &prop_sites);
  layered.key = Some(layered_key);
  layered.modules = Arc::from(module_reactivity.as_slice());
  module_reactivity
}
