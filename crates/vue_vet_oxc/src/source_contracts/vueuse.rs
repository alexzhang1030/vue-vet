//! `VueUse` demand facts: sync ignore windows and shared first-instance args.

use oxc_ast::{
  AstKind,
  ast::{
    Argument, BindingPattern, CallExpression, Expression, FormalParameters, FunctionBody, Statement,
  },
};
use oxc_semantic::{NodeId, SymbolId};
use oxc_span::Span;

use super::Collector;
use super::index::{AwaitPositionSite, CallInfo, CallUse, MemberUse, ObjectProp, Site, ValueWrite};
use super::proof::{
  DemandOrigin, DemandRole, classify_reach, is_ts_wrapper, native_kind_has_method,
};
use super::shape::{NativeKind, SYNC_FLUSH, Scalar, Shape, ShapeHint, is_ref_api, span_key};
use super::timeline;
use vue_vet_core::{IgnorableAsyncIgnoreWindowFact, SharedComposableFirstInstanceArgsFact};

const IGNORE_OPTION_KEYS: &[&str] = &["flush", "immediate", "deep", "once"];

struct StopHandles {
  idents: Vec<SymbolId>,
  bag: Option<SymbolId>,
}

impl Collector<'_> {
  pub(super) fn collect_ignorable_async_ignore_window(
    &mut self,
    node_id: NodeId,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    let Some(api) = info.vueuse else {
      return;
    };
    if !matches!(api, "watchIgnorable" | "ignorableWatch") || info.has_spread {
      return;
    }
    let origin = self.indexes.origin_for(node_id, self.span(call.span).offset);
    let reach = classify_reach(self.semantic, node_id, self.indexes.work_counter());
    if !reach.is_straight() {
      return;
    }
    if !self.sync_flush_closed_options(info.third_arg) {
      return;
    }
    if self.watcher_consumed_once_immediate(info.third_arg) {
      return;
    }
    let Some(source) = info.first_arg.and_then(|span| self.live_primitive_ref(span)) else {
      return;
    };
    let Some(callback) = info.second_arg else {
      return;
    };
    if !self.inline_callback_consumes_value(callback) {
      return;
    }
    let parent = origin.callable;
    let region = origin.region;
    if self.watcher_stopped(node_id, call, parent, region) {
      return;
    }
    let ignore_roots = self.ignore_update_roots(node_id, call);
    if ignore_roots.is_empty() {
      return;
    }
    let stops = self.stop_handles(node_id, call);
    let mut chosen: Option<(usize, Span, Span, Span)> = None;
    for root in ignore_roots {
      self.collect_ignore_root(root, source, parent, region, &stops, &mut chosen);
    }
    if let Some((_, write, ignore, await_span)) = chosen {
      self.facts.ignorable_async_ignore_window.push(IgnorableAsyncIgnoreWindowFact {
        write_span: self.span(write),
        ignore_span: self.span(ignore),
        await_span: self.span(await_span),
        api: api.into(),
      });
    }
  }

  fn collect_ignore_root(
    &self,
    root: SymbolId,
    source: SymbolId,
    parent: Option<NodeId>,
    region: NodeId,
    stops: &StopHandles,
    chosen: &mut Option<(usize, Span, Span, Span)>,
  ) {
    for site in self.indexes.identifier_calls_on(root) {
      let site = member_from_call(site);
      self.consider_ignore_call(&site, None, source, parent, region, stops, chosen);
    }
    for named in self.indexes.member_calls_on(root) {
      if named.key != "ignoreUpdates" {
        continue;
      }
      self.consider_ignore_call(
        &named.site,
        Some(named.site.head.span),
        source,
        parent,
        region,
        stops,
        chosen,
      );
    }
  }

  #[expect(
    clippy::too_many_arguments,
    reason = "one ignoreUpdates site carries owner, source, and stop proof"
  )]
  fn consider_ignore_call(
    &self,
    site: &MemberUse,
    call_span: Option<Span>,
    source: SymbolId,
    parent: Option<NodeId>,
    region: NodeId,
    stops: &StopHandles,
    chosen: &mut Option<(usize, Span, Span, Span)>,
  ) {
    if site.head.callable != parent || site.head.region != region || !self.indexes.demand_ok(site) {
      return;
    }
    let span = call_span.unwrap_or(site.head.span);
    let Some(info) = self.indexes.call_info(span) else {
      return;
    };
    if info.has_spread {
      return;
    }
    let Some(updater) = info.first_arg else {
      return;
    };
    let Some(updater_id) = self.indexes.function_id(updater) else {
      return;
    };
    if self.function_is_async_unknown(updater_id) {
      return;
    }
    let Some(await_site) = self.first_straight_await(Some(updater_id)) else {
      return;
    };
    let Some(write) = self.changed_write_after(
      source,
      Some(updater_id),
      await_site.head.offset,
      await_site.head.region,
    ) else {
      return;
    };
    if self.indexes.await_non_await_barrier_between(
      Some(updater_id),
      await_site.head.region,
      await_site.head.offset,
      write.offset,
    ) {
      return;
    }
    if self.stop_before_write(stops, updater_id, await_site.head.region, write.offset) {
      return;
    }
    let offset = write.offset;
    if chosen.as_ref().is_none_or(|(current, _, _, _)| offset < *current) {
      *chosen = Some((offset, write.span, span, await_site.head.span));
    }
  }

  pub(super) fn collect_shared_first_instance_args(
    &mut self,
    node_id: NodeId,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    let Some(api) = info.vueuse else {
      return;
    };
    if !matches!(api, "createSharedComposable" | "createGlobalState") || info.has_spread {
      return;
    }
    let reach = classify_reach(self.semantic, node_id, self.indexes.work_counter());
    if !reach.is_straight() {
      return;
    }
    let Some(factory) = info.first_arg else {
      return;
    };
    if !self.direct_param_ref_factory(factory) {
      return;
    }
    let Some(callable) = self.result_symbol(node_id) else {
      return;
    };
    let root = self.indexes.root_of(callable);
    if self.indexes.reassigned.contains(&root) || self.indexes.escaped.contains(&root) {
      return;
    }
    if self.indexes.vue_imports.contains_key(&root)
      || self.indexes.vueuse_imports.contains_key(&root)
    {
      return;
    }
    let factory_offset = self.span(call.span).offset;
    let calls = self.indexes.identifier_calls_on(root);
    let call_sites: Vec<MemberUse> = calls.iter().map(member_from_call).collect();
    let calls = call_sites.as_slice();
    let Some(first_pos) = calls.iter().position(|site| site.head.offset > factory_offset) else {
      return;
    };
    let Some(first_site) = calls.get(first_pos) else {
      return;
    };
    if !self.indexes.demand_ok(first_site) {
      return;
    }
    let Some(first_info) = self.indexes.call_info(first_site.head.span) else {
      return;
    };
    if first_info.has_spread {
      return;
    }
    let Some(first_arg) = first_info.first_arg else {
      return;
    };
    let Some(first_scalar) = self.scalar_of_span(first_arg) else {
      return;
    };
    if !self.shared_callable_owner_ok(api, first_site.head.callable) {
      return;
    }
    let Some(rest) = calls.get(first_pos..) else {
      return;
    };
    let Some(aliases) = self.shared_result_aliases(rest) else {
      return;
    };
    let Some((later_arg, later_kind, later_origin)) =
      self.first_incompatible_later(calls, first_pos, first_site, first_scalar.kind())
    else {
      return;
    };
    let Some((demand, method)) =
      self.first_incompatible_demand(&aliases, first_scalar.kind(), later_kind, later_origin)
    else {
      return;
    };
    if self.alias_repaired_before(&aliases, first_site.head.offset, demand.head.offset) {
      return;
    }
    self.facts.shared_composable_first_instance_args.push(SharedComposableFirstInstanceArgsFact {
      demand_span: self.span(demand.head.span),
      first_call_span: self.span(first_site.head.span),
      later_arg_span: self.span(later_arg),
      api: api.into(),
      capability: method,
    });
  }

  fn first_incompatible_later(
    &self,
    calls: &[MemberUse],
    first_pos: usize,
    first_site: &MemberUse,
    first_kind: NativeKind,
  ) -> Option<(Span, NativeKind, DemandOrigin)> {
    for later in calls.iter().skip(first_pos.saturating_add(1)) {
      if later.head.callable != first_site.head.callable
        || later.head.region != first_site.head.region
      {
        continue;
      }
      if !self.indexes.demand_ok(later) {
        continue;
      }
      let Some(later_info) = self.indexes.call_info(later.head.span) else {
        continue;
      };
      if later_info.has_spread {
        continue;
      }
      let Some(later_arg) = later_info.first_arg else {
        continue;
      };
      let Some(later_scalar) = self.scalar_of_span(later_arg) else {
        continue;
      };
      if later_scalar.kind() == first_kind {
        continue;
      }
      if self.indexes.has_barrier_between(
        first_site.head.region,
        first_site.head.offset,
        later.head.offset,
      ) {
        continue;
      }
      return Some((
        later_arg,
        later_scalar.kind(),
        DemandOrigin {
          callable: later.head.callable,
          region: later.head.region,
          offset: later.head.offset,
        },
      ));
    }
    None
  }

  fn shared_result_aliases(&self, calls: &[MemberUse]) -> Option<Vec<SymbolId>> {
    let mut aliases = Vec::new();
    for site in calls {
      let Some(result) = self.indexes.result_of_call(site.head.span) else {
        continue;
      };
      let root = self.indexes.root_of(result);
      if self.indexes.payload_uncertain(root) {
        return None;
      }
      aliases.push(root);
    }
    aliases.sort_unstable();
    aliases.dedup();
    Some(aliases)
  }

  fn first_incompatible_demand(
    &self,
    aliases: &[SymbolId],
    first: NativeKind,
    later: NativeKind,
    origin: DemandOrigin,
  ) -> Option<(MemberUse, String)> {
    let mut chosen: Option<(MemberUse, String)> = None;
    for alias in aliases {
      if let Some((demand, method)) =
        self.incompatible_demand(*alias, first, later, origin, origin.offset)
        && chosen.as_ref().is_none_or(|(current, _)| demand.head.offset < current.head.offset)
      {
        chosen = Some((demand, method));
      }
    }
    chosen
  }

  fn alias_repaired_before(&self, aliases: &[SymbolId], start: usize, end: usize) -> bool {
    aliases.iter().any(|alias| self.indexes.has_simple_value_write_between(*alias, start, end))
  }

  fn sync_flush_closed_options(&self, options: Option<Span>) -> bool {
    let Some(options) = options else {
      return false;
    };
    if self.indexes.object_has_spread(options) {
      return false;
    }
    if self.indexes.has_closed_keys(options) {
      if !self.indexes.closed_object_only_keys(options, IGNORE_OPTION_KEYS) {
        return false;
      }
    } else if self.indexes.object_prop(options, "eventFilter").is_some() {
      return false;
    }
    if self.unproven_flag(options, "once") || self.unproven_flag(options, "immediate") {
      return false;
    }
    match self.indexes.object_prop(options, "flush") {
      Some(ObjectProp::Value(span)) => self.indexes.scalar(span) == Some(Scalar::Str(SYNC_FLUSH)),
      _ => false,
    }
  }

  fn watcher_consumed_once_immediate(&self, options: Option<Span>) -> bool {
    let Some(options) = options else {
      return false;
    };
    self.bool_prop(options, "once") == Some(true)
      && self.bool_prop(options, "immediate") == Some(true)
  }

  fn unproven_flag(&self, options: Span, key: &str) -> bool {
    if self.indexes.closed_object_has_key(options, key) != Some(true) {
      return false;
    }
    !matches!(
      self.indexes.option_value(options, key),
      super::shape::OptionValue::Known(super::shape::Literal::Bool(_))
    )
  }

  fn bool_prop(&self, options: Span, key: &str) -> Option<bool> {
    match self.indexes.option_value(options, key) {
      super::shape::OptionValue::Absent => Some(false),
      super::shape::OptionValue::Known(super::shape::Literal::Bool(value)) => Some(value),
      super::shape::OptionValue::Known(_) | super::shape::OptionValue::Unknown => None,
    }
  }

  fn live_primitive_ref(&mut self, span: Span) -> Option<SymbolId> {
    let hint = self.indexes.hints.get(&span_key(span)).copied()?;
    let ShapeHint::Identifier(Some(symbol_id), false) = hint else {
      return None;
    };
    let root = self.indexes.root_of(symbol_id);
    if self.indexes.reassigned.contains(&root) || self.indexes.unknown_member_touch.contains(&root)
    {
      return None;
    }
    if self.classify_symbol(root, 8) != Shape::RefLike {
      return None;
    }
    let init = self.indexes.init_span.get(&root).copied()?;
    let ShapeHint::Call(call_span) = self.indexes.hints.get(&span_key(init)).copied()? else {
      return None;
    };
    let info = self.indexes.calls.get(&span_key(call_span)).copied()?;
    if info.has_spread
      || !info.api.is_some_and(is_ref_api)
      || !matches!(info.api, Some("ref" | "shallowRef"))
    {
      return None;
    }
    let payload = info.first_arg.and_then(|argument| self.indexes.scalar(argument))?;
    if !matches!(payload.kind(), NativeKind::Number | NativeKind::String | NativeKind::Boolean) {
      return None;
    }
    Some(root)
  }

  fn inline_callback_consumes_value(&self, span: Span) -> bool {
    let Some(node_id) = self.indexes.function_id(span) else {
      return false;
    };
    let params = match self.semantic.nodes().kind(node_id) {
      AstKind::ArrowFunctionExpression(arrow) => &arrow.params,
      AstKind::Function(function) => &function.params,
      _ => return false,
    };
    first_param_is_read(self.semantic, params, self.indexes.work_counter())
  }

  fn function_is_async_unknown(&self, node_id: NodeId) -> bool {
    match self.semantic.nodes().kind(node_id) {
      AstKind::ArrowFunctionExpression(arrow) => !arrow.r#async,
      AstKind::Function(function) => !function.r#async || function.generator,
      _ => true,
    }
  }

  fn first_straight_await(&self, callable: Option<NodeId>) -> Option<AwaitPositionSite> {
    self.indexes.straight_awaits_in(callable).min_by_key(|site| site.head.offset)
  }

  fn unordered_source_write(&self, source: SymbolId, updater: Option<NodeId>) -> bool {
    self
      .indexes
      .value_write_events(source)
      .iter()
      .any(|write| write.callable != updater && !self.callable_is_async(write.callable))
  }

  fn callable_is_async(&self, callable: Option<NodeId>) -> bool {
    let Some(node_id) = callable else {
      return false;
    };
    match self.semantic.nodes().kind(node_id) {
      AstKind::ArrowFunctionExpression(arrow) => arrow.r#async,
      AstKind::Function(function) => function.r#async && !function.generator,
      _ => false,
    }
  }

  fn changed_write_after(
    &self,
    source: SymbolId,
    callable: Option<NodeId>,
    after: usize,
    region: NodeId,
  ) -> Option<ValueWrite> {
    if self.indexes.reassigned.contains(&source)
      || self.indexes.unknown_member_touch.contains(&source)
    {
      return None;
    }
    if self.unordered_source_write(source, callable) {
      return None;
    }
    let writes = self.indexes.value_writes_for(source, callable)?;
    timeline::after(self.indexes.work_counter(), writes, after)
      .iter()
      .find(|write| {
        if !write.simple_assign {
          return false;
        }
        if self.indexes.await_non_await_barrier_between(callable, region, after, write.offset) {
          return false;
        }
        let previous = match self.indexes.last_value_write_in(source, callable, write.offset) {
          Some(prior) if prior.simple_assign => self.indexes.scalar(prior.rhs),
          Some(_) => None,
          None => self.vueuse_ref_init_scalar(source),
        };
        let Some(previous) = previous else {
          return false;
        };
        let Some(next) = self.indexes.scalar(write.rhs) else {
          return false;
        };
        previous != next
      })
      .copied()
  }

  fn vueuse_ref_init_scalar(&self, root: SymbolId) -> Option<Scalar> {
    self.indexes.ref_init_scalar(root, super::index::RefInitLookup::Direct)
  }

  fn watcher_stopped(
    &self,
    node_id: NodeId,
    call: &CallExpression<'_>,
    parent: Option<NodeId>,
    region: NodeId,
  ) -> bool {
    for (symbol, key) in self.indexes.call_bindings(call.span) {
      if key != "stop" {
        continue;
      }
      if self.indexes.identifier_calls_on(*symbol).iter().any(|site| {
        let site = member_from_call(site);
        site.head.callable == parent && site.head.region == region && self.indexes.demand_ok(&site)
      }) {
        return true;
      }
    }
    if let Some(bag) = self.result_symbol(node_id) {
      let root = self.indexes.root_of(bag);
      if self.indexes.last_stop_before(root, parent, region, usize::MAX).is_some() {
        return true;
      }
    }
    false
  }

  fn stop_handles(&self, node_id: NodeId, call: &CallExpression<'_>) -> StopHandles {
    let mut idents = Vec::new();
    for (symbol, key) in self.indexes.call_bindings(call.span) {
      if key == "stop" {
        idents.push(*symbol);
      }
    }
    StopHandles { idents, bag: self.result_symbol(node_id).map(|bag| self.indexes.root_of(bag)) }
  }

  fn stop_before_write(
    &self,
    stops: &StopHandles,
    updater: NodeId,
    region: NodeId,
    before: usize,
  ) -> bool {
    if stops.idents.iter().any(|symbol| {
      self.indexes.identifier_calls_on(*symbol).iter().any(|site| {
        let site = member_from_call(site);
        site.head.callable == Some(updater)
          && site.head.offset < before
          && self.indexes.demand_ok(&site)
          && !self.indexes.await_non_await_barrier_between(
            Some(updater),
            region,
            site.head.offset,
            before,
          )
      })
    }) {
      return true;
    }
    stops.bag.is_some_and(|bag| {
      self.indexes.last_stop_before(bag, Some(updater), region, before).is_some()
    })
  }

  fn ignore_update_roots(&self, node_id: NodeId, call: &CallExpression<'_>) -> Vec<SymbolId> {
    let mut roots = Vec::new();
    for (symbol, key) in self.indexes.call_bindings(call.span) {
      if key == "ignoreUpdates" {
        roots.push(self.indexes.root_of(*symbol));
      }
    }
    if let Some(bag) = self.result_symbol(node_id) {
      roots.push(self.indexes.root_of(bag));
    }
    roots.sort_unstable();
    roots.dedup();
    roots
  }

  fn direct_param_ref_factory(&self, span: Span) -> bool {
    let Some(node_id) = self.indexes.function_id(span) else {
      return false;
    };
    match self.semantic.nodes().kind(node_id) {
      AstKind::ArrowFunctionExpression(arrow) => {
        if arrow.r#async {
          return false;
        }
        let Some(param) = single_binding_param(&arrow.params) else {
          return false;
        };
        returns_ref_of_param(
          &arrow.body,
          arrow.expression,
          param,
          |ident| self.indexes.calls.get(&span_key(ident)).copied().and_then(|info| info.api),
          self.semantic,
          &self.indexes,
        )
      }
      AstKind::Function(function) => {
        if function.r#async || function.generator {
          return false;
        }
        let Some(body) = function.body.as_ref() else {
          return false;
        };
        let Some(param) = single_binding_param(&function.params) else {
          return false;
        };
        returns_ref_of_param(
          body,
          false,
          param,
          |ident| self.indexes.calls.get(&span_key(ident)).copied().and_then(|info| info.api),
          self.semantic,
          &self.indexes,
        )
      }
      _ => false,
    }
  }

  fn shared_callable_owner_ok(&self, api: &str, callable: Option<NodeId>) -> bool {
    if api == "createGlobalState" {
      return true;
    }
    if self.indexes.setup_lane() && callable.is_none() {
      return true;
    }
    let Some(callable) = callable else {
      return false;
    };
    self.callable_is_effect_scope_run(callable)
  }

  fn callable_is_effect_scope_run(&self, callable: NodeId) -> bool {
    let mut current = callable;
    for _ in 0..8 {
      let parent = self.semantic.nodes().parent_id(current);
      match self.semantic.nodes().kind(parent) {
        wrapper if is_ts_wrapper(wrapper) || matches!(wrapper, AstKind::ExpressionStatement(_)) => {
          current = parent;
        }
        AstKind::CallExpression(call) => {
          let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression()
          else {
            return false;
          };
          if member.property.name.as_str() != "run" {
            return false;
          }
          let Some(object) = member.object.get_inner_expression().get_identifier_reference() else {
            return false;
          };
          let Some(symbol_id) = self.reference_symbol(object) else {
            return false;
          };
          return self.symbol_is_effect_scope(self.indexes.root_of(symbol_id));
        }
        _ => return false,
      }
    }
    false
  }

  fn incompatible_demand(
    &self,
    root: SymbolId,
    first: NativeKind,
    later: NativeKind,
    origin: DemandOrigin,
    after: usize,
  ) -> Option<(MemberUse, String)> {
    let mut chosen: Option<(MemberUse, String)> = None;
    for demand in self.indexes.value_demands_on(root) {
      if demand.site.head.offset <= after || !self.indexes.demand_from(&demand.site, origin) {
        continue;
      }
      if native_kind_has_method(first, &demand.member)
        || !native_kind_has_method(later, &demand.member)
      {
        continue;
      }
      if chosen.as_ref().is_none_or(|(current, _)| demand.site.head.offset < current.head.offset) {
        chosen = Some((demand.site, self.indexes.copy_key(&demand.member)));
      }
    }
    chosen
  }

  fn scalar_of_span(&self, span: Span) -> Option<Scalar> {
    if let Some(scalar) = self.indexes.scalar(span) {
      return Some(scalar);
    }
    let hint = self.indexes.hints.get(&span_key(span)).copied()?;
    match hint {
      ShapeHint::Identifier(Some(symbol_id), false) => {
        let root = self.indexes.root_of(symbol_id);
        if self.indexes.reassigned.contains(&root) {
          return None;
        }
        let init = self.indexes.init_span.get(&root).copied()?;
        self.indexes.scalar(init)
      }
      _ => None,
    }
  }
}

