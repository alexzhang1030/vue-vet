//! Narrow `EventTarget` acquisition/release identity helper on the lifetime index.
//!
//! Native `EventTarget` is a baseline intrinsic: a global constructor reference
//! used only as `new EventTarget()`. Any other role, a local/import binding, or
//! method mutation makes identity unproven. Replacement needs allocation identity
//! and an executed watcher schedule, not merely native capability.

use std::collections::{HashMap, HashSet};

use oxc_ast::{
  AstKind,
  ast::{
    AssignmentTarget, BindingPattern, CallExpression, Expression, ObjectPropertyKind, PropertyKind,
    UnaryOperator, VariableDeclarationKind,
  },
};
use oxc_semantic::{IsGlobalReference, NodeId, SymbolId};
use oxc_span::{GetSpan, Span};
use vue_vet_core::{
  ReactivityLifetimeFacts, WatchCleanupCurrentSourceFact, WatchCleanupCurrentSourceKind,
  WatcherApiKind,
};

use super::index::{LifetimeIndex, WatcherSite};
use super::resolve::{
  FunctionResolver, argument_expression, call_has_spread, callback_parameter_at,
  cleanup_parameter_index, enclosing_function, is_global_host, referenced_symbol,
  uncertain_to_function, vue_callee_export,
};
use crate::facts::source_span;

const ALIAS_BOUND: usize = 8;
const ADD_LISTENER: &str = "addEventListener";
const REMOVE_LISTENER: &str = "removeEventListener";

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub(super) struct AllocId(u32);

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum WatchFlush {
  Pre,
  Post,
  Sync,
}

#[derive(Clone, Copy)]
pub(super) struct WatchSchedule {
  pub immediate: Option<bool>,
  pub flush: Option<WatchFlush>,
  pub once: Option<bool>,
  pub unknown: bool,
}

impl WatchSchedule {
  pub(super) const DEFAULT: Self = Self {
    immediate: Some(false),
    flush: Some(WatchFlush::Pre),
    once: Some(false),
    unknown: false,
  };

  const fn inapplicable(self) -> bool {
    self.unknown
      || self.immediate.is_none()
      || self.flush.is_none()
      || !matches!(self.once, Some(false))
  }
}

#[derive(Clone, Copy, Default)]
pub(super) struct IdentityWork {
  #[cfg(test)]
  pub total: usize,
  #[cfg(test)]
  pub comparisons: usize,
  #[cfg(test)]
  pub registrations: usize,
  #[cfg(test)]
  pub alias_work: usize,
  #[cfg(test)]
  pub construction: usize,
  #[cfg(test)]
  pub queries: usize,
  #[cfg(test)]
  pub copies: usize,
  #[cfg(test)]
  pub reference_visits: usize,
}

