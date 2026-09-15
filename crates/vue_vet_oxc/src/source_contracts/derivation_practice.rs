//! Derivation-practice facts: one-way `syncRef` and conditional watch sources.
//!
//! Closed supported grammar only. Every evaluated unsupported shape stays
//! Unknown (no fact). Thin practice rules consume these facts.

use oxc_ast::{
  AstKind,
  ast::{
    Argument, ArrayExpressionElement, AssignmentOperator, AssignmentTarget, BindingPattern,
    CallExpression, Expression, FormalParameter, FunctionBody, ObjectPropertyKind, PropertyKind,
    Statement,
  },
};
use oxc_semantic::SymbolId;
use oxc_span::{GetSpan, Span};
use oxc_syntax::symbol::SymbolFlags;
use vue_vet_core::{ConditionalWatchSourceFact, SyncRefOneWayFact};

use super::index::CallInfo;
use super::shape::{PrimitiveAtom, Shape, ShapeHint, span_key};
use super::{Collector, MAX_DEPTH};

#[derive(Clone, Copy)]
struct OrdinaryRef {
  init_span: Span,
  payload: Option<PrimitiveAtom>,
}

#[derive(Clone, Copy)]
struct ComputedProducer {
  computed_span: Span,
  dep: SymbolId,
}

enum SyncDirection {
  DefaultBoth,
  AlreadyLtr,
  Unknown,
}

#[derive(Clone, Copy, Default)]
enum Effective<T> {
  #[default]
  Absent,
  Known(T),
  Unknown,
}

impl<T: Copy> Effective<T> {
  const fn or_default(self, default: T) -> Self {
    match self {
      Self::Absent => Self::Known(default),
      other => other,
    }
  }
}

#[derive(Clone, Copy)]
pub(super) enum ProducerDemand {
  Unique(Span),
  Shared,
  Escaped,
  None,
}

#[derive(Clone, Copy)]
pub(super) enum SinkOwnership {
  Unique(Span),
  Shared,
  Unknown,
}

#[derive(Clone, Copy)]
enum SinkRole {
  Decl,
  ValueRead,
  SyncRef(Span),
  Other,
}

#[derive(Clone, Copy)]
enum ProducerRole {
  Decl,
  Watch(Span),
  Other,
}

enum ParsedOptions {
  Absent,
  Invalid,
  Present(OptionBag),
}

#[derive(Clone, Copy, Default)]
struct OptionBag {
  direction: Effective<&'static str>,
  flush: Effective<&'static str>,
  deep: Effective<bool>,
  immediate: Effective<bool>,
  once: Effective<bool>,
  transform: Effective<bool>,
  debugger: bool,
}

struct ArrayCallback<'a> {
  guard: SymbolId,
  producer: SymbolId,
  body: &'a FunctionBody<'a>,
}

struct CallSite {
  callable: Option<oxc_semantic::NodeId>,
  block: oxc_semantic::NodeId,
  offset: usize,
}

