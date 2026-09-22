//! Computed identity-stability facts (practice).
//!
//! Closed grammar: a resolved Vue `computed` returns a fresh own-data
//! object/array of proven primitive projections, a later same-owner source
//! replacement keeps those projected contents Object.is-equal, and an
//! established identity consumer repeats work on the new derived object.
//! Unsupported shapes stay Unknown.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use oxc_ast::{
  AstKind,
  ast::{
    Argument, BindingPattern, CallExpression, Expression, FormalParameter, FunctionBody,
    ObjectPropertyKind, Statement, UnaryOperator,
  },
};
use oxc_semantic::{NodeId, SymbolFlags, SymbolId};
use oxc_span::{GetSpan, Span};
use oxc_syntax::operator::BinaryOperator;
use vue_vet_core::{StableComputedIdentityFact, StableComputedIdentityReason};

use super::atom::{PrimitiveAtom, sequences_object_is};
use super::index::ObjectEntry;
use super::proof::is_ts_wrapper;
use super::shape::{Literal, OptionValue, Shape, span_key};
use super::{Collector, MAX_DEPTH};

#[derive(Clone, Copy)]
struct Site {
  node: NodeId,
  callable: Option<NodeId>,
  block: NodeId,
  offset: usize,
  end: usize,
}

struct ComputedSummary {
  binding: SymbolId,
  site: Site,
  computed_span: Span,
  projection: Option<(SymbolId, Span, ProjectionKind)>,
}

enum ProjectionKind {
  ArrayOps(Vec<ArrayOp>),
  ObjectFields(Vec<(String, String)>),
}

enum ArrayOp {
  Map(MapFn),
  Filter(FilterFn),
  Slice { start: i64, end: Option<i64> },
  Concat(Vec<PrimitiveAtom>),
  ToSortedDefault,
  ToSortedNumeric,
}

#[derive(Clone)]
enum MapFn {
  Identity,
  Negate,
  Not,
  Binary { op: BinaryOperator, literal: PrimitiveAtom, param_left: bool },
}

#[derive(Clone)]
enum FilterFn {
  Always,
  Truthy,
  Falsy,
  Compare { op: BinaryOperator, literal: PrimitiveAtom, param_left: bool },
}

struct IdentityConsumer {
  computed: SymbolId,
  span: Span,
  site: Site,
  kind: ConsumerKind,
}

#[derive(Clone, Copy)]
enum ConsumerKind {
  Watch { flush: FlushMode, handle: Option<SymbolId> },
  Computed { binding: SymbolId, declaration: Site },
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum FlushMode {
  Sync,
  Pre,
  Post,
}

enum Payload {
  Array(Vec<PrimitiveAtom>),
  Object(BTreeMap<String, PrimitiveAtom>),
}

impl Collector<'_> {
  pub(super) fn collect_stable_computed_identity(&mut self) {
    let summaries = self.computed_summaries();
    if summaries.is_empty() {
      return;
    }
    let consumers = self.identity_consumers(&summaries);
    for summary in &summaries {
      if summary.projection.is_none() {
        continue;
      }
      let Some(fact) = self.match_summary(summary, &consumers) else {
        continue;
      };
      self.facts.stable_computed_identity.push(fact);
    }
  }

  fn computed_summaries(&mut self) -> Vec<ComputedSummary> {
    let mut summaries = Vec::new();
    let calls = self.computed_calls.clone();
    for node_id in calls {
      self.indexes.note_query();
      let Some(summary) = self.summarize_computed(node_id) else {
        continue;
      };
      summaries.push(summary);
    }
    summaries
  }

  fn summarize_computed(&mut self, node_id: NodeId) -> Option<ComputedSummary> {
    let AstKind::CallExpression(call) = self.semantic.nodes().kind(node_id) else {
      return None;
    };
    if call.arguments.iter().any(Argument::is_spread) {
      return None;
    }
    let binding = self.const_binding_of_call(node_id)?;
    if self.computed_binding_escaped_or_mutated(binding) {
      return None;
    }
    let site = self.site_of(node_id)?;
    let getter = call.arguments.first().and_then(Argument::as_expression)?;
    let projection = match self.getter_projection(getter) {
      Some((source, source_span, kind))
        if !self.indexes.payload_uncertain(source)
          && self.classify_symbol(source, MAX_DEPTH) == Shape::RefLike =>
      {
        Some((source, source_span, kind))
      }
      _ => None,
    };
    Some(ComputedSummary { binding, site, computed_span: call.span, projection })
  }

  fn identity_consumers(
    &self,
    summaries: &[ComputedSummary],
  ) -> HashMap<SymbolId, Vec<IdentityConsumer>> {
    let by_binding: BTreeSet<SymbolId> = summaries.iter().map(|summary| summary.binding).collect();
    let mut by_producer: HashMap<SymbolId, Vec<IdentityConsumer>> = HashMap::new();
    let watch_calls = self.watch_calls.clone();
    for node_id in watch_calls {
      self.indexes.note_query();
      if let Some(consumer) = self.watch_identity_consumer(node_id, &by_binding) {
        by_producer.entry(consumer.computed).or_default().push(consumer);
      }
    }
    for summary in summaries {
      self.indexes.note_query();
      let Some(getter) = self.computed_getter(summary.site.node) else {
        continue;
      };
      let Some(reads) = collect_computed_reads(getter, &by_binding, self, MAX_DEPTH) else {
        continue;
      };
      for producer in reads {
        if producer == summary.binding {
          continue;
        }
        self.indexes.note_query();
        by_producer.entry(producer).or_default().push(IdentityConsumer {
          computed: producer,
          span: summary.computed_span,
          site: summary.site,
          kind: ConsumerKind::Computed { binding: summary.binding, declaration: summary.site },
        });
      }
    }
    by_producer
  }