impl IdentityWork {
  #[cfg(test)]
  const fn add(&mut self, n: usize) {
    self.total = self.total.saturating_add(n);
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  fn add(&mut self, n: usize) {
    let _ = n;
  }

  #[cfg(test)]
  const fn add_comparisons(&mut self, n: usize) {
    self.comparisons = self.comparisons.saturating_add(n);
    self.add(n);
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  fn add_comparisons(&mut self, n: usize) {
    let _ = n;
  }

  #[cfg(test)]
  const fn add_registrations(&mut self, n: usize) {
    self.registrations = self.registrations.saturating_add(n);
    self.add(n);
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  fn add_registrations(&mut self, n: usize) {
    let _ = n;
  }

  #[cfg(test)]
  const fn add_alias(&mut self, n: usize) {
    self.alias_work = self.alias_work.saturating_add(n);
    self.add(n);
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  fn add_alias(&mut self, n: usize) {
    let _ = n;
  }

  #[cfg(test)]
  const fn add_construction(&mut self, n: usize) {
    self.construction = self.construction.saturating_add(n);
    self.add(n);
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  fn add_construction(&mut self, n: usize) {
    let _ = n;
  }

  #[cfg(test)]
  const fn add_queries(&mut self, n: usize) {
    self.queries = self.queries.saturating_add(n);
    self.add(n);
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  fn add_queries(&mut self, n: usize) {
    let _ = n;
  }

  #[cfg(test)]
  const fn add_copies(&mut self, n: usize) {
    self.copies = self.copies.saturating_add(n);
    self.add(n);
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  fn add_copies(&mut self, n: usize) {
    let _ = n;
  }

  #[cfg(test)]
  const fn add_reference_visits(&mut self, n: usize) {
    self.reference_visits = self.reference_visits.saturating_add(n);
    self.add(n);
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  fn add_reference_visits(&mut self, n: usize) {
    let _ = n;
  }

  #[cfg(test)]
  pub(super) const fn absorb(&mut self, other: Self) {
    self.total = self.total.saturating_add(other.total);
    self.comparisons = self.comparisons.saturating_add(other.comparisons);
    self.registrations = self.registrations.saturating_add(other.registrations);
    self.alias_work = self.alias_work.saturating_add(other.alias_work);
    self.construction = self.construction.saturating_add(other.construction);
    self.queries = self.queries.saturating_add(other.queries);
    self.copies = self.copies.saturating_add(other.copies);
    self.reference_visits = self.reference_visits.saturating_add(other.reference_visits);
  }

  #[cfg(not(test))]
  #[expect(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    clippy::missing_const_for_fn,
    reason = "zero-sized production counter keeps the test method shape"
  )]
  pub(super) fn absorb(&mut self, other: Self) {
    let _ = other;
  }
}

pub(super) fn index_written_symbols(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &mut LifetimeIndex,
  work: &mut IdentityWork,
) {
  for symbol_id in semantic.scoping().symbol_ids() {
    work.add_construction(1);
    let mut written = false;
    for reference in semantic.scoping().get_resolved_references(symbol_id) {
      work.add_reference_visits(1);
      if reference.is_write() {
        written = true;
      }
    }
    if written {
      index.identity.written_payload.insert(symbol_id);
    }
  }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum HandleMember {
  Stop,
  Pause,
  Resume,
  Other,
}

#[derive(Default)]
pub(super) struct CleanupIdentityIndex {
  pub event_target_poisoned: bool,
  pub unknown_method_mutation: bool,
  pub method_mutated: HashSet<SymbolId>,
  pub method_mutated_alloc: HashSet<AllocId>,
  pub escaped_symbol: HashSet<SymbolId>,
  pub escaped_alloc: HashSet<AllocId>,
  pub ref_like: HashSet<SymbolId>,
  pub payload_alloc: HashMap<SymbolId, AllocId>,
  pub alloc_of_symbol: HashMap<SymbolId, AllocId>,
  pub written_payload: HashSet<SymbolId>,
  pub event_target_binding: HashSet<SymbolId>,
  pub payload_flow: HashMap<SymbolId, Vec<SymbolId>>,
  pub pending_escapes: Vec<SymbolId>,
  pub alias_of: HashMap<SymbolId, SymbolId>,
  pub ident_init: HashMap<SymbolId, SymbolId>,
  pub value_writes: HashMap<SymbolId, Vec<ValueWriteSite>>,
  pub listeners_by_fn: HashMap<NodeId, Vec<ListenerCall>>,
  pub boundaries: HashMap<Option<NodeId>, Vec<u32>>,
  pub handle_invokes: HashMap<SymbolId, Vec<HandleUse>>,
  pub handle_members: HashMap<SymbolId, Vec<HandleMemberUse>>,
  pub handle_escaped: HashSet<SymbolId>,
}

#[derive(Clone, Copy)]
pub(super) struct HandleUse {
  pub offset: u32,
  pub uncertain: bool,
}

#[derive(Clone, Copy)]
pub(super) struct HandleMemberUse {
  pub kind: HandleMember,
  pub offset: u32,
  pub uncertain: bool,
}

#[derive(Clone, Copy)]
pub(super) struct ValueWriteSite {
  pub offset: u32,
  pub rhs_span: Span,
  pub alloc: Option<AllocId>,
  pub uncertain: bool,
  pub callable: Option<NodeId>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ListenerKind {
  Add,
  Remove,
}

#[derive(Clone, Copy)]
enum Receiver {
  Identifier(SymbolId),
  SourceValue(SymbolId),
}

#[derive(Clone)]
pub(super) struct ListenerCall {
  kind: ListenerKind,
  receiver: Receiver,
  receiver_span: Span,
  call_span: Span,
  event: Option<String>,
  handler: Option<SymbolId>,
  capture: Option<bool>,
  once_or_signal: bool,
  unknown_args: bool,
}

#[derive(Clone, Eq, PartialEq, Hash)]
struct ResourceKey {
  event: String,
  handler: SymbolId,
  capture: bool,
}

impl CleanupIdentityIndex {
  pub(super) fn finalize(&mut self) -> usize {
    let mut work = 0usize;
    work = work.saturating_add(self.alias_of.len());
    for writes in self.value_writes.values_mut() {
      work = work.saturating_add(writes.len());
      writes.sort_by_key(|write| write.offset);
    }
    for calls in self.listeners_by_fn.values_mut() {
      work = work.saturating_add(calls.len());
      calls.sort_by_key(|call| call.call_span.start);
    }
    for offsets in self.boundaries.values_mut() {
      work = work.saturating_add(offsets.len());
      offsets.sort_unstable();
    }
    for uses in self.handle_invokes.values_mut() {
      work = work.saturating_add(uses.len());
      uses.sort_by_key(|use_site| use_site.offset);
    }
    for members in self.handle_members.values_mut() {
      work = work.saturating_add(members.len());
      members.sort_by_key(|use_site| use_site.offset);
    }
    for dests in self.payload_flow.values_mut() {
      work = work.saturating_add(dests.len());
      dests.sort_unstable();
      dests.dedup();
    }
    work
  }

  pub(super) fn finalize_payload_escapes(&mut self, work: &mut IdentityWork) {
    let native = self.native_payload_closure(work);
    let mut seen = HashSet::new();
    let pending = std::mem::take(&mut self.pending_escapes);
    work.add_construction(pending.len());
    for symbol_id in pending {
      if !seen.insert(symbol_id) {
        work.add_queries(1);
        continue;
      }
      self.resolve_payload_escape(symbol_id, &native, work);
    }
  }

  fn native_payload_closure(&self, work: &mut IdentityWork) -> HashSet<SymbolId> {
    let mut native: HashSet<SymbolId> = self.event_target_binding.iter().copied().collect();
    work.add_construction(native.len());
    let mut stack: Vec<SymbolId> = native.iter().copied().collect();
    stack.sort_unstable();
    let edge_bound = ALIAS_BOUND.saturating_mul(self.payload_flow.len().max(1));
    let mut edges = 0usize;
    while let Some(src) = stack.pop() {
      work.add_queries(1);
      let Some(dests) = self.payload_flow.get(&src) else {
        continue;
      };
      for &dest in dests {
        work.add_copies(1);
        edges = edges.saturating_add(1);
        if edges > edge_bound {
          return native;
        }
        work.add_alias(1);
        if native.insert(dest) {
          stack.push(dest);
        }
      }
    }
    native
  }

  fn resolve_payload_escape(
    &mut self,
    symbol_id: SymbolId,
    native: &HashSet<SymbolId>,
    work: &mut IdentityWork,
  ) {
    work.add_queries(1);
    let root = self.root_of_counted(symbol_id, work);
    if let Some(alloc) = stable_alloc_of_symbol(self, symbol_id, root) {
      self.escaped_alloc.insert(alloc);
      return;
    }
    let maybe_native = native.contains(&symbol_id) || native.contains(&root);
    if maybe_native {
      self.unknown_method_mutation = true;
    }
  }

  fn root_of(&self, symbol_id: SymbolId) -> SymbolId {
    let mut current = symbol_id;
    for _ in 0..ALIAS_BOUND {
      match self.alias_of.get(&current) {
        Some(&next) if next != current => current = next,
        _ => break,
      }
    }
    current
  }

  fn root_of_counted(&self, symbol_id: SymbolId, work: &mut IdentityWork) -> SymbolId {
    let mut current = symbol_id;
    for _ in 0..ALIAS_BOUND {
      work.add_alias(1);
      match self.alias_of.get(&current) {
        Some(&next) if next != current => current = next,
        _ => break,
      }
    }
    current
  }
}

pub(super) fn observe(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  kind: AstKind<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  _resolver: &mut FunctionResolver<'_, '_>,
  index: &mut LifetimeIndex,
) {
  match kind {
    AstKind::IdentifierReference(identifier) => {
      observe_event_target_role(semantic, node_id, identifier, index);
    }
    AstKind::VariableDeclarator(declarator) => {
      record_declarator(semantic, declarator, vue_exports, index);
    }
    AstKind::AssignmentExpression(assignment) => {
      record_assignment(
        semantic,
        node_id,
        assignment.operator,
        &assignment.left,
        &assignment.right,
        index,
      );
    }
    AstKind::CallExpression(call) => {
      record_call(semantic, node_id, call, vue_exports, index);
    }
    AstKind::NewExpression(expression) => {
      for argument in &expression.arguments {
        if let Some(value) = argument_expression(argument) {
          mark_event_target_value(semantic, value, index);
          mark_escape_from_expression(semantic, value, index);
        }
      }
    }
    AstKind::TaggedTemplateExpression(expression) => {
      for value in &expression.quasi.expressions {
        mark_escape_from_expression(semantic, value, index);
      }
    }
    AstKind::AwaitExpression(expression) => {
      record_scheduler_boundary(semantic, node_id, &expression.argument, vue_exports, index);
    }
    AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::Delete => {
      mark_method_mutation_from_expression(semantic, &unary.argument, index);
    }
    AstKind::ReturnStatement(ret) => {
      if let Some(argument) = &ret.argument {
        mark_escape_from_expression(semantic, argument, index);
        mark_handle_escape_from_expression(semantic, argument, index);
      }
    }
    _ => {}
  }
}

pub(super) fn watch_source_and_schedule(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  call: &CallExpression<'_>,
) -> (Option<SymbolId>, WatchSchedule, Option<SymbolId>) {
  let source = call.arguments.first().and_then(argument_expression).and_then(|expression| {
    expression
      .get_inner_expression()
      .get_identifier_reference()
      .and_then(|identifier| referenced_symbol(semantic, identifier))
  });
  (source, watch_schedule(call), watch_handle_symbol(semantic, node_id))
}

pub(super) fn emit(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  resolver: &mut FunctionResolver<'_, '_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  facts: &mut ReactivityLifetimeFacts,
) -> IdentityWork {
  let mut work = IdentityWork::default();
  if index.identity.event_target_poisoned || index.identity.unknown_method_mutation {
    return work;
  }
  let queries = QueryIndex::build(semantic, index, &mut work);
  for watcher in &index.watchers {
    work.add(1);
    for fact in fact_for_watcher(semantic, index, resolver, watcher, &queries, &mut work) {
      facts.watch_cleanup_current_sources.push(WatchCleanupCurrentSourceFact {
        kind: WatchCleanupCurrentSourceKind::RereadReplacedEventTarget,
        release_span: span_of(line_index, sfc_source, script_offset, fact.release),
        acquisition_span: span_of(line_index, sfc_source, script_offset, fact.acquisition),
        replacement_span: span_of(line_index, sfc_source, script_offset, fact.replacement),
        callback_span: span_of(line_index, sfc_source, script_offset, fact.callback),
        registration_span: span_of(line_index, sfc_source, script_offset, watcher.span),
      });
    }
  }
  work
}

struct PendingFact {
  release: Span,
  acquisition: Span,
  replacement: Span,
  callback: Span,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ListenerCapability {
  None,
  Function,
  HandleEvent,
  Unknown,
}

struct ReplacementProof {
  acquired: AllocId,
  replacement: ValueWriteSite,
}

struct Timeline {
  writes: Vec<ValueWriteSite>,
  after_value: Vec<Option<AllocId>>,
  next_diff: Vec<usize>,
}

struct AggregatedReleases {
  captured: HashSet<ResourceKey>,
  source_release: HashMap<(SymbolId, ResourceKey), Span>,
}

struct QueryIndex {
  timelines: HashMap<(SymbolId, Option<NodeId>), Timeline>,
  releases: HashMap<NodeId, AggregatedReleases>,
}

impl QueryIndex {
  fn build(
    semantic: &oxc_semantic::Semantic<'_>,
    index: &LifetimeIndex,
    work: &mut IdentityWork,
  ) -> Self {
    let mut grouped: HashMap<(SymbolId, Option<NodeId>), Vec<ValueWriteSite>> = HashMap::new();
    for (root, writes) in &index.identity.value_writes {
      work.add_construction(writes.len());
      for write in writes {
        grouped.entry((*root, write.callable)).or_default().push(*write);
      }
    }
    let mut timelines = HashMap::new();
    for (key, writes) in grouped {
      timelines.insert(key, Timeline::from_writes(writes, work));
    }
    let mut releases = HashMap::new();
    for watcher in &index.watchers {
      let Some(callback) = watcher.callback else {
        continue;
      };
      if releases.contains_key(&callback.node_id) {
        work.add_queries(1);
        continue;
      }
      let functions = cleanup_functions(index, semantic, callback.node_id, watcher.api, work);
      releases.insert(
        callback.node_id,
        aggregate_releases(index, semantic, callback.node_id, &functions, work),
      );
    }
    Self { timelines, releases }
  }
}

impl Timeline {
  fn from_writes(writes: Vec<ValueWriteSite>, work: &mut IdentityWork) -> Self {
    work.add_construction(writes.len());
    let n = writes.len();
    let mut after_value = Vec::with_capacity(n);
    for write in &writes {
      work.add_construction(1);
      after_value.push(if write.uncertain { None } else { write.alloc });
    }
    let mut next_diff = vec![n; n];
    if n > 0 {
      for i in (0..n).rev() {
        work.add_construction(1);
        let next = i.saturating_add(1);
        let value = if next >= n {
          n
        } else if after_value.get(i) != after_value.get(next)
          || after_value.get(next) == Some(&None)
        {
          next
        } else {
          next_diff.get(next).copied().unwrap_or(n)
        };
        if let Some(slot) = next_diff.get_mut(i) {
          *slot = value;
        }
      }
    }
    Self { writes, after_value, next_diff }
  }

  fn value_at(&self, offset: u32, init: AllocId, work: &mut IdentityWork) -> Option<AllocId> {
    work.add_queries(1);
    match self.last_index_before(offset, work) {
      None => Some(init),
      Some(index) => self.after_value.get(index).copied()?,
    }
  }

  fn last_index_before(&self, offset: u32, work: &mut IdentityWork) -> Option<usize> {
    partition_first_ge(&self.writes, offset, work).checked_sub(1)
  }

  fn next_transition_after(
    &self,
    offset: u32,
    current: AllocId,
    callback_id: NodeId,
    active_until: u32,
    work: &mut IdentityWork,
  ) -> Option<ValueWriteSite> {
    work.add_queries(1);
    let mut index = partition_after(&self.writes, offset, work);
    while index < self.writes.len() {
      work.add_comparisons(1);
      let write = *self.writes.get(index)?;
      if write.offset >= active_until {
        return None;
      }
      if write.callable == Some(callback_id) {
        index = index.saturating_add(1);
        continue;
      }
      if write.uncertain || write.alloc.is_none() {
        return None;
      }
      if write.alloc != Some(current) {
        return Some(write);
      }
      let jump = self.next_diff.get(index).copied().unwrap_or_else(|| index.saturating_add(1));
      index = if jump <= index { index.saturating_add(1) } else { jump };
    }
    None
  }

  fn has_later_write(
    &self,
    after: u32,
    callback_id: NodeId,
    active_until: u32,
    work: &mut IdentityWork,
  ) -> bool {
    work.add_queries(1);
    let start = partition_after(&self.writes, after, work);
    self.writes.get(start..).is_some_and(|writes| {
      writes.iter().any(|write| write.offset < active_until && write.callable != Some(callback_id))
    })
  }
}

fn aggregate_releases(
  index: &LifetimeIndex,
  semantic: &oxc_semantic::Semantic<'_>,
  callback_id: NodeId,
  cleanup_ids: &[NodeId],
  work: &mut IdentityWork,
) -> AggregatedReleases {
  let target = callback_parameter_at(semantic, callback_id, 0);
  let mut captured = HashSet::new();
  let mut source_release: HashMap<(SymbolId, ResourceKey), Span> = HashMap::new();
  work.add_construction(cleanup_ids.len());
  for &cleanup_id in cleanup_ids {
    let Some(calls) = index.identity.listeners_by_fn.get(&cleanup_id) else {
      continue;
    };
    work.add_comparisons(calls.len());
    for call in calls {
      if call.kind != ListenerKind::Remove || !usable_listener(call) {
        continue;
      }
      let Some(key) = resource_key(call) else {
        continue;
      };
      work.add_construction(1);
      if target.is_some_and(|target| is_captured_receiver(index, call.receiver, target, work)) {
        work.add_copies(1);
        captured.insert(key.clone());
      }
      if let Receiver::SourceValue(symbol_id) = call.receiver {
        if symbol_write_summary(index, symbol_id, work) {
          continue;
        }
        let root = index.identity.root_of_counted(symbol_id, work);
        work.add_copies(1);
        source_release
          .entry((root, key))
          .and_modify(|span| {
            if call.receiver_span.start < span.start {
              *span = call.receiver_span;
            }
          })
          .or_insert(call.receiver_span);
      }
    }
  }
  AggregatedReleases { captured, source_release }
}

fn symbol_write_summary(
  index: &LifetimeIndex,
  symbol_id: SymbolId,
  work: &mut IdentityWork,
) -> bool {
  work.add_queries(1);
  index.identity.written_payload.contains(&symbol_id)
}

fn fact_for_watcher(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  resolver: &mut FunctionResolver<'_, '_>,
  watcher: &WatcherSite,
  queries: &QueryIndex,
  work: &mut IdentityWork,
) -> Vec<PendingFact> {
  if watcher.api != WatcherApiKind::Watch || watcher.schedule.inapplicable() {
    return Vec::new();
  }
  if !watch_creation_proven(semantic, index, watcher) {
    return Vec::new();
  }
  let Some(callback) = watcher.callback else {
    return Vec::new();
  };
  let Some(source_symbol) = watcher.source_symbol else {
    return Vec::new();
  };
  if resolver.symbol_has_write(source_symbol) {
    return Vec::new();
  }
  let source_root = index.identity.root_of_counted(source_symbol, work);
  if !index.identity.ref_like.contains(&source_root) {
    return Vec::new();
  }
  let Some(&init_alloc) = index.identity.payload_alloc.get(&source_root) else {
    return Vec::new();
  };
  if index.identity.method_mutated.contains(&source_root)
    || index.identity.escaped_symbol.contains(&source_root)
  {
    return Vec::new();
  }
  if index.incomplete_registration.contains(&callback.node_id) {
    return Vec::new();
  }
  let Some(target) = callback_parameter_at(semantic, callback.node_id, 0) else {
    return Vec::new();
  };
  if resolver.symbol_has_write(target)
    || index.identity.method_mutated.contains(&target)
    || index.identity.escaped_symbol.contains(&target)
  {
    return Vec::new();
  }
  let Some(active_until) = active_until(index, resolver, watcher, work) else {
    return Vec::new();
  };
  let Some(proof) = proving_replacement(
    index,
    queries,
    &ReplacementNeed {
      watcher,
      callback_id: callback.node_id,
      source_root,
      init_alloc,
      active_until,
    },
    work,
  ) else {
    return Vec::new();
  };
  if index.identity.method_mutated_alloc.contains(&proof.acquired)
    || index.identity.escaped_alloc.contains(&proof.acquired)
  {
    return Vec::new();
  }
  let adds = index.identity.listeners_by_fn.get(&callback.node_id).map_or(&[][..], Vec::as_slice);
  work.add_comparisons(adds.len());
  work.add_queries(1);
  let Some(releases) = queries.releases.get(&callback.node_id) else {
    return Vec::new();
  };
  let mut facts = Vec::new();
  let mut emitted = HashSet::new();
  for add in adds {
    if add.kind != ListenerKind::Add || !usable_listener(add) {
      continue;
    }
    if !is_run_local_receiver(
      semantic,
      index,
      resolver,
      add.receiver,
      target,
      callback.node_id,
      work,
    ) {
      continue;
    }
    let Some(event) = add.event.as_deref() else {
      continue;
    };
    let Some(handler) = add.handler else {
      continue;
    };
    if index.identity.escaped_symbol.contains(&handler) {
      continue;
    }
    match handler_capability(semantic, index, resolver, handler, work) {
      ListenerCapability::Function | ListenerCapability::HandleEvent => {}
      ListenerCapability::None | ListenerCapability::Unknown => continue,
    }
    let Some(capture) = add.capture else {
      continue;
    };
    let key = ResourceKey { event: event.to_string(), handler, capture };
    if !emitted.insert(key.clone()) {
      continue;
    }
    work.add_queries(1);
    if releases.captured.contains(&key) {
      continue;
    }
    work.add_queries(1);
    let Some(&release) = releases.source_release.get(&(source_root, key)) else {
      continue;
    };
    facts.push(PendingFact {
      release,
      acquisition: add.call_span,
      replacement: proof.replacement.rhs_span,
      callback: callback.span,
    });
  }
  facts.sort_by_key(|fact| (fact.release.start, fact.acquisition.start));
  facts
}

fn watch_creation_proven(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  watcher: &WatcherSite,
) -> bool {
  if watcher.owner.is_some_and(|function_id| {
    index.reentered_functions.contains(&function_id)
      || index.deferred_parents.contains_key(&function_id)
  }) {
    return false;
  }
  !uncertain_in_owner(semantic, watcher.node_id, watcher.owner)
}

fn active_until(
  index: &LifetimeIndex,
  resolver: &mut FunctionResolver<'_, '_>,
  watcher: &WatcherSite,
  work: &mut IdentityWork,
) -> Option<u32> {
  let Some(handle) = watcher.handle_symbol else {
    return watcher.unused.then_some(u32::MAX);
  };
  let root = index.identity.root_of_counted(handle, work);
  if resolver.symbol_has_write(handle)
    || resolver.symbol_has_write(root)
    || index.identity.handle_escaped.contains(&handle)
    || index.identity.handle_escaped.contains(&root)
  {
    return None;
  }
  let mut earliest_possible = u32::MAX;
  let mut earliest_definite = u32::MAX;
  let mut saw_uncertain = false;
  if let Some(invokes) = index.identity.handle_invokes.get(&root) {
    work.add_registrations(invokes.len());
    for invoke in invokes {
      earliest_possible = earliest_possible.min(invoke.offset);
      if invoke.uncertain {
        saw_uncertain = true;
      } else {
        earliest_definite = earliest_definite.min(invoke.offset);
      }
    }
  }
  if let Some(members) = index.identity.handle_members.get(&root) {
    work.add_registrations(members.len());
    for member in members {
      match member.kind {
        HandleMember::Stop => {
          earliest_possible = earliest_possible.min(member.offset);
          if member.uncertain {
            saw_uncertain = true;
          } else {
            earliest_definite = earliest_definite.min(member.offset);
          }
        }
        HandleMember::Pause | HandleMember::Resume | HandleMember::Other => return None,
      }
    }
  }
  if earliest_definite == u32::MAX {
    return (!saw_uncertain).then_some(u32::MAX);
  }
  if saw_uncertain && earliest_possible < earliest_definite {
    return None;
  }
  Some(earliest_definite)
}

struct ReplacementNeed<'a> {
  watcher: &'a WatcherSite,
  callback_id: NodeId,
  source_root: SymbolId,
  init_alloc: AllocId,
  active_until: u32,
}

fn proving_replacement(
  index: &LifetimeIndex,
  queries: &QueryIndex,
  need: &ReplacementNeed<'_>,
  work: &mut IdentityWork,
) -> Option<ReplacementProof> {
  work.add_queries(1);
  let timeline = queries.timelines.get(&(need.source_root, need.watcher.owner))?;
  let registered = timeline.value_at(need.watcher.span.end, need.init_alloc, work)?;
  let immediate = need.watcher.schedule.immediate?;
  let flush = need.watcher.schedule.flush?;
  if immediate {
    return immediate_replacement(index, timeline, need, registered, flush, work);
  }
  match flush {
    WatchFlush::Sync => sync_replacement(timeline, need, registered, work),
    WatchFlush::Pre | WatchFlush::Post => {
      queued_acquisition(index, timeline, need, registered, work)
    }
  }
}

fn sync_replacement(
  timeline: &Timeline,
  need: &ReplacementNeed<'_>,
  registered: AllocId,
  work: &mut IdentityWork,
) -> Option<ReplacementProof> {
  let first = timeline.next_transition_after(
    need.watcher.span.end,
    registered,
    need.callback_id,
    need.active_until,
    work,
  )?;
  let acquired = first.alloc?;
  let replacement = timeline.next_transition_after(
    first.offset,
    acquired,
    need.callback_id,
    need.active_until,
    work,
  )?;
  Some(ReplacementProof { acquired, replacement })
}

fn immediate_replacement(
  index: &LifetimeIndex,
  timeline: &Timeline,
  need: &ReplacementNeed<'_>,
  acquired: AllocId,
  flush: WatchFlush,
  work: &mut IdentityWork,
) -> Option<ReplacementProof> {
  match flush {
    WatchFlush::Sync => {
      let replacement = timeline.next_transition_after(
        need.watcher.span.end,
        acquired,
        need.callback_id,
        need.active_until,
        work,
      )?;
      Some(ReplacementProof { acquired, replacement })
    }
    WatchFlush::Pre | WatchFlush::Post => {
      queued_after(index, timeline, need, need.watcher.span.end, acquired, work)
    }
  }
}

fn queued_acquisition(
  index: &LifetimeIndex,
  timeline: &Timeline,
  need: &ReplacementNeed<'_>,
  registered: AllocId,
  work: &mut IdentityWork,
) -> Option<ReplacementProof> {
  let mut cursor = need.watcher.span.end;
  loop {
    let boundary = first_boundary_after(&index.identity, need.watcher.owner, cursor, work)?;
    if boundary >= need.active_until {
      return None;
    }
    let settled = timeline.value_at(boundary, need.init_alloc, work)?;
    if settled != registered {
      return queued_after(index, timeline, need, boundary, settled, work);
    }
    cursor = boundary;
  }
}

fn queued_after(
  index: &LifetimeIndex,
  timeline: &Timeline,
  need: &ReplacementNeed<'_>,
  after: u32,
  acquired: AllocId,
  work: &mut IdentityWork,
) -> Option<ReplacementProof> {
  let mut cursor = after;
  loop {
    match first_boundary_after(&index.identity, need.watcher.owner, cursor, work) {
      Some(boundary) if boundary < need.active_until => {
        let settled = timeline.value_at(boundary, need.init_alloc, work)?;
        if settled != acquired {
          let replacement = timeline.next_transition_after(
            cursor,
            acquired,
            need.callback_id,
            need.active_until,
            work,
          )?;
          return Some(ReplacementProof { acquired, replacement });
        }
        cursor = boundary;
      }
      _ => {
        return queued_unbounded(timeline, need, cursor, acquired, work);
      }
    }
  }
}

fn queued_unbounded(
  timeline: &Timeline,
  need: &ReplacementNeed<'_>,
  after: u32,
  acquired: AllocId,
  work: &mut IdentityWork,
) -> Option<ReplacementProof> {
  let replacement =
    timeline.next_transition_after(after, acquired, need.callback_id, need.active_until, work)?;
  if timeline.has_later_write(replacement.offset, need.callback_id, need.active_until, work) {
    return None;
  }
  Some(ReplacementProof { acquired, replacement })
}

fn first_boundary_after(
  identity: &CleanupIdentityIndex,
  owner: Option<NodeId>,
  after: u32,
  work: &mut IdentityWork,
) -> Option<u32> {
  let offsets = identity.boundaries.get(&owner)?;
  let start = partition_u32(offsets, after, work);
  offsets.get(start).copied()
}

fn partition_first_ge(writes: &[ValueWriteSite], offset: u32, work: &mut IdentityWork) -> usize {
  let mut lo = 0;
  let mut hi = writes.len();
  while lo < hi {
    work.add_comparisons(1);
    let mid = lo.saturating_add(hi) / 2;
    let Some(write) = writes.get(mid) else {
      break;
    };
    if write.offset < offset {
      lo = mid.saturating_add(1);
    } else {
      hi = mid;
    }
  }
  lo
}

fn partition_after(writes: &[ValueWriteSite], after: u32, work: &mut IdentityWork) -> usize {
  let mut lo = 0;
  let mut hi = writes.len();
  while lo < hi {
    work.add_comparisons(1);
    let mid = lo.saturating_add(hi) / 2;
    let Some(write) = writes.get(mid) else {
      break;
    };
    if write.offset <= after {
      lo = mid.saturating_add(1);
    } else {
      hi = mid;
    }
  }
  lo
}

fn partition_u32(offsets: &[u32], after: u32, work: &mut IdentityWork) -> usize {
  let mut lo = 0;
  let mut hi = offsets.len();
  while lo < hi {
    work.add_comparisons(1);
    let mid = lo.saturating_add(hi) / 2;
    let Some(offset) = offsets.get(mid) else {
      break;
    };
    if *offset <= after {
      lo = mid.saturating_add(1);
    } else {
      hi = mid;
    }
  }
  lo
}

fn cleanup_functions(
  index: &LifetimeIndex,
  semantic: &oxc_semantic::Semantic<'_>,
  callback_id: NodeId,
  api: WatcherApiKind,
  work: &mut IdentityWork,
) -> Vec<NodeId> {
  let cleanup_param = callback_parameter_at(semantic, callback_id, cleanup_parameter_index(api));
  let await_end = index.first_await.get(&callback_id).map(|span| span.end);
  let mut functions = Vec::new();
  let Some(sites) = index.registers_by_fn.get(&callback_id) else {
    return functions;
  };
  work.add_registrations(sites.len());
  for site in sites {
    let bound = site.callee.is_some_and(|callee| {
      cleanup_param.is_some_and(|param| {
        index.identity.root_of_counted(callee, work) == param
          || index.identity.ident_init.get(&callee).copied() == Some(param)
      })
    });
    if bound {
      if let Some(identity) = site.identity {
        functions.push(identity);
      }
      continue;
    }
    if !site.vue_cleanup {
      continue;
    }
    if !site.explicit_owner && await_end.is_some_and(|end| site.span.start >= end) {
      continue;
    }
    if let Some(identity) = site.identity {
      functions.push(identity);
    }
  }
  functions
}

const fn usable_listener(call: &ListenerCall) -> bool {
  !call.unknown_args
    && !call.once_or_signal
    && call.event.is_some()
    && call.handler.is_some()
    && call.capture.is_some()
}

fn resource_key(call: &ListenerCall) -> Option<ResourceKey> {
  Some(ResourceKey {
    event: call.event.as_ref()?.clone(),
    handler: call.handler?,
    capture: call.capture?,
  })
}

fn handler_capability(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  resolver: &mut FunctionResolver<'_, '_>,
  handler: SymbolId,
  work: &mut IdentityWork,
) -> ListenerCapability {
  work.add_queries(1);
  if resolver.symbol_has_write(handler) {
    return ListenerCapability::Unknown;
  }
  let root = index.identity.root_of_counted(handler, work);
  if resolver.symbol_has_write(root)
    || index.identity.method_mutated.contains(&handler)
    || index.identity.method_mutated.contains(&root)
  {
    return ListenerCapability::Unknown;
  }
  if resolver.resolve_symbol_id(handler).is_some() || resolver.resolve_symbol_id(root).is_some() {
    return ListenerCapability::Function;
  }
  capability_from_symbol(semantic, index, resolver, handler, 0, work)
}

fn capability_from_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  resolver: &mut FunctionResolver<'_, '_>,
  symbol_id: SymbolId,
  depth: usize,
  work: &mut IdentityWork,
) -> ListenerCapability {
  if depth >= ALIAS_BOUND {
    return ListenerCapability::Unknown;
  }
  work.add_alias(1);
  if resolver.symbol_has_write(symbol_id) {
    return ListenerCapability::Unknown;
  }
  if let Some(&next) =
    index.identity.alias_of.get(&symbol_id).or_else(|| index.identity.ident_init.get(&symbol_id))
    && next != symbol_id
  {
    return capability_from_symbol(semantic, index, resolver, next, depth.saturating_add(1), work);
  }
  let declaration = semantic.scoping().symbol_declaration(symbol_id);
  match semantic.nodes().kind(declaration) {
    AstKind::Function(_) => ListenerCapability::Function,
    AstKind::VariableDeclarator(declarator) => {
      let Some(init) = &declarator.init else {
        return ListenerCapability::Unknown;
      };
      capability_from_expression(semantic, index, resolver, init, depth.saturating_add(1), work)
    }
    _ => ListenerCapability::Unknown,
  }
}

fn capability_from_expression(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  resolver: &mut FunctionResolver<'_, '_>,
  expression: &Expression<'_>,
  depth: usize,
  work: &mut IdentityWork,
) -> ListenerCapability {
  if depth >= ALIAS_BOUND {
    return ListenerCapability::Unknown;
  }
  match expression.get_inner_expression() {
    Expression::NullLiteral(_) => ListenerCapability::None,
    Expression::Identifier(identifier)
      if identifier.name.as_str() == "undefined"
        && identifier.is_global_reference(semantic.scoping()) =>
    {
      ListenerCapability::None
    }
    Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::Void => {
      ListenerCapability::None
    }
    Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
      ListenerCapability::Function
    }
    Expression::ObjectExpression(object) => {
      handle_event_capability(semantic, index, resolver, object, depth, work)
    }
    Expression::Identifier(_) => {
      let Some(symbol_id) = identifier_symbol(semantic, expression) else {
        return ListenerCapability::Unknown;
      };
      capability_from_symbol(semantic, index, resolver, symbol_id, depth.saturating_add(1), work)
    }
    _ => ListenerCapability::Unknown,
  }
}

fn handle_event_capability(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  resolver: &mut FunctionResolver<'_, '_>,
  object: &oxc_ast::ast::ObjectExpression<'_>,
  depth: usize,
  work: &mut IdentityWork,
) -> ListenerCapability {
  for property in &object.properties {
    match property {
      ObjectPropertyKind::SpreadProperty(_) => return ListenerCapability::Unknown,
      ObjectPropertyKind::ObjectProperty(property) => {
        if static_property_name(property) != Some("handleEvent") {
          continue;
        }
        match property.value.get_inner_expression() {
          Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
            return ListenerCapability::HandleEvent;
          }
          other => {
            return match capability_from_expression(
              semantic,
              index,
              resolver,
              other,
              depth.saturating_add(1),
              work,
            ) {
              ListenerCapability::Function => ListenerCapability::HandleEvent,
              other => other,
            };
          }
        }
      }
    }
  }
  ListenerCapability::Unknown
}

fn is_run_local_receiver(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  resolver: &mut FunctionResolver<'_, '_>,
  receiver: Receiver,
  target: SymbolId,
  callback_id: NodeId,
  work: &mut IdentityWork,
) -> bool {
  let Receiver::Identifier(symbol_id) = receiver else {
    return false;
  };
  work.add_alias(1);
  if symbol_id == target {
    return true;
  }
  if resolver.symbol_has_write(symbol_id) {
    return false;
  }
  let Some(&init) = index.identity.ident_init.get(&symbol_id) else {
    return false;
  };
  work.add_alias(1);
  if init != target {
    return false;
  }
  let declaration = semantic.scoping().symbol_declaration(symbol_id);
  enclosing_function(semantic, declaration, &mut HashMap::new()) == Some(callback_id)
}

fn is_captured_receiver(
  index: &LifetimeIndex,
  receiver: Receiver,
  target: SymbolId,
  work: &mut IdentityWork,
) -> bool {
  let Receiver::Identifier(symbol_id) = receiver else {
    return false;
  };
  work.add_alias(1);
  symbol_id == target || index.identity.ident_init.get(&symbol_id).copied() == Some(target)
}

fn observe_event_target_role(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  identifier: &oxc_ast::ast::IdentifierReference<'_>,
  index: &mut LifetimeIndex,
) {
  if identifier.name.as_str() != "EventTarget"
    || !identifier.is_global_reference(semantic.scoping())
  {
    return;
  }
  if matches!(skip_wrappers(semantic, node_id), AstKind::NewExpression(_)) {
    return;
  }
  index.identity.event_target_poisoned = true;
}

fn skip_wrappers<'a>(semantic: &'a oxc_semantic::Semantic<'a>, mut node_id: NodeId) -> AstKind<'a> {
  loop {
    let parent = semantic.nodes().parent_id(node_id);
    if parent == node_id {
      return semantic.nodes().kind(node_id);
    }
    match semantic.nodes().kind(parent) {
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_) => node_id = parent,
      other => return other,
    }
  }
}

fn record_declarator(
  semantic: &oxc_semantic::Semantic<'_>,
  declarator: &oxc_ast::ast::VariableDeclarator<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  index: &mut LifetimeIndex,
) {
  match &declarator.id {
    BindingPattern::BindingIdentifier(binding) => {
      let Some(symbol_id) = binding.symbol_id.get() else {
        return;
      };
      let Some(init) = &declarator.init else {
        return;
      };
      mark_event_target_value(semantic, init, index);
      if let Some(from) = identifier_symbol(semantic, init) {
        index.identity.ident_init.insert(symbol_id, from);
        record_payload_flow(&mut index.identity, from, symbol_id);
        if declarator.kind == VariableDeclarationKind::Const {
          index.identity.alias_of.insert(symbol_id, from);
        }
      }
      if let Some(alloc) = alloc_of_expression(semantic, init, index, 0) {
        index.identity.event_target_binding.insert(symbol_id);
        if !index.identity.written_payload.contains(&symbol_id) {
          index.identity.alloc_of_symbol.insert(symbol_id, alloc);
        }
      }
      let Some(call) = as_call(init) else {
        return;
      };
      let Some(api) = vue_callee_export(semantic, &call.callee, vue_exports) else {
        return;
      };
      if !matches!(api, "ref" | "shallowRef") || call_has_spread(call) {
        return;
      }
      index.identity.ref_like.insert(symbol_id);
      if let Some(argument) = call.arguments.first().and_then(argument_expression)
        && let Some(alloc) = alloc_of_expression(semantic, argument, index, 0)
      {
        index.identity.payload_alloc.insert(symbol_id, alloc);
      }
    }
    _ => {
      if let Some(init) = &declarator.init {
        mark_escape_from_expression(semantic, init, index);
      }
    }
  }
}

fn record_assignment(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  operator: oxc_ast::ast::AssignmentOperator,
  left: &AssignmentTarget<'_>,
  right: &Expression<'_>,
  index: &mut LifetimeIndex,
) {
  mark_event_target_value(semantic, right, index);
  mark_method_mutation_target(semantic, left, index);
  if operator != oxc_ast::ast::AssignmentOperator::Assign {
    return;
  }
  match left {
    AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
      if identifier.name.as_str() == "EventTarget"
        && identifier.is_global_reference(semantic.scoping())
      {
        index.identity.event_target_poisoned = true;
      }
      if let Some(symbol_id) = referenced_symbol(semantic, identifier) {
        mark_handle_escape_symbol(index, symbol_id);
        if let Some(from) = identifier_symbol(semantic, right) {
          index.identity.ident_init.entry(symbol_id).or_insert(from);
          record_payload_flow(&mut index.identity, from, symbol_id);
        }
        if alloc_of_expression(semantic, right, index, 0).is_some() {
          index.identity.event_target_binding.insert(symbol_id);
        }
        index.identity.written_payload.insert(symbol_id);
        index.identity.alloc_of_symbol.remove(&symbol_id);
      }
    }
    AssignmentTarget::StaticMemberExpression(member) => {
      record_member_write(
        semantic,
        node_id,
        &member.object,
        member.property.name.as_str(),
        right,
        index,
      );
    }
    AssignmentTarget::ComputedMemberExpression(member) => {
      if let Expression::StringLiteral(literal) = member.expression.get_inner_expression() {
        record_member_write(
          semantic,
          node_id,
          &member.object,
          literal.value.as_str(),
          right,
          index,
        );
      } else {
        index.identity.unknown_method_mutation = true;
      }
    }
    _ => {
      mark_escape_from_expression(semantic, right, index);
    }
  }
}

fn record_member_write(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  object: &Expression<'_>,
  property: &str,
  right: &Expression<'_>,
  index: &mut LifetimeIndex,
) {
  if property == ADD_LISTENER || property == REMOVE_LISTENER {
    mark_method_mutation_from_expression(semantic, object, index);
    return;
  }
  if property == "EventTarget" && is_global_host(semantic, object) {
    index.identity.event_target_poisoned = true;
    return;
  }
  if property != "value" {
    mark_escape_from_expression(semantic, right, index);
    return;
  }
  let Some(identifier) = object.get_inner_expression().get_identifier_reference() else {
    return;
  };
  let Some(symbol_id) = referenced_symbol(semantic, identifier) else {
    return;
  };
  let root = index.identity.root_of(symbol_id);
  let callable = enclosing_function(semantic, node_id, &mut index.enclosing);
  let uncertain = uncertain_in_owner(semantic, node_id, callable);
  let alloc = alloc_of_expression(semantic, right, index, 0);
  index.identity.value_writes.entry(root).or_default().push(ValueWriteSite {
    offset: right.span().start,
    rhs_span: right.get_inner_expression().span(),
    alloc,
    uncertain,
    callable,
  });
}

fn record_call(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  call: &CallExpression<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  index: &mut LifetimeIndex,
) {
  let listener = record_listener_call(semantic, node_id, call, index);
  record_handle_callee(semantic, node_id, call, index);
  for argument in &call.arguments {
    if let Some(expression) = argument_expression(argument) {
      mark_event_target_value(semantic, expression, index);
    }
  }
  if listener || is_recognized_api(semantic, call, vue_exports) {
    return;
  }
  for argument in &call.arguments {
    if let Some(expression) = argument_expression(argument) {
      mark_escape_from_expression(semantic, expression, index);
      mark_handle_escape_from_expression(semantic, expression, index);
    } else {
      index.identity.unknown_method_mutation = true;
    }
  }
}

fn record_handle_callee(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  call: &CallExpression<'_>,
  index: &mut LifetimeIndex,
) {
  let owner = enclosing_function(semantic, node_id, &mut index.enclosing);
  let uncertain = uncertain_in_owner(semantic, node_id, owner)
    || owner.is_some_and(|function_id| {
      index.reentered_functions.contains(&function_id)
        || index.deferred_parents.contains_key(&function_id)
    });
  let callee = call.callee.get_inner_expression();
  if let Some(symbol_id) = identifier_symbol(semantic, callee) {
    let root = index.identity.root_of(symbol_id);
    index
      .identity
      .handle_invokes
      .entry(root)
      .or_default()
      .push(HandleUse { offset: call.span.start, uncertain });
    return;
  }
  let Some((object, property)) = listener_member(callee) else {
    return;
  };
  let Some(symbol_id) = identifier_symbol(semantic, object) else {
    return;
  };
  let kind = match property {
    "stop" => HandleMember::Stop,
    "pause" => HandleMember::Pause,
    "resume" => HandleMember::Resume,
    _ => HandleMember::Other,
  };
  let root = index.identity.root_of(symbol_id);
  index.identity.handle_members.entry(root).or_default().push(HandleMemberUse {
    kind,
    offset: call.span.start,
    uncertain,
  });
}

fn record_scheduler_boundary(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  argument: &Expression<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  index: &mut LifetimeIndex,
) {
  let Some(call) = as_call(argument) else {
    return;
  };
  if vue_callee_export(semantic, &call.callee, vue_exports) != Some("nextTick") {
    return;
  }
  let owner = enclosing_function(semantic, node_id, &mut index.enclosing);
  if uncertain_in_owner(semantic, node_id, owner) {
    return;
  }
  index.identity.boundaries.entry(owner).or_default().push(argument.span().start);
}

fn record_listener_call(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  call: &CallExpression<'_>,
  index: &mut LifetimeIndex,
) -> bool {
  let Some((object, property)) = listener_member(&call.callee) else {
    return false;
  };
  let kind = match property {
    ADD_LISTENER => ListenerKind::Add,
    REMOVE_LISTENER => ListenerKind::Remove,
    _ => return false,
  };
  let Some(function_id) = enclosing_function(semantic, node_id, &mut index.enclosing) else {
    return true;
  };
  if uncertain_to_function(semantic, node_id, function_id) {
    return true;
  }
  let Some(receiver) = receiver_of(semantic, object, index) else {
    return true;
  };
  let (event, handler, capture, once_or_signal, unknown_args) = listener_arguments(semantic, call);
  index.identity.listeners_by_fn.entry(function_id).or_default().push(ListenerCall {
    kind,
    receiver,
    receiver_span: object.span(),
    call_span: call.span,
    event,
    handler,
    capture,
    once_or_signal,
    unknown_args,
  });
  true
}

fn listener_member<'a>(callee: &'a Expression<'a>) -> Option<(&'a Expression<'a>, &'a str)> {
  match callee.get_inner_expression() {
    Expression::StaticMemberExpression(member) => {
      Some((&member.object, member.property.name.as_str()))
    }
    Expression::ComputedMemberExpression(member) => {
      let Expression::StringLiteral(literal) = member.expression.get_inner_expression() else {
        return None;
      };
      Some((&member.object, literal.value.as_str()))
    }
    _ => None,
  }
}

fn receiver_of(
  semantic: &oxc_semantic::Semantic<'_>,
  object: &Expression<'_>,
  index: &LifetimeIndex,
) -> Option<Receiver> {
  let inner = object.get_inner_expression();
  if let Some(symbol_id) = identifier_symbol(semantic, inner) {
    return Some(Receiver::Identifier(index.identity.root_of(symbol_id)));
  }
  source_value_symbol(semantic, inner).map(Receiver::SourceValue)
}

fn listener_arguments(
  semantic: &oxc_semantic::Semantic<'_>,
  call: &CallExpression<'_>,
) -> (Option<String>, Option<SymbolId>, Option<bool>, bool, bool) {
  if call_has_spread(call) || call.arguments.len() > 3 {
    return (None, None, None, false, true);
  }
  let event = call.arguments.first().and_then(argument_expression).and_then(event_literal);
  let handler = call
    .arguments
    .get(1)
    .and_then(argument_expression)
    .and_then(|expression| identifier_symbol(semantic, expression));
  let (capture, once_or_signal, unknown) = call
    .arguments
    .get(2)
    .and_then(argument_expression)
    .map_or((Some(false), false, false), parse_listener_options);
  let unknown_args = unknown || event.is_none() || handler.is_none() || capture.is_none();
  (event, handler, capture, once_or_signal, unknown_args)
}

fn event_literal(expression: &Expression<'_>) -> Option<String> {
  match expression.get_inner_expression() {
    Expression::StringLiteral(literal) => Some(literal.value.to_string()),
    Expression::TemplateLiteral(literal) if literal.expressions.is_empty() => literal
      .quasis
      .first()
      .map(|quasi| quasi.value.cooked.as_ref().unwrap_or(&quasi.value.raw).to_string()),
    _ => None,
  }
}

fn parse_listener_options(expression: &Expression<'_>) -> (Option<bool>, bool, bool) {
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(literal) => (Some(literal.value), false, false),
    Expression::ObjectExpression(object) => {
      let mut capture = Some(false);
      let mut once_or_signal = false;
      for property in &object.properties {
        match property {
          ObjectPropertyKind::SpreadProperty(_) => return (None, false, true),
          ObjectPropertyKind::ObjectProperty(property) => {
            if property.kind != PropertyKind::Init {
              return (None, false, true);
            }
            let Some(name) = static_property_name(property) else {
              return (None, false, true);
            };
            match name {
              "capture" => match property.value.get_inner_expression() {
                Expression::BooleanLiteral(literal) => capture = Some(literal.value),
                _ => return (None, false, true),
              },
              "once" => match property.value.get_inner_expression() {
                Expression::BooleanLiteral(literal) => {
                  if literal.value {
                    once_or_signal = true;
                  }
                }
                _ => return (None, false, true),
              },
              "signal" => once_or_signal = true,
              _ => {}
            }
          }
        }
      }
      (capture, once_or_signal, false)
    }
    _ => (None, false, true),
  }
}

fn static_property_name<'a>(property: &'a oxc_ast::ast::ObjectProperty<'a>) -> Option<&'a str> {
  match &property.key {
    oxc_ast::ast::PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.as_str()),
    oxc_ast::ast::PropertyKey::StringLiteral(literal) => Some(literal.value.as_str()),
    _ => None,
  }
}

