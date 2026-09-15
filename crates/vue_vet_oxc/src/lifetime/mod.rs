//! Watcher and effect-scope lifetime facts.

mod cleanup_identity;
mod emit;
mod index;
mod ownership;
mod resolve;
mod stats;

use vue_vet_core::ReactivityLifetimeFacts;

use self::index::LifetimeIndex;
use self::resolve::FunctionResolver;

pub use self::stats::CollectStats;

#[must_use]
pub const fn counter_layout() -> (usize, usize) {
  (std::mem::size_of::<cleanup_identity::IdentityWork>(), std::mem::size_of::<CollectStats>())
}

#[cfg(not(test))]
const _: () = {
  let layout = counter_layout();
  assert!(layout.0 == 0, "production IdentityWork is a ZST");
  assert!(layout.1 == 0, "production CollectStats is a ZST");
};

pub fn collect(
  semantic: &oxc_semantic::Semantic<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) -> ReactivityLifetimeFacts {
  let (facts, stats) = collect_with_visits(semantic, line_index, sfc_source, script_offset);
  let inner_work = stats.identity_inner_work();
  let ownership_work = stats.work();
  debug_assert!(
    stats.total() < usize::MAX && inner_work < usize::MAX && ownership_work < usize::MAX,
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
  let mut identity_work = cleanup_identity::IdentityWork::default();
  cleanup_identity::index_written_symbols(semantic, &mut index, &mut identity_work);

  for (node_id, node) in semantic.nodes().iter_enumerated() {
    index.work.add_nodes(1);
    index::observe_node(semantic, node_id, node.kind(), &vue_exports, &mut resolver, &mut index);
  }
  let selection_work = index.finalize(semantic);
  let identity_index_work = index.identity.finalize();
  index.identity.finalize_payload_escapes(&mut identity_work);

  let mut facts = ReactivityLifetimeFacts::default();
  let emit_all = emit::emit_all(
    semantic,
    &index,
    &mut resolver,
    line_index,
    sfc_source,
    script_offset,
    &mut facts,
  );
  identity_work.absorb(cleanup_identity::emit(
    semantic,
    &index,
    &mut resolver,
    line_index,
    sfc_source,
    script_offset,
    &mut facts,
  ));
  facts.sort_by_source_order();
  #[cfg(test)]
  let stats = {
    let mut stats = index.work.snapshot();
    stats.emit_candidates = emit_all
      .saturating_add(selection_work)
      .saturating_add(identity_index_work)
      .saturating_add(identity_work.total);
    stats.inner_visits = stats.work();
    stats.identity_comparisons = identity_work.comparisons;
    stats.identity_registrations = identity_work.registrations;
    stats.identity_alias_work = identity_work.alias_work;
    stats.identity_construction = identity_work.construction;
    stats.identity_queries = identity_work.queries;
    stats.identity_copies = identity_work.copies;
    stats.identity_reference_visits = identity_work.reference_visits;
    stats
  };
  #[cfg(not(test))]
  let stats = {
    let _ = (emit_all, selection_work, identity_index_work, identity_work);
    index.work.snapshot()
  };
  (facts, stats)
}