  fn computed_getter(&self, node_id: NodeId) -> Option<&Expression<'_>> {
    let AstKind::CallExpression(call) = self.semantic.nodes().kind(node_id) else {
      return None;
    };
    call.arguments.first().and_then(Argument::as_expression)
  }

  fn watch_identity_consumer(
    &self,
    node_id: NodeId,
    computed: &BTreeSet<SymbolId>,
  ) -> Option<IdentityConsumer> {
    let AstKind::CallExpression(call) = self.semantic.nodes().kind(node_id) else {
      return None;
    };
    if call.arguments.iter().any(Argument::is_spread) {
      return None;
    }
    let options = self.identity_watch_options(call)?;
    if options.once || options.deep {
      return None;
    }
    let source = call.arguments.first().and_then(Argument::as_expression)?;
    let binding = self.watch_source_computed_binding(source, computed)?;
    let site = self.site_of(node_id)?;
    Some(IdentityConsumer {
      computed: binding,
      span: source.span(),
      site,
      kind: ConsumerKind::Watch {
        flush: options.flush,
        handle: self.const_binding_of_call(node_id),
      },
    })
  }

  fn watch_source_computed_binding(
    &self,
    source: &Expression<'_>,
    computed: &BTreeSet<SymbolId>,
  ) -> Option<SymbolId> {
    let inner = source.get_inner_expression();
    match inner {
      Expression::Identifier(identifier) => {
        let symbol = self.indexes.root_of(self.reference_symbol(identifier.as_ref())?);
        computed.contains(&symbol).then_some(symbol)
      }
      Expression::ArrowFunctionExpression(arrow) if arrow.expression && !arrow.r#async => {
        let expr = expression_body(arrow.body.statements.first()?)?;
        let (object, property) = static_member(expr)?;
        if property != "value" {
          return None;
        }
        let symbol = self.indexes.root_of(self.reference_symbol(object)?);
        computed.contains(&symbol).then_some(symbol)
      }
      _ => None,
    }
  }

  fn match_summary(
    &mut self,
    summary: &ComputedSummary,
    consumers: &HashMap<SymbolId, Vec<IdentityConsumer>>,
  ) -> Option<StableComputedIdentityFact> {
    let (source, source_span, kind) = summary.projection.as_ref()?;
    self.indexes.note_query();
    let candidates = consumers.get(&summary.binding)?;
    let mut chosen: Option<(Site, Span, usize)> = None;
    for consumer in candidates {
      self.indexes.note_query();
      if consumer.site.callable != summary.site.callable
        || consumer.site.block != summary.site.block
      {
        continue;
      }
      let Some((population, consumer_span)) = self.live_consumer(consumer, summary) else {
        continue;
      };
      if population.offset <= summary.site.offset {
        continue;
      }
      if chosen.is_none_or(|(current, _, _)| population.offset < current.offset) {
        chosen = Some((population, consumer_span, consumer.site.end));
      }
    }
    let (population, consumer_span, _end) = chosen?;
    let baseline = self.payload_at_population(*source, population)?;
    let replacement = self.indexes.first_value_write_after(
      *source,
      summary.site.callable,
      summary.site.block,
      population.offset,
    )?;
    if replacement.offset <= population.end {
      return None;
    }
    if self.indexes.has_control_event_between(
      summary.site.block,
      population.end,
      replacement.offset,
    ) {
      return None;
    }
    let next = self.payload_from_span(replacement.rhs)?;
    let work = self.indexes.work();
    let left = apply_projection(&baseline, kind, work)?;
    let right = apply_projection(&next, kind, work)?;
    if !payloads_equal(&left, &right, work) {
      return None;
    }
    Some(StableComputedIdentityFact {
      computed_span: self.span(summary.computed_span),
      source_span: self.span(*source_span),
      replacement_span: self.span(replacement.rhs),
      consumer_span: self.span(consumer_span),
      reason: StableComputedIdentityReason::FreshPrimitiveProjection,
    })
  }

  fn live_consumer(
    &self,
    consumer: &IdentityConsumer,
    summary: &ComputedSummary,
  ) -> Option<(Site, Span)> {
    match consumer.kind {
      ConsumerKind::Watch { flush, handle } => {
        if consumer.site.offset <= summary.site.offset {
          return None;
        }
        let replacement = self.indexes.first_value_write_after(
          summary.projection.as_ref()?.0,
          summary.site.callable,
          summary.site.block,
          consumer.site.offset,
        )?;
        if replacement.offset <= consumer.site.end {
          return None;
        }
        if self.delivery_cancelled(handle, flush, consumer.site, replacement.offset) {
          return None;
        }
        Some((consumer.site, consumer.span))
      }
      ConsumerKind::Computed { binding, declaration } => {
        let activation = self.first_activation(binding, declaration)?;
        let replacement = self.indexes.first_value_write_after(
          summary.projection.as_ref()?.0,
          summary.site.callable,
          summary.site.block,
          activation.offset,
        )?;
        if activation.offset >= replacement.offset {
          return None;
        }
        let demand = self.first_value_read_after(binding, declaration, replacement.offset)?;
        if demand.callable != summary.site.callable || demand.block != summary.site.block {
          return None;
        }
        Some((activation, self.semantic.nodes().kind(demand.node).span()))
      }
    }
  }

  fn delivery_cancelled(
    &self,
    handle: Option<SymbolId>,
    flush: FlushMode,
    consumer: Site,
    replacement: usize,
  ) -> bool {
    if handle.is_some_and(|handle| self.handle_stopped(handle, consumer, replacement)) {
      return true;
    }
    if flush == FlushMode::Sync {
      return false;
    }
    let delivery_end = self.indexes.next_control_after(consumer.block, replacement);
    if self.indexes.has_pause_between(consumer.block, replacement, delivery_end) {
      return true;
    }
    handle.is_some_and(|handle| {
      self.handle_stopped_between(handle, consumer, replacement, delivery_end)
    })
  }

  fn payload_at_population(&mut self, source: SymbolId, population: Site) -> Option<Payload> {
    if self.indexes.value_writes_mixed(source) {
      return None;
    }
    match self.indexes.last_value_write_before(
      source,
      population.callable,
      population.block,
      population.offset,
    ) {
      Some(write) if write.simple_assign && write.fresh_alloc => self.payload_from_span(write.rhs),
      Some(_) => None,
      None => self.ref_payload(source),
    }
  }

  fn payload_from_span(&mut self, span: Span) -> Option<Payload> {
    if let Some(array) = self.primitive_array_of(span) {
      return Some(Payload::Array(array));
    }
    self.primitive_object_of(span).map(Payload::Object)
  }

  fn getter_projection(
    &mut self,
    getter: &Expression<'_>,
  ) -> Option<(SymbolId, Span, ProjectionKind)> {
    match getter.get_inner_expression() {
      Expression::ArrowFunctionExpression(arrow) => {
        if arrow.r#async || arrow.params.rest.is_some() {
          return None;
        }
        let prev = first_simple_param(arrow.params.items.first());
        if prev.is_some_and(|param| returns_param(&arrow.body, param.0, param.1)) {
          return None;
        }
        let returned = returned_expression(&arrow.body, arrow.expression)?;
        self.projection_of(returned)
      }
      Expression::FunctionExpression(function) => {
        if function.r#async || function.generator || function.params.rest.is_some() {
          return None;
        }
        let body = function.body.as_ref()?;
        let prev = first_simple_param(function.params.items.first());
        if prev.is_some_and(|param| returns_param(body, param.0, param.1)) {
          return None;
        }
        let returned = returned_expression(body, false)?;
        self.projection_of(returned)
      }
      _ => None,
    }
  }

  fn projection_of(
    &mut self,
    expression: &Expression<'_>,
  ) -> Option<(SymbolId, Span, ProjectionKind)> {
    let inner = expression.get_inner_expression();
    if let Some(fields) = self.object_field_projection(inner) {
      return Some(fields);
    }
    let (root, source_span, ops) = self.array_pipeline(inner, MAX_DEPTH)?;
    if ops.is_empty() {
      return None;
    }
    Some((root, source_span, ProjectionKind::ArrayOps(ops)))
  }

  fn array_pipeline(
    &mut self,
    expression: &Expression<'_>,
    remaining: u8,
  ) -> Option<(SymbolId, Span, Vec<ArrayOp>)> {
    self.indexes.note_query();
    if remaining == 0 {
      return None;
    }
    let inner = expression.get_inner_expression();
    if let Some((root, span)) = self.ref_value_source(inner) {
      return Some((root, span, Vec::new()));
    }
    let Expression::CallExpression(call) = inner else {
      return None;
    };
    if call.arguments.iter().any(Argument::is_spread) {
      return None;
    }
    let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
      return None;
    };
    let op = self.array_op(member.property.name.as_str(), call)?;
    let (root, span, mut ops) = self.array_pipeline(&member.object, remaining.saturating_sub(1))?;
    ops.push(op);
    Some((root, span, ops))
  }

  fn array_op(&mut self, name: &str, call: &CallExpression<'_>) -> Option<ArrayOp> {
    match name {
      "map" => {
        Some(ArrayOp::Map(self.map_fn(call.arguments.first().and_then(Argument::as_expression)?)?))
      }
      "filter" => Some(ArrayOp::Filter(
        self.filter_fn(call.arguments.first().and_then(Argument::as_expression)?)?,
      )),
      "slice" => Some(ArrayOp::Slice {
        start: match call.arguments.first().and_then(Argument::as_expression) {
          Some(expr) => self.integer_atom(expr)?,
          None => 0,
        },
        end: match call.arguments.get(1).and_then(Argument::as_expression) {
          Some(expr) => Some(self.integer_atom(expr)?),
          None => None,
        },
      }),
      "concat" => {
        let mut extra = Vec::new();
        for argument in &call.arguments {
          self.indexes.note_query();
          let expr = argument.as_expression()?;
          if let Some(array) = self.primitive_array_of(expr.span()) {
            extra.extend(array);
          } else {
            extra.push(self.atom_at(expr.span(), MAX_DEPTH)?);
          }
        }
        Some(ArrayOp::Concat(extra))
      }
      "toSorted" => call
        .arguments
        .first()
        .and_then(Argument::as_expression)
        .map_or(Some(ArrayOp::ToSortedDefault), |expr| {
          numeric_sort_fn(expr).then_some(ArrayOp::ToSortedNumeric)
        }),
      _ => None,
    }
  }

  fn map_fn(&mut self, expression: &Expression<'_>) -> Option<MapFn> {
    let (param, param_name, body) = simple_one_param_callback(expression)?;
    let expr = callback_expression(&body)?;
    self.indexes.note_query();
    match expr.get_inner_expression() {
      Expression::Identifier(identifier)
        if identifier.name.as_str() == param_name
          && self.reference_symbol(identifier.as_ref()) == Some(param) =>
      {
        Some(MapFn::Identity)
      }
      Expression::UnaryExpression(unary) => {
        let Expression::Identifier(identifier) = unary.argument.get_inner_expression() else {
          return None;
        };
        if identifier.name.as_str() != param_name
          || self.reference_symbol(identifier.as_ref()) != Some(param)
        {
          return None;
        }
        match unary.operator {
          UnaryOperator::UnaryNegation => Some(MapFn::Negate),
          UnaryOperator::LogicalNot => Some(MapFn::Not),
          _ => None,
        }
      }
      Expression::BinaryExpression(binary) => {
        let (param_left, literal) = self.binary_param_literal(
          binary.left.get_inner_expression(),
          binary.right.get_inner_expression(),
          param,
          param_name,
        )?;
        if !matches!(
          binary.operator,
          BinaryOperator::Addition
            | BinaryOperator::Subtraction
            | BinaryOperator::Multiplication
            | BinaryOperator::Division
            | BinaryOperator::Remainder
        ) {
          return None;
        }
        Some(MapFn::Binary { op: binary.operator, literal, param_left })
      }
      _ => None,
    }
  }

  fn filter_fn(&mut self, expression: &Expression<'_>) -> Option<FilterFn> {
    let (param, param_name, body) = simple_one_param_callback(expression)?;
    let expr = callback_expression(&body)?;
    self.indexes.note_query();
    match expr.get_inner_expression() {
      Expression::BooleanLiteral(literal) if literal.value => Some(FilterFn::Always),
      Expression::Identifier(identifier)
        if identifier.name.as_str() == param_name
          && self.reference_symbol(identifier.as_ref()) == Some(param) =>
      {
        Some(FilterFn::Truthy)
      }
      Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
        let Expression::Identifier(identifier) = unary.argument.get_inner_expression() else {
          return None;
        };
        (identifier.name.as_str() == param_name
          && self.reference_symbol(identifier.as_ref()) == Some(param))
        .then_some(FilterFn::Falsy)
      }
      Expression::BinaryExpression(binary) => {
        let (param_left, literal) = self.binary_param_literal(
          binary.left.get_inner_expression(),
          binary.right.get_inner_expression(),
          param,
          param_name,
        )?;
        if !matches!(
          binary.operator,
          BinaryOperator::StrictEquality
            | BinaryOperator::StrictInequality
            | BinaryOperator::LessThan
            | BinaryOperator::LessEqualThan
            | BinaryOperator::GreaterThan
            | BinaryOperator::GreaterEqualThan
        ) {
          return None;
        }
        Some(FilterFn::Compare { op: binary.operator, literal, param_left })
      }
      _ => None,
    }
  }

  fn object_field_projection(
    &self,
    expression: &Expression<'_>,
  ) -> Option<(SymbolId, Span, ProjectionKind)> {
    let Expression::ObjectExpression(_) = expression.get_inner_expression() else {
      return None;
    };
    self.indexes.note_query();
    let entries = self.indexes.objects.get(&span_key(expression.span()))?.clone();
    let mut fields = Vec::new();
    let mut source = None;
    let mut source_span = None;
    for entry in &entries {
      self.indexes.add_queries(1);
      let ObjectEntry::Data { name, value, .. } = entry else {
        return None;
      };
      let (root, span, key) = self.member_of_ref_value(*value)?;
      match source {
        None => {
          source = Some(root);
          source_span = Some(span);
        }
        Some(existing) if existing != root => return None,
        Some(_) => {}
      }
      fields.push((name.clone(), key));
    }
    if fields.is_empty() {
      return None;
    }
    Some((source?, source_span?, ProjectionKind::ObjectFields(fields)))
  }

  fn member_of_ref_value(&self, span: Span) -> Option<(SymbolId, Span, String)> {
    self.indexes.note_query();
    let outer = self.indexes.members.get(&span_key(span))?.clone();
    self.indexes.note_query();
    let inner = self.indexes.members.get(&span_key(outer.object))?;
    if inner.property != "value" {
      return None;
    }
    self.indexes.note_query();
    let super::shape::ShapeHint::Identifier(Some(symbol_id), _) =
      self.indexes.hints.get(&span_key(inner.object)).copied()?
    else {
      return None;
    };
    let root = self.indexes.root_of(symbol_id);
    Some((root, inner.span, outer.property))
  }

  fn ref_value_source(&self, expression: &Expression<'_>) -> Option<(SymbolId, Span)> {
    let (object, property) = static_member(expression)?;
    if property != "value" {
      return None;
    }
    let root = self.indexes.root_of(self.reference_symbol(object)?);
    Some((root, expression.span()))
  }

  fn ref_payload(&mut self, root: SymbolId) -> Option<Payload> {
    self.indexes.note_query();
    let init = *self.indexes.init_span.get(&root)?;
    self.indexes.note_query();
    let super::shape::ShapeHint::Call(call_span) =
      self.indexes.hints.get(&span_key(init)).copied()?
    else {
      return None;
    };
    self.indexes.note_query();
    let info = self.indexes.calls.get(&span_key(call_span)).copied()?;
    if !matches!(info.api, Some("ref" | "shallowRef")) || info.has_spread {
      return None;
    }
    let argument = info.first_arg?;
    if let Some(array) = self.primitive_array_of(argument) {
      return Some(Payload::Array(array));
    }
    self.primitive_object_of(argument).map(Payload::Object)
  }

  fn primitive_array_of(&mut self, span: Span) -> Option<Vec<PrimitiveAtom>> {
    if let Some(direct) = self.array_atoms(span) {
      return Some(direct);
    }
    self.indexes.note_query();
    let hint = self.indexes.hints.get(&span_key(span)).copied()?;
    match hint {
      super::shape::ShapeHint::Identifier(Some(symbol_id), _) => {
        let root = self.indexes.root_of(symbol_id);
        if self.indexes.reassigned.contains(&root) {
          return None;
        }
        let init = *self.indexes.init_span.get(&root)?;
        self.primitive_array_of(init)
      }
      _ => None,
    }
  }

  fn array_atoms(&mut self, span: Span) -> Option<Vec<PrimitiveAtom>> {
    self.indexes.note_query();
    let elements = self.indexes.array_elements.get(&span_key(span))?.clone();
    let mut atoms = Vec::with_capacity(elements.len());
    for element in elements {
      atoms.push(self.atom_at(element, MAX_DEPTH)?);
    }
    Some(atoms)
  }

  fn primitive_object_of(&mut self, span: Span) -> Option<BTreeMap<String, PrimitiveAtom>> {
    self.indexes.note_query();
    let closed = self.indexes.closed_object_literal(span)?;
    if !closed {
      return None;
    }
    self.indexes.note_query();
    let entries = self.indexes.objects.get(&span_key(span))?.clone();
    let mut fields = BTreeMap::new();
    for entry in &entries {
      self.indexes.add_queries(1);
      let ObjectEntry::Data { name, value, .. } = entry else {
        return None;
      };
      fields.insert(name.clone(), self.atom_at(*value, MAX_DEPTH)?);
    }
    Some(fields)
  }

  fn atom_at(&mut self, span: Span, remaining: u8) -> Option<PrimitiveAtom> {
    self.indexes.note_query();
    if remaining == 0 {
      return None;
    }
    if let Some(atom) = self.indexes.atoms.get(&span_key(span)) {
      return Some(atom.clone());
    }
    self.indexes.note_query();
    let hint = self.indexes.hints.get(&span_key(span)).copied()?;
    match hint {
      super::shape::ShapeHint::Identifier(Some(symbol_id), _) => {
        let root = self.indexes.root_of(symbol_id);
        if self.indexes.reassigned.contains(&root) {
          return None;
        }
        let init = *self.indexes.init_span.get(&root)?;
        self.atom_at(init, remaining.saturating_sub(1))
      }
      _ => None,
    }
  }

  fn integer_atom(&mut self, expression: &Expression<'_>) -> Option<i64> {
    let atom = self.atom_at(expression.span(), MAX_DEPTH)?;
    let value = atom.as_f64()?;
    if value.fract() != 0.0 || !(0.0..1_000_000.0).contains(&value) {
      return None;
    }
    #[expect(
      clippy::cast_possible_truncation,
      reason = "range-checked non-negative integer literal"
    )]
    Some(value as i64)
  }

  fn identity_watch_options(&self, call: &CallExpression<'_>) -> Option<WatchIdentityOptions> {
    self.indexes.note_query();
    let Some(options_expr) = call.arguments.get(2).and_then(Argument::as_expression) else {
      return Some(WatchIdentityOptions { once: false, deep: false, flush: FlushMode::Pre });
    };
    let options_expr = options_expr.get_inner_expression();
    let Expression::ObjectExpression(object) = options_expr else {
      return None;
    };
    if !identity_option_shape(&self.indexes, object) {
      return None;
    }
    let span = options_expr.span();
    let once = identity_bool(self.indexes.option_value(span, "once"))?;
    let deep = identity_bool(self.indexes.option_value(span, "deep"))?;
    match self.indexes.option_value(span, "immediate") {
      OptionValue::Absent | OptionValue::Known(Literal::Bool(_)) => {}
      OptionValue::Known(_) | OptionValue::Unknown => return None,
    }
    if self.indexes.object_prop(span, "equals").is_some() {
      return None;
    }
    Some(WatchIdentityOptions { once, deep, flush: identity_flush(object)? })
  }

  fn const_binding_of_call(&self, node_id: NodeId) -> Option<SymbolId> {
    let mut current = node_id;
    for _ in 0..MAX_DEPTH {
      self.indexes.note_query();
      let parent = self.semantic.nodes().parent_id(current);
      match self.semantic.nodes().kind(parent) {
        wrapper if is_ts_wrapper(wrapper) => current = parent,
        AstKind::VariableDeclarator(declarator) => {
          let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
            return None;
          };
          let symbol_id = binding.symbol_id.get()?;
          return self
            .semantic
            .scoping()
            .symbol_flags(symbol_id)
            .contains(SymbolFlags::ConstVariable)
            .then_some(symbol_id);
        }
        _ => return None,
      }
    }
    None
  }

  fn computed_binding_escaped_or_mutated(&self, symbol_id: SymbolId) -> bool {
    let root = self.indexes.root_of(symbol_id);
    for reference in self.semantic.symbol_references(root) {
      self.indexes.add_queries(1);
      if reference.flags().is_write() {
        return true;
      }
      if self.computed_use_is_foreign(reference.node_id()) {
        return true;
      }
    }
    false
  }

  fn computed_use_is_foreign(&self, node_id: NodeId) -> bool {
    let ident_span = self.semantic.nodes().kind(node_id).span();
    let mut current = node_id;
    for _ in 0..MAX_DEPTH {
      self.indexes.note_query();
      let parent = self.semantic.nodes().parent_id(current);
      match self.semantic.nodes().kind(parent) {
        wrapper if is_ts_wrapper(wrapper) || matches!(wrapper, AstKind::ChainExpression(_)) => {
          current = parent;
        }
        AstKind::StaticMemberExpression(member) if member.property.name.as_str() == "value" => {
          return false;
        }
        AstKind::CallExpression(call) => {
          self.indexes.note_query();
          let Some(info) = self.indexes.calls.get(&span_key(call.span)).copied() else {
            return true;
          };
          return info.api != Some("watch") || !argument_is_node(call, ident_span);
        }
        AstKind::VariableDeclarator(_) => return false,
        _ => return true,
      }
    }
    true
  }

  fn first_activation(&self, binding: SymbolId, after: Site) -> Option<Site> {
    let mut chosen: Option<Site> = None;
    for reference in self.semantic.symbol_references(binding) {
      self.indexes.add_queries(1);
      if reference.flags().is_write() {
        continue;
      }
      let Some(site) = self.value_read_site(reference.node_id()) else {
        continue;
      };
      if site.callable != after.callable || site.block != after.block || site.offset <= after.offset
      {
        continue;
      }
      if chosen.is_none_or(|current| site.offset < current.offset) {
        chosen = Some(site);
      }
    }
    chosen
  }

  fn value_read_site(&self, node_id: NodeId) -> Option<Site> {
    let parent = self.semantic.nodes().parent_id(node_id);
    match self.semantic.nodes().kind(parent) {
      AstKind::StaticMemberExpression(member) if member.property.name.as_str() == "value" => {
        self.site_of(parent)
      }
      _ => None,
    }
  }

  fn handle_stopped(&self, handle: SymbolId, after: Site, before: usize) -> bool {
    self.handle_stopped_between(handle, after, after.offset, before)
  }

  fn handle_stopped_between(
    &self,
    handle: SymbolId,
    owner: Site,
    start: usize,
    end: usize,
  ) -> bool {
    for reference in self.semantic.symbol_references(handle) {
      self.indexes.add_queries(1);
      let parent = self.semantic.nodes().parent_id(reference.node_id());
      let AstKind::CallExpression(call) = self.semantic.nodes().kind(parent) else {
        continue;
      };
      if call.callee.get_inner_expression().span()
        != self.semantic.nodes().kind(reference.node_id()).span()
      {
        continue;
      }
      let Some(site) = self.site_of(parent) else {
        continue;
      };
      if site.callable == owner.callable
        && site.block == owner.block
        && site.offset > start
        && site.offset < end
      {
        return true;
      }
    }
    false
  }

  fn first_value_read_after(
    &self,
    binding: SymbolId,
    after: Site,
    min_offset: usize,
  ) -> Option<Site> {
    let mut chosen: Option<Site> = None;
    for reference in self.semantic.symbol_references(binding) {
      self.indexes.add_queries(1);
      if reference.flags().is_write() {
        continue;
      }
      let Some(site) = self.value_read_site(reference.node_id()) else {
        continue;
      };
      if site.callable != after.callable || site.block != after.block || site.offset <= min_offset {
        continue;
      }
      if chosen.is_none_or(|current| site.offset < current.offset) {
        chosen = Some(site);
      }
    }
    chosen
  }

  fn binary_param_literal(
    &mut self,
    left: &Expression<'_>,
    right: &Expression<'_>,
    param: SymbolId,
    param_name: &str,
  ) -> Option<(bool, PrimitiveAtom)> {
    if is_param_ident(left, param, param_name, self) {
      return Some((true, self.atom_of(right)?));
    }
    if is_param_ident(right, param, param_name, self) {
      return Some((false, self.atom_of(left)?));
    }
    None
  }

  fn atom_of(&mut self, expression: &Expression<'_>) -> Option<PrimitiveAtom> {
    if let Some(atom) = super::atom::atom_of_expression(expression, self.indexes.work(), MAX_DEPTH)
    {
      return Some(atom);
    }
    let inner = expression.get_inner_expression();
    if let Expression::Identifier(identifier) = inner
      && self.reference_symbol(identifier.as_ref()).is_none()
    {
      self.indexes.note_query();
      return PrimitiveAtom::unresolved_global(identifier.name.as_str());
    }
    self.atom_at(expression.span(), MAX_DEPTH)
  }

  fn site_of(&self, node_id: NodeId) -> Option<Site> {
    let (callable, block) = self.indexes.owner_callable_block(node_id);
    let block = block?;
    let span = self.span(self.semantic.nodes().kind(node_id).span());
    Some(Site {
      node: node_id,
      callable,
      block,
      offset: span.offset,
      end: span.offset.saturating_add(span.length),
    })
  }
}