fn alloc_of_expression(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  index: &LifetimeIndex,
  depth: usize,
) -> Option<AllocId> {
  if index.identity.event_target_poisoned || depth >= ALIAS_BOUND {
    return None;
  }
  match expression.get_inner_expression() {
    Expression::NewExpression(new_expression)
      if is_global_event_target(semantic, &new_expression.callee) =>
    {
      Some(AllocId(new_expression.span.start))
    }
    Expression::Identifier(identifier) => {
      let symbol_id = referenced_symbol(semantic, identifier)?;
      if index.identity.written_payload.contains(&symbol_id) {
        return None;
      }
      if let Some(&alloc) = index.identity.alloc_of_symbol.get(&symbol_id) {
        return Some(alloc);
      }
      let declaration = semantic.scoping().symbol_declaration(symbol_id);
      let AstKind::VariableDeclarator(declarator) = semantic.nodes().kind(declaration) else {
        return None;
      };
      if declarator.kind != VariableDeclarationKind::Const {
        return None;
      }
      alloc_of_expression(semantic, declarator.init.as_ref()?, index, depth.saturating_add(1))
    }
    _ => None,
  }
}

fn stable_alloc_of_symbol(
  identity: &CleanupIdentityIndex,
  symbol_id: SymbolId,
  root: SymbolId,
) -> Option<AllocId> {
  if identity.written_payload.contains(&symbol_id) || identity.written_payload.contains(&root) {
    return None;
  }
  identity
    .alloc_of_symbol
    .get(&symbol_id)
    .or_else(|| identity.alloc_of_symbol.get(&root))
    .or_else(|| identity.payload_alloc.get(&root))
    .copied()
}

