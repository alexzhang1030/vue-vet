//! Read-only index queries. The scan that fills these maps stays in `mod.rs`.

use std::collections::HashMap;

use oxc_semantic::{NodeId, SymbolId};
use oxc_span::Span;
use vue_vet_core::ScriptKind;

use super::{
  ArgUse, AwaitClosedSource, AwaitPositionSite, AwaitSite, CAPABILITY_KEYS, CallInfo, CallUse,
  ClassNewInfo, CollectionCtor, DemandOrigin, DemandRole, DisposeSite, EffectCallback,
  ExtractedMethod, FunctionInfo, HintClass, Indexes, InjectionSite, Literal, MemberCall,
  MemberRecord, MemberUse, MemberWrite, NamedUse, NativeSymbol, NestedWrite, ObjectEntry,
  ObjectProp, OptionValue, PathCall, PathRead, PathWrite, PrimitiveKind, RefInitLookup,
  ResultDemand, Scalar, ShapeHint, ShapePrimitiveAtom, SnapshotCall, SourceContractStats, Timeline,
  ValueDemand, ValueRead, ValueReadRole, ValueWrite, WatchConsumer, WatchConsumerOptions,
  chain_optional, intern_native_ctor, region_of, span_key, timeline,
};

impl Indexes {
  pub(in crate::source_contracts) fn has_foreign_event_between(
    &self,
    block: NodeId,
    start: usize,
    end: usize,
  ) -> bool {
    let Some(events) = self.events_by_block.get(&block) else {
      self.note_query();
      return false;
    };
    events.count_between(&self.work, start, end)
      > self.allowed_offsets.count_between(&self.work, start, end)
  }

  pub(in crate::source_contracts) fn sort_timeline<T, K, F>(&self, items: &mut [T], key: F)
  where
    K: Ord,
    F: FnMut(&T) -> K,
  {
    self.work.sort_by_key(items, key);
  }