struct WatchIdentityOptions {
  once: bool,
  deep: bool,
  flush: FlushMode,
}

const fn identity_bool(value: OptionValue<Literal>) -> Option<bool> {
  match value {
    OptionValue::Absent => Some(false),
    OptionValue::Known(Literal::Bool(value)) => Some(value),
    OptionValue::Known(_) | OptionValue::Unknown => None,
  }
}

fn identity_option_shape(
  indexes: &super::index::Indexes,
  object: &oxc_ast::ast::ObjectExpression<'_>,
) -> bool {
  object.properties.iter().all(|property| {
    indexes.note_query();
    match property {
      ObjectPropertyKind::SpreadProperty(_) => false,
      ObjectPropertyKind::ObjectProperty(prop) => {
        prop.kind == oxc_ast::ast::PropertyKind::Init
          && !prop.method
          && !prop.shorthand
          && !prop.computed
          && prop.key.static_name().is_some()
      }
    }
  })
}

fn identity_flush(object: &oxc_ast::ast::ObjectExpression<'_>) -> Option<FlushMode> {
  let mut flush = FlushMode::Pre;
  for property in &object.properties {
    let ObjectPropertyKind::ObjectProperty(prop) = property else {
      return None;
    };
    if prop.key.static_name().as_deref() != Some("flush") {
      continue;
    }
    let Expression::StringLiteral(literal) = prop.value.get_inner_expression() else {
      return None;
    };
    flush = match literal.value.as_str() {
      "sync" => FlushMode::Sync,
      "pre" => FlushMode::Pre,
      "post" => FlushMode::Post,
      _ => return None,
    };
  }
  Some(flush)
}