const fn member_from_call(site: &CallUse) -> MemberUse {
  MemberUse {
    head: Site {
      offset: site.head.offset,
      span: site.head.span,
      callable: site.head.callable,
      region: site.head.region,
      reach: site.head.reach,
    },
    optional: site.optional,
    call_optional: false,
    role: DemandRole::Other,
  }
}

fn first_param_is_read(
  semantic: &oxc_semantic::Semantic<'_>,
  params: &FormalParameters<'_>,
  work: &super::stats::WorkCounter,
) -> bool {
  if params.rest.is_some() || params.items.len() != 1 {
    return false;
  }
  let Some(item) = params.items.first() else {
    return false;
  };
  let BindingPattern::BindingIdentifier(binding) = &item.pattern else {
    return false;
  };
  let Some(symbol_id) = binding.symbol_id.get() else {
    return false;
  };
  semantic.symbol_references(symbol_id).any(|reference| {
    work.add_references(1);
    reference.flags().is_read()
  })
}

fn single_binding_param(params: &FormalParameters<'_>) -> Option<SymbolId> {
  if params.rest.is_some() || params.items.len() != 1 {
    return None;
  }
  let item = params.items.first()?;
  let BindingPattern::BindingIdentifier(binding) = &item.pattern else {
    return None;
  };
  binding.symbol_id.get()
}