impl Collector<'_> {
  pub(super) fn collect_sync_ref_one_way(
    &mut self,
    node_id: oxc_semantic::NodeId,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    if info.has_spread || info.api != Some("syncRef") {
      return;
    }
    match self.sync_ref_direction(call) {
      SyncDirection::AlreadyLtr | SyncDirection::Unknown => return,
      SyncDirection::DefaultBoth => {}
    }
    let Some(left_expr) = nth_expr(call, 0) else {
      return;
    };
    let Some(right_expr) = nth_expr(call, 1) else {
      return;
    };
    if nth_expr(call, 3).is_some() {
      return;
    }
    let Some(left_id) = ident_symbol(left_expr, self) else {
      return;
    };
    let Some(right_id) = ident_symbol(right_expr, self) else {
      return;
    };
    let left_root = self.indexes.root_of(left_id);
    let right_root = self.indexes.root_of(right_id);
    self.indexes.note_query();
    if left_root == right_root {
      return;
    }
    let Some(left) = self.ordinary_primitive_ref(left_root) else {
      return;
    };
    let Some(right) = self.ordinary_primitive_ref(right_root) else {
      return;
    };
    if !self.right_is_closed_sink(right_root, call.span) {
      return;
    }
    let Some(site) = self.call_site(node_id, call.span) else {
      return;
    };
    if !self.handle_stays_live(node_id, &site) {
      return;
    }
    let Some(source_span) = self.changed_left_write(left_root, left.payload, &site) else {
      return;
    };
    let Some(demand_span) = self.ordinary_right_demand(right_root, &site, source_span) else {
      return;
    };
    self.facts.derivation_practice.sync_ref_one_way.push(SyncRefOneWayFact {
      call_span: self.span(call.span),
      source_span: self.span(source_span),
      sink_span: self.span(right.init_span),
      demand_span: self.span(demand_span),
      sink_name: self.symbol_name(right_root),
      sink_names: self.root_names(right_root),
    });
  }

  pub(super) fn collect_conditional_watch_source(
    &mut self,
    node_id: oxc_semantic::NodeId,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    if info.has_spread || info.api != Some("watch") {
      return;
    }
    if !self.watch_options_admit_idle_skip(call) {
      return;
    }
    let Some(source_expr) = nth_expr(call, 0) else {
      return;
    };
    let Some(callback_expr) = nth_expr(call, 1) else {
      return;
    };
    let inner = source_expr.get_inner_expression();
    let Expression::ArrayExpression(array) = inner else {
      return;
    };
    if array.elements.len() != 2
      || array.elements.iter().any(|element| {
        element.is_elision() || matches!(element, ArrayExpressionElement::SpreadElement(_))
      })
    {
      return;
    }
    let Some(guard_expr) = array.elements.first().and_then(ArrayExpressionElement::as_expression)
    else {
      return;
    };
    let Some(producer_expr) = array.elements.get(1).and_then(ArrayExpressionElement::as_expression)
    else {
      return;
    };
    let Some(guard_id) = ident_symbol(guard_expr, self) else {
      return;
    };
    let Some(producer_id) = ident_symbol(producer_expr, self) else {
      return;
    };
    let guard_root = self.indexes.root_of(guard_id);
    let producer_root = self.indexes.root_of(producer_id);
    self.indexes.note_query();
    if guard_root == producer_root {
      return;
    }
    if self.ordinary_primitive_ref(guard_root).is_none() {
      return;
    }
    let Some(producer) = self.pure_primitive_computed(producer_root) else {
      return;
    };
    let Some(callback) = array_callback(callback_expr) else {
      return;
    };
    let Some(sink_root) = self.guarded_idempotent_sink(&callback) else {
      return;
    };
    if sink_root == guard_root || sink_root == producer_root || sink_root == producer.dep {
      return;
    }
    if self.ordinary_primitive_ref(sink_root).is_none() {
      return;
    }
    if !self.sole_producer_demand(producer_root, call.span) {
      return;
    }
    let Some(site) = self.call_site(node_id, call.span) else {
      return;
    };
    if !self.handle_stays_live(node_id, &site) {
      return;
    }
    let Some(idle_write) = self.inactive_dep_write(guard_root, producer.dep, &site) else {
      return;
    };
    self.facts.derivation_practice.conditional_watch_source.push(ConditionalWatchSourceFact {
      source_array_span: self.span(inner.span()),
      guard_span: self.span(guard_expr.span()),
      producer_span: self.span(producer.computed_span),
      idle_write_span: self.span(idle_write),
      producer_name: self.symbol_name(producer_root),
      producer_names: self.root_names(producer_root),
    });
  }

  fn ordinary_primitive_ref(&mut self, root: SymbolId) -> Option<OrdinaryRef> {
    self.indexes.note_query();
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
    if info.has_spread || !matches!(info.api, Some("ref" | "shallowRef")) {
      return None;
    }
    let payload = match info.first_arg {
      None => Some(PrimitiveAtom::Undefined),
      Some(argument) => match self.classify_span(argument, MAX_DEPTH) {
        Shape::Primitive | Shape::Nullish => self.indexes.primitive_at(argument),
        _ => return None,
      },
    };
    Some(OrdinaryRef { init_span, payload })
  }

  fn pure_primitive_computed(&mut self, root: SymbolId) -> Option<ComputedProducer> {
    self.indexes.note_query();
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
    if info.has_spread || info.api != Some("computed") || info.arg_count > 2 {
      return None;
    }
    if let Some(debug_span) = info.second_arg
      && !self.pure_debug_options(debug_span)
    {
      return None;
    }
    let getter_span = info.first_arg?;
    let dep = self.computed_getter_dep(getter_span)?;
    let dep_root = self.indexes.root_of(dep);
    self.ordinary_primitive_ref(dep_root)?;
    Some(ComputedProducer { computed_span: call_span, dep: dep_root })
  }

  fn computed_getter_dep(&self, getter_span: Span) -> Option<SymbolId> {
    let node_id = self.indexes.callable_node(getter_span)?;
    match self.semantic.nodes().kind(node_id) {
      AstKind::ArrowFunctionExpression(arrow) => {
        if arrow.r#async || arrow.params.rest.is_some() || !arrow.params.items.is_empty() {
          return None;
        }
        value_read_from_body(&arrow.body, arrow.expression, self)
      }
      AstKind::Function(function) => {
        if function.r#async
          || function.generator
          || function.params.rest.is_some()
          || !function.params.items.is_empty()
        {
          return None;
        }
        value_read_from_body(function.body.as_ref()?, false, self)
      }
      _ => None,
    }
  }

  fn right_is_closed_sink(&mut self, root: SymbolId, call_span: Span) -> bool {
    matches!(self.sink_use_of(root), SinkOwnership::Unique(span) if span == call_span)
  }

  fn sink_use_of(&mut self, root: SymbolId) -> SinkOwnership {
    if let Some(summary) = self.sink_use.get(&root).copied() {
      self.indexes.note_query();
      return summary;
    }
    let user_writes = !self.indexes.value_writes_of(root).is_empty()
      || self.indexes.unknown_member_touch.contains(&root);
    if user_writes || self.is_parameter(root) || self.is_exported(root) {
      self.sink_use.insert(root, SinkOwnership::Unknown);
      return SinkOwnership::Unknown;
    }
    let mut unique = None;
    for symbol_id in self.root_symbol_ids(root) {
      if self.is_parameter(symbol_id) {
        self.sink_use.insert(root, SinkOwnership::Unknown);
        return SinkOwnership::Unknown;
      }
      for reference in self.semantic.symbol_references(symbol_id) {
        self.indexes.add_references(1);
        match self.sink_reference_role(reference.node_id()) {
          SinkRole::Decl | SinkRole::ValueRead => {}
          SinkRole::SyncRef(span) => {
            self.indexes.note_query();
            match unique {
              None => unique = Some(span),
              Some(existing) if existing == span => {}
              Some(_) => {
                self.sink_use.insert(root, SinkOwnership::Shared);
                return SinkOwnership::Shared;
              }
            }
          }
          SinkRole::Other => {
            self.sink_use.insert(root, SinkOwnership::Unknown);
            return SinkOwnership::Unknown;
          }
        }
      }
    }
    let summary = unique.map_or(SinkOwnership::Unknown, SinkOwnership::Unique);
    self.sink_use.insert(root, summary);
    summary
  }

  fn sink_reference_role(&self, node_id: oxc_semantic::NodeId) -> SinkRole {
    self.indexes.note_query();
    let ident_span = self.semantic.nodes().kind(node_id).span();
    let parent_id = self.semantic.nodes().parent_id(node_id);
    self.indexes.note_query();
    match self.semantic.nodes().kind(parent_id) {
      oxc_ast::AstKind::VariableDeclarator(_) => SinkRole::Decl,
      oxc_ast::AstKind::StaticMemberExpression(member)
        if member.property.name.as_str() == "value"
          && callee_object_span(&member.object, ident_span) =>
      {
        if matches!(
          self.semantic.nodes().parent_kind(parent_id),
          oxc_ast::AstKind::AssignmentExpression(_) | oxc_ast::AstKind::UpdateExpression(_)
        ) {
          SinkRole::Other
        } else {
          SinkRole::ValueRead
        }
      }
      oxc_ast::AstKind::CallExpression(call) => {
        self.indexes.note_query();
        let Some(info) = self.indexes.calls.get(&span_key(call.span)).copied() else {
          return SinkRole::Other;
        };
        if info.api == Some("syncRef")
          && call.arguments.iter().any(|argument| {
            argument.as_expression().is_some_and(|expression| {
              let inner = expression.get_inner_expression();
              inner.span() == ident_span || expression.span() == ident_span
            })
          })
        {
          SinkRole::SyncRef(call.span)
        } else {
          SinkRole::Other
        }
      }
      oxc_ast::AstKind::ParenthesizedExpression(_)
      | oxc_ast::AstKind::TSAsExpression(_)
      | oxc_ast::AstKind::TSSatisfiesExpression(_)
      | oxc_ast::AstKind::TSNonNullExpression(_)
      | oxc_ast::AstKind::TSTypeAssertion(_) => self.sink_reference_role(parent_id),
      _ => SinkRole::Other,
    }
  }

  fn changed_left_write(
    &self,
    root: SymbolId,
    init: Option<PrimitiveAtom>,
    site: &CallSite,
  ) -> Option<Span> {
    let mut previous = init;
    for write in self.indexes.value_writes_of(root) {
      self.indexes.add_writes(1);
      if !self.guaranteed_execution(write.node_id) {
        continue;
      }
      if write.callable != site.callable || write.block != site.block {
        if write.offset > site.offset {
          return None;
        }
        previous = self.indexes.primitive_at(write.rhs).or(previous);
        continue;
      }
      if write.offset <= site.offset {
        previous = self.indexes.primitive_at(write.rhs).or(previous);
        continue;
      }
      if self.indexes.has_event_between(site.block, site.offset, write.offset) {
        return None;
      }
      let next = self.indexes.primitive_at(write.rhs)?;
      if let Some(prior) = previous
        && !prior.object_is(next)
      {
        return Some(write.rhs);
      }
      previous = Some(next);
    }
    None
  }

  fn ordinary_right_demand(
    &self,
    root: SymbolId,
    site: &CallSite,
    write_span: Span,
  ) -> Option<Span> {
    let write_offset = self.span(write_span).offset;
    let mut cursor = write_offset;
    loop {
      let read = self.indexes.first_value_read_after(root, site.callable, site.block, cursor)?;
      if !self.guaranteed_execution(read.node_id) {
        cursor = read.offset;
        continue;
      }
      if self.indexes.has_event_between(site.block, write_offset, read.offset) {
        return None;
      }
      return Some(read.span);
    }
  }

  fn sole_producer_demand(&mut self, root: SymbolId, watch_span: Span) -> bool {
    match self.producer_demand_of(root) {
      ProducerDemand::Unique(span) => span == watch_span,
      _ => false,
    }
  }

  fn producer_demand_of(&mut self, root: SymbolId) -> ProducerDemand {
    if let Some(demand) = self.producer_demand.get(&root).copied() {
      self.indexes.note_query();
      return demand;
    }
    let mut watch_span = None;
    let mut escaped = false;
    for symbol_id in self.root_symbol_ids(root) {
      for reference in self.semantic.symbol_references(symbol_id) {
        self.indexes.add_references(1);
        match self.producer_reference_role(reference.node_id()) {
          ProducerRole::Decl => {}
          ProducerRole::Watch(span) => match watch_span {
            None => watch_span = Some(span),
            Some(existing) if existing == span => {}
            Some(_) => {
              self.producer_demand.insert(root, ProducerDemand::Shared);
              return ProducerDemand::Shared;
            }
          },
          ProducerRole::Other => escaped = true,
        }
      }
    }
    let demand = if escaped {
      ProducerDemand::Escaped
    } else {
      watch_span.map_or(ProducerDemand::None, ProducerDemand::Unique)
    };
    self.producer_demand.insert(root, demand);
    demand
  }

  fn producer_reference_role(&self, node_id: oxc_semantic::NodeId) -> ProducerRole {
    self.indexes.note_query();
    let ident_span = self.semantic.nodes().kind(node_id).span();
    let parent_id = self.semantic.nodes().parent_id(node_id);
    self.indexes.note_query();
    match self.semantic.nodes().kind(parent_id) {
      oxc_ast::AstKind::VariableDeclarator(_) => ProducerRole::Decl,
      oxc_ast::AstKind::ArrayExpression(_) => self
        .enclosing_watch_span(parent_id, ident_span)
        .map_or(ProducerRole::Other, ProducerRole::Watch),
      oxc_ast::AstKind::ParenthesizedExpression(_)
      | oxc_ast::AstKind::TSAsExpression(_)
      | oxc_ast::AstKind::TSSatisfiesExpression(_)
      | oxc_ast::AstKind::TSNonNullExpression(_)
      | oxc_ast::AstKind::TSTypeAssertion(_) => self.producer_reference_role(parent_id),
      _ => ProducerRole::Other,
    }
  }

  fn enclosing_watch_span(&self, array_id: oxc_semantic::NodeId, ident_span: Span) -> Option<Span> {
    let mut current = array_id;
    for _ in 0..8 {
      let parent_id = self.semantic.nodes().parent_id(current);
      self.indexes.note_query();
      match self.semantic.nodes().kind(parent_id) {
        oxc_ast::AstKind::ParenthesizedExpression(_)
        | oxc_ast::AstKind::TSAsExpression(_)
        | oxc_ast::AstKind::TSSatisfiesExpression(_)
        | oxc_ast::AstKind::TSNonNullExpression(_)
        | oxc_ast::AstKind::TSTypeAssertion(_)
        | oxc_ast::AstKind::ArrayExpression(_) => current = parent_id,
        oxc_ast::AstKind::CallExpression(call) => {
          self.indexes.note_query();
          let info = self.indexes.calls.get(&span_key(call.span)).copied()?;
          if info.api != Some("watch") {
            return None;
          }
          let first = nth_expr(call, 0)?;
          let inner = first.get_inner_expression();
          if span_covers(inner.span(), ident_span) || span_covers(first.span(), ident_span) {
            return Some(call.span);
          }
          return None;
        }
        _ => return None,
      }
    }
    None
  }

  fn inactive_dep_write(&self, guard: SymbolId, dep: SymbolId, site: &CallSite) -> Option<Span> {
    let watch_offset = site.offset;
    let guard_init = self.indexes.init_span.get(&guard).copied().and_then(|span| {
      self.indexes.note_query();
      let ShapeHint::Call(call_span) = self.indexes.hints.get(&span_key(span)).copied()? else {
        return None;
      };
      self.indexes.note_query();
      let info = self.indexes.calls.get(&span_key(call_span)).copied()?;
      info.first_arg.and_then(|argument| self.indexes.primitive_at(argument))
    });
    if !guard_init.is_some_and(PrimitiveAtom::is_falsy_guard) {
      return None;
    }
    if self.unknown_helper_before(guard, site) {
      return None;
    }
    for write in self.indexes.value_writes_of(guard) {
      self.indexes.add_writes(1);
      if write.offset > watch_offset {
        return None;
      }
      if !self.guaranteed_execution(write.node_id) {
        return None;
      }
      let value = self.indexes.primitive_at(write.rhs)?;
      if !value.is_falsy_guard() {
        return None;
      }
    }
    let mut previous = self.ordinary_primitive_ref_payload(dep);
    for write in self.indexes.value_writes_of(dep) {
      self.indexes.add_writes(1);
      if !self.guaranteed_execution(write.node_id) {
        continue;
      }
      if write.callable != site.callable || write.block != site.block {
        if write.offset > watch_offset {
          return None;
        }
        previous = self.indexes.primitive_at(write.rhs).or(previous);
        continue;
      }
      if write.offset <= watch_offset {
        previous = self.indexes.primitive_at(write.rhs).or(previous);
        continue;
      }
      if self.indexes.has_event_between(site.block, watch_offset, write.offset) {
        return None;
      }
      let next = self.indexes.primitive_at(write.rhs)?;
      if let Some(prior) = previous
        && !prior.object_is(next)
      {
        return Some(write.rhs);
      }
      previous = Some(next);
    }
    None
  }

  fn ordinary_primitive_ref_payload(&self, root: SymbolId) -> Option<PrimitiveAtom> {
    self.indexes.note_query();
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

  fn guarded_idempotent_sink(&self, callback: &ArrayCallback<'_>) -> Option<SymbolId> {
    let statements = callback.body.statements.as_slice();
    if statements.len() != 1 {
      self.indexes.add_queries(statements.len() as u64);
      return None;
    }
    self.indexes.note_query();
    let Statement::IfStatement(if_stmt) = statements.first()? else {
      return None;
    };
    if if_stmt.alternate.is_some() {
      return None;
    }
    let test = if_stmt.test.get_inner_expression();
    let Expression::Identifier(identifier) = test else {
      return None;
    };
    if self.reference_symbol(identifier) != Some(callback.guard) {
      return None;
    }
    let assignment = assignment_statement(&if_stmt.consequent)?;
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
    let sink = self.reference_symbol(object)?;
    let rhs = assignment.right.get_inner_expression();
    let Expression::Identifier(value) = rhs else {
      return None;
    };
    let value_id = self.reference_symbol(value)?;
    if self.indexes.root_of(value_id) != self.indexes.root_of(callback.producer) {
      return None;
    }
    Some(self.indexes.root_of(sink))
  }

  fn call_site(&self, node_id: oxc_semantic::NodeId, span: Span) -> Option<CallSite> {
    if !self.guaranteed_execution(node_id) {
      return None;
    }
    let (callable, block) = self.indexes.site_owner(node_id);
    Some(CallSite { callable, block: block?, offset: self.span(span).offset })
  }

  fn guaranteed_execution(&self, node_id: oxc_semantic::NodeId) -> bool {
    let mut current = node_id;
    for _ in 0..32 {
      self.indexes.note_query();
      let parent_id = self.semantic.nodes().parent_id(current);
      match self.semantic.nodes().kind(parent_id) {
        AstKind::IfStatement(_)
        | AstKind::WhileStatement(_)
        | AstKind::DoWhileStatement(_)
        | AstKind::ForStatement(_)
        | AstKind::ForInStatement(_)
        | AstKind::ForOfStatement(_)
        | AstKind::SwitchStatement(_)
        | AstKind::TryStatement(_)
        | AstKind::CatchClause(_)
        | AstKind::LogicalExpression(_)
        | AstKind::ConditionalExpression(_)
        | AstKind::AwaitExpression(_) => return false,
        AstKind::Program(_) | AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
          return true;
        }
        _ => current = parent_id,
      }
    }
    false
  }

  fn unknown_helper_before(&self, root: SymbolId, site: &CallSite) -> bool {
    for symbol_id in self.root_symbol_ids(root) {
      for reference in self.semantic.symbol_references(symbol_id) {
        self.indexes.add_references(1);
        let node_id = reference.node_id();
        if self.is_declaration_reference(node_id) {
          continue;
        }
        let ident_span = self.semantic.nodes().kind(node_id).span();
        let offset = self.span(ident_span).offset;
        if offset >= site.offset {
          continue;
        }
        let (callable, block) = self.indexes.site_owner(node_id);
        if callable != site.callable || block != Some(site.block) {
          continue;
        }
        if self.ident_is_unknown_call_argument(node_id) {
          return true;
        }
      }
    }
    false
  }

  fn ident_is_unknown_call_argument(&self, node_id: oxc_semantic::NodeId) -> bool {
    let ident_span = self.semantic.nodes().kind(node_id).span();
    let mut current = node_id;
    for _ in 0..MAX_DEPTH {
      self.indexes.note_query();
      let parent_id = self.semantic.nodes().parent_id(current);
      match self.semantic.nodes().kind(parent_id) {
        AstKind::ParenthesizedExpression(_)
        | AstKind::TSAsExpression(_)
        | AstKind::TSSatisfiesExpression(_)
        | AstKind::TSNonNullExpression(_)
        | AstKind::TSTypeAssertion(_)
        | AstKind::ChainExpression(_)
        | AstKind::ArrayExpression(_)
        | AstKind::ObjectExpression(_)
        | AstKind::ObjectProperty(_)
        | AstKind::SpreadElement(_)
        | AstKind::LogicalExpression(_)
        | AstKind::ConditionalExpression(_) => current = parent_id,
        AstKind::CallExpression(call) => {
          if call.callee.span() == ident_span
            || call.callee.get_inner_expression().span() == ident_span
          {
            return false;
          }
          self.indexes.note_query();
          return self
            .indexes
            .calls
            .get(&span_key(call.span))
            .copied()
            .is_none_or(|info| info.api.is_none());
        }
        _ => return false,
      }
    }
    false
  }

  fn root_names(&self, root: SymbolId) -> Vec<String> {
    let mut names = Vec::new();
    names.push(self.symbol_name(root));
    for alias in self.indexes.alias_members(root) {
      names.push(self.symbol_name(*alias));
    }
    names.sort_by(|left, right| {
      self.indexes.note_query();
      left.cmp(right)
    });
    let before = names.len();
    names.dedup();
    self.indexes.add_queries(before as u64);
    names
  }

  fn root_symbol_ids(&self, root: SymbolId) -> Vec<SymbolId> {
    let aliases = self.indexes.alias_members(root);
    self.indexes.add_queries(aliases.len() as u64);
    let mut ids = Vec::with_capacity(aliases.len().saturating_add(1));
    ids.push(root);
    ids.extend_from_slice(aliases);
    ids
  }

  fn handle_stays_live(&self, call_node: oxc_semantic::NodeId, site: &CallSite) -> bool {
    let Some(handle) = self.assigned_const_handle(call_node) else {
      return true;
    };
    let root = self.indexes.root_of(handle);
    for symbol_id in self.root_symbol_ids(root) {
      for reference in self.semantic.symbol_references(symbol_id) {
        self.indexes.add_references(1);
        let node_id = reference.node_id();
        if self.is_declaration_reference(node_id) {
          continue;
        }
        let (callable, block) = self.indexes.site_owner(node_id);
        if self.identifier_is_callee(node_id)
          && callable == site.callable
          && block == Some(site.block)
        {
          continue;
        }
        return false;
      }
    }
    true
  }

  fn assigned_const_handle(&self, call_node: oxc_semantic::NodeId) -> Option<SymbolId> {
    self.indexes.note_query();
    let parent_id = self.semantic.nodes().parent_id(call_node);
    let oxc_ast::AstKind::VariableDeclarator(declarator) = self.semantic.nodes().kind(parent_id)
    else {
      return None;
    };
    let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &declarator.id else {
      return None;
    };
    let symbol_id = binding.symbol_id.get()?;
    self.symbol_is_const(symbol_id).then_some(symbol_id)
  }

  fn is_declaration_reference(&self, node_id: oxc_semantic::NodeId) -> bool {
    self.indexes.note_query();
    matches!(self.semantic.nodes().parent_kind(node_id), oxc_ast::AstKind::VariableDeclarator(_))
  }

  fn identifier_is_callee(&self, node_id: oxc_semantic::NodeId) -> bool {
    self.indexes.note_query();
    let ident_span = self.semantic.nodes().kind(node_id).span();
    let oxc_ast::AstKind::CallExpression(call) = self.semantic.nodes().parent_kind(node_id) else {
      return false;
    };
    call.callee.span() == ident_span || call.callee.get_inner_expression().span() == ident_span
  }

  fn sync_ref_direction(&self, call: &CallExpression<'_>) -> SyncDirection {
    if nth_expr(call, 3).is_some() {
      return SyncDirection::Unknown;
    }
    let parsed = match self.parse_option_object(nth_expr(call, 2)) {
      ParsedOptions::Absent => return SyncDirection::DefaultBoth,
      ParsedOptions::Invalid => return SyncDirection::Unknown,
      ParsedOptions::Present(parsed) => parsed,
    };
    if parsed.debugger {
      return SyncDirection::Unknown;
    }
    let direction = parsed.direction.or_default("both");
    let flush = parsed.flush.or_default("sync");
    let deep = parsed.deep.or_default(false);
    let immediate = parsed.immediate.or_default(true);
    match (direction, flush, deep, immediate, parsed.transform) {
      (
        Effective::Known("both"),
        Effective::Known("sync"),
        Effective::Known(false),
        Effective::Known(true),
        Effective::Absent | Effective::Known(true),
      ) => SyncDirection::DefaultBoth,
      (
        Effective::Known("ltr"),
        Effective::Known("sync"),
        Effective::Known(false),
        Effective::Known(true),
        Effective::Absent | Effective::Known(true),
      ) => SyncDirection::AlreadyLtr,
      _ => SyncDirection::Unknown,
    }
  }

  fn watch_options_admit_idle_skip(&self, call: &CallExpression<'_>) -> bool {
    if nth_expr(call, 3).is_some() {
      return false;
    }
    let parsed = match self.parse_option_object(nth_expr(call, 2)) {
      ParsedOptions::Absent => return true,
      ParsedOptions::Invalid => return false,
      ParsedOptions::Present(parsed) => parsed,
    };
    if parsed.debugger {
      return false;
    }
    matches!(parsed.once.or_default(false), Effective::Known(false))
      && matches!(parsed.flush, Effective::Absent | Effective::Known(_))
      && matches!(parsed.deep, Effective::Absent | Effective::Known(_))
      && matches!(parsed.immediate, Effective::Absent | Effective::Known(_))
      && matches!(parsed.direction, Effective::Absent)
      && matches!(parsed.transform, Effective::Absent)
  }

  fn pure_debug_options(&self, span: Span) -> bool {
    self.indexes.note_query();
    let Some(entries) = self.indexes.objects.get(&span_key(span)) else {
      return false;
    };
    if !entries.is_empty() {
      self.indexes.add_object_entries(entries.len() as u64);
      return false;
    }
    true
  }

  fn parse_option_object(&self, options: Option<&Expression<'_>>) -> ParsedOptions {
    let Some(options) = options else {
      return ParsedOptions::Absent;
    };
    if self.proven_undefined(options) {
      return ParsedOptions::Absent;
    }
    let Expression::ObjectExpression(object) = options.get_inner_expression() else {
      return ParsedOptions::Invalid;
    };
    let mut bag = OptionBag::default();
    for property in &object.properties {
      self.indexes.add_object_entries(1);
      match property {
        ObjectPropertyKind::SpreadProperty(_) => return ParsedOptions::Invalid,
        ObjectPropertyKind::ObjectProperty(prop) => {
          if prop.kind != PropertyKind::Init || prop.method || prop.shorthand || prop.computed {
            return ParsedOptions::Invalid;
          }
          let Some(name) = prop.key.static_name() else {
            return ParsedOptions::Invalid;
          };
          match name.as_ref() {
            "direction" => bag.direction = self.effective_string(&prop.value),
            "flush" => bag.flush = self.effective_string(&prop.value),
            "deep" => bag.deep = self.effective_bool(&prop.value),
            "immediate" => bag.immediate = self.effective_bool(&prop.value),
            "once" => bag.once = self.effective_bool(&prop.value),
            "transform" => bag.transform = self.effective_empty_object(&prop.value),
            "onTrack" | "onTrigger" => bag.debugger = true,
            _ => return ParsedOptions::Invalid,
          }
        }
      }
    }
    ParsedOptions::Present(bag)
  }

  fn effective_string(&self, expression: &Expression<'_>) -> Effective<&'static str> {
    if self.proven_undefined(expression) {
      return Effective::Absent;
    }
    self.proven_string(expression).map_or(Effective::Unknown, Effective::Known)
  }

  fn effective_bool(&self, expression: &Expression<'_>) -> Effective<bool> {
    if self.proven_undefined(expression) {
      return Effective::Absent;
    }
    self.proven_bool(expression).map_or(Effective::Unknown, Effective::Known)
  }

  fn effective_empty_object(&self, expression: &Expression<'_>) -> Effective<bool> {
    if self.proven_undefined(expression) {
      return Effective::Absent;
    }
    let Expression::ObjectExpression(object) = expression.get_inner_expression() else {
      return Effective::Unknown;
    };
    self.indexes.add_object_entries(object.properties.len() as u64);
    if object.properties.is_empty() { Effective::Known(true) } else { Effective::Unknown }
  }

  fn proven_undefined(&self, expression: &Expression<'_>) -> bool {
    match expression.get_inner_expression() {
      Expression::Identifier(identifier)
        if identifier.name.as_str() == "undefined"
          && self.reference_symbol(identifier).is_none() =>
      {
        true
      }
      Expression::UnaryExpression(unary)
        if unary.operator == oxc_syntax::operator::UnaryOperator::Void =>
      {
        matches!(unary.argument.get_inner_expression(), Expression::NumericLiteral(literal) if literal.value == 0.0)
      }
      _ => false,
    }
  }

  fn proven_string(&self, expression: &Expression<'_>) -> Option<&'static str> {
    match expression.get_inner_expression() {
      Expression::StringLiteral(literal) => intern_option_string(literal.value.as_str()),
      Expression::Identifier(identifier) => {
        let symbol_id = self.reference_symbol(identifier)?;
        if !self.symbol_is_const(symbol_id) {
          return None;
        }
        let init = self.indexes.init_span.get(&symbol_id).copied()?;
        intern_option_string(self.literal_text(init)?.as_str())
      }
      _ => None,
    }
  }

  fn proven_bool(&self, expression: &Expression<'_>) -> Option<bool> {
    match expression.get_inner_expression() {
      Expression::BooleanLiteral(literal) => Some(literal.value),
      Expression::Identifier(identifier) => {
        let symbol_id = self.reference_symbol(identifier)?;
        if !self.symbol_is_const(symbol_id) {
          return None;
        }
        let init = self.indexes.init_span.get(&symbol_id).copied()?;
        match self.indexes.primitive_at(init) {
          Some(PrimitiveAtom::Bool(value)) => Some(value),
          _ => None,
        }
      }
      _ => None,
    }
  }

  fn literal_text(&self, span: Span) -> Option<String> {
    let start = usize::try_from(span.start).ok()?;
    let end = usize::try_from(span.end).ok()?;
    let script_end = self.script_offset.saturating_add(end);
    let script_start = self.script_offset.saturating_add(start);
    let raw = self.sfc_source.get(script_start..script_end)?;
    let trimmed = raw.trim();
    let inner = trimmed
      .strip_prefix(['\'', '"', '`'])
      .and_then(|rest| rest.strip_suffix(['\'', '"', '`']))?;
    Some(inner.to_string())
  }

  fn symbol_is_const(&self, symbol_id: SymbolId) -> bool {
    self.indexes.note_query();
    self.semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable)
  }

  fn is_parameter(&self, symbol_id: SymbolId) -> bool {
    self.indexes.note_query();
    let declaration = self.semantic.symbol_declaration(symbol_id);
    matches!(declaration.kind(), oxc_ast::AstKind::FormalParameter(_))
      || matches!(
        self.semantic.nodes().parent_kind(declaration.id()),
        oxc_ast::AstKind::FormalParameter(_)
      )
  }

  fn symbol_name(&self, symbol_id: SymbolId) -> String {
    self.indexes.note_query();
    self.semantic.scoping().symbol_name(symbol_id).to_string()
  }

  fn is_exported(&self, symbol_id: SymbolId) -> bool {
    self.indexes.note_query();
    let mut current = self.semantic.symbol_declaration(symbol_id).id();
    for _ in 0..6 {
      let parent_id = self.semantic.nodes().parent_id(current);
      self.indexes.note_query();
      match self.semantic.nodes().kind(parent_id) {
        oxc_ast::AstKind::ExportNamedDeclaration(_)
        | oxc_ast::AstKind::ExportDefaultDeclaration(_) => return true,
        oxc_ast::AstKind::VariableDeclarator(_)
        | oxc_ast::AstKind::VariableDeclaration(_)
        | oxc_ast::AstKind::BindingIdentifier(_) => current = parent_id,
        _ => return false,
      }
    }
    false
  }
}