fn apply_projection(
  payload: &Payload,
  kind: &ProjectionKind,
  work: &super::stats::WorkCounter,
) -> Option<Payload> {
  match (payload, kind) {
    (Payload::Array(items), ProjectionKind::ArrayOps(ops)) => {
      work.add_object_entries(items.len() as u64);
      let mut current = items.clone();
      for op in ops {
        current = apply_array_op(&current, op, work)?;
      }
      Some(Payload::Array(current))
    }
    (Payload::Object(fields), ProjectionKind::ObjectFields(keys)) => {
      let mut out = BTreeMap::new();
      for (result_key, source_key) in keys {
        work.add_queries(1);
        work.add_object_entries(1);
        out.insert(result_key.clone(), fields.get(source_key)?.clone());
      }
      Some(Payload::Object(out))
    }
    _ => None,
  }
}

fn apply_array_op(
  items: &[PrimitiveAtom],
  op: &ArrayOp,
  work: &super::stats::WorkCounter,
) -> Option<Vec<PrimitiveAtom>> {
  match op {
    ArrayOp::Map(map) => {
      let mut out = Vec::with_capacity(items.len());
      for item in items {
        work.add_object_entries(1);
        out.push(apply_map(item, map, work)?);
      }
      Some(out)
    }
    ArrayOp::Filter(filter) => {
      let mut out = Vec::new();
      for item in items {
        work.add_object_entries(1);
        if apply_filter(item, filter, work)? {
          work.add_object_entries(1);
          out.push(item.clone());
        }
      }
      Some(out)
    }
    ArrayOp::Slice { start, end } => {
      let start = usize::try_from(*start).ok()?;
      let end = match end {
        Some(end) => usize::try_from(*end).ok()?,
        None => items.len(),
      };
      if start > items.len() || end > items.len() || start > end {
        return None;
      }
      let slice = items.get(start..end)?;
      work.add_object_entries(slice.len() as u64);
      Some(slice.to_vec())
    }
    ArrayOp::Concat(extra) => {
      work.add_object_entries(items.len().saturating_add(extra.len()) as u64);
      let mut out = items.to_vec();
      out.extend(extra.iter().cloned());
      Some(out)
    }
    ArrayOp::ToSortedDefault => {
      let mut out = items.to_vec();
      work.add_object_entries(out.len() as u64);
      out.sort_by(|left, right| {
        work.add_queries(1);
        left.js_to_string().cmp(&right.js_to_string())
      });
      Some(out)
    }
    ArrayOp::ToSortedNumeric => {
      let mut out = items.to_vec();
      work.add_object_entries(out.len() as u64);
      if out.iter().any(|item| {
        work.add_queries(1);
        item.as_f64().is_none()
      }) {
        return None;
      }
      out.sort_by(|left, right| {
        work.add_queries(1);
        match (left.as_f64(), right.as_f64()) {
          (Some(left), Some(right)) => {
            left.partial_cmp(&right).unwrap_or(std::cmp::Ordering::Equal)
          }
          _ => std::cmp::Ordering::Equal,
        }
      });
      Some(out)
    }
  }
}

