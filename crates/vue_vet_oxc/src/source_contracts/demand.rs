//! Demand-gated customRef / stopped-scope / missing toRefs-key facts.

use oxc_ast::{
  AstKind,
  ast::{
    Argument, BindingPattern, CallExpression, Expression, FunctionBody, ObjectExpression,
    ObjectPropertyKind, PropertyKind, Statement,
  },
};
use oxc_semantic::{NodeId, SymbolFlags, SymbolId};
use oxc_span::{GetSpan, Span};

use super::Collector;
use super::index::{CallInfo, MemberUse, NamedUse};
use super::proof::{
  DemandOrigin, DemandRole, ReceiverEffect, callable_receiver_effect, classify_reach,
  expression_is_noncallable_literal, is_object_prototype_key,
};
use super::shape::{Shape, span_key};
use vue_vet_core::{
  CustomRefCapability, InactiveScopeResultFact, InvalidCustomRefInterfaceFact, MissingToRefsKeyFact,
};

impl Collector<'_> {
  pub(super) fn collect_custom_ref(
    &mut self,
    node_id: NodeId,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    if info.has_spread {
      return;
    }
    let Some(factory) = call.arguments.first().and_then(Argument::as_expression) else {
      return;
    };
    let Some(interface) = self.factory_slots(factory) else {
      return;
    };
    let origin = self.indexes.origin_for(node_id, self.span(call.span).offset);
    let Some(result) = self.result_symbol(node_id) else {
      self.collect_chained_custom_ref_value(node_id, interface, factory.span(), origin);
      return;
    };
    let root = self.indexes.root_of(result);
    if !self.indexes.capability_intact(root) {
      return;
    }
    self.emit_custom_capability(root, origin, interface, factory.span());
  }

  fn emit_custom_capability(
    &mut self,
    root: oxc_semantic::SymbolId,
    origin: DemandOrigin,
    interface: FactoryInterface,
    factory: Span,
  ) {
    if matches!(interface.get, Slot::Missing | Slot::Invalid(_))
      && let Some(demand) = self.indexes.first_straight_value(root, DemandRole::needs_get, origin)
      && !prior_uncertain_other(
        &self.indexes,
        root,
        origin,
        demand.offset,
        DemandRole::needs_set,
        interface.set_effect,
      )
    {
      self.push_custom_ref(
        interface.get,
        interface.object_span,
        demand,
        factory,
        CustomRefCapability::Get,
      );
    }
    if matches!(interface.set, Slot::Missing | Slot::Invalid(_))
      && let Some(demand) = self.indexes.first_straight_value(root, DemandRole::needs_set, origin)
      && !prior_uncertain_other(
        &self.indexes,
        root,
        origin,
        demand.offset,
        DemandRole::needs_get,
        interface.get_effect,
      )
    {
      self.push_custom_ref(
        interface.set,
        interface.object_span,
        demand,
        factory,
        CustomRefCapability::Set,
      );
    }
  }

  fn collect_chained_custom_ref_value(
    &mut self,
    node_id: NodeId,
    interface: FactoryInterface,
    factory: Span,
    origin: DemandOrigin,
  ) {
    let parent = skip_ts(self.semantic, node_id);
    let AstKind::StaticMemberExpression(member) = self.semantic.nodes().kind(parent) else {
      return;
    };
    if member.property.name.as_str() != "value" {
      return;
    }
    let reach = classify_reach(self.semantic, parent, self.indexes.work_counter());
    if !reach.is_straight() {
      return;
    }
    let role = super::proof::classify_role(self.semantic, parent, self.indexes.work_counter());
    let owner = self.indexes.owner(parent);
    let demand = MemberUse {
      offset: self.span(member.span).offset,
      span: member.span,
      callable: owner.callable,
      region: owner.region.unwrap_or(origin.region),
      optional: false,
      reach,
      role,
    };
    if !self.indexes.demand_from(&demand, origin) {
      return;
    }
    if role.needs_set()
      && matches!(interface.set, Slot::Missing | Slot::Invalid(_))
      && !(role.needs_get() && interface.get_effect == ReceiverEffect::Uncertain)
    {
      self.push_custom_ref(
        interface.set,
        interface.object_span,
        demand,
        factory,
        CustomRefCapability::Set,
      );
    }
    if role.needs_get() && matches!(interface.get, Slot::Missing | Slot::Invalid(_)) {
      self.push_custom_ref(
        interface.get,
        interface.object_span,
        demand,
        factory,
        CustomRefCapability::Get,
      );
    }
  }

  fn push_custom_ref(
    &mut self,
    slot: Slot,
    object_span: Span,
    demand: MemberUse,
    factory: Span,
    missing: CustomRefCapability,
  ) {
    let interface = match slot {
      Slot::Invalid(span) => span,
      _ => object_span,
    };
    self.facts.invalid_custom_ref_interface.push(InvalidCustomRefInterfaceFact {
      interface_span: self.span(interface),
      demand_span: self.span(demand.span),
      factory_span: self.span(factory),
      missing,
    });
  }

  pub(super) fn collect_inactive_scope_run(&mut self, node_id: NodeId, call: &CallExpression<'_>) {
    let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
      return;
    };
    if member.property.name.as_str() != "run" {
      return;
    }
    let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
      return;
    };
    let Some(symbol_id) = self.reference_symbol(object) else {
      return;
    };
    let root = self.indexes.root_of(symbol_id);
    if self.indexes.vue_imports.contains_key(&root) {
      return;
    }
    if !self.symbol_is_effect_scope(root) {
      return;
    }
    if !self.indexes.capability_intact(root) {
      return;
    }
    let Some(run) = self.indexes.member_call_at(call.span) else {
      return;
    };
    if run.key != "run" || !self.indexes.demand_ok(&run.site) {
      return;
    }
    let run_site = run.site;
    let Some(stop) =
      self.indexes.last_stop_before(root, run_site.callable, run_site.region, run_site.offset)
    else {
      return;
    };
    if self.indexes.has_barrier_between(run_site.region, stop.offset, run_site.offset) {
      return;
    }
    let origin = DemandOrigin {
      callable: run_site.callable,
      region: run_site.region,
      offset: run_site.offset,
    };
    let Some(consumer) = self.run_result_consumer(node_id, call, origin) else {
      return;
    };
    self.facts.inactive_scope_result.push(InactiveScopeResultFact {
      consumer_span: self.span(consumer),
      stop_span: self.span(stop.span),
      run_span: self.span(call.span),
    });
  }

  pub(super) fn collect_missing_torefs_key(
    &mut self,
    node_id: NodeId,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    if info.has_spread {
      return;
    }
    let Some(argument) = info.first_arg else {
      return;
    };
    let shape = self.classify_span(argument, 8);
    if !shape.is_deep_mutable_proxy()
      && shape != Shape::ReadonlyProxy
      && shape != Shape::ShallowProxy
    {
      return;
    }
    let Some(object_span) = self.proxy_object_from_arg(argument) else {
      return;
    };
    if !self.indexes.has_closed_keys(object_span) {
      return;
    }
    let torefs_span = self.span(call.span);
    let origin = self.indexes.origin_for(node_id, torefs_span.offset);
    if let Some(consumer) = self.chained_call_missing(node_id, object_span, origin) {
      self.facts.missing_torefs_key.push(MissingToRefsKeyFact {
        demand_span: self.span(consumer.1),
        torefs_span,
        key: consumer.0,
      });
      return;
    }
    if let Some(symbol_id) = self.result_symbol(node_id) {
      let root = self.indexes.root_of(symbol_id);
      if self.indexes.reassigned.contains(&root) || self.indexes.escaped.contains(&root) {
        return;
      }
      self.emit_bag_missing_keys(root, object_span, torefs_span, origin);
      return;
    }
    self.emit_direct_destructure_missing(node_id, object_span, torefs_span, origin);
  }

  fn proxy_object_from_arg(&self, argument: Span) -> Option<Span> {
    if let Some(span) = self.proxy_object_span(argument) {
      return Some(span);
    }
    let hint = self.indexes.hints.get(&span_key(argument)).copied()?;
    let super::shape::ShapeHint::Identifier(Some(symbol_id), false) = hint else {
      return None;
    };
    let root = self.indexes.root_of(symbol_id);
    // Generic source5 still marks `toRefs(state)` as escaped/uncertain. Demand
    // discounts only proven Vue `toRefs` first-arg borrows; any other helper,
    // store, export, `new`, tag, or receiver-call use keeps keys unknown.
    if !self.indexes.keys_closed(root) {
      return None;
    }
    let init = self.indexes.init_span.get(&root).copied()?;
    self.proxy_object_span(init)
  }

  fn emit_bag_missing_keys(
    &mut self,
    root: SymbolId,
    object_span: Span,
    torefs_span: vue_vet_core::SourceSpan,
    origin: DemandOrigin,
  ) {
    let uses = self.indexes.chained_values_on(root);
    if let Some(use_site) = uses.iter().find(|named| {
      self.indexes.note_query();
      self.indexes.demand_from(&named.site, origin)
        && named.site.role.needs_get()
        && self.indexes.closed_object_has_key(object_span, &named.key) == Some(false)
        && !is_object_prototype_key(&named.key)
        && !self.indexes.key_mutated(root, &named.key)
    }) {
      self.facts.missing_torefs_key.push(MissingToRefsKeyFact {
        demand_span: self.span(use_site.site.span),
        torefs_span,
        key: self.indexes.copy_key(&use_site.key),
      });
    }
    let mut chosen: Option<(usize, vue_vet_core::SourceSpan, String)> = None;
    for (local, key) in self.indexes.destructures_of(root) {
      self.indexes.note_query();
      if self.indexes.closed_object_has_key(object_span, key) != Some(false)
        || is_object_prototype_key(key)
      {
        continue;
      }
      if self.indexes.reassigned.contains(local) || self.indexes.key_mutated(root, key) {
        continue;
      }
      let Some(demand) = self.indexes.first_straight_value(*local, DemandRole::needs_get, origin)
      else {
        continue;
      };
      let offset = demand.offset;
      if chosen.as_ref().is_none_or(|(current, _, _)| offset < *current) {
        chosen = Some((offset, self.span(demand.span), self.indexes.copy_key(key)));
      }
    }
    if let Some((_, demand_span, key)) = chosen {
      self.facts.missing_torefs_key.push(MissingToRefsKeyFact { demand_span, torefs_span, key });
    }
  }

  fn emit_direct_destructure_missing(
    &mut self,
    node_id: NodeId,
    object_span: Span,
    torefs_span: vue_vet_core::SourceSpan,
    origin: DemandOrigin,
  ) {
    let parent = skip_ts(self.semantic, node_id);
    let AstKind::VariableDeclarator(declarator) = self.semantic.nodes().kind(parent) else {
      return;
    };
    let BindingPattern::ObjectPattern(object) = &declarator.id else {
      return;
    };
    if object.rest.is_some() {
      return;
    }
    let mut chosen: Option<(usize, vue_vet_core::SourceSpan, String)> = None;
    for property in &object.properties {
      self.indexes.note_query();
      let Some(key) = property.key.static_name() else {
        return;
      };
      if self.indexes.closed_object_has_key(object_span, key.as_ref()) != Some(false)
        || is_object_prototype_key(key.as_ref())
      {
        continue;
      }
      let BindingPattern::BindingIdentifier(binding) = &property.value else {
        continue;
      };
      let Some(local) = binding.symbol_id.get() else {
        continue;
      };
      if self.indexes.reassigned.contains(&local) {
        continue;
      }
      let Some(demand) = self.indexes.first_straight_value(local, DemandRole::needs_get, origin)
      else {
        continue;
      };
      if chosen.as_ref().is_none_or(|(current, _, _)| demand.offset < *current) {
        chosen = Some((demand.offset, self.span(demand.span), self.indexes.copy_key(key.as_ref())));
      }
    }
    if let Some((_, demand_span, key)) = chosen {
      self.facts.missing_torefs_key.push(MissingToRefsKeyFact { demand_span, torefs_span, key });
    }
  }

  fn chained_call_missing(
    &self,
    node_id: NodeId,
    object_span: Span,
    origin: DemandOrigin,
  ) -> Option<(String, Span)> {
    let parent = skip_ts(self.semantic, node_id);
    let AstKind::StaticMemberExpression(member) = self.semantic.nodes().kind(parent) else {
      return None;
    };
    let key = member.property.name.as_str();
    if self.indexes.closed_object_has_key(object_span, key) != Some(false)
      || is_object_prototype_key(key)
    {
      return None;
    }
    let next = skip_ts(self.semantic, parent);
    let AstKind::StaticMemberExpression(value) = self.semantic.nodes().kind(next) else {
      return None;
    };
    if value.property.name.as_str() != "value" {
      return None;
    }
    let reach = classify_reach(self.semantic, next, self.indexes.work_counter());
    if !reach.is_straight() {
      return None;
    }
    let role = super::proof::classify_role(self.semantic, next, self.indexes.work_counter());
    if !role.needs_get() {
      return None;
    }
    let owner = self.indexes.owner(next);
    let demand = MemberUse {
      offset: self.span(value.span).offset,
      span: value.span,
      callable: owner.callable,
      region: owner.region.unwrap_or(origin.region),
      optional: false,
      reach,
      role,
    };
    if !self.indexes.demand_from(&demand, origin) {
      return None;
    }
    Some((self.indexes.copy_key(key), value.span))
  }

  fn result_symbol(&self, node_id: NodeId) -> Option<SymbolId> {
    let parent = skip_ts(self.semantic, node_id);
    match self.semantic.nodes().kind(parent) {
      AstKind::VariableDeclarator(declarator) => {
        let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
          return None;
        };
        let symbol_id = binding.symbol_id.get()?;
        if !self.semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable) {
          return None;
        }
        Some(symbol_id)
      }
      _ => None,
    }
  }

  fn symbol_is_effect_scope(&self, root: SymbolId) -> bool {
    let Some(init) = self.indexes.init_span.get(&root).copied() else {
      return false;
    };
    let Some(info) = self.indexes.calls.get(&span_key(init)).copied() else {
      if let super::shape::ShapeHint::Call(span) =
        self.indexes.hints.get(&span_key(init)).copied().unwrap_or(super::shape::ShapeHint::Unknown)
      {
        return self
          .indexes
          .calls
          .get(&span_key(span))
          .copied()
          .is_some_and(|info| info.api == Some("effectScope") && !info.has_spread);
      }
      return false;
    };
    info.api == Some("effectScope") && !info.has_spread
  }

  fn run_result_consumer(
    &self,
    node_id: NodeId,
    call: &CallExpression<'_>,
    origin: DemandOrigin,
  ) -> Option<Span> {
    let parent = skip_ts(self.semantic, node_id);
    match self.semantic.nodes().kind(parent) {
      AstKind::StaticMemberExpression(member) => {
        let reach = classify_reach(self.semantic, parent, self.indexes.work_counter());
        reach.is_straight().then_some(member.span)
      }
      AstKind::CallExpression(outer) if expression_is_callee(&outer.callee, call) => {
        let reach = classify_reach(self.semantic, parent, self.indexes.work_counter());
        reach.is_straight().then_some(outer.span)
      }
      AstKind::VariableDeclarator(declarator) => {
        let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
          return None;
        };
        let symbol_id = binding.symbol_id.get()?;
        if !self.semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable) {
          return None;
        }
        let root = self.indexes.root_of(symbol_id);
        if self.indexes.escaped.contains(&root) || self.indexes.reassigned.contains(&root) {
          return None;
        }
        let reads = self.indexes.member_reads_on(root);
        let calls = self.indexes.member_calls_on(root);
        min_straight_span(&self.indexes, reads, calls, origin)
      }
      _ => None,
    }
  }

  fn factory_slots(&self, factory: &Expression<'_>) -> Option<FactoryInterface> {
    let inner = factory.get_inner_expression();
    let object = match inner {
      Expression::ArrowFunctionExpression(arrow) => {
        return_object_from_body(&arrow.body, arrow.expression)?
      }
      Expression::FunctionExpression(function) => {
        let body = function.body.as_ref()?;
        return_object_from_body(body, false)?
      }
      _ => return None,
    };
    slots_of_object(object, |ident| self.reference_symbol(ident), self.indexes.work_counter())
  }
}