fn is_global_event_target(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
) -> bool {
  let Some(identifier) = expression.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  identifier.name.as_str() == "EventTarget" && identifier.is_global_reference(semantic.scoping())
}

fn mark_event_target_value(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  index: &mut LifetimeIndex,
) {
  match expression.get_inner_expression() {
    Expression::Identifier(identifier)
      if identifier.name.as_str() == "EventTarget"
        && identifier.is_global_reference(semantic.scoping()) =>
    {
      index.identity.event_target_poisoned = true;
    }
    Expression::StaticMemberExpression(member)
      if member.property.name.as_str() == "prototype"
        && is_global_event_target(semantic, &member.object) =>
    {
      index.identity.event_target_poisoned = true;
    }
    _ => {}
  }
}

fn mark_method_mutation_target(
  semantic: &oxc_semantic::Semantic<'_>,
  target: &AssignmentTarget<'_>,
  index: &mut LifetimeIndex,
) {
  match target {
    AssignmentTarget::StaticMemberExpression(member) => {
      if member.property.name.as_str() == ADD_LISTENER
        || member.property.name.as_str() == REMOVE_LISTENER
      {
        mark_method_mutation_from_expression(semantic, &member.object, index);
      }
      if member.property.name.as_str() == "prototype"
        && is_global_event_target(semantic, &member.object)
      {
        index.identity.event_target_poisoned = true;
      }
    }
    AssignmentTarget::ComputedMemberExpression(member) => {
      if let Expression::StringLiteral(literal) = member.expression.get_inner_expression()
        && (literal.value.as_str() == ADD_LISTENER || literal.value.as_str() == REMOVE_LISTENER)
      {
        mark_method_mutation_from_expression(semantic, &member.object, index);
      }
    }
    AssignmentTarget::AssignmentTargetIdentifier(identifier)
      if identifier.name.as_str() == "EventTarget"
        && identifier.is_global_reference(semantic.scoping()) =>
    {
      index.identity.event_target_poisoned = true;
    }
    _ => {}
  }
}