fn apply_map(
  item: &PrimitiveAtom,
  map: &MapFn,
  work: &super::stats::WorkCounter,
) -> Option<PrimitiveAtom> {
  work.add_queries(1);
  match map {
    MapFn::Identity => Some(item.clone()),
    MapFn::Negate => item.as_f64().map(|value| PrimitiveAtom::from_f64(-value)),
    MapFn::Not => Some(PrimitiveAtom::Bool(!is_truthy(item))),
    MapFn::Binary { op, literal, param_left } => binary_eval(*op, item, literal, *param_left, work),
  }
}

fn apply_filter(
  item: &PrimitiveAtom,
  filter: &FilterFn,
  work: &super::stats::WorkCounter,
) -> Option<bool> {
  work.add_queries(1);
  match filter {
    FilterFn::Always => Some(true),
    FilterFn::Truthy => Some(is_truthy(item)),
    FilterFn::Falsy => Some(!is_truthy(item)),
    FilterFn::Compare { op, literal, param_left } => {
      match binary_eval(*op, item, literal, *param_left, work)? {
        PrimitiveAtom::Bool(value) => Some(value),
        _ => None,
      }
    }
  }
}

fn binary_eval(
  op: BinaryOperator,
  item: &PrimitiveAtom,
  literal: &PrimitiveAtom,
  param_left: bool,
  work: &super::stats::WorkCounter,
) -> Option<PrimitiveAtom> {
  work.add_queries(1);
  let (left, right) = if param_left { (item, literal) } else { (literal, item) };
  match op {
    BinaryOperator::Addition => match (left, right) {
      (PrimitiveAtom::String(a), PrimitiveAtom::String(b)) => {
        Some(PrimitiveAtom::String(format!("{a}{b}")))
      }
      _ => Some(PrimitiveAtom::from_f64(left.as_f64()? + right.as_f64()?)),
    },
    BinaryOperator::Subtraction => Some(PrimitiveAtom::from_f64(left.as_f64()? - right.as_f64()?)),
    BinaryOperator::Multiplication => {
      Some(PrimitiveAtom::from_f64(left.as_f64()? * right.as_f64()?))
    }
    BinaryOperator::Division => {
      let divisor = right.as_f64()?;
      Some(PrimitiveAtom::from_f64(left.as_f64()? / divisor))
    }
    BinaryOperator::Remainder => Some(PrimitiveAtom::from_f64(left.as_f64()? % right.as_f64()?)),
    BinaryOperator::StrictEquality => Some(PrimitiveAtom::Bool(left.strict_eq(right))),
    BinaryOperator::StrictInequality => Some(PrimitiveAtom::Bool(!left.strict_eq(right))),
    BinaryOperator::LessThan => Some(PrimitiveAtom::Bool(left.as_f64()? < right.as_f64()?)),
    BinaryOperator::LessEqualThan => Some(PrimitiveAtom::Bool(left.as_f64()? <= right.as_f64()?)),
    BinaryOperator::GreaterThan => Some(PrimitiveAtom::Bool(left.as_f64()? > right.as_f64()?)),
    BinaryOperator::GreaterEqualThan => {
      Some(PrimitiveAtom::Bool(left.as_f64()? >= right.as_f64()?))
    }
    _ => None,
  }
}