fn min_straight_span(
  indexes: &super::index::Indexes,
  reads: &[NamedUse],
  calls: &[NamedUse],
  origin: DemandOrigin,
) -> Option<Span> {
  let mut chosen: Option<MemberUse> = None;
  for named in reads.iter().chain(calls) {
    indexes.note_query();
    if !indexes.demand_from(&named.site, origin) {
      continue;
    }
    if chosen.is_none_or(|current| named.site.offset < current.offset) {
      chosen = Some(named.site);
    }
  }
  chosen.map(|site| site.span)
}

fn prior_uncertain_other(
  indexes: &super::index::Indexes,
  root: oxc_semantic::SymbolId,
  origin: DemandOrigin,
  before: usize,
  needs: impl Fn(DemandRole) -> bool,
  effect: ReceiverEffect,
) -> bool {
  effect == ReceiverEffect::Uncertain
    && indexes.has_straight_value_before(root, needs, origin, before)
}

#[derive(Clone, Copy)]
struct FactoryInterface {
  get: Slot,
  set: Slot,
  get_effect: ReceiverEffect,
  set_effect: ReceiverEffect,
  object_span: Span,
}

#[derive(Clone, Copy)]
enum Slot {
  Missing,
  Callable,
  Invalid(Span),
}

fn return_object_from_body<'a>(
  body: &'a FunctionBody<'a>,
  expression_arrow: bool,
) -> Option<&'a ObjectExpression<'a>> {
  if expression_arrow {
    let Statement::ExpressionStatement(statement) = body.statements.first()? else {
      return None;
    };
    return as_object(&statement.expression);
  }
  let mut returned = None;
  for statement in &body.statements {
    match statement {
      Statement::ReturnStatement(ret) => {
        if returned.is_some() {
          return None;
        }
        returned = as_object(ret.argument.as_ref()?);
      }
      Statement::VariableDeclaration(_) | Statement::FunctionDeclaration(_) => {}
      _ => return None,
    }
  }
  returned
}

