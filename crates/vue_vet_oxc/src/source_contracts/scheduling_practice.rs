//! Scheduling-practice facts: queued watch flush, attached child scopes, and
//! lazy `computedAsync` startup.
//!
//! Closed supported grammar only. Every evaluated unsupported shape stays
//! Unknown (no fact). Thin practice rules consume these facts.

use oxc_ast::{
  AstKind,
  ast::{
    Argument, AssignmentOperator, AssignmentTarget, BinaryOperator, BindingPattern, CallExpression,
    Expression, FunctionBody, ObjectPropertyKind, PropertyKind, Statement, UnaryOperator,
  },
};
use oxc_semantic::{NodeId, SymbolId};
use oxc_span::{GetSpan, Span};
use vue_vet_core::{AttachedEffectScopeFact, LazyComputedAsyncFact, QueuedWatchFlushFact};

use super::index::{AwaitPositionSite, CallInfo, MemberCall, WatchConsumer};
use super::proof::is_ts_wrapper;
use super::shape::{PrimitiveAtom, ShapeHint, primitive_atom, span_key};
use super::timeline;
use super::{Collector, MAX_DEPTH};

#[derive(Clone, Copy)]
struct OrdinaryRef {
  payload: Option<PrimitiveAtom>,
}

struct LastValueWatch {
  source: SymbolId,
  sink: SymbolId,
  callback: NodeId,
  flush_span: Span,
  immediate: Option<bool>,
}