fn mark_method_mutation_from_expression(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  index: &mut LifetimeIndex,
) {
  let inner = expression.get_inner_expression();
  if is_global_event_target(semantic, inner) {
    index.identity.event_target_poisoned = true;
    return;
  }
  if let Expression::StaticMemberExpression(member) = inner
    && member.property.name.as_str() == "prototype"
    && is_global_event_target(semantic, &member.object)
  {
    index.identity.event_target_poisoned = true;
    return;
  }
  if let Some(symbol_id) = identifier_symbol(semantic, inner) {
    let root = index.identity.root_of(symbol_id);
    if index.identity.written_payload.contains(&symbol_id)
      || index.identity.written_payload.contains(&root)
    {
      index.identity.unknown_method_mutation = true;
      return;
    }
    let alloc = stable_alloc_of_symbol(&index.identity, symbol_id, root);
    index.identity.method_mutated.insert(root);
    if let Some(alloc) = alloc {
      index.identity.method_mutated_alloc.insert(alloc);
    } else {
      index.identity.unknown_method_mutation = true;
    }
    return;
  }
  if let Some(symbol_id) = source_value_symbol(semantic, inner) {
    let root = index.identity.root_of(symbol_id);
    index.identity.method_mutated.insert(root);
    if let Some(&alloc) = index.identity.payload_alloc.get(&root) {
      index.identity.method_mutated_alloc.insert(alloc);
    }
    return;
  }
  index.identity.unknown_method_mutation = true;
}