fn as_object<'a>(expression: &'a Expression<'a>) -> Option<&'a ObjectExpression<'a>> {
  match expression.get_inner_expression() {
    Expression::ObjectExpression(object) => Some(object),
    _ => None,
  }
}

fn slots_of_object(
  object: &ObjectExpression<'_>,
  symbol_of: impl Fn(&oxc_ast::ast::IdentifierReference<'_>) -> Option<SymbolId>,
  work: &super::stats::WorkCounter,
) -> Option<FactoryInterface> {
  let mut get = Slot::Missing;
  let mut set = Slot::Missing;
  let mut get_effect = ReceiverEffect::Closed;
  let mut set_effect = ReceiverEffect::Closed;
  for property in &object.properties {
    match property {
      ObjectPropertyKind::SpreadProperty(_) => return None,
      ObjectPropertyKind::ObjectProperty(prop) => {
        if prop.kind != PropertyKind::Init {
          return None;
        }
        let name = prop.key.static_name()?;
        if name == "__proto__" || name == "prototype" {
          return None;
        }
        if name != "get" && name != "set" {
          continue;
        }
        let (slot, effect) = if prop.method || is_function_expr(&prop.value) {
          (Slot::Callable, callable_receiver_effect(&prop.value, work))
        } else if expression_is_noncallable_literal(&prop.value, &symbol_of) {
          (Slot::Invalid(prop.value.span()), ReceiverEffect::Closed)
        } else {
          return None;
        };
        if name == "get" {
          get = slot;
          get_effect = effect;
        } else {
          set = slot;
          set_effect = effect;
        }
      }
    }
  }
  Some(FactoryInterface { get, set, get_effect, set_effect, object_span: object.span })
}

fn is_function_expr(expression: &Expression<'_>) -> bool {
  matches!(
    expression.get_inner_expression(),
    Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_)
  )
}

fn skip_ts(semantic: &oxc_semantic::Semantic<'_>, mut node_id: NodeId) -> NodeId {
  for _ in 0..8 {
    let parent = semantic.nodes().parent_id(node_id);
    match semantic.nodes().kind(parent) {
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_) => node_id = parent,
      _ => return parent,
    }
  }
  node_id
}

fn expression_is_callee(callee: &Expression<'_>, call: &CallExpression<'_>) -> bool {
  callee.get_inner_expression().span() == call.span
}