fn payloads_equal(left: &Payload, right: &Payload, work: &super::stats::WorkCounter) -> bool {
  match (left, right) {
    (Payload::Array(a), Payload::Array(b)) => sequences_object_is(a, b, work),
    (Payload::Object(a), Payload::Object(b)) => {
      work.add_queries(1);
      a.len() == b.len()
        && a.iter().all(|(key, value)| {
          work.add_queries(1);
          b.get(key).is_some_and(|other| value.object_is(other))
        })
    }
    _ => false,
  }
}

fn is_truthy(atom: &PrimitiveAtom) -> bool {
  match atom {
    PrimitiveAtom::Number { bits } => {
      let value = f64::from_bits(*bits);
      value != 0.0 && !value.is_nan()
    }
    PrimitiveAtom::String(text) => !text.is_empty(),
    PrimitiveAtom::Bool(value) => *value,
    PrimitiveAtom::Null | PrimitiveAtom::Undefined => false,
  }
}

fn first_simple_param<'a>(
  parameter: Option<&'a FormalParameter<'a>>,
) -> Option<(SymbolId, &'a str)> {
  simple_param(parameter?)
}

fn simple_param<'a>(parameter: &'a FormalParameter<'a>) -> Option<(SymbolId, &'a str)> {
  if parameter.initializer.is_some() || parameter.optional {
    return None;
  }
  match &parameter.pattern {
    BindingPattern::BindingIdentifier(binding) => {
      Some((binding.symbol_id.get()?, binding.name.as_str()))
    }
    _ => None,
  }
}