  pub(in crate::source_contracts) fn identifier_calls_on(&self, root: SymbolId) -> &[CallUse] {
    self.note_query();
    self.identifier_calls.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn result_demands_on(&self, root: SymbolId) -> &[ResultDemand] {
    self.note_query();
    self.result_demands.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn value_demands_on(&self, root: SymbolId) -> &[ValueDemand] {
    self.note_query();
    self.value_demands.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn value_writes_on(&self, root: SymbolId) -> &[ValueWrite] {
    self.value_writes_of(root)
  }

  pub(in crate::source_contracts) fn value_reads_on(&self, root: SymbolId) -> &[MemberUse] {
    self.note_query();
    self.value_reads.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn class_identity_intact(&self, class: SymbolId) -> bool {
    self.note_query();
    let Some(record) = self.classes.get(&class) else {
      return false;
    };
    record.ordinary
      && !self.reassigned.contains(&class)
      && !self.escaped.contains(&class)
      && !self.prototype_touch.contains(&class)
      && !self.unknown_member_touch.contains(&class)
      && !self.capability_touch.contains(&class)
  }

  pub(in crate::source_contracts) fn class_declared_before(
    &self,
    class: SymbolId,
    new_span: Span,
  ) -> bool {
    self.note_query();
    self.classes.get(&class).is_some_and(|record| record.span.start < new_span.start)
  }

  pub(in crate::source_contracts) fn class_member(
    &self,
    class: SymbolId,
    name: &str,
  ) -> Option<&MemberRecord> {
    self.note_query();
    self.work.add_key_lookups(1);
    self.classes.get(&class).and_then(|record| record.members.get(name))
  }

  pub(in crate::source_contracts) fn new_at(&self, span: Span) -> Option<ClassNewInfo> {
    self.note_query();
    self.class_news.get(&span_key(span)).copied()
  }

  pub(in crate::source_contracts) fn is_optional_chain(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
  ) -> bool {
    chain_optional(semantic, node_id, &self.work)
  }

  pub(in crate::source_contracts) fn ctor_shadowed(&self, name: &str) -> bool {
    self.note_query();
    self.shadowed_ctors.contains(name)
  }

  pub(in crate::source_contracts) fn native_capability_intact(&self) -> bool {
    self.note_query();
    !self.prototype_mutated
      && !self.ctor_shadowed("String")
      && !self.ctor_shadowed("Number")
      && !self.ctor_shadowed("Boolean")
      && !self.ctor_shadowed("BigInt")
      && !self.ctor_shadowed("Object")
      && !self.ctor_shadowed("Symbol")
  }

  pub(in crate::source_contracts) fn setup_lane(&self) -> bool {
    self.note_query();
    self.script_kind == ScriptKind::Setup
  }

  /// Optional member (`count?.toFixed`) guards only a nullish fallback.
  /// Optional *call* (`toFixed?.()`) always guards.
  pub(in crate::source_contracts) fn injection_demand_from(
    &self,
    site: &MemberUse,
    origin: DemandOrigin,
    fallback: PrimitiveKind,
  ) -> bool {
    self.note_query();
    if !site.head.reach.is_straight() || site.call_optional {
      return false;
    }
    if site.optional && fallback == PrimitiveKind::Nullish {
      return false;
    }
    site.head.callable == origin.callable
      && site.head.region == origin.region
      && self.await_interval_open(origin.callable, origin.region, origin.offset, site.head.offset)
  }

  pub(in crate::source_contracts) fn inject_site(
    &self,
    key: SymbolId,
    offset: usize,
    node_id: NodeId,
  ) -> Option<InjectionSite> {
    let sites = self.injects_on(key);
    timeline::from(&self.work, sites, offset)
      .first()
      .copied()
      .filter(|site| site.node_id == node_id)
  }

  pub(in crate::source_contracts) fn native_symbol(&self, root: SymbolId) -> Option<NativeSymbol> {
    self.note_query();
    self.native_symbols.get(&root).copied()
  }

  pub(in crate::source_contracts) fn provides_on(&self, root: SymbolId) -> &[InjectionSite] {
    self.note_query();
    self.provides_by_key.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn injects_on(&self, root: SymbolId) -> &[InjectionSite] {
    self.note_query();
    self.injects_by_key.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn result_binding_intact(&self, root: SymbolId) -> bool {
    self.note_query();
    !self.reassigned.contains(&root)
      && !self.escaped.contains(&root)
      && !self.unknown_member_touch.contains(&root)
      && !self.capability_touch.contains(&root)
  }

  pub(in crate::source_contracts) fn function(&self, span: Span) -> Option<&FunctionInfo> {
    self.note_query();
    self.functions.get(&span_key(span))
  }

  pub(in crate::source_contracts) fn arg_uses_of(&self, root: SymbolId) -> &[ArgUse] {
    self.note_query();
    self.arg_uses.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn value_reads_of(&self, root: SymbolId) -> &[ValueRead] {
    self.note_query();
    self.reads.of(root, ValueReadRole::CustomRef)
  }

  pub(in crate::source_contracts) fn value_writes_of(&self, root: SymbolId) -> &[ValueWrite] {
    self.note_query();
    self.value_writes.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn value_write_events(&self, root: SymbolId) -> &[ValueWrite] {
    self.note_query();
    self.value_write_events.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn value_writes_for(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
  ) -> Option<&[ValueWrite]> {
    self.note_query();
    self.value_writes_by_callable.get(&(root, callable)).map(Vec::as_slice)
  }

  pub(in crate::source_contracts) fn last_value_write_in(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    before: usize,
  ) -> Option<ValueWrite> {
    let writes = self.value_writes_for(root, callable)?;
    timeline::last_before(&self.work, writes, before).copied()
  }

  pub(in crate::source_contracts) fn has_simple_value_write_between(
    &self,
    root: SymbolId,
    start: usize,
    end: usize,
  ) -> bool {
    let Some(writes) = self.value_writes.get(&root) else {
      self.note_query();
      return false;
    };
    timeline::after(&self.work, writes, start).iter().any(|write| {
      self.note_query();
      write.offset < end && write.simple_assign
    })
  }

  pub(in crate::source_contracts) fn call_result(&self, call_span: Span) -> Option<SymbolId> {
    self.note_query();
    self.call_results.get(&span_key(call_span)).copied()
  }

  pub(in crate::source_contracts) fn watch_options_of(
    &self,
    call_span: Span,
    api: Option<&str>,
  ) -> WatchConsumerOptions {
    self.note_query();
    self
      .watch_options
      .get(&span_key(call_span))
      .copied()
      .unwrap_or_else(|| WatchConsumerOptions::default_for(api))
  }

  pub(in crate::source_contracts) fn symbols_of_root(&self, root: SymbolId) -> &[SymbolId] {
    self.note_query();
    self.root_members.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn first_inactivity_after(
    &self,
    handle_root: SymbolId,
    block: NodeId,
    after: usize,
  ) -> Option<usize> {
    let Some(events) = self.inactivity_by_handle_block.get(&(handle_root, block)) else {
      self.note_query();
      return None;
    };
    timeline::first_after(&self.work, events, after).map(|event| event.offset)
  }

  pub(in crate::source_contracts) fn has_member_mutation(&self, root: SymbolId) -> bool {
    self.note_query();
    self.member_write_roots.contains(&root) || self.unknown_member_touch.contains(&root)
  }

  pub(in crate::source_contracts) const fn stats(&self) -> SourceContractStats {
    self.work.snapshot()
  }

  pub(in crate::source_contracts) fn root_of(&self, symbol_id: SymbolId) -> SymbolId {
    self.alias_root.get(&symbol_id).copied().unwrap_or(symbol_id)
  }

  pub(in crate::source_contracts) fn alias_members(&self, root: SymbolId) -> &[SymbolId] {
    self.note_query();
    self.aliases_of.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn payload_uncertain(&self, symbol_id: SymbolId) -> bool {
    let root = self.root_of(symbol_id);
    self.uncertain.contains(&root)
      || self.escaped.contains(&root)
      || self.reassigned.contains(&root)
      || self.unknown_member_touch.contains(&root)
  }

  pub(in crate::source_contracts) fn toref_identity_unproven(&self, symbol_id: SymbolId) -> bool {
    let root = self.root_of(symbol_id);
    self.reassigned.contains(&root)
      || self.unknown_member_touch.contains(&root)
      || self.toref_identity_uncertain.contains(&root)
      || self.toref_helper_escape.contains(&root)
  }

  /// `(lo, hi)`-exclusive membership on one keyed timeline; a missing key
  /// still charges a query so lane growth stays observable.
  pub(super) fn timeline_has_between<K: std::hash::Hash + Eq>(
    &self,
    timelines: &HashMap<K, Timeline>,
    key: &K,
    lo: usize,
    hi: usize,
  ) -> bool {
    let Some(events) = timelines.get(key) else {
      self.note_query();
      return false;
    };
    events.has_between(&self.work, lo, hi)
  }

  pub(in crate::source_contracts) fn has_pause_between(
    &self,
    block: NodeId,
    start: usize,
    end: usize,
  ) -> bool {
    self.timeline_has_between(&self.pause_events_by_block, &block, start, end)
  }

  pub(in crate::source_contracts) fn next_control_after(
    &self,
    block: NodeId,
    offset: usize,
  ) -> usize {
    let Some(events) = self.control_events_by_block.get(&block) else {
      self.note_query();
      return usize::MAX;
    };
    events.first_after(&self.work, offset).unwrap_or(usize::MAX)
  }

  pub(in crate::source_contracts) fn has_control_event_between(
    &self,
    block: NodeId,
    start: usize,
    end: usize,
  ) -> bool {
    self.timeline_has_between(&self.control_events_by_block, &block, start, end)
  }

  pub(in crate::source_contracts) fn has_event_between(
    &self,
    block: NodeId,
    start: usize,
    end: usize,
  ) -> bool {
    self.timeline_has_between(&self.events_by_block, &block, start, end)
  }

  pub(in crate::source_contracts) fn first_value_write_after(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    block: NodeId,
    offset: usize,
  ) -> Option<ValueWrite> {
    if self.mixed_value_owners.contains(&root) {
      self.note_query();
      return None;
    }
    let Some(writes) = self.value_writes.get(&root) else {
      self.note_query();
      return None;
    };
    let next = timeline::first_after(&self.work, writes, offset).copied()?;
    (next.simple_assign && next.fresh_alloc && next.callable == callable && next.block == block)
      .then_some(next)
  }

  pub(in crate::source_contracts) fn last_value_write_before(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    block: NodeId,
    offset: usize,
  ) -> Option<ValueWrite> {
    if self.mixed_value_owners.contains(&root) {
      self.note_query();
      return None;
    }
    let Some(writes) = self.value_writes.get(&root) else {
      self.note_query();
      return None;
    };
    let prior = timeline::last_before(&self.work, writes, offset).copied()?;
    (prior.callable == callable && prior.block == block).then_some(prior)
  }

  pub(in crate::source_contracts) fn value_writes_mixed(&self, root: SymbolId) -> bool {
    self.note_query();
    self.mixed_value_owners.contains(&root)
  }

  pub(in crate::source_contracts) fn first_value_read_after(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    block: NodeId,
    offset: usize,
  ) -> Option<ValueRead> {
    self.note_query();
    let reads = self.reads.of(root, ValueReadRole::Derivation);
    timeline::after(&self.work, reads, offset).iter().copied().find(|read| {
      self.note_query();
      read.callable == callable && read.block == block
    })
  }

  pub(in crate::source_contracts) fn site_owner(
    &self,
    node_id: NodeId,
  ) -> (Option<NodeId>, Option<NodeId>) {
    self.note_query();
    let owner = self.owner(node_id);
    (owner.callable, owner.block)
  }

  /// One query, then the shared primitive-hint classification.
  ///
  /// Depth `0` and a missing hint are [`HintClass::Other`]. The caller applies
  /// its own symbol policy on [`HintClass::Follow`]. Filter keeps an atom
  /// pre-check and calls [`ShapeHint::classify_primitive`] itself.
  pub(in crate::source_contracts) fn primitive_kind_step(
    &self,
    span: Span,
    remaining: u8,
  ) -> HintClass {
    self.note_query();
    if remaining == 0 {
      return HintClass::Other;
    }
    self.hints.get(&span_key(span)).copied().map_or(HintClass::Other, ShapeHint::classify_primitive)
  }

  pub(in crate::source_contracts) fn primitive_at(&self, span: Span) -> Option<ShapePrimitiveAtom> {
    self.note_query();
    self.primitives.get(&span_key(span)).copied()
  }

  pub(in crate::source_contracts) fn atoms_object_is(
    &self,
    left: ShapePrimitiveAtom,
    right: ShapePrimitiveAtom,
  ) -> bool {
    self.note_query();
    left.object_is(right, &self.interned)
  }

  pub(in crate::source_contracts) fn atoms_js_strict_eq(
    &self,
    left: ShapePrimitiveAtom,
    right: ShapePrimitiveAtom,
  ) -> bool {
    self.note_query();
    left.js_strict_eq(right, &self.interned)
  }

  pub(in crate::source_contracts) fn scheduling_value_reads_of(
    &self,
    root: SymbolId,
  ) -> &[ValueRead] {
    self.note_query();
    self.reads.of(root, ValueReadRole::Scheduling)
  }

  pub(in crate::source_contracts) fn member_calls_of(&self, root: SymbolId) -> &[MemberCall] {
    self.note_query();
    self.reads.scheduling_calls.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn awaits_of(&self, callable: Option<NodeId>) -> &[AwaitSite] {
    self.note_query();
    self.awaits_by_callable.get(&callable).map_or(&[], Vec::as_slice)
  }

  /// Straight-line awaits inside `callable`. Ignore-window proof uses this
  /// list, not `has_barrier_between`, so an updater `await` stays a signal
  /// rather than a stack-wide barrier that would hide the later write.
  pub(in crate::source_contracts) fn straight_awaits_in(
    &self,
    callable: Option<NodeId>,
  ) -> impl Iterator<Item = AwaitPositionSite> + '_ {
    self.note_query();
    self.await_index.awaits.iter().copied().filter(move |site| {
      self.note_query();
      site.head.callable == callable && site.head.reach.is_straight()
    })
  }

  pub(in crate::source_contracts) fn function_id(&self, span: Span) -> Option<NodeId> {
    self.note_query();
    self.callables.get(&span_key(span)).copied()
  }

  pub(in crate::source_contracts) fn scalar(&self, span: Span) -> Option<Scalar> {
    self.note_query();
    self.await_index.scalars.get(&span_key(span)).copied()
  }

  pub(in crate::source_contracts) fn call_bindings(
    &self,
    call_span: Span,
  ) -> &[(SymbolId, String)] {
    self.note_query();
    self.call_destructure.get(&span_key(call_span)).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn call_info(&self, span: Span) -> Option<CallInfo> {
    self.note_query();
    self.calls.get(&span_key(span)).copied()
  }

  pub(in crate::source_contracts) fn await_scalar(&self, span: Span) -> Option<Scalar> {
    self.note_query();
    self.await_index.scalars.get(&span_key(span)).copied()
  }

  /// Scalar stored in a `ref` initializer.
  ///
  /// `Closed` is the until-timeout walk: a closed source, a missing argument
  /// is nullish, and the lookup is `await_scalar`. `Direct` is the `VueUse` walk:
  /// the init span itself, a hint-call only, a missing argument is absent, and
  /// the lookup is `scalar`. The two walks do not charge the same queries.
  pub(in crate::source_contracts) fn ref_init_scalar(
    &self,
    root: SymbolId,
    lookup: RefInitLookup,
  ) -> Option<Scalar> {
    match lookup {
      RefInitLookup::Closed => {
        let summary = self.await_closed_source(root)?;
        let info = self.call_info(summary.init_span).or_else(|| {
          let ShapeHint::Call(call_span) = self.hints.get(&span_key(summary.init_span)).copied()?
          else {
            return None;
          };
          self.call_info(call_span)
        })?;
        info.first_arg.map_or(Some(Scalar::Nullish), |argument| self.await_scalar(argument))
      }
      RefInitLookup::Direct => {
        let init = self.init_span.get(&root).copied()?;
        let ShapeHint::Call(call_span) = self.hints.get(&span_key(init)).copied()? else {
          return None;
        };
        let info = self.calls.get(&span_key(call_span)).copied()?;
        info.first_arg.and_then(|argument| self.scalar(argument))
      }
    }
  }

  pub(in crate::source_contracts) fn await_closed_source(
    &self,
    root: SymbolId,
  ) -> Option<AwaitClosedSource> {
    self.note_query();
    self.await_index.closed_sources.get(&root).copied()
  }

  pub(in crate::source_contracts) fn result_of_call(&self, span: Span) -> Option<SymbolId> {
    self.note_query();
    self.call_results.get(&span_key(span)).copied()
  }

  pub(in crate::source_contracts) fn path_calls_on(&self, root: SymbolId) -> &[PathCall] {
    self.note_query();
    self.path_calls.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn path_reads_on(&self, root: SymbolId) -> &[PathRead] {
    self.note_query();
    self.path_reads.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn path_value_writes_on(
    &self,
    root: SymbolId,
  ) -> &[(Vec<String>, PathWrite)] {
    self.note_query();
    self.path_value_writes.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn nested_writes_on(
    &self,
    root: SymbolId,
  ) -> &[(String, NestedWrite)] {
    self.note_query();
    self.nested_writes.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn literal_at(&self, span: Span) -> Option<Literal> {
    self.note_query();
    self.object_index.literals.get(&span_key(span)).copied()
  }

  pub(in crate::source_contracts) fn object_is_closed_data(&self, span: Span) -> bool {
    self.note_query();
    let Some(entries) = self.object_index.objects.get(&span_key(span)) else {
      return false;
    };
    entries.iter().all(|entry| {
      self.work.add_object_entries(1);
      matches!(entry, ObjectEntry::Data { .. })
    })
  }

  pub(in crate::source_contracts) fn object_has_key_named(&self, span: Span, name: &str) -> bool {
    self.note_query();
    let Some(entries) = self.object_index.objects.get(&span_key(span)) else {
      return false;
    };
    entries.iter().any(|entry| {
      self.work.add_object_entries(1);
      match entry {
        ObjectEntry::Data { name: key, .. } | ObjectEntry::Accessor { name: Some(key) } => {
          key == name
        }
        _ => false,
      }
    })
  }

  #[expect(dead_code, reason = "array literals remain indexed for closed-object proofs")]
  pub(in crate::source_contracts) fn array_elements(&self, span: Span) -> Option<&[Span]> {
    self.note_query();
    self.array_elements.get(&span_key(span)).map(Vec::as_slice)
  }

  pub(in crate::source_contracts) fn is_native_date(&self, span: Span) -> bool {
    self.note_query();
    !self.date_poisoned && self.dates.contains(&span_key(span))
  }

  pub(in crate::source_contracts) fn ident_demand(
    &self,
    site: &SnapshotCall,
    origin: DemandOrigin,
  ) -> bool {
    self.note_query();
    site.head.reach.is_straight()
      && !site.optional
      && site.head.callable == origin.callable
      && site.head.region == origin.region
      && !self.has_barrier_between(origin.region, origin.offset, site.head.offset)
  }

  pub(in crate::source_contracts) fn nested_demand(
    &self,
    write: &NestedWrite,
    origin: DemandOrigin,
  ) -> bool {
    self.note_query();
    write.simple_assign
      && write.callable == origin.callable
      && write.region == origin.region
      && write.offset > origin.offset
      && !self.has_barrier_between(origin.region, origin.offset, write.offset)
  }

  pub(in crate::source_contracts) fn disposals_of(
    &self,
    callable: Option<NodeId>,
  ) -> &[DisposeSite] {
    self.note_query();
    self.disposals_by_callable.get(&callable).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn watch_consumers_of(&self, root: SymbolId) -> &[WatchConsumer] {
    self.note_query();
    self.watches_by_source.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn effect_callback(
    &self,
    callable: NodeId,
  ) -> Option<EffectCallback> {
    self.note_query();
    self.effect_callbacks.get(&callable).copied()
  }

  pub(in crate::source_contracts) fn run_scope_of(&self, callback: NodeId) -> Option<SymbolId> {
    self.note_query();
    self.run_callback_scope.get(&callback).copied()
  }

  pub(in crate::source_contracts) fn is_async_callable(&self, callable: NodeId) -> bool {
    self.note_query();
    self.async_callables.contains(&callable)
  }

  pub(in crate::source_contracts) fn block_of(&self, node_id: NodeId) -> Option<NodeId> {
    self.note_query();
    self.owner(node_id).block
  }

  pub(in crate::source_contracts) fn call_node(&self, span: Span) -> Option<NodeId> {
    self.note_query();
    self.call_nodes.get(&span_key(span)).copied()
  }

  pub(in crate::source_contracts) fn callable_of(&self, node_id: NodeId) -> Option<NodeId> {
    self.note_query();
    self.owner(node_id).callable
  }

  pub(in crate::source_contracts) fn symbols_for_root(&self, root: SymbolId) -> Vec<SymbolId> {
    self.note_query();
    let mut symbols = vec![root];
    if let Some(aliases) = self.aliases_of.get(&root) {
      self.work.add_queries(aliases.len() as u64);
      symbols.extend(aliases.iter().copied());
    }
    symbols
  }

  pub(in crate::source_contracts) fn callable_node(&self, span: Span) -> Option<NodeId> {
    self.note_query();
    self.callables.get(&span_key(span)).copied()
  }

  pub(in crate::source_contracts) fn value_write_owner_mismatch(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    block: NodeId,
  ) -> bool {
    self.note_query();
    self.value_write_owner.get(&root).is_some_and(|owner| *owner != (callable, block))
  }

  pub(in crate::source_contracts) fn member_writes_mixed(
    &self,
    root: SymbolId,
    property: &str,
  ) -> bool {
    self.note_query();
    self.mixed_member_owners.contains(&(root, property.to_string()))
  }

  pub(in crate::source_contracts) fn member_write_owner_mismatch(
    &self,
    root: SymbolId,
    property: &str,
    callable: Option<NodeId>,
    block: NodeId,
  ) -> bool {
    self.note_query();
    self
      .member_write_owner
      .get(&(root, property.to_string()))
      .is_some_and(|owner| *owner != (callable, block))
  }

  pub(in crate::source_contracts) fn last_member_write_before(
    &self,
    root: SymbolId,
    property: &str,
    callable: Option<NodeId>,
    block: NodeId,
    offset: usize,
  ) -> Option<MemberWrite> {
    if self.mixed_member_owners.contains(&(root, property.to_string())) {
      self.note_query();
      return None;
    }
    let Some(writes) = self.member_writes.get(&(root, property.to_string())) else {
      self.note_query();
      return None;
    };
    let prior = timeline::last_before(&self.work, writes, offset).copied()?;
    (prior.callable == callable && prior.block == block).then_some(prior)
  }

  pub(in crate::source_contracts) fn first_member_write_after(
    &self,
    root: SymbolId,
    property: &str,
    block: NodeId,
    offset: usize,
  ) -> Option<MemberWrite> {
    let Some(writes) = self.member_writes.get(&(root, property.to_string())) else {
      self.note_query();
      return None;
    };
    let next = timeline::first_after(&self.work, writes, offset).copied()?;
    (next.simple_assign && next.fresh_alloc && next.block == block).then_some(next)
  }

  pub(in crate::source_contracts) fn object_prop(
    &self,
    object_span: Span,
    property: &str,
  ) -> Option<ObjectProp> {
    self.note_query();
    self
      .object_index
      .object_props
      .get(&span_key(object_span))
      .and_then(|props| props.get(property))
      .copied()
  }

  /// Shared options read. `undefined` is absent, so only a missing key or an
  /// explicit `undefined` takes the caller's default. Anything else that is
  /// not a literal is unknown.
  pub(in crate::source_contracts) fn option_value(
    &self,
    object: Span,
    key: &str,
  ) -> OptionValue<Literal> {
    match self.object_prop(object, key) {
      None => OptionValue::Absent,
      Some(ObjectProp::Unknown) => OptionValue::Unknown,
      Some(ObjectProp::Value(span)) => match self.literal_at(span) {
        Some(Literal::Undefined) => OptionValue::Absent,
        Some(literal) => OptionValue::Known(literal),
        None => OptionValue::Unknown,
      },
    }
  }

  pub(in crate::source_contracts) fn extracted_method(
    &self,
    symbol_id: SymbolId,
  ) -> Option<ExtractedMethod> {
    self.note_query();
    self.extracted_methods.get(&self.root_of(symbol_id)).copied()
  }

  pub(in crate::source_contracts) fn capability_poisoned(&self, symbol_id: SymbolId) -> bool {
    self.note_query();
    let root = self.root_of(symbol_id);
    self.capability_poisoned.contains(&root) || self.skip_written.contains(&root)
  }

  pub(in crate::source_contracts) fn collection_capability_invalid(
    &self,
    symbol_id: SymbolId,
  ) -> bool {
    self.capability_poisoned(symbol_id)
  }

  pub(in crate::source_contracts) fn extracted_call_eligible(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    extracted: ExtractedMethod,
    call_offset: usize,
  ) -> bool {
    let owner = self.owner(node_id);
    if owner.callable != extracted.callable {
      self.note_query();
      return false;
    }
    if call_offset <= extracted.extract_offset {
      self.note_query();
      return false;
    }
    if self.has_callable_termination_between(
      extracted.callable,
      extracted.extract_offset,
      call_offset,
    ) {
      return false;
    }
    self.call_reach_is_straight(semantic, node_id)
  }

  pub(in crate::source_contracts) fn ctor_tainted(&self, name: &str) -> bool {
    self.note_query();
    intern_native_ctor(name).is_some_and(|ctor| self.tainted_ctors.contains(ctor))
  }

  pub(in crate::source_contracts) fn collection_ctor(&self, span: Span) -> Option<CollectionCtor> {
    self.note_query();
    self.collections.get(&span_key(span)).copied()
  }

  pub(in crate::source_contracts) fn is_array_span(&self, span: Span) -> bool {
    self.note_query();
    self.arrays.contains(&span_key(span))
  }

  /// Proven closed object literal, or `None` when `span` is not an object.
  /// Precomputed once per object span; lookup charges a query, not a rescan.
  pub(in crate::source_contracts) fn closed_object_literal(&self, span: Span) -> Option<bool> {
    self.note_query();
    self.closed_objects.get(&span_key(span)).copied()
  }

  pub(in crate::source_contracts) fn is_array_literal(&self, span: Span) -> bool {
    self.note_query();
    self.arrays.contains(&span_key(span))
  }

  /// Capability-changing mutation or unknown helper use of a construction /
  /// watched root. Ordinary `state.n` writes stay ordinary member writes.
  /// Unknown flow uses the dedicated `capability_uncertain` role index:
  /// storage, return, unknown call, spread, sequence, receiver, dynamic target.
  /// Generic `escaped` / `uncertain` remain watch/reactive-argument facts.
  pub(in crate::source_contracts) fn construction_mutated(&self, root: SymbolId) -> bool {
    self.note_query();
    self.reassigned.contains(&root)
      || self.unknown_member_touch.contains(&root)
      || self.capability_uncertain.contains(&root)
      || self.has_capability_member_write(root)
  }

  pub(in crate::source_contracts) fn has_capability_member_write(&self, root: SymbolId) -> bool {
    CAPABILITY_KEYS.iter().any(|key| {
      self.note_query();
      self.member_writes.contains_key(&(root, (*key).to_string()))
    })
  }

  pub(in crate::source_contracts) fn has_closed_keys(&self, object_span: Span) -> bool {
    self.note_query();
    self.closed_keys.contains_key(&span_key(object_span))
  }

  pub(in crate::source_contracts) fn closed_object_has_key(
    &self,
    object_span: Span,
    key: &str,
  ) -> Option<bool> {
    self.note_query();
    let keys = self.closed_keys.get(&span_key(object_span))?;
    self.work.add_key_lookups(1);
    Some(keys.contains(key))
  }

  pub(in crate::source_contracts) fn closed_object_only_keys(
    &self,
    object_span: Span,
    allowed: &[&str],
  ) -> bool {
    self.note_query();
    let Some(keys) = self.closed_keys.get(&span_key(object_span)) else {
      return false;
    };
    keys.iter().all(|key| {
      self.work.add_key_lookups(1);
      allowed.contains(&key.as_str())
    })
  }

  pub(in crate::source_contracts) fn keys_closed(&self, root: SymbolId) -> bool {
    self.note_query();
    !self.reassigned.contains(&root)
      && !self.unknown_member_touch.contains(&root)
      && !self.capability_touch.contains(&root)
      && !self.closed_key_unknown.contains(&root)
  }

  pub(in crate::source_contracts) fn demand_ok(&self, site: &MemberUse) -> bool {
    self.note_query();
    site.head.reach.is_straight() && !site.optional
  }

  pub(in crate::source_contracts) fn origin_for(
    &self,
    node_id: NodeId,
    offset: usize,
  ) -> DemandOrigin {
    let owner = self.owner(node_id);
    DemandOrigin { callable: owner.callable, region: region_of(owner, node_id), offset }
  }

  pub(in crate::source_contracts) fn has_barrier_between(
    &self,
    region: NodeId,
    start: usize,
    end: usize,
  ) -> bool {
    self.timeline_has_between(&self.barriers_by_region, &region, start, end)
  }

  pub(in crate::source_contracts) fn demand_from(
    &self,
    site: &MemberUse,
    origin: DemandOrigin,
  ) -> bool {
    self.demand_ok(site)
      && site.head.callable == origin.callable
      && site.head.region == origin.region
      && !self.has_barrier_between(origin.region, origin.offset, site.head.offset)
  }

  pub(in crate::source_contracts) fn member_call_at(&self, span: Span) -> Option<&NamedUse> {
    self.note_query();
    self.member_call_by_span.get(&span_key(span))
  }

  pub(in crate::source_contracts) fn last_stop_before(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    region: NodeId,
    offset: usize,
  ) -> Option<MemberUse> {
    let Some(stops) = self.stops_by_region.get(&(root, callable, region)) else {
      self.note_query();
      return None;
    };
    timeline::last_before(&self.work, stops, offset).copied()
  }

  pub(in crate::source_contracts) fn capability_intact(&self, root: SymbolId) -> bool {
    self.note_query();
    !self.payload_uncertain(root) && !self.capability_touch.contains(&root)
  }

  pub(in crate::source_contracts) fn key_mutated(&self, root: SymbolId, key: &str) -> bool {
    self.note_query();
    if self.unknown_member_touch.contains(&root) {
      return true;
    }
    self.work.add_key_copies(1);
    self.work.add_key_lookups(1);
    self.member_writes.contains_key(&(root, key.to_string()))
  }

  pub(in crate::source_contracts) fn copy_key(&self, key: &str) -> String {
    self.work.add_key_copies(1);
    key.to_string()
  }

  pub(in crate::source_contracts) fn first_straight_value(
    &self,
    root: SymbolId,
    needs: impl Fn(DemandRole) -> bool,
    origin: DemandOrigin,
  ) -> Option<MemberUse> {
    let Some(uses) = self.value_reads.get(&root) else {
      self.note_query();
      return None;
    };
    uses.iter().copied().find(|site| {
      self.note_query();
      self.demand_from(site, origin) && needs(site.role)
    })
  }

  pub(in crate::source_contracts) fn has_straight_value_before(
    &self,
    root: SymbolId,
    needs: impl Fn(DemandRole) -> bool,
    origin: DemandOrigin,
    before: usize,
  ) -> bool {
    let Some(uses) = self.value_reads.get(&root) else {
      self.note_query();
      return false;
    };
    timeline::before(&self.work, uses, before).iter().any(|site| {
      self.note_query();
      self.demand_from(site, origin) && needs(site.role)
    })
  }

  pub(in crate::source_contracts) fn member_calls_on(&self, root: SymbolId) -> &[NamedUse] {
    self.note_query();
    self.member_calls_by_root.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn member_reads_on(&self, root: SymbolId) -> &[NamedUse] {
    self.note_query();
    self.member_reads_by_root.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn chained_values_on(&self, root: SymbolId) -> &[NamedUse] {
    self.note_query();
    self.chained_value_by_root.get(&root).map_or(&[], Vec::as_slice)
  }

  pub(in crate::source_contracts) fn destructures_of(
    &self,
    root: SymbolId,
  ) -> &[(SymbolId, String)] {
    self.note_query();
    self.destructure_by_object.get(&root).map_or(&[], Vec::as_slice)
  }
}