fn nth_expr<'a>(call: &'a CallExpression<'a>, index: usize) -> Option<&'a Expression<'a>> {
  call.arguments.get(index).and_then(Argument::as_expression)
}

fn ident_symbol(expression: &Expression<'_>, collector: &Collector<'_>) -> Option<SymbolId> {
  let identifier = expression.get_inner_expression().get_identifier_reference()?;
  collector.reference_symbol(identifier)
}

fn intern_option_string(value: &str) -> Option<&'static str> {
  match value {
    "both" => Some("both"),
    "ltr" => Some("ltr"),
    "rtl" => Some("rtl"),
    "sync" => Some("sync"),
    "pre" => Some("pre"),
    "post" => Some("post"),
    _ => None,
  }
}

const fn span_covers(outer: Span, inner: Span) -> bool {
  outer.start <= inner.start && inner.end <= outer.end
}

fn callee_object_span(expression: &Expression<'_>, ident_span: Span) -> bool {
  expression.span() == ident_span || expression.get_inner_expression().span() == ident_span
}

fn value_read_from_body(
  body: &FunctionBody<'_>,
  expression_body: bool,
  collector: &Collector<'_>,
) -> Option<SymbolId> {
  if expression_body {
    let statement = body.statements.first()?;
    collector.indexes.note_query();
    let Statement::ExpressionStatement(expr) = statement else {
      return None;
    };
    return value_member(&expr.expression, collector);
  }
  if body.statements.len() != 1 {
    collector.indexes.add_queries(body.statements.len() as u64);
    return None;
  }
  collector.indexes.note_query();
  let Statement::ReturnStatement(ret) = body.statements.first()? else {
    return None;
  };
  value_member(ret.argument.as_ref()?, collector)
}