fn mark_escape_from_expression(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  index: &mut LifetimeIndex,
) {
  if let Some(symbol_id) = identifier_symbol(semantic, expression) {
    mark_symbol_escaped(index, symbol_id);
  }
  if let Some(symbol_id) = source_value_symbol(semantic, expression) {
    mark_symbol_escaped(index, symbol_id);
  }
}

fn record_payload_flow(identity: &mut CleanupIdentityIndex, source: SymbolId, dest: SymbolId) {
  if source != dest {
    identity.payload_flow.entry(source).or_default().push(dest);
  }
}

fn mark_symbol_escaped(index: &mut LifetimeIndex, symbol_id: SymbolId) {
  let root = index.identity.root_of(symbol_id);
  index.identity.escaped_symbol.insert(symbol_id);
  index.identity.escaped_symbol.insert(root);
  index.identity.pending_escapes.push(symbol_id);
}

fn mark_handle_escape_from_expression(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  index: &mut LifetimeIndex,
) {
  if let Some(symbol_id) = identifier_symbol(semantic, expression) {
    mark_handle_escape_symbol(index, symbol_id);
  }
}

fn mark_handle_escape_symbol(index: &mut LifetimeIndex, symbol_id: SymbolId) {
  index.identity.handle_escaped.insert(symbol_id);
  index.identity.handle_escaped.insert(index.identity.root_of(symbol_id));
}

