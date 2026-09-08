//! Adapter-only work counters for source-contract collection.
//!
//! `SourceContractStats::work` is completed collector work: Vue-import
//! node and specifier visits, per-node owner construction, alias-root
//! precompute and bounded compression, native constructor/prototype alias
//! map construction and identity resolution, the main scan,
//! reference-role indexing, unresolved leftover-escape span copy/sort/merge
//! and covering `partition_point` checks, unresolved native/global reference
//! covering, object-entry summary visits and per-property
//! max-index comparisons, watch-option unique-static-key inspections (each
//! own property, recognized or not), write-owner summary visits, the
//! fact-collection walk, diagnostic-ordering comparisons, query-time map
//! lookups, ancestor eligibility walks, and `partition_point` predicate executions. Shape classification
//! records one query per `classify_maybe`. Shared object summarization does
//! not treat computed literal keys as uncertain; watch options check the
//! original `ObjectProperty::computed` flag instead. Indexed actual-Proxy
//! import-source lookups (Vue constructor calls only) increment
//! `import_source_steps`. Demanded-key contains/hash lookups increment
//! `key_lookups`; remaining key-string copies increment `key_copies`.
//! Each examined `AssignmentTarget` in the native-clone poison walk, plus
//! `for...in` / `for...of` left classification, increments `queries`.
//! Assignment-form loop heads also increment `writes` once.
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
  /// Proven import-source examinations for Vue constructors that can allocate
  /// a Proxy. Non-Vue and unresolved calls must not increment this.
  pub import_source_steps: u64,
  pub key_lookups: u64,
  pub key_copies: u64,
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
      .saturating_add(self.import_source_steps)
      .saturating_add(self.key_lookups)
      .saturating_add(self.key_copies)
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
  #[cfg(test)]
  import_source_steps: Cell<u64>,
  #[cfg(test)]
  key_lookups: Cell<u64>,
  #[cfg(test)]
  key_copies: Cell<u64>,
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
  pub(super) fn add_import_source_steps(&self, n: u64) {
    self.import_source_steps.set(self.import_source_steps.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn add_import_source_steps(&self, n: u64) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) fn add_key_lookups(&self, n: u64) {
    self.key_lookups.set(self.key_lookups.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn add_key_lookups(&self, n: u64) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) fn add_key_copies(&self, n: u64) {
    self.key_copies.set(self.key_copies.get().saturating_add(n));
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn add_key_copies(&self, n: u64) {
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
  pub(super) const fn snapshot(&self) -> SourceContractStats {
    SourceContractStats {
      nodes: self.nodes.get(),
      owners: self.owners.get(),
      references: self.references.get(),
      object_entries: self.object_entries.get(),
      writes: self.writes.get(),
      queries: self.queries.get(),
      import_source_steps: self.import_source_steps.get(),
      key_lookups: self.key_lookups.get(),
      key_copies: self.key_copies.get(),
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
      import_source_steps: 0,
      key_lookups: 0,
      key_copies: 0,
    }
  }
}