fn value_member(expression: &Expression<'_>, collector: &Collector<'_>) -> Option<SymbolId> {
  let mut inner = expression;
  for _ in 0..MAX_DEPTH {
    collector.indexes.add_queries(1);
    match inner {
      Expression::ParenthesizedExpression(paren) => inner = &paren.expression,
      Expression::TSAsExpression(expr) => inner = &expr.expression,
      Expression::TSSatisfiesExpression(expr) => inner = &expr.expression,
      Expression::TSNonNullExpression(expr) => inner = &expr.expression,
      Expression::TSTypeAssertion(expr) => inner = &expr.expression,
      Expression::ChainExpression(chain) => {
        let oxc_ast::ast::ChainElement::StaticMemberExpression(member) = &chain.expression else {
          return None;
        };
        if member.property.name.as_str() != "value" {
          return None;
        }
        let object = member.object.get_inner_expression().get_identifier_reference()?;
        return collector.reference_symbol(object);
      }
      other => {
        inner = other;
        break;
      }
    }
  }
  let Expression::StaticMemberExpression(member) = inner.get_inner_expression() else {
    return None;
  };
  if member.property.name.as_str() != "value" {
    return None;
  }
  let object = member.object.get_inner_expression().get_identifier_reference()?;
  collector.reference_symbol(object)
}