fn is_recognized_api(
  semantic: &oxc_semantic::Semantic<'_>,
  call: &CallExpression<'_>,
  vue_exports: &HashMap<SymbolId, String>,
) -> bool {
  matches!(
    vue_callee_export(semantic, &call.callee, vue_exports),
    Some(
      "ref"
        | "shallowRef"
        | "watch"
        | "watchEffect"
        | "watchPostEffect"
        | "watchSyncEffect"
        | "onWatcherCleanup"
        | "onScopeDispose"
        | "nextTick"
        | "effectScope"
    )
  )
}

fn identifier_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
) -> Option<SymbolId> {
  expression
    .get_inner_expression()
    .get_identifier_reference()
    .and_then(|identifier| referenced_symbol(semantic, identifier))
}

fn source_value_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
) -> Option<SymbolId> {
  let Expression::StaticMemberExpression(member) = expression.get_inner_expression() else {
    return None;
  };
  if member.property.name.as_str() != "value" {
    return None;
  }
  identifier_symbol(semantic, &member.object)
}

fn as_call<'a>(expression: &'a Expression<'a>) -> Option<&'a CallExpression<'a>> {
  match expression.get_inner_expression() {
    Expression::CallExpression(call) => Some(call),
    _ => None,
  }
}

fn watch_schedule(call: &CallExpression<'_>) -> WatchSchedule {
  let Some(argument) = call.arguments.get(2) else {
    return WatchSchedule::DEFAULT;
  };
  let Some(expression) = argument_expression(argument) else {
    return WatchSchedule { unknown: true, ..WatchSchedule::DEFAULT };
  };
  let Expression::ObjectExpression(object) = expression.get_inner_expression() else {
    return WatchSchedule { unknown: true, ..WatchSchedule::DEFAULT };
  };
  let mut schedule = WatchSchedule::DEFAULT;
  for property in &object.properties {
    match property {
      ObjectPropertyKind::SpreadProperty(_) => {
        return WatchSchedule { unknown: true, ..schedule };
      }
      ObjectPropertyKind::ObjectProperty(property) => {
        if property.kind != PropertyKind::Init || static_property_name(property).is_none() {
          return WatchSchedule { unknown: true, ..schedule };
        }
        match static_property_name(property) {
          Some("immediate") => match property.value.get_inner_expression() {
            Expression::BooleanLiteral(literal) => schedule.immediate = Some(literal.value),
            _ => return WatchSchedule { unknown: true, ..schedule },
          },
          Some("once") => match property.value.get_inner_expression() {
            Expression::BooleanLiteral(literal) => schedule.once = Some(literal.value),
            _ => return WatchSchedule { unknown: true, ..schedule },
          },
          Some("flush") => match property.value.get_inner_expression() {
            Expression::StringLiteral(literal) => {
              schedule.flush = match literal.value.as_str() {
                "pre" => Some(WatchFlush::Pre),
                "post" => Some(WatchFlush::Post),
                "sync" => Some(WatchFlush::Sync),
                _ => return WatchSchedule { unknown: true, ..schedule },
              };
            }
            _ => return WatchSchedule { unknown: true, ..schedule },
          },
          _ => {}
        }
      }
    }
  }
  schedule
}