fn returns_ref_of_param(
  body: &FunctionBody<'_>,
  expression_arrow: bool,
  param: SymbolId,
  api_of: impl Fn(Span) -> Option<&'static str>,
  semantic: &oxc_semantic::Semantic<'_>,
  indexes: &super::index::Indexes,
) -> bool {
  let call = if expression_arrow {
    let Some(Statement::ExpressionStatement(statement)) = body.statements.first() else {
      return false;
    };
    if body.statements.len() != 1 {
      return false;
    }
    as_ref_call(&statement.expression)
  } else {
    let mut returned = None;
    for statement in &body.statements {
      match statement {
        Statement::ReturnStatement(ret) => {
          if returned.is_some() {
            return false;
          }
          returned = ret.argument.as_ref().and_then(as_ref_call);
        }
        Statement::VariableDeclaration(_) | Statement::FunctionDeclaration(_) => {}
        _ => return false,
      }
    }
    returned
  };
  let Some(call) = call else {
    return false;
  };
  if call.arguments.iter().any(Argument::is_spread) || call.arguments.len() != 1 {
    return false;
  }
  let Some(api) = api_of(call.span) else {
    return false;
  };
  if !matches!(api, "ref" | "shallowRef") {
    return false;
  }
  let Some(arg) = call.arguments.first().and_then(Argument::as_expression) else {
    return false;
  };
  let Some(ident) = arg.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  let Some(symbol_id) =
    ident.reference_id.get().and_then(|id| semantic.scoping().get_reference(id).symbol_id())
  else {
    return false;
  };
  indexes.root_of(symbol_id) == param
}

fn as_ref_call<'a>(expression: &'a Expression<'a>) -> Option<&'a CallExpression<'a>> {
  match expression.get_inner_expression() {
    Expression::CallExpression(call) => Some(call),
    _ => None,
  }
}
