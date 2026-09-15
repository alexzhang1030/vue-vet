//! Cancelled `useDebounceFn` promise demand facts.

use oxc_ast::{
  AstKind,
  ast::{
    Argument, BindingPattern, CallExpression, Expression, FormalParameter, FunctionBody, Statement,
  },
};
use oxc_semantic::{NodeId, SymbolFlags, SymbolId};
use oxc_span::{GetSpan, Span};

use super::Collector;
use super::index::{CallInfo, CallUse, NamedUse, UntilAwaitSite};
use super::proof::skip_ts_parent;
use super::shape::{PrimitiveAtom, PrimitiveKind, native_callable, native_return_kind};
use vue_vet_core::CancelledFilterPromiseDemandFact;

const KIND_DEPTH: u8 = 8;

struct FilterSite {
  demand: Span,
  first: Span,
  superseding: Span,
  awaited: Span,
  member: String,
}

impl Collector<'_> {
  pub(super) fn collect_cancelled_filter(
    &mut self,
    node_id: NodeId,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    if info.has_spread || info.vueuse != Some("useDebounceFn") || info.arg_count < 2 {
      return;
    }
    if !self.indexes.native_capability_intact() {
      return;
    }
    if !self.positive_literal_delay(info.second_arg) {
      return;
    }
    if self.max_wait_absent(info.third_arg) != Some(true) {
      return;
    }
    if self.reject_on_cancel(info.third_arg) != Some(false) {
      return;
    }
    let Some(callback) = call.arguments.first().and_then(Argument::as_expression) else {
      return;
    };
    if is_async_fn(callback) {
      return;
    }
    let Some(wrapper) = self.filter_bound_const_symbol(node_id) else {
      return;
    };
    let root = self.indexes.root_of(wrapper);
    if !self.indexes.result_binding_intact(root) {
      return;
    }
    let origin = self.indexes.origin_for(node_id, self.span(call.span).offset);
    self.emit_cancelled_demands(root, origin, callback, call.span);
  }

  fn emit_cancelled_demands(
    &mut self,
    wrapper: SymbolId,
    origin: super::proof::DemandOrigin,
    callback: &Expression<'_>,
    producer: Span,
  ) {
    let calls = self.indexes.identifier_calls_on(wrapper).to_vec();
    for (index, first) in calls.iter().enumerate() {
      self.indexes.note_query();
      if !self.wrapper_call_ok(first, origin) {
        continue;
      }
      let Some(promise) = self.const_result_of(first) else {
        continue;
      };
      if !self.indexes.result_binding_intact(promise) {
        continue;
      }
      for later in calls.iter().skip(index.saturating_add(1)) {
        self.indexes.note_query();
        if !self.wrapper_call_ok(later, origin)
          || later.offset <= first.offset
          || later.region != first.region
          || later.callable != first.callable
          || later.block != first.block
        {
          continue;
        }
        if self.indexes.has_barrier_between(first.region, first.offset, later.offset)
          || self.indexes.until_has_await_between(
            first.callable,
            first.region,
            first.offset,
            later.offset,
          )
        {
          continue;
        }
        let Some(result_kind) = self.filter_callback_result_kind(callback, first, later) else {
          continue;
        };
        let Some(site) = self.demand_after_cancel(promise, origin, first, later, result_kind)
        else {
          continue;
        };
        self.facts.cancelled_filter_promise_demand.push(CancelledFilterPromiseDemandFact {
          demand_span: self.span(site.demand),
          producer_span: self.span(producer),
          first_call_span: self.span(site.first),
          superseding_call_span: self.span(site.superseding),
          await_span: self.span(site.awaited),
          member: site.member,
          api: "useDebounceFn".into(),
        });
        break;
      }
    }
  }

  fn wrapper_call_ok(&self, call: &CallUse, origin: super::proof::DemandOrigin) -> bool {
    self.indexes.note_query();
    call.reach.is_straight()
      && !call.optional
      && !call.has_spread
      && call.callable == origin.callable
      && call.region == origin.region
      && call.offset >= origin.offset
  }

  fn demand_after_cancel(
    &self,
    promise: SymbolId,
    origin: super::proof::DemandOrigin,
    first: &CallUse,
    later: &CallUse,
    result_kind: PrimitiveKind,
  ) -> Option<FilterSite> {
    let mut chosen = self.chained_await_demand(promise, origin, first, later, result_kind);
    if let Some(site) =
      self.named_demand_after_await(promise, promise, origin, first, later, result_kind)
      && chosen.as_ref().is_none_or(|current| site.demand.start < current.demand.start)
    {
      chosen = Some(site);
    }
    for alias in self.filter_awaited_aliases(promise) {
      self.indexes.note_query();
      if !self.indexes.result_binding_intact(alias) {
        continue;
      }
      if let Some(site) =
        self.named_demand_after_await(alias, promise, origin, first, later, result_kind)
        && chosen.as_ref().is_none_or(|current| site.demand.start < current.demand.start)
      {
        chosen = Some(site);
      }
    }
    chosen
  }

  fn chained_await_demand(
    &self,
    promise: SymbolId,
    origin: super::proof::DemandOrigin,
    first: &CallUse,
    later: &CallUse,
    result_kind: PrimitiveKind,
  ) -> Option<FilterSite> {
    let mut chosen = None;
    for awaited in self.indexes.until_awaits_for_bound(promise) {
      self.indexes.note_query();
      if !self.await_ok(awaited, origin, later.offset) {
        continue;
      }
      for demand in self.indexes.until_await_method_calls_on(awaited.span) {
        self.indexes.note_query();
        let Some(site) = self.demand_site(demand, origin, first, later, awaited, result_kind, true)
        else {
          continue;
        };
        if chosen
          .as_ref()
          .is_none_or(|current: &FilterSite| site.demand.start < current.demand.start)
        {
          chosen = Some(site);
        }
      }
    }
    chosen
  }

  fn named_demand_after_await(
    &self,
    demand_root: SymbolId,
    await_root: SymbolId,
    origin: super::proof::DemandOrigin,
    first: &CallUse,
    later: &CallUse,
    result_kind: PrimitiveKind,
  ) -> Option<FilterSite> {
    let demands = self.indexes.member_calls_on(demand_root);
    let start = self
      .indexes
      .work_counter()
      .partition_point(demands, |demand| demand.site.offset <= later.offset);
    demands.get(start..).and_then(|rest| {
      rest.iter().find_map(|named| {
        self.indexes.note_query();
        let awaited =
          self.await_before_demand(await_root, origin, later.offset, named.site.offset)?;
        self.demand_site(named, origin, first, later, awaited, result_kind, false)
      })
    })
  }

  fn await_ok(
    &self,
    awaited: &UntilAwaitSite,
    origin: super::proof::DemandOrigin,
    after: usize,
  ) -> bool {
    self.indexes.note_query();
    awaited.offset >= after
      && awaited.callable == origin.callable
      && awaited.region == origin.region
      && awaited.reach.is_straight()
  }

  fn await_before_demand(
    &self,
    promise: SymbolId,
    origin: super::proof::DemandOrigin,
    after: usize,
    demand_offset: usize,
  ) -> Option<&UntilAwaitSite> {
    let awaits = self.indexes.until_awaits_for_bound(promise);
    let start =
      self.indexes.work_counter().partition_point(awaits, |awaited| awaited.offset < after);
    awaits.get(start..).and_then(|rest| {
      rest.iter().find(|awaited| {
        self.indexes.note_query();
        self.await_ok(awaited, origin, after) && awaited.offset <= demand_offset
      })
    })
  }

  #[expect(
    clippy::too_many_arguments,
    reason = "demand proof needs call pair, await, and result kind"
  )]
  fn demand_site(
    &self,
    demand: &NamedUse,
    origin: super::proof::DemandOrigin,
    first: &CallUse,
    later: &CallUse,
    awaited: &UntilAwaitSite,
    result_kind: PrimitiveKind,
    chained_await: bool,
  ) -> Option<FilterSite> {
    if demand.site.optional
      || !demand.site.reach.is_straight()
      || demand.site.callable != origin.callable
      || demand.site.region != origin.region
    {
      return None;
    }
    if demand.site.offset <= later.offset {
      return None;
    }
    if chained_await {
      if awaited.offset <= later.offset {
        return None;
      }
    } else if demand.site.offset < awaited.offset {
      return None;
    }
    if native_callable(PrimitiveKind::Nullish, &demand.key) != Some(false)
      || native_callable(result_kind, &demand.key) != Some(true)
    {
      return None;
    }
    Some(FilterSite {
      demand: demand.site.span,
      first: first.span,
      superseding: later.span,
      awaited: awaited.span,
      member: self.indexes.copy_key(&demand.key),
    })
  }

  fn filter_callback_result_kind(
    &self,
    callback: &Expression<'_>,
    first: &CallUse,
    later: &CallUse,
  ) -> Option<PrimitiveKind> {
    match callback.get_inner_expression() {
      Expression::ArrowFunctionExpression(arrow) if arrow.expression => {
        let Statement::ExpressionStatement(statement) = arrow.body.statements.first()? else {
          return None;
        };
        self.filter_returned_kind(&statement.expression, &arrow.params.items, first, later)
      }
      Expression::ArrowFunctionExpression(arrow) => {
        self.filter_single_return_kind(&arrow.body, &arrow.params.items, first, later)
      }
      Expression::FunctionExpression(function) => self.filter_single_return_kind(
        function.body.as_ref()?,
        &function.params.items,
        first,
        later,
      ),
      _ => None,
    }
  }

  fn filter_single_return_kind(
    &self,
    body: &FunctionBody<'_>,
    params: &[FormalParameter<'_>],
    first: &CallUse,
    later: &CallUse,
  ) -> Option<PrimitiveKind> {
    let mut returned: Option<&Expression<'_>> = None;
    for statement in &body.statements {
      self.indexes.note_query();
      match statement {
        Statement::ReturnStatement(ret) => {
          if returned.is_some() {
            return None;
          }
          returned = Some(ret.argument.as_ref()?);
        }
        Statement::FunctionDeclaration(_) | Statement::EmptyStatement(_) => {}
        _ => return None,
      }
    }
    self.filter_returned_kind(returned?, params, first, later)
  }

  fn filter_returned_kind(
    &self,
    expression: &Expression<'_>,
    params: &[FormalParameter<'_>],
    first: &CallUse,
    later: &CallUse,
  ) -> Option<PrimitiveKind> {
    if let Some(kind) = self.filter_kind_of_span(expression.span(), KIND_DEPTH) {
      return (kind != PrimitiveKind::Unknown && kind != PrimitiveKind::Nullish).then_some(kind);
    }
    let Expression::CallExpression(call) = expression.get_inner_expression() else {
      return None;
    };
    if call.optional || call.arguments.iter().any(Argument::is_spread) {
      return None;
    }
    let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
      return None;
    };
    let ident = member.object.get_inner_expression().get_identifier_reference()?;
    let symbol_id = self.reference_symbol(ident)?;
    if !param_is(params, symbol_id) {
      return None;
    }
    let method = member.property.name.as_str();
    let first_kind = self.call_first_arg_kind(first)?;
    let later_kind = self.call_first_arg_kind(later)?;
    if native_callable(first_kind, method) != Some(true)
      || native_callable(later_kind, method) != Some(true)
    {
      return None;
    }
    let returned = native_return_kind(later_kind, method);
    (returned != PrimitiveKind::Unknown).then_some(returned)
  }

  fn call_first_arg_kind(&self, call: &CallUse) -> Option<PrimitiveKind> {
    let info = self.indexes.call_info(call.span)?;
    self.filter_kind_of_span(info.first_arg?, KIND_DEPTH)
  }

  fn max_wait_absent(&self, options: Option<Span>) -> Option<bool> {
    let Some(span) = options else {
      return Some(true);
    };
    match self.indexes.object_prop(span, "maxWait") {
      None => {
        if self.indexes.has_closed_keys(span) {
          Some(true)
        } else {
          None
        }
      }
      Some(super::index::ObjectProp::Unknown) => None,
      Some(super::index::ObjectProp::Value(value)) => {
        match self.filter_kind_of_span(value, KIND_DEPTH) {
          Some(PrimitiveKind::Nullish) => Some(true),
          Some(PrimitiveKind::Unknown) | None => None,
          Some(_) => Some(false),
        }
      }
    }
  }

  fn reject_on_cancel(&self, options: Option<Span>) -> Option<bool> {
    let Some(span) = options else {
      return Some(false);
    };
    match self.indexes.object_prop(span, "rejectOnCancel") {
      None => {
        if self.indexes.has_closed_keys(span) {
          Some(false)
        } else {
          None
        }
      }
      Some(super::index::ObjectProp::Unknown) => None,
      Some(super::index::ObjectProp::Value(value)) => {
        match self.filter_kind_of_span(value, KIND_DEPTH) {
          Some(PrimitiveKind::Boolean) => self.filter_boolean_at(value),
          Some(PrimitiveKind::Nullish) => Some(false),
          _ => None,
        }
      }
    }
  }

  fn filter_bound_const_symbol(&self, node_id: NodeId) -> Option<SymbolId> {
    let parent = skip_ts_parent(self.semantic, node_id, self.indexes.work_counter());
    let AstKind::VariableDeclarator(declarator) = self.semantic.nodes().kind(parent) else {
      return None;
    };
    let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
      return None;
    };
    let symbol_id = binding.symbol_id.get()?;
    self
      .semantic
      .scoping()
      .symbol_flags(symbol_id)
      .contains(SymbolFlags::ConstVariable)
      .then_some(symbol_id)
  }

  fn const_result_of(&self, call: &CallUse) -> Option<SymbolId> {
    let symbol_id = self.indexes.result_of_call(call.span)?;
    self
      .semantic
      .scoping()
      .symbol_flags(symbol_id)
      .contains(SymbolFlags::ConstVariable)
      .then_some(self.indexes.root_of(symbol_id))
  }

  fn filter_awaited_aliases(&self, promise: SymbolId) -> Vec<SymbolId> {
    let mut aliases = Vec::new();
    for awaited in self.indexes.until_awaits_for_bound(promise) {
      self.indexes.note_query();
      let Some(alias) = self.indexes.until_result_of_await(awaited.span) else {
        continue;
      };
      if !self.semantic.scoping().symbol_flags(alias).contains(SymbolFlags::ConstVariable) {
        continue;
      }
      aliases.push(self.indexes.root_of(alias));
    }
    aliases
  }

  fn filter_kind_of_span(&self, span: Span, remaining: u8) -> Option<PrimitiveKind> {
    self.indexes.note_query();
    if remaining == 0 {
      return None;
    }
    if let Some(kind) = self.filter_kind_of_atom(span) {
      return Some(kind);
    }
    let hint = self.indexes.hints.get(&super::shape::span_key(span)).copied()?;
    match hint {
      super::shape::ShapeHint::Primitive(kind) => Some(kind),
      super::shape::ShapeHint::Nullish | super::shape::ShapeHint::Identifier(_, true) => {
        Some(PrimitiveKind::Nullish)
      }
      super::shape::ShapeHint::Identifier(Some(symbol_id), false) => {
        self.filter_kind_of_symbol(symbol_id, remaining.saturating_sub(1))
      }
      _ => None,
    }
  }

  fn filter_kind_of_atom(&self, span: Span) -> Option<PrimitiveKind> {
    match self.indexes.primitive_at(span)? {
      PrimitiveAtom::Bool(_) => Some(PrimitiveKind::Boolean),
      PrimitiveAtom::Number { .. } => Some(PrimitiveKind::Number),
      PrimitiveAtom::Str(_) => Some(PrimitiveKind::String),
      PrimitiveAtom::BigInt(_) => Some(PrimitiveKind::BigInt),
      PrimitiveAtom::Null | PrimitiveAtom::Undefined => Some(PrimitiveKind::Nullish),
    }
  }

  fn filter_kind_of_symbol(&self, symbol_id: SymbolId, remaining: u8) -> Option<PrimitiveKind> {
    let root = self.indexes.root_of(symbol_id);
    self.indexes.note_query();
    if remaining == 0 {
      return None;
    }
    if self.indexes.reassigned.contains(&root)
      || self.indexes.unknown_member_touch.contains(&root)
      || self.indexes.capability_touch.contains(&root)
    {
      return None;
    }
    let init = self.indexes.init_span.get(&root).copied()?;
    if self.indexes.escaped.contains(&root) {
      if !self.semantic.scoping().symbol_flags(root).contains(SymbolFlags::ConstVariable) {
        return None;
      }
      if self.filter_kind_of_atom(init).is_none() {
        let hint = self.indexes.hints.get(&super::shape::span_key(init)).copied()?;
        if !matches!(hint, super::shape::ShapeHint::Primitive(_)) {
          return None;
        }
      }
    }
    self.filter_kind_of_span(init, remaining.saturating_sub(1))
  }

  fn filter_boolean_at(&self, span: Span) -> Option<bool> {
    match self.indexes.primitive_at(span)? {
      PrimitiveAtom::Bool(value) => Some(value),
      _ => None,
    }
  }

  fn filter_number_at(&self, span: Span) -> Option<f64> {
    match self.indexes.primitive_at(span)? {
      PrimitiveAtom::Number { bits, nan: false } => Some(f64::from_bits(bits)),
      _ => None,
    }
  }

  fn positive_literal_delay(&self, delay: Option<Span>) -> bool {
    delay.and_then(|span| self.filter_number_at(span)).is_some_and(|value| {
      self.indexes.note_query();
      value.is_finite() && value > 0.0
    })
  }
}

fn is_async_fn(expression: &Expression<'_>) -> bool {
  match expression.get_inner_expression() {
    Expression::ArrowFunctionExpression(arrow) => arrow.r#async,
    Expression::FunctionExpression(function) => function.r#async,
    _ => false,
  }
}

fn param_is(params: &[FormalParameter<'_>], symbol_id: SymbolId) -> bool {
  params.iter().any(|param| {
    matches!(
      &param.pattern,
      BindingPattern::BindingIdentifier(binding)
        if binding.symbol_id.get() == Some(symbol_id)
    )
  })
}
