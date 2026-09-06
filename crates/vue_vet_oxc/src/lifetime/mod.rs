//! Watcher and effect-scope lifetime facts.

mod emit;
mod index;
mod resolve;

use vue_vet_core::ReactivityLifetimeFacts;

use self::index::LifetimeIndex;
use self::resolve::FunctionResolver;

#[derive(Clone, Copy)]
pub struct CollectStats {
  pub index_nodes: usize,
  pub emit_candidates: usize,
}

impl CollectStats {
  #[must_use]
  pub const fn total(self) -> usize {
    self.index_nodes.saturating_add(self.emit_candidates)
  }
}

pub fn collect(
  semantic: &oxc_semantic::Semantic<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) -> ReactivityLifetimeFacts {
  let (facts, stats) = collect_with_visits(semantic, line_index, sfc_source, script_offset);
  debug_assert!(
    stats.total() < usize::MAX,
    "lifetime index+emit work is counted for scaling tests"
  );
  facts
}

pub fn collect_with_visits(
  semantic: &oxc_semantic::Semantic<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) -> (ReactivityLifetimeFacts, CollectStats) {
  let vue_exports = resolve::vue_export_symbols(semantic);
  let mut resolver = FunctionResolver::new(semantic);
  let mut index = LifetimeIndex::default();
  let mut index_nodes = 0usize;

  for (node_id, node) in semantic.nodes().iter_enumerated() {
    index_nodes += 1;
    index::observe_node(semantic, node_id, node.kind(), &vue_exports, &mut resolver, &mut index);
  }
  let selection_work = index.finalize_selected_runs();

  let mut facts = ReactivityLifetimeFacts::default();
  let emit_candidates = emit::emit_all(
    semantic,
    &index,
    &mut resolver,
    line_index,
    sfc_source,
    script_offset,
    &mut facts,
  )
  .saturating_add(selection_work);
  facts.sort_by_source_order();
  (facts, CollectStats { index_nodes, emit_candidates })
}
