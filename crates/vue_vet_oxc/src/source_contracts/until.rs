//! Timeout unmatched-demand facts for `until`.

use oxc_ast::{
  AstKind,
  ast::{CallExpression, Expression},
};
use oxc_semantic::{NodeId, SymbolId};
use oxc_span::Span;

use super::Collector;
use super::index::{MemberUse, NamedUse, ObjectEntry, UntilAwaitSite, ValueWrite};
use super::proof::{DemandOrigin, classify_reach, native_kind_has_method};
use super::shape::{NativeKind, SYNC_FLUSH, Scalar, Shape, ShapeHint, span_key};
use super::timeline;
use vue_vet_core::UntilTimeoutUnmatchedDemandFact;

impl Collector<'_> {
  pub(super) fn collect_until_timeout_unmatched_demand(
    &mut self,
    node_id: NodeId,
    call: &CallExpression<'_>,
  ) {
    let Some(site) = self.until_to_be(call) else {
      return;
    };
    let Some(source) = self.closed_primitive_ref(site.source_arg) else {
      return;
    };
    let Some(expected) = self.literal_expected(site.expected) else {
      return;
    };
    let Some(timeout_span) = self.timeout_options(site.options) else {
      return;
    };
    let origin = self.indexes.origin_for(node_id, self.span(call.span).offset);
    if !self.to_be_path_is_straight(node_id) {
      return;
    }
    let Some(await_site) = self.await_of_to_be(call.span, origin) else {
      return;
    };
    let block = self.indexes.owner(node_id).block.unwrap_or(node_id);
    let Some((timeout_value, source_span)) = self.unmatched_timeout_value(
      source,
      expected,
      site.comparison_offset,
      await_site.end,
      origin.callable,
      block,
    ) else {
      return;
    };
    if timeout_value == expected {
      return;
    }
    let expected_kind = expected.kind();
    let timeout_kind = timeout_value.kind();
    if expected_kind == timeout_kind {
      return;
    }
    let demand_origin =
      DemandOrigin { callable: origin.callable, region: origin.region, offset: await_site.offset };
    let Some((demand, capability)) =
      self.incompatible_result_demand(await_site, timeout_kind, expected_kind, demand_origin)
    else {
      return;
    };
    self.facts.until_timeout_unmatched_demand.push(UntilTimeoutUnmatchedDemandFact {
      demand_span: self.span(demand.span),
      comparison_span: self.span(call.span),
      options_span: self.span(timeout_span),
      source_span: self.span(source_span),
      capability,
    });
  }

  fn until_to_be(&self, call: &CallExpression<'_>) -> Option<UntilToBe> {
    let info = self.indexes.call_info(call.span)?;
    if info.has_spread {
      return None;
    }
    let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
      return None;
    };
    if member.property.name.as_str() != "toBe" {
      return None;
    }
    let Expression::CallExpression(until_call) = member.object.get_inner_expression() else {
      return None;
    };
    let until_info = self.indexes.call_info(until_call.span)?;
    if until_info.vueuse != Some("until") || until_info.has_spread {
      return None;
    }
    let source_arg = until_info.first_arg?;
    let expected = info.first_arg?;
    let options = info.second_arg?;
    Some(UntilToBe {
      source_arg,
      expected,
      options,
      comparison_offset: self.span(call.span).offset,
    })
  }

  fn closed_primitive_ref(&mut self, span: Span) -> Option<SymbolId> {
    let hint = self.indexes.hints.get(&span_key(span)).copied()?;
    let ShapeHint::Identifier(Some(symbol_id), false) = hint else {
      return None;
    };
    let root = self.indexes.root_of(symbol_id);
    let summary = self.indexes.until_closed_source(root)?;
    if !summary.closed {
      return None;
    }
    if self.classify_symbol(root, MAX_REF_DEPTH) != Shape::RefLike {
      return None;
    }
    let info = self.indexes.call_info(summary.init_span).or_else(|| {
      let ShapeHint::Call(call_span) =
        self.indexes.hints.get(&span_key(summary.init_span)).copied()?
      else {
        return None;
      };
      self.indexes.call_info(call_span)
    })?;
    if info.has_spread || !matches!(info.api, Some("ref" | "shallowRef")) {
      return None;
    }
    let payload = match info.first_arg {
      None => Scalar::Nullish,
      Some(argument) => self.indexes.until_scalar(argument)?,
    };
    match payload.kind() {
      NativeKind::Number | NativeKind::String | NativeKind::Boolean | NativeKind::Nullish => {
        Some(root)
      }
    }
  }

  fn literal_expected(&self, span: Span) -> Option<Scalar> {
    let scalar = self.indexes.until_scalar(span)?;
    match scalar.kind() {
      NativeKind::Number | NativeKind::String | NativeKind::Boolean | NativeKind::Nullish => {
        Some(scalar)
      }
    }
  }

  fn timeout_options(&self, options: Span) -> Option<Span> {
    if self.indexes.object_has_spread(options) {
      return None;
    }
    let mut timeout = None;
    for entry in self.indexes.object_entries(options) {
      self.indexes.note_query();
      match entry {
        ObjectEntry::Spread
        | ObjectEntry::Computed
        | ObjectEntry::Accessor { .. }
        | ObjectEntry::Method { .. } => return None,
        ObjectEntry::Data { name, value, .. } => match name.as_str() {
          "timeout" => {
            if !self.finite_timeout(*value) {
              return None;
            }
            timeout = Some(*value);
          }
          "throwOnTimeout" | "deep" => {
            if self.indexes.until_scalar(*value) != Some(Scalar::Bool(false)) {
              return None;
            }
          }
          "flush" => {
            if self.indexes.until_scalar(*value) != Some(Scalar::Str(SYNC_FLUSH)) {
              return None;
            }
          }
          _ => return None,
        },
      }
    }
    timeout
  }

  fn finite_timeout(&self, span: Span) -> bool {
    let Some(Scalar::Number(bits)) = self.indexes.until_scalar(span) else {
      return false;
    };
    f64::from_bits(bits).is_finite()
  }

  fn to_be_path_is_straight(&self, node_id: NodeId) -> bool {
    let parent = self.semantic.nodes().parent_id(node_id);
    match self.semantic.nodes().kind(parent) {
      AstKind::AwaitExpression(_)
      | AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_) => true,
      _ => classify_reach(self.semantic, node_id, self.indexes.work_counter()).is_straight(),
    }
  }

  fn await_of_to_be(&self, to_be: Span, origin: DemandOrigin) -> Option<UntilAwaitSite> {
    if let Some(site) = self.indexes.until_await_for_argument(to_be)
      && self.await_belongs(site, origin)
    {
      return Some(site);
    }
    let symbol_id = self.indexes.result_of_call(to_be)?;
    let root = self.indexes.root_of(symbol_id);
    self
      .indexes
      .until_awaits_for_bound(root)
      .iter()
      .copied()
      .find(|site| self.await_belongs(*site, origin))
  }

  fn await_belongs(&self, site: UntilAwaitSite, origin: DemandOrigin) -> bool {
    self.indexes.note_query();
    site.callable == origin.callable && site.region == origin.region && site.reach.is_straight()
  }

  fn unmatched_timeout_value(
    &self,
    root: SymbolId,
    expected: Scalar,
    start: usize,
    end: usize,
    callable: Option<NodeId>,
    block: NodeId,
  ) -> Option<(Scalar, Span)> {
    let summary = self.indexes.until_closed_source(root)?;
    if !summary.closed {
      return None;
    }
    if let Some(first) = self.indexes.value_writes_on(root).first()
      && first.callable != callable
    {
      return None;
    }
    let mut current = self.scalar_before(root, callable, block, start)?;
    if current.0 == expected {
      return None;
    }
    for write in self.indexes.until_writes_in(root, start, end) {
      self.indexes.note_query();
      if write.callable != callable || write.block != block || !write.simple_assign {
        return None;
      }
      let next = self.indexes.until_scalar(write.rhs)?;
      if next == expected {
        return None;
      }
      current = (next, write.span);
    }
    Some(current)
  }

  fn scalar_before(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    block: NodeId,
    offset: usize,
  ) -> Option<(Scalar, Span)> {
    if self.has_unproven_write_before(root, callable, block, offset) {
      return None;
    }
    if let Some(prior) = self.last_same_owner_write(root, callable, block, offset) {
      let scalar = self.indexes.until_scalar(prior.rhs)?;
      return Some((scalar, prior.span));
    }
    let summary = self.indexes.until_closed_source(root)?;
    let init = self.ref_init_scalar(root)?;
    Some((init, summary.init_span))
  }

  fn has_unproven_write_before(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    block: NodeId,
    offset: usize,
  ) -> bool {
    let writes = self.indexes.value_writes_on(root);
    if writes.is_empty() {
      return false;
    }
    let prior = timeline::before(self.indexes.work_counter(), writes, offset);
    let last_same = prior.iter().rev().find(|write| {
      self.indexes.note_query();
      write.callable == callable && write.block == block && write.simple_assign
    });
    let proven_offset = last_same.map(|write| write.offset);
    prior.iter().any(|write| {
      self.indexes.note_query();
      let after_proven = proven_offset.is_none_or(|offset| write.offset > offset);
      after_proven && (write.callable != callable || write.block != block || !write.simple_assign)
    })
  }

  fn last_same_owner_write(
    &self,
    root: SymbolId,
    callable: Option<NodeId>,
    block: NodeId,
    offset: usize,
  ) -> Option<ValueWrite> {
    let writes = self.indexes.value_writes_on(root);
    if writes.is_empty() {
      return None;
    }
    timeline::before(self.indexes.work_counter(), writes, offset)
      .iter()
      .rev()
      .find(|write| {
        self.indexes.note_query();
        write.callable == callable && write.block == block && write.simple_assign
      })
      .copied()
  }

  fn ref_init_scalar(&self, root: SymbolId) -> Option<Scalar> {
    let summary = self.indexes.until_closed_source(root)?;
    let info = self.indexes.call_info(summary.init_span).or_else(|| {
      let ShapeHint::Call(call_span) =
        self.indexes.hints.get(&span_key(summary.init_span)).copied()?
      else {
        return None;
      };
      self.indexes.call_info(call_span)
    })?;
    info.first_arg.map_or(Some(Scalar::Nullish), |argument| self.indexes.until_scalar(argument))
  }

  fn incompatible_result_demand(
    &self,
    await_site: UntilAwaitSite,
    timeout_kind: NativeKind,
    expected_kind: NativeKind,
    origin: DemandOrigin,
  ) -> Option<(MemberUse, String)> {
    let mut chosen: Option<(MemberUse, String)> = None;
    for named in self.indexes.until_await_method_calls_on(await_site.span) {
      self.consider_demand(named, timeout_kind, expected_kind, origin, true, &mut chosen);
    }
    if let Some(result) = self.indexes.until_result_of_await(await_site.span) {
      let root = self.indexes.root_of(result);
      if self.indexes.result_reassigned(root) {
        return None;
      }
      for named in self.indexes.until_result_method_calls_on(root) {
        self.consider_demand(named, timeout_kind, expected_kind, origin, false, &mut chosen);
      }
    }
    chosen
  }

  fn consider_demand(
    &self,
    named: &NamedUse,
    timeout_kind: NativeKind,
    expected_kind: NativeKind,
    origin: DemandOrigin,
    chained: bool,
    chosen: &mut Option<(MemberUse, String)>,
  ) {
    self.indexes.note_query();
    if !chained && named.site.offset < origin.offset {
      return;
    }
    if !self.unmatched_demand_from(&named.site, origin, timeout_kind) {
      return;
    }
    if native_kind_has_method(timeout_kind, &named.key)
      || !native_kind_has_method(expected_kind, &named.key)
    {
      return;
    }
    if chosen.as_ref().is_none_or(|(current, _)| named.site.offset < current.offset) {
      *chosen = Some((named.site, self.indexes.copy_key(&named.key)));
    }
  }

  fn unmatched_demand_from(
    &self,
    site: &MemberUse,
    origin: DemandOrigin,
    timeout_kind: NativeKind,
  ) -> bool {
    if site.optional && timeout_kind == NativeKind::Nullish {
      return false;
    }
    if site.optional {
      return site.reach.is_straight()
        && site.callable == origin.callable
        && site.region == origin.region
        && self.indexes.until_interval_open(
          origin.callable,
          origin.region,
          origin.offset,
          site.offset,
        );
    }
    self.indexes.until_demand_from(site, origin)
  }
}

struct UntilToBe {
  source_arg: Span,
  expected: Span,
  options: Span,
  comparison_offset: usize,
}

const MAX_REF_DEPTH: u8 = 8;
