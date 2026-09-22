//! Adapter-only work counters for source-contract collection.
//!
//! `SourceContractStats::work` is completed collector work: Vue-import
//! node and specifier visits, effect-family import-map entries, ancestor
//! hops, TS/parenthesis wrapper peels, remaining argument-candidate
//! inspections, resolved-reference eligibility lookups, per-node owner construction, alias-root
//! precompute and bounded compression, native constructor/prototype alias
//! map construction and identity resolution, the main scan,
//! reference-role indexing, unresolved leftover-escape span copy/sort/merge
//! and covering `partition_point` checks, unresolved native/global reference
//! covering, object-entry summary visits and per-property
//! max-index comparisons, watch-option unique-static-key inspections (each
//! own property, recognized or not), write-owner summary visits, the
//! fact-collection walk, diagnostic-ordering comparisons, query-time map
//! lookups, ancestor eligibility walks, computed-identity projection queries,
//! and `partition_point` predicate executions. Shape classification records
//! one query per `classify_maybe`. Shared object summarization treats computed
//! literal keys as known; watch options check the original
//! `ObjectProperty::computed` flag instead. Indexed actual-Proxy import-source
//! lookups, demanded-key contains/hash lookups, and remaining key-string
//! copies increment `queries`. Each examined `AssignmentTarget`
//! in the native-clone poison walk, plus `for...in` / `for...of` left
//! classification, increments `queries`. Assignment-form loop heads also
//! increment `writes` once. Map-op / member-call / root / barrier index sort
//! comparisons, constructor-entry visits, keyed identity lookups, and mutation
//! visits also increment `queries`. Per-root Map replay counts
//! constructor classification once, each mutating operation once, and each
//! read query once. Cached-result producer/fill/write/repair/demand joins,
//! exclusive-interval iterator visits, counted `binary_search` comparisons,
//! and counted sort comparisons also increment `queries`. Class-body elements
//! count as object-entry scans; member-name and private-field lookups count
//! as key lookups. Injection key/provide/inject/demand joins increment
//! `queries` on the existing counter set. `VueUse` ignore-window and shared
//! first-instance joins reuse those same query counters. Snapshot-demand
//! Date-path, nested-write, and ordered history-operation visits also charge
//! the existing `queries` counter.
//!
//! Production `WorkCounter` is zero-sized and does not record. Test builds
//! keep saturating `Cell` counters so inner-work growth tests stay real.
//! `SourceContractStats` is a separate snapshot DTO: five `u64` fields
//! (`nodes`, `owners`, `object_entries`, `writes`, `queries`). Reference
//! walks, import-source steps, key lookups, and key copies charge `queries`.
//! The CLI does not report this DTO. Production and test layouts match.

#[cfg(test)]
use std::cell::Cell;
use std::mem::size_of;

/// Completed collector work. Not part of the stable Vue Vet fact contract.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SourceContractStats {
  pub nodes: u64,
  pub owners: u64,
  pub object_entries: u64,
  pub writes: u64,
  pub queries: u64,
}

const STATS_BYTES: usize = size_of::<[u64; 5]>();
const _: () = assert!(
  size_of::<SourceContractStats>() == STATS_BYTES,
  "SourceContractStats snapshot DTO is five u64 fields"
);

impl SourceContractStats {
  #[cfg(test)]
  #[must_use]
  pub const fn work(self) -> u64 {
    self
      .nodes
      .saturating_add(self.owners)
      .saturating_add(self.object_entries)
      .saturating_add(self.writes)
      .saturating_add(self.queries)
  }

  /// True when Vue-import preflight ran and owner, object, and write indexes stayed empty.
  #[cfg(test)]
  #[must_use]
  pub const fn is_import_preflight_only(self) -> bool {
    self.owners == 0 && self.object_entries == 0 && self.writes == 0
  }
}

#[derive(Default)]
pub(super) struct WorkCounter {
  #[cfg(test)]
  nodes: Cell<u64>,
  #[cfg(test)]
  owners: Cell<u64>,
  #[cfg(test)]
  object_entries: Cell<u64>,
  #[cfg(test)]
  writes: Cell<u64>,
  #[cfg(test)]
  queries: Cell<u64>,
}

#[cfg(not(test))]
const _: () = assert!(
  core::mem::size_of::<WorkCounter>() == 0,
  "production source-contract WorkCounter must stay zero-sized"
);

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

  pub(super) fn add_references(&self, n: u64) {
    self.add_queries(n);
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

  pub(super) fn add_import_source_steps(&self, n: u64) {
    self.add_queries(n);
  }

  pub(super) fn add_key_lookups(&self, n: u64) {
    self.add_queries(n);
  }

  pub(super) fn add_key_copies(&self, n: u64) {
    self.add_queries(n);
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

  /// Counted leftover-range ordering. `sort_unstable` would hide comparison work.
  #[cfg(test)]
  pub(super) fn sort_unstable<T: Ord>(&self, items: &mut [T]) {
    items.sort_unstable_by(|left, right| {
      self.add_queries(1);
      left.cmp(right)
    });
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    reason = "production path forwards to slice::sort_unstable without counting"
  )]
  pub(super) fn sort_unstable<T: Ord>(&self, items: &mut [T]) {
    items.sort_unstable();
  }

  #[cfg(test)]
  pub(super) fn sort_by_key<T, K, F>(&self, items: &mut [T], mut key: F)
  where
    K: Ord,
    F: FnMut(&T) -> K,
  {
    self.add_queries(1);
    items.sort_by(|left, right| {
      self.add_queries(1);
      key(left).cmp(&key(right))
    });
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    reason = "production path forwards to slice::sort_by_key without counting"
  )]
  pub(super) fn sort_by_key<T, K, F>(&self, items: &mut [T], key: F)
  where
    K: Ord,
    F: FnMut(&T) -> K,
  {
    items.sort_by_key(key);
  }

  #[cfg(test)]
  pub(super) const fn snapshot(&self) -> SourceContractStats {
    SourceContractStats {
      nodes: self.nodes.get(),
      owners: self.owners.get(),
      object_entries: self.object_entries.get(),
      writes: self.writes.get(),
      queries: self.queries.get(),
    }
  }

  #[cfg(not(test))]
  #[expect(clippy::unused_self, reason = "production snapshot is always zero")]
  pub(super) const fn snapshot(&self) -> SourceContractStats {
    SourceContractStats { nodes: 0, owners: 0, object_entries: 0, writes: 0, queries: 0 }
  }
}

#[cfg(test)]
mod size_tests {
  use super::{STATS_BYTES, SourceContractStats, WorkCounter};
  use std::mem::size_of;

  #[test]
  fn stats_dto_keeps_five_u64_layout() {
    assert_eq!(size_of::<SourceContractStats>(), STATS_BYTES);
    assert_eq!(STATS_BYTES, 40);
  }

  #[test]
  fn test_work_counter_records() {
    assert!(size_of::<WorkCounter>() > 0);
  }
}