impl Collector<'_> {
  pub(super) fn collect_queued_watch_flush(&mut self, call: &CallExpression<'_>, info: CallInfo) {
    if info.has_spread || info.api != Some("watch") {
      return;
    }
    let Some(watch) = self.last_value_watch(call) else {
      return;
    };
    let Some(source) = self.scheduling_ordinary_primitive_ref(watch.source) else {
      return;
    };
    let Some(sink) = self.scheduling_ordinary_primitive_ref(watch.sink) else {
      return;
    };
    if watch.source == watch.sink {
      return;
    }
    let Some(watch_node) = self.indexes.call_node(call.span) else {
      return;
    };
    let watch_callable = self.indexes.callable_of(watch_node);
    if !self.sink_is_queued_closed(watch.sink, watch.callback, call.span) {
      return;
    }
    if self.handle_stopped_or_paused(call, watch_callable) {
      return;
    }
    if !self.ancestor_runs_survive(watch_callable) {
      return;
    }
    let Some((write_span, cluster_end, cluster_block)) = self.clustered_changed_writes(
      watch.source,
      source.payload,
      sink.payload,
      watch.immediate,
      call.span,
      watch_callable,
    ) else {
      return;
    };
    let Some(tick) = self.next_tick_after(cluster_end, watch_callable, cluster_block) else {
      return;
    };
    if !self.watcher_active_through(call, tick.head.offset) {
      return;
    }
    if !self.sink_reads_only_after(watch.sink, tick.head.offset) {
      return;
    }
    let Some(demand_span) = self.first_sink_read_after(watch.sink, tick.head.offset) else {
      return;
    };
    self.facts.scheduling_practice.queued_watch_flush.push(QueuedWatchFlushFact {
      flush_span: self.span(watch.flush_span),
      write_span: self.span(write_span),
      demand_span: self.span(demand_span),
      sink_name: self.symbol_name(watch.sink),
    });
  }

  pub(super) fn collect_attached_effect_scope(
    &mut self,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    if info.has_spread || info.api != Some("effectScope") {
      return;
    }
    let Some(true_span) = detached_true_span(call) else {
      return;
    };
    let Some(child_root) = self.assigned_const_root(call.span) else {
      return;
    };
    if self.is_exported(child_root) || self.is_parameter(child_root) {
      return;
    }
    let Some(child_node) = self.indexes.call_node(call.span) else {
      return;
    };
    let Some(run) = self.enclosing_parent_run(child_node) else {
      return;
    };
    let Some(parent) = self.attached_parent_scope(run.parent) else {
      return;
    };
    if parent == child_root {
      return;
    }
    if !self.closed_child_handle(child_root, run.callback, call.span) {
      return;
    }
    let Some(cleanup_span) = self.parent_disposes_child(child_root, run.callback) else {
      return;
    };
    let Some(watch_call) = self.child_run_watch(child_root, run.callback) else {
      return;
    };
    let Some(watch) = self.last_value_watch(watch_call) else {
      return;
    };
    let Some(source) = self.scheduling_ordinary_primitive_ref(watch.source) else {
      return;
    };
    let Some(sink) = self.scheduling_ordinary_primitive_ref(watch.sink) else {
      return;
    };
    if !self.sink_is_scope_closed(watch.sink, watch.callback) {
      return;
    }
    if self.child_explicit_pause(child_root) {
      return;
    }
    let run_offset = self.span(run.run_span).offset;
    let Some(pause) = self.sole_method_after(parent, "pause", run_offset, run.callable, run.block)
    else {
      return;
    };
    let Some(resume) =
      self.sole_method_after(parent, "resume", pause.offset, run.callable, run.block)
    else {
      return;
    };
    if resume.offset <= pause.offset {
      return;
    }
    if self.method_between(parent, "stop", pause.offset, resume.offset) {
      return;
    }
    let Some(write_span) = self.changed_writes_between(
      watch.source,
      source.payload,
      sink.payload,
      watch.immediate,
      pause.offset,
      resume.offset,
      run.callable,
      run.block,
    ) else {
      return;
    };
    if self.live_sink_observer(watch.sink, pause.offset, resume.offset, watch.callback) {
      return;
    }
    if self.first_sink_read_after(watch.sink, resume.offset).is_none() {
      return;
    }
    if !self.closed_parent_handle(parent, run.run_span) {
      return;
    }
    self.facts.scheduling_practice.attached_effect_scope.push(AttachedEffectScopeFact {
      detached_span: self.span(true_span),
      parent_span: self.span(parent_init_span(self, parent).unwrap_or(run.run_span)),
      cleanup_span: self.span(cleanup_span),
      pause_span: self.span(pause.span),
      write_span: self.span(write_span),
    });
  }

  pub(super) fn collect_lazy_computed_async(&mut self, call: &CallExpression<'_>, info: CallInfo) {
    if info.has_spread || info.api != Some("computedAsync") {
      return;
    }
    if nth_expr(call, 3).is_some() {
      return;
    }
    if !self.eager_computed_async_options(call) {
      return;
    }
    let Some(producer) = nth_expr(call, 0) else {
      return;
    };
    let Some(initial) = nth_expr(call, 1) else {
      return;
    };
    let Some(initial_atom) = self.indexes.primitive_at(initial.span()) else {
      return;
    };
    let Some(source) = self.pure_async_source(producer) else {
      return;
    };
    if self.scheduling_ordinary_primitive_ref(source).is_none() {
      return;
    }
    let Some(result) = self.assigned_const_root(call.span) else {
      return;
    };
    if self.is_exported(result) || self.is_parameter(result) || result == source {
      return;
    }
    if !self.result_is_readonly_closed(result, call.span) {
      return;
    }
    let Some(call_node) = self.indexes.call_node(call.span) else {
      return;
    };
    let call_callable = self.indexes.callable_of(call_node);
    let Some(call_block) = self.indexes.block_of(call_node) else {
      return;
    };
    let call_offset = self.span(call.span).offset;
    let Some(demand_span) = self.loading_tolerant_demand(result, initial_atom, call.span) else {
      return;
    };
    let demand_offset = self.span(demand_span).offset;
    if demand_offset <= call_offset {
      return;
    }
    if self.result_demanded_before(result, demand_offset, call_offset) {
      return;
    }
    let Some(source_span) =
      self.changed_write_before(source, call_offset, demand_offset, call_callable, call_block)
    else {
      return;
    };
    self.facts.scheduling_practice.lazy_computed_async.push(LazyComputedAsyncFact {
      call_span: self.span(call.span),
      source_span: self.span(source_span),
      demand_span: self.span(demand_span),
    });
  }

  fn last_value_watch(&self, call: &CallExpression<'_>) -> Option<LastValueWatch> {
    let source_expr = nth_expr(call, 0)?;
    let callback_expr = nth_expr(call, 1)?;
    let source_id = ident_symbol(source_expr, self)?;
    let source = self.indexes.root_of(source_id);
    self.indexes.note_query();
    let (callback_id, sink) = self.last_value_callback(callback_expr)?;
    if sink == source {
      return None;
    }
    let (flush_span, immediate) = self.sync_flush_options(call)?;
    Some(LastValueWatch { source, sink, callback: callback_id, flush_span, immediate })
  }

  fn last_value_callback(&self, expression: &Expression<'_>) -> Option<(NodeId, SymbolId)> {
    let inner = expression.get_inner_expression();
    let (node_id, param, body) = match inner {
      Expression::ArrowFunctionExpression(arrow) => {
        if arrow.r#async || arrow.params.rest.is_some() || arrow.params.items.len() != 1 {
          return None;
        }
        let param = binding_ident(arrow.params.items.first()?)?;
        (self.indexes.callable_node(arrow.span)?, param, &arrow.body)
      }
      Expression::FunctionExpression(function) => {
        if function.r#async
          || function.generator
          || function.params.rest.is_some()
          || function.params.items.len() != 1
        {
          return None;
        }
        let param = binding_ident(function.params.items.first()?)?;
        (self.indexes.callable_node(function.span)?, param, function.body.as_ref()?)
      }
      _ => return None,
    };
    let assignment = match inner {
      Expression::ArrowFunctionExpression(arrow) if arrow.expression => {
        let Statement::ExpressionStatement(stmt) = body.statements.first()? else {
          return None;
        };
        match stmt.expression.get_inner_expression() {
          Expression::AssignmentExpression(assign) => assign,
          _ => return None,
        }
      }
      _ => {
        if body.statements.len() != 1 {
          self.indexes.add_queries(body.statements.len() as u64);
          return None;
        }
        assignment_statement(body.statements.first()?)?
      }
    };
    if assignment.operator != AssignmentOperator::Assign {
      return None;
    }
    let AssignmentTarget::StaticMemberExpression(member) = &assignment.left else {
      return None;
    };
    if member.property.name.as_str() != "value" {
      return None;
    }
    let object = member.object.get_inner_expression().get_identifier_reference()?;
    let sink = self.indexes.root_of(self.reference_symbol(object)?);
    let rhs = assignment.right.get_inner_expression();
    let Expression::Identifier(value) = rhs else {
      return None;
    };
    if self.reference_symbol(value)? != param {
      return None;
    }
    Some((node_id, sink))
  }

  fn sync_flush_options(&self, call: &CallExpression<'_>) -> Option<(Span, Option<bool>)> {
    let options = nth_expr(call, 2)?;
    let options = options.get_inner_expression();
    let Expression::ObjectExpression(_) = options else {
      return None;
    };
    let span = options.span();
    if self.indexes.object_has_spread(span) || !self.scheduling_option_keys_closed(span) {
      return None;
    }
    let closed = self.indexes.closed_options(span, true, true, false, false);
    let flush = match closed.flush {
      super::shape::OptionValue::Known(super::shape::Literal::String) => {
        let super::index::ObjectProp::Value(value) = self.indexes.object_prop(span, "flush")?
        else {
          return None;
        };
        if self.indexes.scalar(value) != Some(super::shape::Scalar::Str(super::shape::SYNC_FLUSH)) {
          return None;
        }
        value
      }
      _ => return None,
    };
    let immediate = match closed.immediate {
      super::shape::OptionValue::Absent => None,
      super::shape::OptionValue::Known(super::shape::Literal::Bool(value)) => Some(value),
      super::shape::OptionValue::Known(_) | super::shape::OptionValue::Unknown => return None,
    };
    Some((flush, immediate))
  }

  fn scheduling_option_keys_closed(&self, span: Span) -> bool {
    let Some(entries) = self.indexes.object_index.objects.get(&super::shape::span_key(span)) else {
      self.indexes.note_query();
      return false;
    };
    entries.iter().all(|entry| {
      self.indexes.note_query();
      match entry {
        super::index::ObjectEntry::Data { name, .. } => {
          matches!(name.as_str(), "flush" | "immediate")
        }
        _ => false,
      }
    })
  }

  fn scheduling_ordinary_primitive_ref(&mut self, root: SymbolId) -> Option<OrdinaryRef> {
    self.indexes.note_query();
    if self.indexes.reassigned.contains(&root)
      || self.indexes.construction_mutated(root)
      || self.is_parameter(root)
      || self.is_exported(root)
      || !self.symbol_is_const(root)
    {
      return None;
    }
    let init_span = self.indexes.init_span.get(&root).copied()?;
    self.indexes.note_query();
    let ShapeHint::Call(call_span) = self.indexes.hints.get(&span_key(init_span)).copied()? else {
      return None;
    };
    self.indexes.note_query();
    let info = self.indexes.calls.get(&span_key(call_span)).copied()?;
    if info.has_spread || !matches!(info.api, Some("ref" | "shallowRef")) {
      return None;
    }
    let payload = match info.first_arg {
      None => Some(PrimitiveAtom::Undefined),
      Some(argument) => match self.classify_span(argument, MAX_DEPTH) {
        super::shape::Shape::Primitive(_) => self.indexes.primitive_at(argument),
        _ => return None,
      },
    };
    Some(OrdinaryRef { payload })
  }

  fn clustered_changed_writes(
    &self,
    root: SymbolId,
    init: Option<PrimitiveAtom>,
    sink_init: Option<PrimitiveAtom>,
    immediate: Option<bool>,
    watch_span: Span,
    callable: Option<NodeId>,
  ) -> Option<(Span, usize, NodeId)> {
    let watch_offset = self.span(watch_span).offset;
    let mut cursor = init;
    let mut baseline = init;
    let mut first_changed: Option<Span> = None;
    let mut cluster_start: Option<usize> = None;
    let mut cluster_block: Option<NodeId> = None;
    let mut last_offset: Option<usize> = None;
    let mut last_atom: Option<PrimitiveAtom> = None;
    let mut changed = 0u8;
    for write in self.indexes.value_writes_of(root) {
      self.indexes.note_query();
      if write.offset <= watch_offset {
        let atom = self.indexes.primitive_at(write.rhs)?;
        cursor = Some(atom);
        baseline = Some(atom);
        continue;
      }
      if write.callable != callable {
        return None;
      }
      if let (Some(start), Some(block)) = (cluster_start, cluster_block) {
        if write.block != block {
          return None;
        }
        if self.indexes.has_event_between(block, start, write.offset) {
          break;
        }
      } else {
        cluster_start = Some(write.offset);
        cluster_block = Some(write.block);
      }
      let next = self.indexes.primitive_at(write.rhs)?;
      let is_changed = cursor.is_none_or(|prior| !self.indexes.atoms_object_is(prior, next));
      if is_changed {
        changed = changed.saturating_add(1);
        if first_changed.is_none() {
          first_changed = Some(write.rhs);
        }
      }
      cursor = Some(next);
      last_atom = Some(next);
      last_offset = Some(write.offset);
    }
    if changed < 2 {
      return None;
    }
    let baseline_atom = baseline?;
    let final_atom = last_atom?;
    if !self.queued_delivery_or_equivalent(baseline_atom, final_atom, sink_init, immediate) {
      return None;
    }
    Some((first_changed?, last_offset?, cluster_block?))
  }

  fn queued_delivery_or_equivalent(
    &self,
    baseline: PrimitiveAtom,
    final_atom: PrimitiveAtom,
    sink_init: Option<PrimitiveAtom>,
    immediate: Option<bool>,
  ) -> bool {
    if !self.indexes.atoms_object_is(final_atom, baseline) {
      return true;
    }
    let sink_now = if immediate == Some(true) { Some(baseline) } else { sink_init };
    sink_now.is_some_and(|sink| self.indexes.atoms_object_is(sink, baseline))
  }

  fn next_tick_after(
    &self,
    offset: usize,
    callable: Option<NodeId>,
    block: NodeId,
  ) -> Option<&AwaitPositionSite> {
    let work = self.indexes.work_counter();
    timeline::after(work, self.indexes.awaits_of(callable), offset).iter().find(|site| {
      self.indexes.note_query();
      site.head.callable == callable && site.block == block && site.callee_api == Some("nextTick")
    })
  }

  fn sink_reads_only_after(&self, root: SymbolId, offset: usize) -> bool {
    let work = self.indexes.work_counter();
    timeline::through(work, self.indexes.scheduling_value_reads_of(root), offset).is_empty()
  }

  fn first_sink_read_after(&self, root: SymbolId, offset: usize) -> Option<Span> {
    let work = self.indexes.work_counter();
    timeline::first_after(work, self.indexes.scheduling_value_reads_of(root), offset)
      .map(|read| read.span)
  }

  fn live_sink_observer(&self, root: SymbolId, start: usize, end: usize, producer: NodeId) -> bool {
    for read in self.indexes.scheduling_value_reads_of(root) {
      self.indexes.note_query();
      if read.callable == Some(producer) {
        continue;
      }
      if read.offset > start && read.offset < end {
        return true;
      }
      let Some(callable) = read.callable else {
        continue;
      };
      let Some(effect) = self.indexes.effect_callback(callable) else {
        continue;
      };
      if effect.offset < end && effect.api == "watchSyncEffect" {
        return true;
      }
    }
    false
  }

  fn sink_is_queued_closed(&self, root: SymbolId, callback: NodeId, watch_span: Span) -> bool {
    for write in self.indexes.value_writes_of(root) {
      self.indexes.note_query();
      if write.callable != Some(callback) {
        return false;
      }
    }
    self.closed_symbol(root, |collector, node_id| {
      collector.allowed_queued_sink_reference(node_id, callback, watch_span)
    })
  }

  fn sink_is_scope_closed(&self, root: SymbolId, callback: NodeId) -> bool {
    for write in self.indexes.value_writes_of(root) {
      self.indexes.note_query();
      if write.callable != Some(callback) {
        return false;
      }
    }
    self.closed_symbol(root, |collector, node_id| {
      collector.allowed_value_member(node_id) || collector.is_declaration(node_id)
    })
  }

  fn allowed_queued_sink_reference(
    &self,
    node_id: NodeId,
    _callback: NodeId,
    watch_span: Span,
  ) -> bool {
    if self.is_declaration(node_id) {
      return true;
    }
    if self.allowed_value_member(node_id) {
      return true;
    }
    self.watch_argument_reference(node_id, watch_span)
  }

  fn watcher_active_through(&self, call: &CallExpression<'_>, tick_offset: usize) -> bool {
    let Some(handle) = self.assigned_const_root(call.span) else {
      return !self.call_is_reassigned(call.span);
    };
    if self.indexes.reassigned.contains(&handle) || self.is_exported(handle) {
      self.indexes.note_query();
      return false;
    }
    for symbol_id in self.indexes.symbols_for_root(handle) {
      for reference in self.semantic.symbol_references(symbol_id) {
        self.indexes.note_query();
        let node_id = reference.node_id();
        if self.is_declaration(node_id) {
          continue;
        }
        let ident_span = self.semantic.nodes().kind(node_id).span();
        let parent_id = self.semantic.nodes().parent_id(node_id);
        self.indexes.note_query();
        match self.semantic.nodes().kind(parent_id) {
          AstKind::CallExpression(stop) if callee_span_matches(stop, ident_span) => {
            if self.span(stop.span).offset < tick_offset {
              return false;
            }
          }
          wrapper
            if is_ts_wrapper(wrapper) || matches!(wrapper, AstKind::VariableDeclarator(_)) => {}
          _ => return false,
        }
      }
    }
    true
  }

  fn handle_stopped_or_paused(&self, call: &CallExpression<'_>, _callable: Option<NodeId>) -> bool {
    self.assigned_const_root(call.span).is_some_and(|handle| {
      self.indexes.member_calls_of(handle).iter().any(|site| {
        self.indexes.note_query();
        matches!(site.method, "pause" | "resume" | "stop")
      })
    })
  }

  fn call_is_reassigned(&self, call_span: Span) -> bool {
    let Some(node_id) = self.indexes.call_node(call_span) else {
      return true;
    };
    matches!(self.semantic.nodes().parent_kind(node_id), AstKind::AssignmentExpression(_))
  }

  fn enclosing_parent_run(&self, node_id: NodeId) -> Option<ParentRun> {
    let callback = self.indexes.callable_of(node_id)?;
    self.indexes.note_query();
    let mut current = callback;
    for _ in 0..8 {
      let parent_id = self.semantic.nodes().parent_id(current);
      self.indexes.note_query();
      match self.semantic.nodes().kind(parent_id) {
        wrapper if is_ts_wrapper(wrapper) || matches!(wrapper, AstKind::ExpressionStatement(_)) => {
          current = parent_id;
        }
        AstKind::CallExpression(call) => {
          let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression()
          else {
            return None;
          };
          if member.property.name.as_str() != "run" {
            return None;
          }
          let object = member.object.get_inner_expression().get_identifier_reference()?;
          let parent = self.indexes.root_of(self.reference_symbol(object)?);
          return Some(ParentRun {
            parent,
            run_span: call.span,
            callback,
            callable: self.indexes.callable_of(parent_id),
            block: self.indexes.block_of(parent_id)?,
          });
        }
        _ => return None,
      }
    }
    None
  }

  fn attached_parent_scope(&self, root: SymbolId) -> Option<SymbolId> {
    if self.indexes.reassigned.contains(&root)
      || self.is_parameter(root)
      || self.is_exported(root)
      || !self.symbol_is_const(root)
    {
      return None;
    }
    let init_span = self.indexes.init_span.get(&root).copied()?;
    self.indexes.note_query();
    let ShapeHint::Call(call_span) = self.indexes.hints.get(&span_key(init_span)).copied()? else {
      return None;
    };
    self.indexes.note_query();
    let info = self.indexes.calls.get(&span_key(call_span)).copied()?;
    if info.has_spread || info.api != Some("effectScope") {
      return None;
    }
    info.first_arg.map_or(Some(root), |argument| {
      matches!(self.indexes.primitive_at(argument), Some(PrimitiveAtom::Bool(false)))
        .then_some(root)
    })
  }

  fn closed_child_handle(&self, root: SymbolId, callback: NodeId, ctor_span: Span) -> bool {
    self.closed_symbol(root, |collector, node_id| {
      collector.is_declaration(node_id)
        || collector.scope_method_reference(node_id, &["run", "stop"])
        || collector.call_argument_of(node_id, ctor_span)
        || collector.on_scope_dispose_argument(node_id, callback)
    })
  }

  fn closed_parent_handle(&self, root: SymbolId, run_span: Span) -> bool {
    self.closed_symbol(root, |collector, node_id| {
      collector.is_declaration(node_id)
        || collector.scope_method_reference(node_id, &["run", "pause", "resume", "stop"])
        || collector.call_argument_of(node_id, run_span)
    })
  }

  fn parent_disposes_child(&self, child: SymbolId, callback: NodeId) -> Option<Span> {
    let mut found = None;
    for site in self.indexes.disposals_of(Some(callback)) {
      self.indexes.note_query();
      if site.callable != Some(callback) || site.child != Some(child) {
        return None;
      }
      let node_id = self.indexes.call_node(site.span)?;
      let AstKind::CallExpression(call) = self.semantic.nodes().kind(node_id) else {
        return None;
      };
      let dispose = nth_expr(call, 0)?;
      if !self.stop_only_callback(dispose, child) {
        return None;
      }
      if found.is_some() {
        return None;
      }
      found = Some(site.span);
    }
    found
  }

  fn stop_only_callback(&self, expression: &Expression<'_>, child: SymbolId) -> bool {
    let inner = expression.get_inner_expression();
    let body = match inner {
      Expression::ArrowFunctionExpression(arrow) => {
        if arrow.r#async || arrow.params.rest.is_some() || !arrow.params.items.is_empty() {
          return false;
        }
        &arrow.body
      }
      Expression::FunctionExpression(function) => {
        if function.r#async
          || function.generator
          || function.params.rest.is_some()
          || !function.params.items.is_empty()
        {
          return false;
        }
        let Some(body) = function.body.as_ref() else {
          return false;
        };
        body
      }
      _ => return false,
    };
    if body.statements.len() != 1 {
      self.indexes.add_queries(body.statements.len() as u64);
      return false;
    }
    let Some(Statement::ExpressionStatement(stmt)) = body.statements.first() else {
      return false;
    };
    let Expression::CallExpression(call) = stmt.expression.get_inner_expression() else {
      return false;
    };
    self.is_method_on(call, child, "stop")
  }

  fn child_run_watch(&self, child: SymbolId, callback: NodeId) -> Option<&CallExpression<'_>> {
    let run = self.indexes.member_calls_of(child).iter().find(|site| {
      self.indexes.note_query();
      site.method == "run" && site.callable == Some(callback)
    })?;
    let node_id = self.indexes.call_node(run.span)?;
    let AstKind::CallExpression(call) = self.semantic.nodes().kind(node_id) else {
      return None;
    };
    let runner = nth_expr(call, 0)?;
    let inner = runner.get_inner_expression();
    let body = match inner {
      Expression::ArrowFunctionExpression(arrow) => {
        if arrow.r#async || !arrow.params.items.is_empty() {
          return None;
        }
        &arrow.body
      }
      Expression::FunctionExpression(function) => {
        if function.r#async || function.generator || !function.params.items.is_empty() {
          return None;
        }
        function.body.as_ref()?
      }
      _ => return None,
    };
    if body.statements.len() != 1 {
      self.indexes.add_queries(body.statements.len() as u64);
      return None;
    }
    let Statement::ExpressionStatement(stmt) = body.statements.first()? else {
      return None;
    };
    let Expression::CallExpression(watch) = stmt.expression.get_inner_expression() else {
      return None;
    };
    let info = self.indexes.calls.get(&span_key(watch.span)).copied()?;
    (info.api == Some("watch") && !info.has_spread).then_some(watch)
  }

  fn child_explicit_pause(&self, child: SymbolId) -> bool {
    self.indexes.member_calls_of(child).iter().any(|site| {
      self.indexes.note_query();
      matches!(site.method, "pause" | "resume")
    })
  }

  fn sole_method_after(
    &self,
    root: SymbolId,
    method: &'static str,
    after: usize,
    callable: Option<NodeId>,
    block: NodeId,
  ) -> Option<&MemberCall> {
    let mut found = None;
    let work = self.indexes.work_counter();
    for site in timeline::after(work, self.indexes.member_calls_of(root), after) {
      self.indexes.note_query();
      if site.method != method || site.callable != callable || site.block != block {
        continue;
      }
      if found.is_some() {
        return None;
      }
      found = Some(site);
    }
    found
  }

  fn method_between(&self, root: SymbolId, method: &'static str, start: usize, end: usize) -> bool {
    let work = self.indexes.work_counter();
    timeline::between(work, self.indexes.member_calls_of(root), start, end).iter().any(|site| {
      self.indexes.note_query();
      site.method == method
    })
  }

  #[expect(
    clippy::too_many_arguments,
    reason = "batch delivery needs source, sink, window, and owner"
  )]
  fn changed_writes_between(
    &self,
    root: SymbolId,
    init: Option<PrimitiveAtom>,
    sink_init: Option<PrimitiveAtom>,
    immediate: Option<bool>,
    start: usize,
    end: usize,
    callable: Option<NodeId>,
    block: NodeId,
  ) -> Option<Span> {
    let mut cursor = init;
    let mut baseline = init;
    let mut first = None;
    let mut last_atom = None;
    let mut count = 0u8;
    for write in self.indexes.value_writes_of(root) {
      self.indexes.note_query();
      if write.offset <= start {
        let atom = self.indexes.primitive_at(write.rhs)?;
        cursor = Some(atom);
        baseline = Some(atom);
        continue;
      }
      if write.offset >= end {
        break;
      }
      if write.callable != callable || write.block != block {
        return None;
      }
      let next = self.indexes.primitive_at(write.rhs)?;
      let is_changed = cursor.is_none_or(|prior| !self.indexes.atoms_object_is(prior, next));
      if is_changed {
        count = count.saturating_add(1);
        if first.is_none() {
          first = Some(write.rhs);
        }
      }
      cursor = Some(next);
      last_atom = Some(next);
    }
    if count < 2 {
      return None;
    }
    let baseline_atom = baseline?;
    let final_atom = last_atom?;
    if !self.queued_delivery_or_equivalent(baseline_atom, final_atom, sink_init, immediate) {
      return None;
    }
    first
  }

  fn eager_computed_async_options(&self, call: &CallExpression<'_>) -> bool {
    let Some(options) = nth_expr(call, 2) else {
      return true;
    };
    let Expression::ObjectExpression(object) = options.get_inner_expression() else {
      return false;
    };
    let mut lazy = None;
    for property in &object.properties {
      self.indexes.note_query();
      match property {
        ObjectPropertyKind::SpreadProperty(_) => return false,
        ObjectPropertyKind::ObjectProperty(prop) => {
          if prop.kind != PropertyKind::Init || prop.method || prop.shorthand || prop.computed {
            return false;
          }
          let Some(name) = prop.key.static_name() else {
            return false;
          };
          if name != "lazy" {
            return false;
          }
          if lazy.is_some() {
            return false;
          }
          let Some(value) = bool_literal(&prop.value) else {
            return false;
          };
          lazy = Some(value);
        }
      }
    }
    matches!(lazy, None | Some(false))
  }

  fn pure_async_source(&self, expression: &Expression<'_>) -> Option<SymbolId> {
    let inner = expression.get_inner_expression();
    match inner {
      Expression::ArrowFunctionExpression(arrow) => {
        if !arrow.r#async || arrow.params.rest.is_some() || !arrow.params.items.is_empty() {
          return None;
        }
        value_from_async_body(&arrow.body, arrow.expression, self)
      }
      Expression::FunctionExpression(function) => {
        if !function.r#async
          || function.generator
          || function.params.rest.is_some()
          || !function.params.items.is_empty()
        {
          return None;
        }
        value_from_async_body(function.body.as_ref()?, false, self)
      }
      _ => None,
    }
  }

  fn result_is_readonly_closed(&self, root: SymbolId, call_span: Span) -> bool {
    if !self.indexes.value_writes_of(root).is_empty() {
      return false;
    }
    if self.indexes.unknown_member_touch.contains(&root) {
      self.indexes.note_query();
      return false;
    }
    self.closed_symbol(root, |collector, node_id| {
      collector.is_declaration(node_id)
        || collector.allowed_value_member(node_id)
        || collector.call_argument_of(node_id, call_span)
        || collector.watch_source_argument(node_id)
    })
  }

  fn loading_tolerant_demand(
    &mut self,
    result: SymbolId,
    initial: PrimitiveAtom,
    call_span: Span,
  ) -> Option<Span> {
    let consumers: Vec<WatchConsumer> = self.indexes.watch_consumers_of(result).to_vec();
    self.indexes.add_queries(consumers.len() as u64);
    for consumer in consumers {
      self.indexes.note_query();
      if consumer.span == call_span {
        continue;
      }
      if !self.consumer_alive_through_settlement(&consumer) {
        continue;
      }
      let Some(node_id) = self.indexes.call_node(consumer.span) else {
        continue;
      };
      let AstKind::CallExpression(call) = self.semantic.nodes().kind(node_id) else {
        continue;
      };
      let Some(callback) = nth_expr(call, 1) else {
        continue;
      };
      if !self.guarded_initial_skip(callback, result, initial) {
        continue;
      }
      return Some(consumer.span);
    }
    None
  }

  fn consumer_alive_through_settlement(&self, consumer: &WatchConsumer) -> bool {
    if consumer.options_unknown || consumer.immediate != Some(true) || consumer.once == Some(true) {
      return false;
    }
    if let Some(handle) = consumer.handle
      && self.handle_invoked_or_uncertain(handle)
    {
      return false;
    }
    self.ancestor_runs_survive(consumer.callable)
  }

  fn handle_invoked_or_uncertain(&self, handle: SymbolId) -> bool {
    if self.indexes.reassigned.contains(&handle) || self.is_exported(handle) {
      self.indexes.note_query();
      return true;
    }
    if self.indexes.member_calls_of(handle).iter().any(|site| {
      self.indexes.note_query();
      matches!(site.method, "pause" | "resume" | "stop")
    }) {
      return true;
    }
    for symbol_id in self.indexes.symbols_for_root(handle) {
      for reference in self.semantic.symbol_references(symbol_id) {
        self.indexes.note_query();
        let node_id = reference.node_id();
        if self.is_declaration(node_id) {
          continue;
        }
        let ident_span = self.semantic.nodes().kind(node_id).span();
        match self.semantic.nodes().parent_kind(node_id) {
          AstKind::CallExpression(call) if callee_span_matches(call, ident_span) => return true,
          wrapper
            if is_ts_wrapper(wrapper) || matches!(wrapper, AstKind::VariableDeclarator(_)) => {}
          _ => return true,
        }
      }
    }
    false
  }

  fn ancestor_runs_survive(&self, start: Option<NodeId>) -> bool {
    let mut current = start;
    for _ in 0..8 {
      let Some(callable) = current else {
        return true;
      };
      if let Some(scope) = self.indexes.run_scope_of(callable) {
        let async_run = self.indexes.is_async_callable(callable);
        for site in self.indexes.member_calls_of(scope) {
          self.indexes.note_query();
          if site.method == "stop" && site.callable != Some(callable) && async_run {
            return false;
          }
        }
      }
      let parent_id = self.semantic.nodes().parent_id(callable);
      let parent = self.indexes.callable_of(parent_id);
      if parent == Some(callable) {
        break;
      }
      current = parent;
    }
    true
  }

  fn guarded_initial_skip(
    &mut self,
    expression: &Expression<'_>,
    result: SymbolId,
    initial: PrimitiveAtom,
  ) -> bool {
    let inner = expression.get_inner_expression();
    let (param, body) = match inner {
      Expression::ArrowFunctionExpression(arrow) => {
        if arrow.r#async || arrow.params.rest.is_some() || arrow.params.items.len() != 1 {
          return false;
        }
        let Some(item) = arrow.params.items.first() else {
          return false;
        };
        let Some(param) = binding_ident(item) else {
          return false;
        };
        (param, &arrow.body)
      }
      Expression::FunctionExpression(function) => {
        if function.r#async
          || function.generator
          || function.params.rest.is_some()
          || function.params.items.len() != 1
        {
          return false;
        }
        let Some(item) = function.params.items.first() else {
          return false;
        };
        let Some(param) = binding_ident(item) else {
          return false;
        };
        let Some(body) = function.body.as_ref() else {
          return false;
        };
        (param, body)
      }
      _ => return false,
    };
    if body.statements.len() != 1 {
      self.indexes.add_queries(body.statements.len() as u64);
      return false;
    }
    let Some(Statement::IfStatement(if_stmt)) = body.statements.first() else {
      return false;
    };
    if if_stmt.alternate.is_some() {
      return false;
    }
    if !self.compares_param_against(if_stmt.test.get_inner_expression(), param, initial) {
      return false;
    }
    let Some(assignment) = assignment_statement(&if_stmt.consequent) else {
      return false;
    };
    if assignment.operator != AssignmentOperator::Assign {
      return false;
    }
    let AssignmentTarget::StaticMemberExpression(member) = &assignment.left else {
      return false;
    };
    if member.property.name.as_str() != "value" {
      return false;
    }
    let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
      return false;
    };
    let Some(sink) = self.reference_symbol(object) else {
      return false;
    };
    let sink = self.indexes.root_of(sink);
    if self.scheduling_ordinary_primitive_ref(sink).is_none() || sink == result {
      return false;
    }
    let Expression::Identifier(value) = assignment.right.get_inner_expression() else {
      return false;
    };
    self.reference_symbol(value) == Some(param)
  }

  fn compares_param_against(
    &self,
    test: &Expression<'_>,
    param: SymbolId,
    initial: PrimitiveAtom,
  ) -> bool {
    let Expression::BinaryExpression(binary) = test.get_inner_expression() else {
      return false;
    };
    if !matches!(binary.operator, BinaryOperator::StrictInequality | BinaryOperator::Inequality) {
      return false;
    }
    let left = binary.left.get_inner_expression();
    let right = binary.right.get_inner_expression();
    let (ident, literal) = if let Expression::Identifier(ident) = left {
      (ident, right)
    } else if let Expression::Identifier(ident) = right {
      (ident, left)
    } else {
      return false;
    };
    if self.reference_symbol(ident) != Some(param) {
      return false;
    }
    self
      .indexes
      .primitive_at(literal.span())
      .or_else(|| {
        self.indexes.note_query();
        primitive_atom(literal)
      })
      .is_some_and(|atom| self.indexes.atoms_js_strict_eq(atom, initial))
  }

  fn result_demanded_before(
    &self,
    root: SymbolId,
    demand_offset: usize,
    call_offset: usize,
  ) -> bool {
    let work = self.indexes.work_counter();
    !timeline::between(
      work,
      self.indexes.scheduling_value_reads_of(root),
      call_offset,
      demand_offset,
    )
    .is_empty()
  }

  fn changed_write_before(
    &self,
    root: SymbolId,
    after: usize,
    before: usize,
    callable: Option<NodeId>,
    block: NodeId,
  ) -> Option<Span> {
    let mut previous = self.ordinary_init_payload(root);
    for write in self.indexes.value_writes_of(root) {
      self.indexes.note_query();
      if write.offset <= after {
        previous = Some(self.indexes.primitive_at(write.rhs)?);
        continue;
      }
      if write.offset >= before {
        break;
      }
      if write.callable != callable || write.block != block {
        return None;
      }
      let next = self.indexes.primitive_at(write.rhs)?;
      if previous.is_none_or(|prior| !self.indexes.atoms_object_is(prior, next)) {
        return Some(write.rhs);
      }
      previous = Some(next);
    }
    None
  }

  fn ordinary_init_payload(&self, root: SymbolId) -> Option<PrimitiveAtom> {
    let init_span = self.indexes.init_span.get(&root).copied()?;
    self.indexes.note_query();
    let ShapeHint::Call(call_span) = self.indexes.hints.get(&span_key(init_span)).copied()? else {
      return None;
    };
    self.indexes.note_query();
    let info = self.indexes.calls.get(&span_key(call_span)).copied()?;
    info
      .first_arg
      .map_or(Some(PrimitiveAtom::Undefined), |argument| self.indexes.primitive_at(argument))
  }

  fn assigned_const_root(&self, call_span: Span) -> Option<SymbolId> {
    let node_id = self.indexes.call_node(call_span)?;
    let mut current = node_id;
    for _ in 0..6 {
      let parent_id = self.semantic.nodes().parent_id(current);
      self.indexes.note_query();
      match self.semantic.nodes().kind(parent_id) {
        wrapper if is_ts_wrapper(wrapper) => current = parent_id,
        AstKind::VariableDeclarator(declarator) => {
          let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
            return None;
          };
          let symbol_id = binding.symbol_id.get()?;
          if !self.symbol_is_const(symbol_id) {
            return None;
          }
          return Some(self.indexes.root_of(symbol_id));
        }
        _ => return None,
      }
    }
    None
  }

  fn closed_symbol(&self, root: SymbolId, allowed: impl Fn(&Self, NodeId) -> bool) -> bool {
    if self.indexes.unknown_member_touch.contains(&root) {
      self.indexes.note_query();
      return false;
    }
    for symbol_id in self.indexes.symbols_for_root(root) {
      if self.is_parameter(symbol_id) {
        return false;
      }
      for reference in self.semantic.symbol_references(symbol_id) {
        self.indexes.note_query();
        if !allowed(self, reference.node_id()) {
          return false;
        }
      }
    }
    true
  }

  fn is_declaration(&self, node_id: NodeId) -> bool {
    self.indexes.note_query();
    matches!(self.semantic.nodes().parent_kind(node_id), AstKind::VariableDeclarator(_))
      || matches!(self.semantic.nodes().kind(node_id), AstKind::BindingIdentifier(_))
  }

  fn allowed_value_member(&self, node_id: NodeId) -> bool {
    self.indexes.note_query();
    let ident_span = self.semantic.nodes().kind(node_id).span();
    let parent_id = self.semantic.nodes().parent_id(node_id);
    self.indexes.note_query();
    match self.semantic.nodes().kind(parent_id) {
      AstKind::StaticMemberExpression(member)
        if member.property.name.as_str() == "value"
          && callee_object_span(&member.object, ident_span) =>
      {
        true
      }
      wrapper if is_ts_wrapper(wrapper) => self.allowed_value_member(parent_id),
      _ => false,
    }
  }

  fn scope_method_reference(&self, node_id: NodeId, methods: &[&str]) -> bool {
    self.indexes.note_query();
    let ident_span = self.semantic.nodes().kind(node_id).span();
    let parent_id = self.semantic.nodes().parent_id(node_id);
    self.indexes.note_query();
    match self.semantic.nodes().kind(parent_id) {
      AstKind::StaticMemberExpression(member)
        if methods.contains(&member.property.name.as_str())
          && callee_object_span(&member.object, ident_span) =>
      {
        true
      }
      wrapper if is_ts_wrapper(wrapper) => self.scope_method_reference(parent_id, methods),
      _ => false,
    }
  }

  fn call_argument_of(&self, node_id: NodeId, call_span: Span) -> bool {
    let ident_span = self.semantic.nodes().kind(node_id).span();
    let mut current = node_id;
    for _ in 0..8 {
      let parent_id = self.semantic.nodes().parent_id(current);
      self.indexes.note_query();
      match self.semantic.nodes().kind(parent_id) {
        wrapper if is_ts_wrapper(wrapper) => current = parent_id,
        AstKind::CallExpression(call) if call.span == call_span => {
          return call.arguments.iter().any(|argument| {
            argument.as_expression().is_some_and(|expression| {
              expression.span() == ident_span
                || expression.get_inner_expression().span() == ident_span
            })
          });
        }
        _ => return false,
      }
    }
    false
  }

  fn watch_source_argument(&self, node_id: NodeId) -> bool {
    let ident_span = self.semantic.nodes().kind(node_id).span();
    let mut current = node_id;
    for _ in 0..8 {
      let parent_id = self.semantic.nodes().parent_id(current);
      self.indexes.note_query();
      match self.semantic.nodes().kind(parent_id) {
        wrapper if is_ts_wrapper(wrapper) => current = parent_id,
        AstKind::CallExpression(call) => {
          let Some(info) = self.indexes.calls.get(&span_key(call.span)).copied() else {
            return false;
          };
          if info.api != Some("watch") {
            return false;
          }
          let Some(first) = nth_expr(call, 0) else {
            return false;
          };
          return first.span() == ident_span || first.get_inner_expression().span() == ident_span;
        }
        _ => return false,
      }
    }
    false
  }

  fn watch_argument_reference(&self, node_id: NodeId, watch_span: Span) -> bool {
    self.call_argument_of(node_id, watch_span)
  }

  fn on_scope_dispose_argument(&self, node_id: NodeId, callback: NodeId) -> bool {
    let _ = callback;
    let mut current = node_id;
    for _ in 0..10 {
      let parent_id = self.semantic.nodes().parent_id(current);
      self.indexes.note_query();
      match self.semantic.nodes().kind(parent_id) {
        AstKind::ParenthesizedExpression(_)
        | AstKind::TSAsExpression(_)
        | AstKind::TSSatisfiesExpression(_)
        | AstKind::TSNonNullExpression(_)
        | AstKind::TSTypeAssertion(_)
        | AstKind::StaticMemberExpression(_)
        | AstKind::CallExpression(_)
        | AstKind::ExpressionStatement(_)
        | AstKind::FunctionBody(_)
        | AstKind::ArrowFunctionExpression(_)
        | AstKind::Function(_) => {
          if let AstKind::CallExpression(call) = self.semantic.nodes().kind(parent_id) {
            let Some(info) = self.indexes.calls.get(&span_key(call.span)).copied() else {
              current = parent_id;
              continue;
            };
            if info.api == Some("onScopeDispose") {
              return true;
            }
          }
          current = parent_id;
        }
        _ => return false,
      }
    }
    false
  }

  fn is_method_on(&self, call: &CallExpression<'_>, root: SymbolId, method: &str) -> bool {
    let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
      return false;
    };
    if member.property.name.as_str() != method {
      return false;
    }
    let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
      return false;
    };
    self.reference_symbol(object).is_some_and(|symbol_id| self.indexes.root_of(symbol_id) == root)
  }
}

