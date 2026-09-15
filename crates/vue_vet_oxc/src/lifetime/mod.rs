//! Watcher and effect-scope lifetime facts.

mod cleanup_identity;
mod emit;
mod index;
mod resolve;

use vue_vet_core::ReactivityLifetimeFacts;

use self::index::LifetimeIndex;
use self::resolve::FunctionResolver;

#[derive(Clone, Copy, Debug, Default)]
pub struct CollectStats {
  #[cfg(test)]
  pub index_nodes: usize,
  #[cfg(test)]
  pub emit_candidates: usize,
  #[cfg(test)]
  pub identity_comparisons: usize,
  #[cfg(test)]
  pub identity_registrations: usize,
  #[cfg(test)]
  pub identity_alias_work: usize,
  #[cfg(test)]
  pub identity_construction: usize,
  #[cfg(test)]
  pub identity_queries: usize,
  #[cfg(test)]
  pub identity_copies: usize,
  #[cfg(test)]
  pub identity_reference_visits: usize,
}

impl CollectStats {
  #[must_use]
  pub const fn total(self) -> usize {
    #[cfg(test)]
    {
      self.index_nodes.saturating_add(self.emit_candidates)
    }
    #[cfg(not(test))]
    {
      let _ = self;
      0
    }
  }

  #[must_use]
  pub const fn identity_inner_work(self) -> usize {
    #[cfg(test)]
    {
      self
        .identity_comparisons
        .saturating_add(self.identity_registrations)
        .saturating_add(self.identity_alias_work)
        .saturating_add(self.identity_construction)
        .saturating_add(self.identity_queries)
        .saturating_add(self.identity_copies)
        .saturating_add(self.identity_reference_visits)
    }
    #[cfg(not(test))]
    {
      let _ = self;
      0
    }
  }
}

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
  debug_assert!(
    stats.total() < usize::MAX && inner_work < usize::MAX,
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
  #[cfg(test)]
  let mut index_nodes = 0usize;

  for (node_id, node) in semantic.nodes().iter_enumerated() {
    #[cfg(test)]
    {
      index_nodes += 1;
    }
    index::observe_node(semantic, node_id, node.kind(), &vue_exports, &mut resolver, &mut index);
  }
  let selection_work = index.finalize_selected_runs();
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
    let emit_candidates = emit_all
      .saturating_add(selection_work)
      .saturating_add(identity_index_work)
      .saturating_add(identity_work.total);
    CollectStats {
      index_nodes,
      emit_candidates,
      identity_comparisons: identity_work.comparisons,
      identity_registrations: identity_work.registrations,
      identity_alias_work: identity_work.alias_work,
      identity_construction: identity_work.construction,
      identity_queries: identity_work.queries,
      identity_copies: identity_work.copies,
      identity_reference_visits: identity_work.reference_visits,
    }
  };
  #[cfg(not(test))]
  let stats = {
    let _ = (emit_all, selection_work, identity_index_work, identity_work);
    CollectStats::default()
  };
  (facts, stats)
}
