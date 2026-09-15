//! Memoize / controlled-computed stale-result demand facts.

use oxc_ast::{
  AstKind,
  ast::{
    Argument, BindingPattern, CallExpression, Expression, FunctionBody, IdentifierReference,
    Statement,
  },
};
use oxc_semantic::{NodeId, SymbolFlags, SymbolId};
use oxc_span::Span;

use super::Collector;
use super::index::{CallInfo, CallUse, MemberUse};
use super::proof::{DemandOrigin, DemandRole};
use super::shape::{PrimitiveKind, native_callable, span_key};
use vue_vet_core::CachedResultDemandFact;

const NATIVE_REF: &[&str] = &["ref", "shallowRef"];
const MEMO_REPAIR: &[&str] = &["load", "delete", "clear"];
const MEMO_UNKNOWN: &[&str] = &["cache", "generateKey"];

impl Collector<'_> {
  pub(super) fn collect_cached_result(
    &mut self,
    node_id: NodeId,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    if info.has_spread || !self.indexes.native_capability_intact() {
      return;
    }
    match info.vueuse {
      Some("useMemoize") => self.collect_memoize(node_id, call, info),
      Some("computedWithControl" | "controlledComputed") => {
        self.collect_controlled(node_id, call, info);
      }
      _ => {}
    }
  }

  fn collect_memoize(&mut self, node_id: NodeId, call: &CallExpression<'_>, _info: CallInfo) {
    if !default_memo_options(call) {
      return;
    }
    let Some(resolver) = call.arguments.first().and_then(Argument::as_expression) else {
      return;
    };
    let Some(source) = self.zero_arg_ref_value_read(resolver) else {
      return;
    };
    if !self.is_native_local_ref(source) {
      return;
    }
    let Some(result) = self.bound_const_symbol(node_id) else {
      return;
    };
    let root = self.indexes.root_of(result);
    if !self.indexes.result_binding_intact(root) || self.memo_unknown_members(root) {
      return;
    }
    let origin = self.indexes.origin_for(node_id, self.span(call.span).offset);
    let Some(site) = self.stale_memo_demand(root, source, origin) else {
      return;
    };
    self.facts.memoize_stale_result_demand.push(CachedResultDemandFact {
      demand_span: self.span(site.demand),
      fill_span: self.span(site.fill),
      write_span: self.span(site.write),
      producer_span: self.span(call.span),
      cached_kind: site.cached,
      current_kind: site.current,
      member: self.indexes.copy_key(&site.member),
      api: "useMemoize".into(),
    });
  }

  fn collect_controlled(&mut self, node_id: NodeId, call: &CallExpression<'_>, info: CallInfo) {
    if info.arg_count != 2 || info.second_arg.is_none() {
      return;
    }
    let Some(listed) = self.listed_native_refs(call) else {
      return;
    };
    let Some(getter) = call.arguments.get(1).and_then(Argument::as_expression) else {
      return;
    };
    let Some(source) = self.zero_arg_ref_value_read(getter) else {
      return;
    };
    if !self.is_native_local_ref(source) {
      return;
    }
    if listed.iter().any(|symbol| {
      self.indexes.note_query();
      *symbol == source
    }) {
      return;
    }
    let Some(result) = self.bound_const_symbol(node_id) else {
      return;
    };
    let root = self.indexes.root_of(result);
    if !self.indexes.result_binding_intact(root) || self.controlled_unknown_members(root) {
      return;
    }
    let origin = self.indexes.origin_for(node_id, self.span(call.span).offset);
    let Some(site) = self.stale_controlled_demand(root, source, &listed, origin) else {
      return;
    };
    let api = match info.vueuse {
      Some("controlledComputed") => "controlledComputed",
      _ => "computedWithControl",
    };
    self.facts.controlled_computed_stale_result_demand.push(CachedResultDemandFact {
      demand_span: self.span(site.demand),
      fill_span: self.span(site.fill),
      write_span: self.span(site.write),
      producer_span: self.span(call.span),
      cached_kind: site.cached,
      current_kind: site.current,
      member: self.indexes.copy_key(&site.member),
      api: api.into(),
    });
  }

  fn stale_memo_demand(
    &self,
    root: SymbolId,
    source: SymbolId,
    origin: DemandOrigin,
  ) -> Option<StaleSite> {
    let mut events = Vec::new();
    let mut block = None;
    for call in self.indexes.identifier_calls_on(root) {
      self.indexes.note_query();
      if !zero_arg_access(call, origin) {
        continue;
      }
      if !same_block(&mut block, call.block) {
        return None;
      }
      events.push(TimelineEvent {
        offset: call.offset,
        rank: 2,
        kind: TimelineKind::Access { span: call.span },
      });
    }
    for demand in self.indexes.result_demands_on(root) {
      self.indexes.note_query();
      if !self.demand_call_ok(&demand.site, origin)
        || demand.inner.argc != 0
        || demand.inner.has_spread
        || demand.inner.callable != origin.callable
        || demand.inner.region != origin.region
      {
        continue;
      }
      if !same_block(&mut block, demand.block) {
        return None;
      }
      events.push(TimelineEvent {
        offset: demand.site.offset,
        rank: 3,
        kind: TimelineKind::Demand { span: demand.site.span, member: demand.member.as_str() },
      });
    }
    for named in self.indexes.member_calls_on(root) {
      self.indexes.note_query();
      if MEMO_REPAIR.contains(&named.key.as_str())
        && named.site.callable == origin.callable
        && named.site.region == origin.region
        && self.indexes.demand_ok(&named.site)
      {
        events.push(TimelineEvent {
          offset: named.site.offset,
          rank: 1,
          kind: TimelineKind::Repair,
        });
      }
    }
    self.push_source_writes(&mut events, source, origin, &mut block)?;
    self.fold_cache(origin, block?, source, &mut events)
  }

  fn stale_controlled_demand(
    &self,
    root: SymbolId,
    source: SymbolId,
    listed: &[SymbolId],
    origin: DemandOrigin,
  ) -> Option<StaleSite> {
    let mut events = Vec::new();
    let mut block = None;
    for site in self.indexes.value_reads_on(root) {
      self.indexes.note_query();
      if !self.indexes.demand_from(site, origin) || !site.role.needs_get() {
        continue;
      }
      events.push(TimelineEvent {
        offset: site.offset,
        rank: 2,
        kind: TimelineKind::Access { span: site.span },
      });
    }
    for demand in self.indexes.value_demands_on(root) {
      self.indexes.note_query();
      if !self.demand_call_ok(&demand.site, origin)
        || !demand.value_read.reach.is_straight()
        || demand.value_read.optional
        || demand.value_read.callable != origin.callable
        || demand.value_read.region != origin.region
        || !demand.value_read.role.needs_get()
      {
        continue;
      }
      if !same_block(&mut block, demand.block) {
        return None;
      }
      events.push(TimelineEvent {
        offset: demand.site.offset,
        rank: 3,
        kind: TimelineKind::Demand { span: demand.site.span, member: demand.member.as_str() },
      });
    }
    for named in self.indexes.member_calls_on(root) {
      self.indexes.note_query();
      if named.key == "trigger"
        && named.site.callable == origin.callable
        && named.site.region == origin.region
        && self.indexes.demand_ok(&named.site)
      {
        events.push(TimelineEvent {
          offset: named.site.offset,
          rank: 1,
          kind: TimelineKind::Repair,
        });
      }
    }
    for listed_root in listed {
      for write in self.indexes.value_writes_on(*listed_root) {
        self.indexes.note_query();
        if write.callable != origin.callable {
          continue;
        }
        if !same_block(&mut block, write.block) {
          return None;
        }
        events.push(TimelineEvent { offset: write.offset, rank: 1, kind: TimelineKind::Repair });
      }
    }
    self.push_source_writes(&mut events, source, origin, &mut block)?;
    self.fold_cache(origin, block?, source, &mut events)
  }

  fn kind_demand(
    &self,
    cached: PrimitiveKind,
    current: PrimitiveKind,
    member: &str,
    fill: Span,
    write: Span,
    demand: Span,
  ) -> Option<StaleSite> {
    let cached_fact = cached.to_fact()?;
    let current_fact = current.to_fact()?;
    if native_callable(cached, member) != Some(false)
      || native_callable(current, member) != Some(true)
    {
      return None;
    }
    Some(StaleSite {
      demand,
      fill,
      write,
      cached: cached_fact,
      current: current_fact,
      member: member.to_string(),
      demand_offset: self.span(demand).offset,
    })
  }

  fn push_source_writes(
    &self,
    events: &mut Vec<TimelineEvent<'_>>,
    source: SymbolId,
    origin: DemandOrigin,
    block: &mut Option<NodeId>,
  ) -> Option<()> {
    if self.indexes.value_writes_mixed(source) {
      return None;
    }
    for write in self.indexes.value_writes_on(source) {
      self.indexes.note_query();
      if write.callable != origin.callable {
        continue;
      }
      if !same_block(block, write.block) {
        return None;
      }
      let kind = self.kind_of_span(write.rhs, 8);
      if kind == PrimitiveKind::Unknown {
        return None;
      }
      events.push(TimelineEvent {
        offset: write.offset,
        rank: 0,
        kind: TimelineKind::SourceWrite { span: write.span, kind },
      });
    }
    Some(())
  }

  fn fold_cache(
    &self,
    origin: DemandOrigin,
    block: NodeId,
    source: SymbolId,
    events: &mut [TimelineEvent<'_>],
  ) -> Option<StaleSite> {
    self.indexes.sort_timeline(events, |event| (event.offset, event.rank));
    let mut current = self.payload_kind_at(source, origin.callable, block, origin.offset);
    let mut retained: Option<(PrimitiveKind, Span)> = None;
    let mut last_write: Option<Span> = None;
    let mut cursor = origin.offset;
    let mut chosen: Option<StaleSite> = None;
    for event in events.iter() {
      self.indexes.note_query();
      if retained.is_some()
        && (self.indexes.has_barrier_between(origin.region, cursor, event.offset)
          || self.indexes.has_foreign_event_between(block, cursor, event.offset))
      {
        return chosen;
      }
      match event.kind {
        TimelineKind::SourceWrite { span, kind } => {
          current = kind;
          last_write = Some(span);
        }
        TimelineKind::Repair => {
          retained = None;
        }
        TimelineKind::Access { span } => {
          if retained.is_none() {
            retained = Some((current, span));
          }
        }
        TimelineKind::Demand { span, member } => {
          if retained.is_none() {
            retained = Some((current, span));
          }
          let Some((cached, fill)) = retained else {
            continue;
          };
          let Some(write) = last_write else {
            continue;
          };
          if let Some(site) = self.kind_demand(cached, current, member, fill, write, span)
            && chosen
              .as_ref()
              .is_none_or(|current_site| site.demand_offset < current_site.demand_offset)
          {
            chosen = Some(site);
          }
        }
      }
      cursor = event.offset;
    }
    chosen
  }

  fn memo_unknown_members(&self, root: SymbolId) -> bool {
    self.indexes.member_calls_on(root).iter().any(|named| {
      self.indexes.note_query();
      !MEMO_REPAIR.contains(&named.key.as_str()) && !MEMO_UNKNOWN.contains(&named.key.as_str())
    }) || self.indexes.member_reads_on(root).iter().any(|named| {
      self.indexes.note_query();
      MEMO_UNKNOWN.contains(&named.key.as_str())
        || (!MEMO_REPAIR.contains(&named.key.as_str()) && named.key != "length")
    })
  }

  fn controlled_unknown_members(&self, root: SymbolId) -> bool {
    self.indexes.member_calls_on(root).iter().any(|named| {
      self.indexes.note_query();
      named.key != "trigger"
    })
  }

  fn demand_call_ok(&self, site: &MemberUse, origin: DemandOrigin) -> bool {
    self.indexes.demand_from(site, origin) && site.role == DemandRole::Other
  }

  fn is_native_local_ref(&self, root: SymbolId) -> bool {
    if self.indexes.vue_imports.contains_key(&root)
      || self.indexes.vueuse_imports.contains_key(&root)
      || self.indexes.reassigned.contains(&root)
      || self.indexes.escaped.contains(&root)
      || self.indexes.unknown_member_touch.contains(&root)
    {
      return false;
    }
    let Some(init) = self.indexes.init_span.get(&root).copied() else {
      return false;
    };
    let Some(info) = self.call_info_for(init) else {
      return false;
    };
    info.api.is_some_and(|api| NATIVE_REF.contains(&api)) && !info.has_spread
  }

  fn listed_native_refs(&self, call: &CallExpression<'_>) -> Option<Vec<SymbolId>> {
    let source = call.arguments.first().and_then(Argument::as_expression)?;
    match source.get_inner_expression() {
      Expression::Identifier(identifier) => {
        let symbol = self.ident_root(identifier)?;
        self.is_native_local_ref(symbol).then(|| vec![symbol])
      }
      Expression::ArrayExpression(array) => {
        if array.elements.iter().any(|element| {
          element.is_elision()
            || matches!(element, oxc_ast::ast::ArrayExpressionElement::SpreadElement(_))
        }) {
          return None;
        }
        let mut listed = Vec::new();
        for element in &array.elements {
          let expression = element.as_expression()?;
          let ident = expression.get_inner_expression().get_identifier_reference()?;
          let symbol = self.ident_root(ident)?;
          if !self.is_native_local_ref(symbol) {
            return None;
          }
          listed.push(symbol);
        }
        (!listed.is_empty()).then_some(listed)
      }
      _ => None,
    }
  }

  fn zero_arg_ref_value_read(&self, expr: &Expression<'_>) -> Option<SymbolId> {
    match expr.get_inner_expression() {
      Expression::ArrowFunctionExpression(arrow) => {
        if !arrow.params.items.is_empty() || arrow.params.rest.is_some() {
          return None;
        }
        if arrow.expression {
          let Statement::ExpressionStatement(statement) = arrow.body.statements.first()? else {
            return None;
          };
          if arrow.body.statements.len() != 1 {
            return None;
          }
          self.static_ref_value(&statement.expression)
        } else {
          single_return_ref_value(&arrow.body, |expression| self.static_ref_value(expression))
        }
      }
      Expression::FunctionExpression(function) => {
        if function.r#async
          || function.generator
          || !function.params.items.is_empty()
          || function.params.rest.is_some()
        {
          return None;
        }
        single_return_ref_value(function.body.as_ref()?, |expression| {
          self.static_ref_value(expression)
        })
      }
      _ => None,
    }
  }

  fn static_ref_value(&self, expr: &Expression<'_>) -> Option<SymbolId> {
    let Expression::StaticMemberExpression(member) = expr.get_inner_expression() else {
      return None;
    };
    if member.property.name.as_str() != "value" {
      return None;
    }
    let ident = member.object.get_inner_expression().get_identifier_reference()?;
    self.ident_root(ident)
  }

  fn bound_const_symbol(&self, node_id: NodeId) -> Option<SymbolId> {
    let parent = skip_ts(self.semantic, node_id);
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

  fn payload_kind_at(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    block: NodeId,
    offset: usize,
  ) -> PrimitiveKind {
    if self.indexes.value_writes_mixed(root)
      || self.indexes.value_write_owner_mismatch(root, callable, block)
    {
      return PrimitiveKind::Unknown;
    }
    if let Some(write) = self.indexes.last_value_write_before(root, callable, block, offset) {
      return self.kind_of_span(write.rhs, 8);
    }
    self.ref_init_kind(root)
  }

  fn ref_init_kind(&self, root: SymbolId) -> PrimitiveKind {
    let Some(init) = self.indexes.init_span.get(&root).copied() else {
      return PrimitiveKind::Unknown;
    };
    let Some(info) = self.call_info_for(init) else {
      return PrimitiveKind::Unknown;
    };
    if !info.api.is_some_and(|api| NATIVE_REF.contains(&api)) || info.has_spread {
      return PrimitiveKind::Unknown;
    }
    info.first_arg.map_or(PrimitiveKind::Nullish, |argument| self.kind_of_span(argument, 8))
  }

  fn kind_of_span(&self, span: Span, remaining: u8) -> PrimitiveKind {
    self.indexes.note_query();
    if remaining == 0 {
      return PrimitiveKind::Unknown;
    }
    let Some(hint) = self.indexes.hints.get(&span_key(span)).copied() else {
      return PrimitiveKind::Unknown;
    };
    match hint {
      super::shape::ShapeHint::Primitive(kind) => kind,
      super::shape::ShapeHint::Nullish | super::shape::ShapeHint::Identifier(_, true) => {
        PrimitiveKind::Nullish
      }
      super::shape::ShapeHint::Identifier(Some(symbol_id), false) => {
        let root = self.indexes.root_of(symbol_id);
        if self.indexes.reassigned.contains(&root) {
          return PrimitiveKind::Unknown;
        }
        let Some(init) = self.indexes.init_span.get(&root).copied() else {
          return PrimitiveKind::Unknown;
        };
        self.kind_of_span(init, remaining.saturating_sub(1))
      }
      _ => PrimitiveKind::Unknown,
    }
  }

  fn call_info_for(&self, init: Span) -> Option<CallInfo> {
    if let Some(info) = self.indexes.calls.get(&span_key(init)).copied() {
      return Some(info);
    }
    let super::shape::ShapeHint::Call(span) = self.indexes.hints.get(&span_key(init)).copied()?
    else {
      return None;
    };
    self.indexes.calls.get(&span_key(span)).copied()
  }

  fn ident_root(&self, identifier: &IdentifierReference<'_>) -> Option<SymbolId> {
    Some(self.indexes.root_of(self.reference_symbol(identifier)?))
  }
}

struct StaleSite {
  demand: Span,
  fill: Span,
  write: Span,
  cached: vue_vet_core::PrimitiveValueKind,
  current: vue_vet_core::PrimitiveValueKind,
  member: String,
  demand_offset: usize,
}

#[derive(Clone, Copy)]
struct TimelineEvent<'a> {
  offset: usize,
  rank: u8,
  kind: TimelineKind<'a>,
}