fn simple_one_param_callback<'a>(
  expression: &'a Expression<'a>,
) -> Option<(SymbolId, &'a str, CallbackBody<'a>)> {
  match expression.get_inner_expression() {
    Expression::ArrowFunctionExpression(arrow) => {
      if arrow.r#async || arrow.params.rest.is_some() || arrow.params.items.len() != 1 {
        return None;
      }
      let (symbol, name) = simple_param(arrow.params.items.first()?)?;
      Some((symbol, name, CallbackBody { body: &arrow.body, expression: arrow.expression }))
    }
    Expression::FunctionExpression(function) => {
      if function.r#async
        || function.generator
        || function.params.rest.is_some()
        || function.params.items.len() != 1
      {
        return None;
      }
      let (symbol, name) = simple_param(function.params.items.first()?)?;
      Some((symbol, name, CallbackBody { body: function.body.as_ref()?, expression: false }))
    }
    _ => None,
  }
}

struct CallbackBody<'a> {
  body: &'a FunctionBody<'a>,
  expression: bool,
}

fn callback_expression<'a>(body: &'a CallbackBody<'a>) -> Option<&'a Expression<'a>> {
  returned_expression(body.body, body.expression)
}

fn returned_expression<'a>(
  body: &'a FunctionBody<'a>,
  expression_arrow: bool,
) -> Option<&'a Expression<'a>> {
  if expression_arrow {
    return expression_body(body.statements.first()?);
  }
  match body.statements.as_slice() {
    [Statement::ReturnStatement(statement)] => statement.argument.as_ref(),
    [Statement::VariableDeclaration(declaration), Statement::ReturnStatement(statement)] => {
      if declaration.kind != oxc_ast::ast::VariableDeclarationKind::Const
        || declaration.declarations.len() != 1
      {
        return None;
      }
      let declarator = declaration.declarations.first()?;
      let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
        return None;
      };
      let init = declarator.init.as_ref()?;
      let Expression::Identifier(identifier) = statement.argument.as_ref()?.get_inner_expression()
      else {
        return None;
      };
      (identifier.name == binding.name).then_some(init)
    }
    _ => None,
  }
}

fn expression_body<'a>(statement: &'a Statement<'a>) -> Option<&'a Expression<'a>> {
  match statement {
    Statement::ExpressionStatement(statement) => Some(&statement.expression),
    Statement::ReturnStatement(statement) => statement.argument.as_ref(),
    _ => None,
  }
}

fn returns_param(body: &FunctionBody<'_>, _symbol: SymbolId, name: &str) -> bool {
  body.statements.iter().any(|statement| statement_returns_param(statement, name))
}

fn statement_returns_param(statement: &Statement<'_>, name: &str) -> bool {
  match statement {
    Statement::ReturnStatement(ret) => ident_is_param(ret.argument.as_ref(), name),
    Statement::IfStatement(if_stmt) => {
      statement_returns_param(&if_stmt.consequent, name)
        || if_stmt.alternate.as_ref().is_some_and(|alt| statement_returns_param(alt, name))
    }
    Statement::BlockStatement(block) => {
      block.body.iter().any(|inner| statement_returns_param(inner, name))
    }
    _ => false,
  }
}

fn ident_is_param(expression: Option<&Expression<'_>>, name: &str) -> bool {
  expression.is_some_and(|expr| {
    matches!(expr.get_inner_expression(), Expression::Identifier(identifier) if identifier.name.as_str() == name)
  })
}

