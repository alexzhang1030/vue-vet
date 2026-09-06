//! Adapter-only work counters for source-contract collection.
//!
//! `SourceContractStats::work` is completed collector work: Vue-import
//! node and specifier visits, per-node owner construction, the main scan,
//! reference-role indexing, object-entry summary visits and per-property
//! max-index comparisons, write-owner summary visits, the fact-collection
//! walk, diagnostic-ordering comparisons, query-time map lookups, and
//! `partition_point` predicate executions. Shape classification records one
//! query per `classify_maybe`. Indexed actual-Proxy import-source lookups
//! (Vue constructor calls only) increment `import_source_steps`. Each
//! examined `AssignmentTarget` in the native-clone poison walk, plus
//! `for...in` / `for...of` left classification, increments `queries`.
//! Assignment-form loop heads also increment `writes` once.

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
}

impl SourceContractStats {
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
  import_source_steps: Cell<u64>,
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

  pub(super) fn add_import_source_steps(&self, n: u64) {
    self.import_source_steps.set(self.import_source_steps.get().saturating_add(n));
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
      import_source_steps: self.import_source_steps.get(),
    }
  }
}
