//! Await-position queries. The scan that fills `await_index` stays in `mod.rs`.

use oxc_semantic::{NodeId, SymbolId};
use oxc_span::Span;

use super::{
  AwaitPositionSite, DemandOrigin, Indexes, MemberUse, NamedUse, ObjectEntry, ValueWrite, span_key,
  timeline,
};

impl Indexes {
  pub(in crate::source_contracts) fn await_result_of_await(&self, span: Span) -> Option<SymbolId> {
    self.work.add_queries(1);
    self.await_index.results_by_await.get(&span_key(span)).copied()
  }

  pub(in crate::source_contracts) fn await_await_for_argument(
    &self,
    argument: Span,
  ) -> Option<AwaitPositionSite> {
    self.work.add_queries(1);
    self.await_index.await_by_argument.get(&span_key(argument)).copied()
  }

  pub(in crate::source_contracts) fn await_awaits_for_bound(
    &self,
    root: SymbolId,
  ) -> &[AwaitPositionSite] {
    self.work.add_queries(1);
    self.await_index.await_by_bound.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn await_awaits_by_region(
    &self,
    callable: Option<NodeId>,
    region: NodeId,
  ) -> &[AwaitPositionSite] {
    self.work.add_queries(1);
    self.await_index.awaits_by_region.get(&(callable, region)).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn await_has_await_between(
    &self,
    callable: Option<NodeId>,
    region: NodeId,
    start: usize,
    end: usize,
  ) -> bool {
    let sites = self.await_awaits_by_region(callable, region);
    !timeline::between(&self.work, sites, start, end).is_empty()
  }

  /// Later `await` expressions are stack-wide barriers. An until timeout
  /// result stays the same value across a later await, so those offsets
  /// must not hide a demand on an earlier settle.
  pub(in crate::source_contracts) fn await_demand_from(
    &self,
    site: &MemberUse,
    origin: DemandOrigin,
  ) -> bool {
    self.demand_ok(site)
      && site.callable == origin.callable
      && site.region == origin.region
      && self.await_interval_open(origin.callable, origin.region, origin.offset, site.offset)
  }

  pub(in crate::source_contracts) fn await_interval_open(
    &self,
    callable: Option<NodeId>,
    region: NodeId,
    start: usize,
    end: usize,
  ) -> bool {
    if !self.has_barrier_between(region, start, end) {
      return true;
    }
    self.await_has_await_between(callable, region, start, end)
      && !self.await_non_await_barrier_between(callable, region, start, end)
  }

  pub(in crate::source_contracts) fn await_non_await_barrier_between(
    &self,
    callable: Option<NodeId>,
    region: NodeId,
    start: usize,
    end: usize,
  ) -> bool {
    let Some(barriers) = self.barriers_by_region.get(&region) else {
      self.work.add_queries(1);
      return false;
    };
    barriers.between(&self.work, start, end).iter().any(|offset| {
      self.work.add_queries(1);
      !self.await_is_await_offset(callable, region, *offset)
    })
  }

  fn await_is_await_offset(&self, callable: Option<NodeId>, region: NodeId, offset: usize) -> bool {
    let sites = self.await_awaits_by_region(callable, region);
    sites.iter().any(|site| {
      self.work.add_queries(1);
      site.offset == offset
    })
  }

  pub(in crate::source_contracts) fn await_await_method_calls_on(
    &self,
    await_span: Span,
  ) -> &[NamedUse] {
    self.work.add_queries(1);
    self.await_index.await_method_calls.get(&span_key(await_span)).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn await_result_method_calls_on(
    &self,
    root: SymbolId,
  ) -> &[NamedUse] {
    self.work.add_queries(1);
    self.await_index.result_method_calls.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn result_reassigned(&self, root: SymbolId) -> bool {
    self.work.add_queries(1);
    self.reassigned.contains(&root)
  }

  pub(in crate::source_contracts) fn object_has_spread(&self, object_span: Span) -> bool {
    self.work.add_queries(1);
    self.objects.get(&span_key(object_span)).is_some_and(|entries| {
      entries.iter().any(|entry| {
        self.work.add_object_entries(1);
        matches!(entry, ObjectEntry::Spread)
      })
    })
  }

  pub(in crate::source_contracts) fn object_entries(&self, object_span: Span) -> &[ObjectEntry] {
    self.work.add_queries(1);
    self.objects.get(&span_key(object_span)).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn await_writes_in(
    &self,
    root: SymbolId,
    start: usize,
    end: usize,
  ) -> impl Iterator<Item = ValueWrite> + '_ {
    let writes = self.value_writes.get(&root).map_or(&[][..], Vec::as_slice);
    timeline::through(&self.work, timeline::after(&self.work, writes, start), end).iter().copied()
  }
}
