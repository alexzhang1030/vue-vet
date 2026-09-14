//! Adapter-only work counters for lifetime collection.
//!
//! Production `WorkCounter` is zero-sized and does not record. Test builds
//! keep saturating `Cell` counters so geometric growth tests measure real
//! statement, reference, watcher, toggle, and computed-edge inspections —
//! including index construction on quiet retained-scope corpora.
//! Identity-path counters live on the same snapshot type so cleanup-identity
//! and ownership share one `CollectStats`.

#[cfg(test)]
use std::cell::Cell;

/// Completed lifetime collector work. Not part of the stable fact contract.
///
/// Production is a ZST. Test builds carry ownership work counters plus
/// cleanup-identity inner-work fields.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CollectStats {
  #[cfg(test)]
  pub index_nodes: usize,
  #[cfg(test)]
  pub emit_candidates: usize,
  #[cfg(test)]
  pub inner_visits: usize,
  #[cfg(test)]
  pub statements: usize,
  #[cfg(test)]
  pub references: usize,
  #[cfg(test)]
  pub watchers: usize,
  #[cfg(test)]
  pub toggles: usize,
  #[cfg(test)]
  pub computed_edges: usize,
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
  #[cfg(test)]
  pub settlement_ast: usize,
  #[cfg(test)]
  pub settlement_references: usize,
  #[cfg(test)]
  pub settlement_wrappers: usize,
  #[cfg(test)]
  pub settlement_aliases: usize,
  #[cfg(test)]
  pub settlement_joins: usize,
  #[cfg(test)]
  pub settlement_sorts: usize,
  #[cfg(test)]
  pub settlement_queries: usize,
}

impl CollectStats {
  #[must_use]
  pub const fn total(self) -> usize {
    #[cfg(test)]
    {
      self.index_nodes.saturating_add(self.emit_candidates).saturating_add(self.inner_visits)
    }
    #[cfg(not(test))]
    {
      let _ = self;
      0
    }
  }

  /// Statement / reference / watcher / toggle / computed-edge inspections, including index work.
  #[must_use]
  pub const fn work(self) -> usize {
    #[cfg(test)]
    {
      self
        .statements
        .saturating_add(self.references)
        .saturating_add(self.watchers)
        .saturating_add(self.toggles)
        .saturating_add(self.computed_edges)
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

  #[must_use]
  pub const fn settlement_inner_work(self) -> usize {
    #[cfg(test)]
    {
      self
        .settlement_ast
        .saturating_add(self.settlement_references)
        .saturating_add(self.settlement_wrappers)
        .saturating_add(self.settlement_aliases)
        .saturating_add(self.settlement_joins)
        .saturating_add(self.settlement_sorts)
        .saturating_add(self.settlement_queries)
    }
    #[cfg(not(test))]
    {
      let _ = self;
      0
    }
  }
}

#[derive(Default)]
pub(super) struct WorkCounter {
  #[cfg(test)]
  nodes: Cell<usize>,
  #[cfg(test)]
  statements: Cell<usize>,
  #[cfg(test)]
  references: Cell<usize>,
  #[cfg(test)]
  watchers: Cell<usize>,
  #[cfg(test)]
  toggles: Cell<usize>,
  #[cfg(test)]
  computed_edges: Cell<usize>,
}

#[cfg(not(test))]
const _: () = {
  assert!(
    core::mem::size_of::<WorkCounter>() == 0,
    "production lifetime WorkCounter must stay zero-sized"
  );
};

impl WorkCounter {
  #[cfg(test)]
  pub(super) fn add_nodes(&self, n: usize) {
    self.nodes.set(self.nodes.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn add_nodes(&self, n: usize) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) fn add_statements(&self, n: usize) {
    self.statements.set(self.statements.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn add_statements(&self, n: usize) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) fn add_references(&self, n: usize) {
    self.references.set(self.references.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn add_references(&self, n: usize) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) fn add_watchers(&self, n: usize) {
    self.watchers.set(self.watchers.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn add_watchers(&self, n: usize) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) fn add_toggles(&self, n: usize) {
    self.toggles.set(self.toggles.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn add_toggles(&self, n: usize) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) fn add_computed_edges(&self, n: usize) {
    self.computed_edges.set(self.computed_edges.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn add_computed_edges(&self, n: usize) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) fn partition_point<T, F>(&self, items: &[T], mut predicate: F) -> usize
  where
    F: FnMut(&T) -> bool,
  {
    self.add_statements(1);
    items.partition_point(|item| {
      self.add_statements(1);
      predicate(item)
    })
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    reason = "production path forwards to slice::partition_point without counting"
  )]
  pub(super) fn partition_point<T, F>(&self, items: &[T], predicate: F) -> usize
  where
    F: FnMut(&T) -> bool,
  {
    items.partition_point(predicate)
  }

  #[cfg(test)]
  pub(super) const fn snapshot(&self) -> CollectStats {
    let statements = self.statements.get();
    let references = self.references.get();
    let watchers = self.watchers.get();
    let toggles = self.toggles.get();
    let computed_edges = self.computed_edges.get();
    CollectStats {
      index_nodes: self.nodes.get(),
      emit_candidates: 0,
      inner_visits: statements
        .saturating_add(references)
        .saturating_add(watchers)
        .saturating_add(toggles)
        .saturating_add(computed_edges),
      statements,
      references,
      watchers,
      toggles,
      computed_edges,
      identity_comparisons: 0,
      identity_registrations: 0,
      identity_alias_work: 0,
      identity_construction: 0,
      identity_queries: 0,
      identity_copies: 0,
      identity_reference_visits: 0,
      settlement_ast: 0,
      settlement_references: 0,
      settlement_wrappers: 0,
      settlement_aliases: 0,
      settlement_joins: 0,
      settlement_sorts: 0,
      settlement_queries: 0,
    }
  }

  #[cfg(not(test))]
  #[expect(clippy::unused_self, reason = "production snapshot is always zero")]
  pub(super) const fn snapshot(&self) -> CollectStats {
    CollectStats {}
  }
}