#[derive(Clone, Copy)]
enum TimelineKind<'a> {
  SourceWrite { span: Span, kind: PrimitiveKind },
  Repair,
  Access { span: Span },
  Demand { span: Span, member: &'a str },
}

fn zero_arg_access(call: &CallUse, origin: DemandOrigin) -> bool {
  call.argc == 0
    && !call.has_spread
    && !call.optional
    && call.reach.is_straight()
    && call.callable == origin.callable
    && call.region == origin.region
}

fn same_block(block: &mut Option<NodeId>, next: NodeId) -> bool {
  (*block).map_or_else(
    || {
      *block = Some(next);
      true
    },
    |current| current == next,
  )
}

fn default_memo_options(call: &CallExpression<'_>) -> bool {
  match call.arguments.get(1) {
    None => call.arguments.len() <= 1,
    Some(argument) => {
      if call.arguments.len() != 2 {
        return false;
      }
      let Some(expression) = argument.as_expression() else {
        return false;
      };
      matches!(
        expression.get_inner_expression(),
        Expression::ObjectExpression(object) if object.properties.is_empty()
      )
    }
  }
}

fn single_return_ref_value(
  body: &FunctionBody<'_>,
  read: impl Fn(&Expression<'_>) -> Option<SymbolId>,
) -> Option<SymbolId> {
  if body.statements.len() != 1 {
    return None;
  }
  let Statement::ReturnStatement(statement) = body.statements.first()? else {
    return None;
  };
  read(statement.argument.as_ref()?)
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
