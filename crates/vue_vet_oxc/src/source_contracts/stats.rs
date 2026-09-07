//! Adapter-only work counters for source-contract collection.
//!
//! `SourceContractStats::work` is completed collector work: Vue-import
//! node and specifier visits, per-node owner construction, the main scan,
//! reference-role indexing, object-entry summary visits and per-property
//! max-index comparisons, write-owner summary visits, demand ancestor
//! reach/role steps, per-region barrier lookups, span-keyed member-call
//! lookups, predecessor stop summaries, memoized closed-key proofs, per-root
//! named-use lookups, the fact-collection walk, diagnostic-ordering
//! comparisons, query-time map lookups, `partition_point` predicate
//! executions, demanded-key contains/hash lookups, and remaining key-string
//! copies. Shape classification records one query per `classify_maybe`.

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
  pub key_lookups: u64,
  pub key_copies: u64,
}

impl SourceContractStats {
  #[must_use]
  #[cfg_attr(
    not(test),
    expect(dead_code, reason = "adapter tests assert counted work including key copies")
  )]
  pub const fn work(self) -> u64 {
    self
      .nodes
      .saturating_add(self.owners)
      .saturating_add(self.references)
      .saturating_add(self.object_entries)
      .saturating_add(self.writes)
      .saturating_add(self.queries)
      .saturating_add(self.key_lookups)
      .saturating_add(self.key_copies)
  }
}

#[derive(Default)]
pub(super) struct WorkCounter {
  nodes: Cell<u64>,
  owners: Cell<u64>,
  references: Cell<u64>,
  object_entries: Cell<u64>,
  writes: Cell<u64>,
  queries: Cell<u64>,
  key_lookups: Cell<u64>,
  key_copies: Cell<u64>,
}

impl WorkCounter {
  pub(super) fn add_nodes(&self, n: u64) {
    self.nodes.set(self.nodes.get().saturating_add(n));
  }

  pub(super) fn add_owners(&self, n: u64) {
    self.owners.set(self.owners.get().saturating_add(n));
  }

  pub(super) fn add_references(&self, n: u64) {
    self.references.set(self.references.get().saturating_add(n));
  }

  pub(super) fn add_object_entries(&self, n: u64) {
    self.object_entries.set(self.object_entries.get().saturating_add(n));
  }

  pub(super) fn add_writes(&self, n: u64) {
    self.writes.set(self.writes.get().saturating_add(n));
  }

  pub(super) fn add_queries(&self, n: u64) {
    self.queries.set(self.queries.get().saturating_add(n));
  }

  pub(super) fn add_key_lookups(&self, n: u64) {
    self.key_lookups.set(self.key_lookups.get().saturating_add(n));
  }

  pub(super) fn add_key_copies(&self, n: u64) {
    self.key_copies.set(self.key_copies.get().saturating_add(n));
  }

  /// Map lookup plus each comparison `slice::partition_point` actually runs.
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

  #[expect(clippy::missing_const_for_fn, reason = "Cell::get is not const")]
  pub(super) fn snapshot(&self) -> SourceContractStats {
    SourceContractStats {
      nodes: self.nodes.get(),
      owners: self.owners.get(),
      references: self.references.get(),
      object_entries: self.object_entries.get(),
      writes: self.writes.get(),
      queries: self.queries.get(),
      key_lookups: self.key_lookups.get(),
      key_copies: self.key_copies.get(),
    }
  }
}