struct ParentRun {
  parent: SymbolId,
  run_span: Span,
  callback: NodeId,
  callable: Option<NodeId>,
  block: NodeId,
}

fn nth_expr<'a>(call: &'a CallExpression<'a>, index: usize) -> Option<&'a Expression<'a>> {
  call.arguments.get(index).and_then(Argument::as_expression)
}

fn ident_symbol(expression: &Expression<'_>, collector: &Collector<'_>) -> Option<SymbolId> {
  let identifier = expression.get_inner_expression().get_identifier_reference()?;
  collector.reference_symbol(identifier)
}

fn bool_literal(expression: &Expression<'_>) -> Option<bool> {
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(literal) => Some(literal.value),
    _ => None,
  }
}

fn detached_true_span(call: &CallExpression<'_>) -> Option<Span> {
  let argument = nth_expr(call, 0)?;
  match argument.get_inner_expression() {
    Expression::BooleanLiteral(literal) if literal.value => Some(argument.span()),
    _ => None,
  }
}

fn parent_init_span(collector: &Collector<'_>, root: SymbolId) -> Option<Span> {
  collector.indexes.init_span.get(&root).copied()
}

fn binding_ident(parameter: &oxc_ast::ast::FormalParameter<'_>) -> Option<SymbolId> {
  if parameter.initializer.is_some() || parameter.optional {
    return None;
  }
  match &parameter.pattern {
    BindingPattern::BindingIdentifier(binding) => binding.symbol_id.get(),
    _ => None,
  }
}

