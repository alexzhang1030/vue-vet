//! Same-instance provide/inject demand facts.

use oxc_ast::{
  AstKind,
  ast::{Argument, BindingPattern, CallExpression, Expression, FunctionBody, Statement},
};
use oxc_semantic::{NodeId, SymbolFlags, SymbolId};
use oxc_span::{GetSpan, Span};

use super::Collector;
use super::demand::skip_ts;
use super::index::{CallInfo, InjectionSite, MemberUse, chain_optional};
use super::proof::{DemandOrigin, DemandRole, classify_reach_except_chain};
use super::shape::{PrimitiveKind, native_callable, span_key};

const KIND_DEPTH: u8 = 8;

impl Collector<'_> {
  pub(super) fn collect_injection(
    &mut self,
    node_id: NodeId,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    if info.has_spread || info.api != Some("inject") {
      return;
    }
    if !self.indexes.setup_lane() || !self.indexes.native_capability_intact() {
      return;
    }
    let origin = self.indexes.origin_for(node_id, self.span(call.span).offset);
    if origin.callable.is_some() {
      return;
    }
    let Some(key) = self.inject_key(call) else {
      return;
    };
    let inject_offset = self.span(call.span).offset;
    let Some(inject) = self.indexes.inject_site(key, inject_offset, node_id) else {
      return;
    };
    if inject.optional || !inject.reach.is_straight() {
      return;
    }
    let Some(native) = self.indexes.native_symbol(key) else {
      return;
    };
    if native.callable != origin.callable || native.region != origin.region {
      return;
    }
    if native.offset >= inject.offset {
      return;
    }
    if self.indexes.payload_uncertain(key) {
      return;
    }
    let Some(provide) = self.sole_provide(key, origin) else {
      return;
    };
    if provide.optional
      || !provide.reach.is_straight()
      || provide.has_spread
      || provide.argc < 2
      || provide.block != inject.block
    {
      return;
    }
    let Some(provided) =
      provide.payload.and_then(|span| self.injection_kind_of_span(span, KIND_DEPTH))
    else {
      return;
    };
    let Some(fallback) = self.inject_fallback_kind(call, info, inject.factory) else {
      return;
    };
    let Some(site) = self.injection_demand(node_id, origin, provided, fallback) else {
      return;
    };
    if native_callable(fallback, site.member) != Some(false)
      || native_callable(provided, site.member) != Some(true)
    {
      return;
    }
    let Some(fallback_kind) = fallback.to_fact() else {
      return;
    };
    let Some(provided_kind) = provided.to_fact() else {
      return;
    };
    self.facts.inject_same_instance_provide.push(vue_vet_core::InjectSameInstanceDemandFact {
      demand_span: self.span(site.demand),
      key_span: self.span(native.span),
      provide_span: self.span(provide.span),
      inject_span: self.span(call.span),
      fallback_kind,
      provided_kind,
      member: self.indexes.copy_key(site.member),
      default_absent: info.arg_count == 1,
      reason: vue_vet_core::InjectSameInstanceDemandReason::LocalProvideMiss,
    });
  }

  fn inject_key(&self, call: &CallExpression<'_>) -> Option<SymbolId> {
    let argument = call.arguments.first().and_then(Argument::as_expression)?;
    let ident = argument.get_inner_expression().get_identifier_reference()?;
    let symbol_id = self.reference_symbol(ident)?;
    Some(self.indexes.root_of(symbol_id))
  }

  fn sole_provide(&self, key: SymbolId, origin: DemandOrigin) -> Option<InjectionSite> {
    let mut chosen: Option<InjectionSite> = None;
    for site in self.indexes.provides_on(key) {
      self.indexes.note_query();
      if site.callable != origin.callable || site.region != origin.region {
        continue;
      }
      if chosen.is_some() {
        return None;
      }
      chosen = Some(*site);
    }
    chosen
  }

  fn inject_fallback_kind(
    &mut self,
    call: &CallExpression<'_>,
    info: CallInfo,
    factory: Option<bool>,
  ) -> Option<PrimitiveKind> {
    match info.arg_count {
      1 => Some(PrimitiveKind::Nullish),
      2 => info.second_arg.and_then(|span| self.injection_kind_of_span(span, KIND_DEPTH)),
      3 => {
        let _ = info.third_arg?;
        let fallback = call.arguments.get(1).and_then(Argument::as_expression)?;
        match factory {
          Some(true) => self.factory_kind(fallback),
          Some(false) => self.injection_kind_of_span(fallback.span(), KIND_DEPTH),
          None => None,
        }
      }
      _ => None,
    }
  }

  fn factory_kind(&mut self, expression: &Expression<'_>) -> Option<PrimitiveKind> {
    match expression.get_inner_expression() {
      Expression::ArrowFunctionExpression(arrow) if arrow.expression => {
        let Statement::ExpressionStatement(statement) = arrow.body.statements.first()? else {
          return None;
        };
        self.kind_of_expression(&statement.expression, KIND_DEPTH)
      }
      Expression::ArrowFunctionExpression(arrow) => self.single_return_kind(&arrow.body),
      Expression::FunctionExpression(function) => self.single_return_kind(function.body.as_ref()?),
      _ => self.kind_of_expression(expression, KIND_DEPTH),
    }
  }

  fn single_return_kind(&mut self, body: &FunctionBody<'_>) -> Option<PrimitiveKind> {
    let mut returned: Option<Span> = None;
    for statement in &body.statements {
      self.indexes.note_query();
      match statement {
        Statement::ReturnStatement(ret) => {
          if returned.is_some() {
            return None;
          }
          returned = Some(ret.argument.as_ref()?.span());
        }
        Statement::FunctionDeclaration(_) | Statement::EmptyStatement(_) => {}
        _ => return None,
      }
    }
    self.injection_kind_of_span(returned?, KIND_DEPTH)
  }

  fn kind_of_expression(
    &mut self,
    expression: &Expression<'_>,
    remaining: u8,
  ) -> Option<PrimitiveKind> {
    match expression.get_inner_expression() {
      Expression::BooleanLiteral(_) => Some(PrimitiveKind::Boolean),
      Expression::NumericLiteral(_) => Some(PrimitiveKind::Number),
      Expression::StringLiteral(_) => Some(PrimitiveKind::String),
      Expression::BigIntLiteral(_) => Some(PrimitiveKind::BigInt),
      Expression::TemplateLiteral(literal) if literal.expressions.is_empty() => {
        Some(PrimitiveKind::String)
      }
      Expression::NullLiteral(_) => Some(PrimitiveKind::Nullish),
      Expression::UnaryExpression(unary) => unary_literal_kind(unary),
      other => self.injection_kind_of_span(other.span(), remaining),
    }
  }

  fn injection_demand(
    &self,
    node_id: NodeId,
    origin: DemandOrigin,
    provided: PrimitiveKind,
    fallback: PrimitiveKind,
  ) -> Option<DemandSite<'_>> {
    if let Some(site) = self.chained_injection_demand(node_id, origin, fallback) {
      return Some(site);
    }
    let parent = skip_ts(self.semantic, node_id);
    let AstKind::VariableDeclarator(declarator) = self.semantic.nodes().kind(parent) else {
      return None;
    };
    let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
      return None;
    };
    let symbol_id = binding.symbol_id.get()?;
    if !self.semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable) {
      return None;
    }
    let root = self.indexes.root_of(symbol_id);
    if self.indexes.reassigned.contains(&root)
      || self.indexes.escaped.contains(&root)
      || self.indexes.unknown_member_touch.contains(&root)
      || !self.indexes.capability_intact(root)
    {
      return None;
    }
    let mut chosen: Option<DemandSite> = None;
    for named in self.indexes.member_calls_on(root) {
      self.indexes.note_query();
      if !self.indexes.injection_demand_from(&named.site, origin, fallback) {
        continue;
      }
      if named.site.offset <= origin.offset {
        continue;
      }
      if native_callable(fallback, &named.key) != Some(false)
        || native_callable(provided, &named.key) != Some(true)
      {
        continue;
      }
      if chosen.as_ref().is_none_or(|current| named.site.offset < current.offset) {
        chosen = Some(DemandSite {
          demand: named.site.span,
          member: named.key.as_str(),
          offset: named.site.offset,
        });
      }
    }
    chosen
  }

  fn chained_injection_demand(
    &self,
    node_id: NodeId,
    origin: DemandOrigin,
    fallback: PrimitiveKind,
  ) -> Option<DemandSite<'_>> {
    let parent = skip_ts(self.semantic, node_id);
    let member = match self.semantic.nodes().kind(parent) {
      AstKind::StaticMemberExpression(member) => member.property.name.as_str(),
      AstKind::ComputedMemberExpression(member) => {
        let Expression::StringLiteral(literal) = member.expression.get_inner_expression() else {
          return None;
        };
        literal.value.as_str()
      }
      _ => return None,
    };
    let next = skip_ts(self.semantic, parent);
    let AstKind::CallExpression(outer) = self.semantic.nodes().kind(next) else {
      return None;
    };
    let reach = classify_reach_except_chain(self.semantic, next, self.indexes.work_counter());
    if !reach.is_straight() {
      return None;
    }
    let owner = self.indexes.owner(next);
    let demand = MemberUse {
      offset: self.span(outer.span).offset,
      span: outer.span,
      callable: owner.callable,
      region: owner.region.unwrap_or(origin.region),
      optional: chain_optional(self.semantic, next, self.indexes.work_counter()),
      call_optional: outer.optional,
      reach,
      role: DemandRole::Other,
    };
    if !self.indexes.injection_demand_from(&demand, origin, fallback) {
      return None;
    }
    Some(DemandSite { demand: outer.span, member, offset: demand.offset })
  }

  fn injection_kind_of_span(&mut self, span: Span, remaining: u8) -> Option<PrimitiveKind> {
    self.indexes.note_query();
    if remaining == 0 {
      return None;
    }
    let hint = self.indexes.hints.get(&span_key(span)).copied()?;
    match hint {
      super::shape::ShapeHint::Primitive(kind) => Some(kind),
      super::shape::ShapeHint::Nullish | super::shape::ShapeHint::Identifier(_, true) => {
        Some(PrimitiveKind::Nullish)
      }
      super::shape::ShapeHint::Identifier(Some(symbol_id), false) => {
        self.kind_of_symbol(symbol_id, remaining.saturating_sub(1))
      }
      _ => None,
    }
  }

  fn kind_of_symbol(&mut self, symbol_id: SymbolId, remaining: u8) -> Option<PrimitiveKind> {
    let root = self.indexes.root_of(symbol_id);
    if let Some(cached) = self.primitive_kind.get(&root) {
      self.indexes.note_query();
      return Some(*cached);
    }
    if remaining == 0
      || self.indexes.reassigned.contains(&root)
      || self.indexes.vue_imports.contains_key(&root)
    {
      return None;
    }
    let init = self.indexes.init_span.get(&root).copied()?;
    let kind = self.injection_kind_of_span(init, remaining)?;
    self.primitive_kind.insert(root, kind);
    Some(kind)
  }
}

struct DemandSite<'a> {
  demand: Span,
  member: &'a str,
  offset: usize,
}

fn unary_literal_kind(unary: &oxc_ast::ast::UnaryExpression<'_>) -> Option<PrimitiveKind> {
  use oxc_ast::ast::UnaryOperator;
  if !matches!(unary.operator, UnaryOperator::UnaryNegation | UnaryOperator::UnaryPlus) {
    return None;
  }
  match unary.argument.get_inner_expression() {
    Expression::NumericLiteral(_) => Some(PrimitiveKind::Number),
    Expression::BigIntLiteral(_) => Some(PrimitiveKind::BigInt),
    _ => None,
  }
}