fn watch_handle_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  mut node_id: NodeId,
) -> Option<SymbolId> {
  loop {
    let parent = semantic.nodes().parent_id(node_id);
    if parent == node_id {
      return None;
    }
    match semantic.nodes().kind(parent) {
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_) => node_id = parent,
      AstKind::VariableDeclarator(declarator) => {
        let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
          return None;
        };
        return binding.symbol_id.get();
      }
      _ => return None,
    }
  }
}

fn uncertain_in_owner(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  owner: Option<NodeId>,
) -> bool {
  let mut current = node_id;
  loop {
    let parent = semantic.nodes().parent_id(current);
    if parent == current {
      return false;
    }
    if owner == Some(parent) {
      return false;
    }
    match semantic.nodes().kind(parent) {
      AstKind::IfStatement(_)
      | AstKind::ForStatement(_)
      | AstKind::ForInStatement(_)
      | AstKind::ForOfStatement(_)
      | AstKind::WhileStatement(_)
      | AstKind::DoWhileStatement(_)
      | AstKind::SwitchStatement(_)
      | AstKind::TryStatement(_)
      | AstKind::ConditionalExpression(_)
      | AstKind::LogicalExpression(_)
      | AstKind::Function(_)
      | AstKind::ArrowFunctionExpression(_) => return true,
      AstKind::Program(_) => return false,
      _ => current = parent,
    }
  }
}

fn span_of(
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  span: Span,
) -> vue_vet_core::SourceSpan {
  source_span(line_index, sfc_source, script_offset, span)
}