fn array_callback<'a>(expression: &'a Expression<'a>) -> Option<ArrayCallback<'a>> {
  match expression.get_inner_expression() {
    Expression::ArrowFunctionExpression(arrow) => {
      if arrow.expression
        || arrow.r#async
        || arrow.params.rest.is_some()
        || arrow.params.items.len() != 1
      {
        return None;
      }
      let (guard, producer) = array_two_bindings(arrow.params.items.first()?)?;
      Some(ArrayCallback { guard, producer, body: &arrow.body })
    }
    Expression::FunctionExpression(function) => {
      if function.r#async
        || function.generator
        || function.params.rest.is_some()
        || function.params.items.len() != 1
      {
        return None;
      }
      let (guard, producer) = array_two_bindings(function.params.items.first()?)?;
      Some(ArrayCallback { guard, producer, body: function.body.as_ref()? })
    }
    _ => None,
  }
}

fn array_two_bindings(parameter: &FormalParameter<'_>) -> Option<(SymbolId, SymbolId)> {
  if parameter.initializer.is_some() || parameter.optional {
    return None;
  }
  let BindingPattern::ArrayPattern(array) = &parameter.pattern else {
    return None;
  };
  if array.rest.is_some() || array.elements.len() != 2 {
    return None;
  }
  let first = binding_ident(array.elements.first()?.as_ref()?)?;
  let second = binding_ident(array.elements.get(1)?.as_ref()?)?;
  Some((first.0, second.0))
}

fn binding_ident<'a>(pattern: &'a BindingPattern<'a>) -> Option<(SymbolId, &'a str)> {
  match pattern {
    BindingPattern::BindingIdentifier(binding) => {
      Some((binding.symbol_id.get()?, binding.name.as_str()))
    }
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