fn assignment_statement<'a>(
  statement: &'a Statement<'a>,
) -> Option<&'a oxc_ast::ast::AssignmentExpression<'a>> {
  match statement {
    Statement::ExpressionStatement(expr) => match expr.expression.get_inner_expression() {
      Expression::AssignmentExpression(assign) => Some(assign),
      _ => None,
    },
    Statement::BlockStatement(block) if block.body.len() == 1 => {
      assignment_statement(block.body.first()?)
    }
    _ => None,
  }
}

fn callee_object_span(expression: &Expression<'_>, ident_span: Span) -> bool {
  expression.span() == ident_span || expression.get_inner_expression().span() == ident_span
}

fn callee_span_matches(call: &CallExpression<'_>, ident_span: Span) -> bool {
  let callee = call.callee.get_inner_expression();
  callee.span() == ident_span
    || callee.get_identifier_reference().is_some_and(|ident| ident.span == ident_span)
}

fn value_from_async_body(
  body: &FunctionBody<'_>,
  expression_body: bool,
  collector: &Collector<'_>,
) -> Option<SymbolId> {
  if expression_body {
    let Statement::ExpressionStatement(stmt) = body.statements.first()? else {
      return None;
    };
    return pure_source_expr(&stmt.expression, collector);
  }
  if body.statements.len() != 1 {
    collector.indexes.add_queries(body.statements.len() as u64);
    return None;
  }
  let Statement::ReturnStatement(ret) = body.statements.first()? else {
    return None;
  };
  pure_source_expr(ret.argument.as_ref()?, collector)
}

fn pure_source_expr(expression: &Expression<'_>, collector: &Collector<'_>) -> Option<SymbolId> {
  match expression.get_inner_expression() {
    Expression::StaticMemberExpression(member) if member.property.name.as_str() == "value" => {
      let object = member.object.get_inner_expression().get_identifier_reference()?;
      Some(collector.indexes.root_of(collector.reference_symbol(object)?))
    }
    Expression::BinaryExpression(binary)
      if matches!(
        binary.operator,
        BinaryOperator::Multiplication
          | BinaryOperator::Addition
          | BinaryOperator::Subtraction
          | BinaryOperator::Division
      ) =>
    {
      match (pure_source_expr(&binary.left, collector), pure_source_expr(&binary.right, collector))
      {
        (Some(symbol_id), None) if primitive_atom(&binary.right).is_some() => Some(symbol_id),
        (None, Some(symbol_id)) if primitive_atom(&binary.left).is_some() => Some(symbol_id),
        _ => None,
      }
    }
    Expression::UnaryExpression(unary)
      if matches!(unary.operator, UnaryOperator::UnaryNegation | UnaryOperator::UnaryPlus) =>
    {
      pure_source_expr(&unary.argument, collector)
    }
    _ => None,
  }
}