fn static_member<'a>(
  expression: &'a Expression<'a>,
) -> Option<(&'a oxc_ast::ast::IdentifierReference<'a>, &'a str)> {
  let Expression::StaticMemberExpression(member) = expression.get_inner_expression() else {
    return None;
  };
  let object = member.object.get_inner_expression().get_identifier_reference()?;
  Some((object, member.property.name.as_str()))
}

fn numeric_sort_fn(expression: &Expression<'_>) -> bool {
  let Some((first, second, body)) = two_param_numeric_body(expression) else {
    return false;
  };
  let Some(expr) = callback_expression(&body) else {
    return false;
  };
  let Expression::BinaryExpression(binary) = expr.get_inner_expression() else {
    return false;
  };
  if binary.operator != BinaryOperator::Subtraction {
    return false;
  }
  ident_named(binary.left.get_inner_expression(), first)
    && ident_named(binary.right.get_inner_expression(), second)
}

fn two_param_numeric_body<'a>(
  expression: &'a Expression<'a>,
) -> Option<(&'a str, &'a str, CallbackBody<'a>)> {
  match expression.get_inner_expression() {
    Expression::ArrowFunctionExpression(arrow)
      if !arrow.r#async && arrow.params.rest.is_none() && arrow.params.items.len() == 2 =>
    {
      let first = simple_param(arrow.params.items.first()?)?;
      let second = simple_param(arrow.params.items.get(1)?)?;
      Some((first.1, second.1, CallbackBody { body: &arrow.body, expression: arrow.expression }))
    }
    _ => None,
  }
}

fn ident_named(expression: &Expression<'_>, name: &str) -> bool {
  matches!(expression.get_inner_expression(), Expression::Identifier(identifier) if identifier.name.as_str() == name)
}

fn is_param_ident(
  expression: &Expression<'_>,
  param: SymbolId,
  param_name: &str,
  collector: &Collector<'_>,
) -> bool {
  let Expression::Identifier(identifier) = expression.get_inner_expression() else {
    return false;
  };
  identifier.name.as_str() == param_name
    && collector.reference_symbol(identifier.as_ref()) == Some(param)
}

fn collect_computed_reads(
  getter: &Expression<'_>,
  known: &BTreeSet<SymbolId>,
  collector: &Collector<'_>,
  remaining: u8,
) -> Option<Vec<SymbolId>> {
  collector.indexes.note_query();
  if remaining == 0 {
    return None;
  }
  let mut reads = Vec::new();
  match getter.get_inner_expression() {
    Expression::ArrowFunctionExpression(arrow) => {
      collect_body_reads(&arrow.body, known, collector, remaining.saturating_sub(1), &mut reads)?;
    }
    Expression::FunctionExpression(function) => {
      let body = function.body.as_ref()?;
      collect_body_reads(body, known, collector, remaining.saturating_sub(1), &mut reads)?;
    }
    _ => return None,
  }
  Some(reads)
}

fn collect_body_reads(
  body: &FunctionBody<'_>,
  known: &BTreeSet<SymbolId>,
  collector: &Collector<'_>,
  remaining: u8,
  reads: &mut Vec<SymbolId>,
) -> Option<()> {
  if remaining == 0 {
    return None;
  }
  for statement in &body.statements {
    collect_statement_reads(statement, known, collector, remaining.saturating_sub(1), reads)?;
  }
  Some(())
}

fn collect_statement_reads(
  statement: &Statement<'_>,
  known: &BTreeSet<SymbolId>,
  collector: &Collector<'_>,
  remaining: u8,
  reads: &mut Vec<SymbolId>,
) -> Option<()> {
  collector.indexes.note_query();
  if remaining == 0 {
    return None;
  }
  match statement {
    Statement::ExpressionStatement(statement) => collect_expr_reads(
      &statement.expression,
      known,
      collector,
      remaining.saturating_sub(1),
      reads,
    ),
    Statement::ReturnStatement(statement) => statement.argument.as_ref().map_or(Some(()), |expr| {
      collect_expr_reads(expr, known, collector, remaining.saturating_sub(1), reads)
    }),
    Statement::VariableDeclaration(declaration) => {
      for declarator in &declaration.declarations {
        if let Some(init) = &declarator.init {
          collect_expr_reads(init, known, collector, remaining.saturating_sub(1), reads)?;
        }
      }
      Some(())
    }
    _ => Some(()),
  }
}

fn collect_expr_reads(
  expression: &Expression<'_>,
  known: &BTreeSet<SymbolId>,
  collector: &Collector<'_>,
  remaining: u8,
  reads: &mut Vec<SymbolId>,
) -> Option<()> {
  collector.indexes.note_query();
  if remaining == 0 {
    return None;
  }
  match expression.get_inner_expression() {
    Expression::StaticMemberExpression(member) => {
      if member.property.name.as_str() == "value"
        && let Some(ident) = member.object.get_inner_expression().get_identifier_reference()
        && let Some(symbol) = collector.reference_symbol(ident)
      {
        let root = collector.indexes.root_of(symbol);
        if known.contains(&root) && !reads.contains(&root) {
          reads.push(root);
        }
      }
      collect_expr_reads(&member.object, known, collector, remaining.saturating_sub(1), reads)
    }
    Expression::ComputedMemberExpression(member) => {
      collect_expr_reads(&member.object, known, collector, remaining.saturating_sub(1), reads)?;
      collect_expr_reads(&member.expression, known, collector, remaining.saturating_sub(1), reads)
    }
    Expression::CallExpression(call) => {
      collect_expr_reads(&call.callee, known, collector, remaining.saturating_sub(1), reads)?;
      for argument in &call.arguments {
        if let Some(expr) = argument.as_expression() {
          collect_expr_reads(expr, known, collector, remaining.saturating_sub(1), reads)?;
        }
      }
      Some(())
    }
    Expression::Identifier(identifier) => {
      if let Some(symbol) = collector.reference_symbol(identifier.as_ref()) {
        let root = collector.indexes.root_of(symbol);
        if known.contains(&root) && !reads.contains(&root) {
          reads.push(root);
        }
      }
      Some(())
    }
    _ => Some(()),
  }
}

fn argument_is_node(call: &CallExpression<'_>, ident_span: Span) -> bool {
  call.arguments.iter().any(|argument| {
    argument.as_expression().is_some_and(|expr| {
      expr.span() == ident_span || expr.get_inner_expression().span() == ident_span
    })
  })
}
