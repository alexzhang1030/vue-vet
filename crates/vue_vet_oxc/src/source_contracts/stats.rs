//! Adapter-only work counters for source-contract collection.
//!
//! `SourceContractStats::work` is completed collector work: Vue-import
//! node and specifier visits, per-node owner construction, the main scan,
//! reference-role indexing, object-entry summary visits and per-property
//! max-index comparisons, watch-option unique-static-key inspections (each
//! own property, recognized or not), write-owner summary visits, the
//! fact-collection walk, diagnostic-ordering comparisons, query-time map
//! lookups, and `partition_point` predicate executions. Shape classification
//! records one query per `classify_maybe`. Shared object summarization does
//! not treat computed literal keys as uncertain; watch options check the
//! original `ObjectProperty::computed` flag instead.
//!
//! Production `WorkCounter` is zero-sized and does not record. Test builds
//! keep saturating `Cell` counters so inner-work growth tests stay real.

#[cfg(test)]
use std::cell::Cell;

/// Completed collector work. Not part of the stable Vue Vet fact contract.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SourceContractStats {
  pub nodes: u64,
  pub owners: u64,
  pub references: u64,
  pub object_entries: u64,
  pub writes: u64,
  pub queries: u64,
}

impl SourceContractStats {
  #[cfg(test)]
  #[must_use]
  pub const fn work(self) -> u64 {
    self
      .nodes
      .saturating_add(self.owners)
      .saturating_add(self.references)
      .saturating_add(self.object_entries)
      .saturating_add(self.writes)
      .saturating_add(self.queries)
  }

  /// True when only the Vue-import preflight ran (no owners, writes, objects, or queries).
  #[cfg(test)]
  #[must_use]
  pub const fn is_import_preflight_only(self) -> bool {
    self.owners == 0 && self.object_entries == 0 && self.writes == 0 && self.queries == 0
  }
}

#[derive(Default)]
pub(super) struct WorkCounter {
  #[cfg(test)]
  nodes: Cell<u64>,
  #[cfg(test)]
  owners: Cell<u64>,
  #[cfg(test)]
  references: Cell<u64>,
  #[cfg(test)]
  object_entries: Cell<u64>,
  #[cfg(test)]
  writes: Cell<u64>,
  #[cfg(test)]
  queries: Cell<u64>,
}

impl WorkCounter {
  #[cfg(test)]
  pub(super) fn add_nodes(&self, n: u64) {
    self.nodes.set(self.nodes.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn add_nodes(&self, n: u64) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) fn add_owners(&self, n: u64) {
    self.owners.set(self.owners.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn add_owners(&self, n: u64) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) fn add_references(&self, n: u64) {
    self.references.set(self.references.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn add_references(&self, n: u64) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) fn add_object_entries(&self, n: u64) {
    self.object_entries.set(self.object_entries.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn add_object_entries(&self, n: u64) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) fn add_writes(&self, n: u64) {
    self.writes.set(self.writes.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn add_writes(&self, n: u64) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) fn add_queries(&self, n: u64) {
    self.queries.set(self.queries.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn add_queries(&self, n: u64) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) fn partition_point<T, F>(&self, items: &[T], mut predicate: F) -> usize
  where
    F: FnMut(&T) -> bool,
  {
    self.add_queries(1);
    items.partition_point(|item| {
      self.add_queries(1);
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
  pub(super) const fn snapshot(&self) -> SourceContractStats {
    SourceContractStats {
      nodes: self.nodes.get(),
      owners: self.owners.get(),
      references: self.references.get(),
      object_entries: self.object_entries.get(),
      writes: self.writes.get(),
      queries: self.queries.get(),
    }
  }

  #[cfg(not(test))]
  #[expect(clippy::unused_self, reason = "production snapshot is always zero")]
  pub(super) const fn snapshot(&self) -> SourceContractStats {
    SourceContractStats {
      nodes: 0,
      owners: 0,
      references: 0,
      object_entries: 0,
      writes: 0,
      queries: 0,
    }
  }
}
