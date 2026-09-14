//! Root/owner keyed queries over the lifetime index for nested-watch and
//! detached-scope ownership. Built from the single semantic walk; emit does
//! not rescan nodes per watcher.

use std::collections::{HashMap, HashSet};

use oxc_ast::{
  AstKind,
  ast::{Expression, Statement},
};
use oxc_semantic::{NodeId, SymbolId};
use oxc_span::{GetSpan, Span};
use vue_vet_core::{
  DetachedEffectScopeWithoutStopFact, NestedWatchWithoutCleanupFact, ReactivityLifetimeFacts,
  SourceSpan, WatcherApiKind,
};

use super::index::{DetachedScopeSite, LifetimeIndex, ProvenRun, ScopeToggle, WatcherSite};
use super::resolve::{enclosing_function, referenced_symbol, uncertain_to_function};
use crate::facts::source_span;

pub(super) fn emit_ownership(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
  orphaned_watchers: &HashSet<NodeId>,
  facts: &mut ReactivityLifetimeFacts,
) -> usize {
  let mut enclosing = index.enclosing.clone();
  let owners = OwnerIndex::build(semantic, index, &mut enclosing);
  let mut ctx = EmitCtx {
    semantic,
    index,
    owners: &owners,
    enclosing: &mut enclosing,
    line_index,
    sfc_source,
    script_offset,
    facts,
    outer_cache: HashMap::new(),
    source_cache: HashMap::new(),
    track_cache: HashMap::new(),
    result_cache: HashMap::new(),
  };
  let mut work = owners.work;
  let suppressed_runs = emit_detached_scopes(&mut ctx);
  work = work.saturating_add(index.detached_scopes.len());
  work.saturating_add(emit_nested_watches(&mut ctx, &suppressed_runs, orphaned_watchers))
}

struct EmitCtx<'a, 'b, 'ast> {
  semantic: &'a oxc_semantic::Semantic<'ast>,
  index: &'a LifetimeIndex,
  owners: &'a OwnerIndex,
  enclosing: &'b mut HashMap<NodeId, Option<NodeId>>,
  line_index: &'a vue_vet_core::LineIndex,
  sfc_source: &'a str,
  script_offset: usize,
  facts: &'a mut ReactivityLifetimeFacts,
  outer_cache: HashMap<NodeId, Option<WatcherSite>>,
  source_cache: HashMap<(NodeId, NodeId), Option<(SymbolId, Span)>>,
  track_cache: HashMap<NodeId, bool>,
  result_cache: HashMap<NodeId, ClassifyState>,
}

impl EmitCtx<'_, '_, '_> {
  fn span_of(&self, span: Span) -> SourceSpan {
    source_span(self.line_index, self.sfc_source, self.script_offset, span)
  }

  fn watcher(&self, watcher_index: usize) -> Option<WatcherSite> {
    self.index.work.add_watchers(1);
    self.index.watchers.get(watcher_index).copied()
  }
}

struct OwnerIndex {
  watchers_in_fn: HashMap<NodeId, Vec<usize>>,
  outers_by_callback: HashMap<NodeId, Vec<usize>>,
  run_callback_set: HashSet<NodeId>,
  runs_by_scope: HashMap<SymbolId, Vec<ProvenRun>>,
  scope_active: HashMap<NodeId, Vec<(u32, bool)>>,
  work: usize,
}

impl OwnerIndex {
  fn build(
    semantic: &oxc_semantic::Semantic<'_>,
    index: &LifetimeIndex,
    enclosing: &mut HashMap<NodeId, Option<NodeId>>,
  ) -> Self {
    let mut watchers_in_fn = HashMap::<NodeId, Vec<usize>>::new();
    let mut outers_by_callback = HashMap::<NodeId, Vec<usize>>::new();
    let mut work = 0usize;
    for (watcher_index, watcher) in index.watchers.iter().enumerate() {
      work = work.saturating_add(1);
      index.work.add_watchers(1);
      if let Some(function_id) = enclosing_function(semantic, watcher.node_id, enclosing) {
        watchers_in_fn.entry(function_id).or_default().push(watcher_index);
      }
      if let Some(callback) = watcher.callback {
        outers_by_callback.entry(callback.node_id).or_default().push(watcher_index);
      }
    }
    let mut runs_by_scope = HashMap::<SymbolId, Vec<ProvenRun>>::new();
    let mut run_callback_set = HashSet::new();
    for runs in index.run_callbacks.values() {
      work = work.saturating_add(runs.len());
      index.work.add_watchers(runs.len());
      for run in runs {
        run_callback_set.insert(run.callback.node_id);
        if let Some(symbol) = run.scope_symbol {
          runs_by_scope.entry(symbol).or_default().push(*run);
        }
      }
    }
    let mut toggles_by_fn = HashMap::<NodeId, Vec<ScopeToggle>>::new();
    for toggle in &index.scope_toggles {
      work = work.saturating_add(1);
      index.work.add_toggles(1);
      toggles_by_fn.entry(toggle.function_id).or_default().push(*toggle);
    }
    let mut scope_active = HashMap::<NodeId, Vec<(u32, bool)>>::new();
    for (function_id, mut toggles) in toggles_by_fn {
      toggles.sort_by_key(|toggle| toggle.span.start);
      let mut active = HashSet::new();
      let mut events = Vec::with_capacity(toggles.len());
      for toggle in toggles {
        index.work.add_toggles(1);
        if !proven_reachable(index, semantic, toggle.node_id, function_id) {
          continue;
        }
        if toggle.on {
          active.insert(toggle.symbol);
        } else {
          active.remove(&toggle.symbol);
        }
        events.push((toggle.span.start, !active.is_empty()));
      }
      scope_active.insert(function_id, events);
    }
    Self { watchers_in_fn, outers_by_callback, run_callback_set, runs_by_scope, scope_active, work }
  }
}

#[derive(Clone, Copy)]
struct DetachedLeak {
  run: ProvenRun,
  watcher: WatcherSite,
  source_span: Span,
  outer: WatcherSite,
}

fn emit_detached_scopes(ctx: &mut EmitCtx<'_, '_, '_>) -> HashSet<NodeId> {
  let mut suppressed = HashSet::new();
  let mut emitted = HashSet::new();
  let site_count = ctx.index.detached_scopes.len();
  for site_index in 0..site_count {
    let Some(site) = ctx.index.detached_scopes.get(site_index).copied() else {
      continue;
    };
    if !emitted.insert(site.symbol) {
      continue;
    }
    let Some(leak) = detached_leak(ctx, &site) else {
      continue;
    };
    suppressed.insert(leak.run.callback.node_id);
    ctx.facts.detached_effect_scopes_without_stop.push(DetachedEffectScopeWithoutStopFact {
      scope_span: ctx.span_of(site.span),
      run_span: ctx.span_of(leak.run.run_span),
      watcher_span: ctx.span_of(leak.watcher.span),
      source_span: ctx.span_of(leak.source_span),
      outer_span: ctx.span_of(leak.outer.span),
    });
  }
  suppressed
}

fn detached_leak(ctx: &mut EmitCtx<'_, '_, '_>, site: &DetachedScopeSite) -> Option<DetachedLeak> {
  if ctx.index.unproven_scopes.contains(&site.symbol)
    || ctx.index.stopped_scopes.contains(&site.symbol)
  {
    return None;
  }
  let factory = enclosing_function(
    ctx.semantic,
    ctx.semantic.scoping().symbol_declaration(site.symbol),
    ctx.enclosing,
  )?;
  let outer = repeatable_outer(ctx, factory)?;
  if !scope_ownership_local(ctx, site.symbol, factory) {
    return None;
  }
  let runs = ctx.owners.runs_by_scope.get(&site.symbol)?;
  ctx.index.work.add_watchers(runs.len());
  let mut chosen: Option<DetachedLeak> = None;
  for run in runs {
    if run.callback.is_async || !proven_reachable(ctx.index, ctx.semantic, run.node_id, factory) {
      continue;
    }
    if let Some((watcher, source_span)) = unused_external_watcher(ctx, factory, run)
      && chosen.is_none_or(|previous| run.run_span.start < previous.run.run_span.start)
    {
      chosen = Some(DetachedLeak { run: *run, watcher, source_span, outer });
    }
  }
  chosen
}

fn scope_ownership_local(ctx: &EmitCtx<'_, '_, '_>, symbol: SymbolId, factory: NodeId) -> bool {
  let Some(uses) = ctx.index.scope_use_fns.get(&symbol) else {
    return true;
  };
  ctx.index.work.add_references(uses.len());
  uses
    .iter()
    .all(|function_id| *function_id == factory || ctx.owners.run_callback_set.contains(function_id))
}

fn unused_external_watcher(
  ctx: &mut EmitCtx<'_, '_, '_>,
  factory: NodeId,
  run: &ProvenRun,
) -> Option<(WatcherSite, Span)> {
  let watchers = ctx.owners.watchers_in_fn.get(&run.callback.node_id)?;
  ctx.index.work.add_watchers(watchers.len());
  let mut chosen: Option<(WatcherSite, Span)> = None;
  for watcher_index in watchers {
    let Some(watcher) = ctx.watcher(*watcher_index) else {
      continue;
    };
    if !inner_subscription_live(ctx.index, ctx.semantic, &watcher, run.callback.node_id) {
      continue;
    }
    let Some((_, source_span)) = proven_external_source(ctx, &watcher, factory) else {
      continue;
    };
    if chosen.is_none_or(|(previous, _)| watcher.span.start < previous.span.start) {
      chosen = Some((watcher, source_span));
    }
  }
  chosen
}

fn emit_nested_watches(
  ctx: &mut EmitCtx<'_, '_, '_>,
  suppressed_runs: &HashSet<NodeId>,
  orphaned_watchers: &HashSet<NodeId>,
) -> usize {
  let mut work = 0usize;
  let watcher_count = ctx.index.watchers.len();
  for watcher_index in 0..watcher_count {
    work = work.saturating_add(1);
    let Some(watcher) = ctx.watcher(watcher_index) else {
      continue;
    };
    if !watcher.unused || orphaned_watchers.contains(&watcher.node_id) {
      continue;
    }
    let Some(function_id) = enclosing_function(ctx.semantic, watcher.node_id, ctx.enclosing) else {
      continue;
    };
    if suppressed_runs.contains(&function_id) {
      continue;
    }
    if !inner_subscription_live(ctx.index, ctx.semantic, &watcher, function_id) {
      continue;
    }
    if inner_owned_by_active_scope(ctx, &watcher, function_id) {
      continue;
    }
    let Some(outer) = repeatable_outer(ctx, function_id) else {
      continue;
    };
    let Some((_, source_span)) = proven_external_source(ctx, &watcher, function_id) else {
      continue;
    };
    let Some(callback) = outer.callback else {
      continue;
    };
    ctx.facts.nested_watch_without_cleanups.push(NestedWatchWithoutCleanupFact {
      api: watcher.api,
      inner_span: ctx.span_of(watcher.span),
      outer_span: ctx.span_of(outer.span),
      outer_callback_span: ctx.span_of(callback.span),
      source_span: ctx.span_of(source_span),
    });
  }
  work
}

fn inner_subscription_live(
  index: &LifetimeIndex,
  semantic: &oxc_semantic::Semantic<'_>,
  watcher: &WatcherSite,
  function_id: NodeId,
) -> bool {
  watcher.unused
    && !watcher.options.unknown
    && !watcher.options.is_exhausted_subscription(watcher.api)
    && proven_reachable(index, semantic, watcher.node_id, function_id)
}

fn inner_owned_by_active_scope(
  ctx: &EmitCtx<'_, '_, '_>,
  watcher: &WatcherSite,
  function_id: NodeId,
) -> bool {
  let Some(events) = ctx.owners.scope_active.get(&function_id) else {
    return false;
  };
  ctx.index.work.add_toggles(1);
  let index = ctx.index.work.partition_point(events, |(start, _)| *start < watcher.span.start);
  index
    .checked_sub(1)
    .and_then(|index| events.get(index).map(|(_, active)| *active))
    .unwrap_or(false)
}

fn repeatable_outer(ctx: &mut EmitCtx<'_, '_, '_>, callback: NodeId) -> Option<WatcherSite> {
  if let Some(&cached) = ctx.outer_cache.get(&callback) {
    ctx.index.work.add_watchers(1);
    return cached;
  }
  let mut chosen: Option<WatcherSite> = None;
  if let Some(outers) = ctx.owners.outers_by_callback.get(&callback) {
    ctx.index.work.add_watchers(outers.len());
    for watcher_index in outers {
      let Some(watcher) = ctx.index.watchers.get(*watcher_index).copied() else {
        continue;
      };
      if !outer_is_repeatable(ctx, &watcher) {
        continue;
      }
      if chosen.is_none_or(|previous| watcher.span.start < previous.span.start) {
        chosen = Some(watcher);
      }
    }
  }
  ctx.outer_cache.insert(callback, chosen);
  chosen
}

fn outer_is_repeatable(ctx: &mut EmitCtx<'_, '_, '_>, watcher: &WatcherSite) -> bool {
  watcher.options.is_repeatable(watcher.api)
    && !ctx.index.stopped_watchers.contains(&watcher.node_id)
    && outer_source_can_change(ctx, watcher)
}

fn outer_source_can_change(ctx: &mut EmitCtx<'_, '_, '_>, watcher: &WatcherSite) -> bool {
  if let Some(&cached) = ctx.track_cache.get(&watcher.node_id) {
    ctx.index.work.add_references(1);
    return cached;
  }
  let can_change = match watcher.api {
    WatcherApiKind::Watch => watch_source_can_change(ctx, watcher),
    WatcherApiKind::WatchEffect
    | WatcherApiKind::WatchPostEffect
    | WatcherApiKind::WatchSyncEffect => {
      let Some(function_id) = watcher.callback.map(|callback| callback.node_id) else {
        ctx.track_cache.insert(watcher.node_id, false);
        return false;
      };
      function_has_live_read(ctx, function_id)
    }
  };
  ctx.track_cache.insert(watcher.node_id, can_change);
  can_change
}

fn watch_source_can_change(ctx: &mut EmitCtx<'_, '_, '_>, watcher: &WatcherSite) -> bool {
  if let Some(symbol) = watcher.source_symbol {
    ctx.index.work.add_references(1);
    let Some(binding) = ctx.index.reactive_bindings.get(&symbol).copied() else {
      return false;
    };
    return match binding.kind {
      super::index::ReactiveKind::Ref | super::index::ReactiveKind::Proxy => true,
      super::index::ReactiveKind::Computed => binding
        .value_getter
        .is_some_and(|getter| classify_source_result(ctx, getter) == SourceResult::Changing),
    };
  }
  let Some(getter) = watcher.source_getter else {
    return false;
  };
  classify_source_result(ctx, getter) == SourceResult::Changing
}

fn function_has_live_read(ctx: &EmitCtx<'_, '_, '_>, function_id: NodeId) -> bool {
  let ops = ctx.index.tracked_ops_by_fn.get(&function_id).map_or(&[][..], Vec::as_slice);
  ctx.index.work.add_references(ops.len());
  ops.iter().any(|op| op.kind.subscribes())
}

fn proven_external_source(
  ctx: &mut EmitCtx<'_, '_, '_>,
  watcher: &WatcherSite,
  outer_fn: NodeId,
) -> Option<(SymbolId, Span)> {
  if watcher.options.unknown {
    return None;
  }
  if let (Some(symbol), Some(span)) = (watcher.source_symbol, watcher.source_span) {
    ctx.index.work.add_references(1);
    if ctx.index.reactive_bindings.contains_key(&symbol)
      && symbol_declared_outside(ctx.semantic, ctx.enclosing, symbol, outer_fn)
    {
      return Some((symbol, span));
    }
    return None;
  }
  let getter = match watcher.source_getter {
    Some(getter) => getter,
    None if watcher.api != WatcherApiKind::Watch => watcher.callback?.node_id,
    None => return None,
  };
  if let Some(&cached) = ctx.source_cache.get(&(getter, outer_fn)) {
    ctx.index.work.add_references(1);
    return cached;
  }
  let ops = ctx.index.tracked_ops_by_fn.get(&getter).map_or(&[][..], Vec::as_slice);
  ctx.index.work.add_references(ops.len());
  let mut chosen: Option<(SymbolId, Span)> = None;
  for op in ops {
    if !op.kind.subscribes() {
      continue;
    }
    if !symbol_declared_outside(ctx.semantic, ctx.enclosing, op.symbol, outer_fn) {
      continue;
    }
    if chosen.is_none_or(|(_, previous)| op.span.start < previous.start) {
      chosen = Some((op.symbol, op.span));
    }
  }
  ctx.source_cache.insert((getter, outer_fn), chosen);
  chosen
}

fn proven_reachable(
  index: &LifetimeIndex,
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  function_id: NodeId,
) -> bool {
  !uncertain_to_function(semantic, node_id, function_id)
    && !index.preceding_exit(semantic, node_id, function_id)
}

fn symbol_declared_outside(
  semantic: &oxc_semantic::Semantic<'_>,
  enclosing: &mut HashMap<NodeId, Option<NodeId>>,
  symbol: SymbolId,
  function_id: NodeId,
) -> bool {
  let declaration_id = semantic.scoping().symbol_declaration(symbol);
  if declaration_id == function_id {
    return false;
  }
  if semantic.nodes().ancestor_ids(declaration_id).any(|ancestor| ancestor == function_id) {
    return false;
  }
  enclosing_function(semantic, declaration_id, enclosing) != Some(function_id)
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SourceResult {
  Stable,
  Changing,
  Unknown,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ClassifyState {
  Visiting,
  Done(SourceResult),
}

#[derive(Clone, Copy)]
enum Scan {
  Returned(SourceResult),
  Fallthrough,
  Unknown,
}

const MAX_COMPUTED_RESULT_DEPTH: u32 = 64;

fn classify_source_result(ctx: &mut EmitCtx<'_, '_, '_>, function_id: NodeId) -> SourceResult {
  classify_function_result(ctx.semantic, ctx.index, &mut ctx.result_cache, function_id, 0)
}

fn classify_function_result(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  cache: &mut HashMap<NodeId, ClassifyState>,
  function_id: NodeId,
  depth: u32,
) -> SourceResult {
  if let Some(&state) = cache.get(&function_id) {
    index.work.add_references(1);
    return match state {
      ClassifyState::Visiting => SourceResult::Unknown,
      ClassifyState::Done(result) => result,
    };
  }
  if depth >= MAX_COMPUTED_RESULT_DEPTH {
    return SourceResult::Unknown;
  }
  cache.insert(function_id, ClassifyState::Visiting);
  let result = match semantic.nodes().kind(function_id) {
    AstKind::ArrowFunctionExpression(arrow) if arrow.expression => {
      let Some(statement) = arrow.body.statements.first() else {
        cache.insert(function_id, ClassifyState::Done(SourceResult::Unknown));
        return SourceResult::Unknown;
      };
      match statement {
        Statement::ReturnStatement(ret) => {
          classify_return_arg(semantic, index, cache, ret.argument.as_ref(), function_id, depth)
        }
        Statement::ExpressionStatement(expression) => {
          classify_expr(semantic, index, cache, &expression.expression, function_id, depth)
        }
        _ => SourceResult::Unknown,
      }
    }
    AstKind::ArrowFunctionExpression(arrow) => {
      match scan_statements(semantic, index, cache, &arrow.body.statements, function_id, depth) {
        Scan::Returned(result) => result,
        Scan::Fallthrough => SourceResult::Stable,
        Scan::Unknown => SourceResult::Unknown,
      }
    }
    AstKind::Function(function) => {
      let Some(body) = &function.body else {
        cache.insert(function_id, ClassifyState::Done(SourceResult::Unknown));
        return SourceResult::Unknown;
      };
      match scan_statements(semantic, index, cache, &body.statements, function_id, depth) {
        Scan::Returned(result) => result,
        Scan::Fallthrough => SourceResult::Stable,
        Scan::Unknown => SourceResult::Unknown,
      }
    }
    _ => SourceResult::Unknown,
  };
  cache.insert(function_id, ClassifyState::Done(result));
  result
}

fn scan_statements(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  cache: &mut HashMap<NodeId, ClassifyState>,
  statements: &[Statement<'_>],
  function_id: NodeId,
  depth: u32,
) -> Scan {
  index.work.add_statements(statements.len());
  for statement in statements {
    match statement {
      Statement::ReturnStatement(ret) => {
        return Scan::Returned(classify_return_arg(
          semantic,
          index,
          cache,
          ret.argument.as_ref(),
          function_id,
          depth,
        ));
      }
      Statement::ThrowStatement(_)
      | Statement::IfStatement(_)
      | Statement::ForStatement(_)
      | Statement::ForInStatement(_)
      | Statement::ForOfStatement(_)
      | Statement::WhileStatement(_)
      | Statement::DoWhileStatement(_)
      | Statement::SwitchStatement(_)
      | Statement::TryStatement(_)
      | Statement::WithStatement(_)
      | Statement::LabeledStatement(_) => return Scan::Unknown,
      Statement::BlockStatement(block) => {
        match scan_statements(semantic, index, cache, &block.body, function_id, depth) {
          Scan::Fallthrough => {}
          other => return other,
        }
      }
      _ => {}
    }
  }
  Scan::Fallthrough
}

fn classify_return_arg(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  cache: &mut HashMap<NodeId, ClassifyState>,
  argument: Option<&Expression<'_>>,
  function_id: NodeId,
  depth: u32,
) -> SourceResult {
  argument.map_or(SourceResult::Stable, |expression| {
    classify_expr(semantic, index, cache, expression, function_id, depth)
  })
}

fn classify_expr(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  cache: &mut HashMap<NodeId, ClassifyState>,
  expression: &Expression<'_>,
  function_id: NodeId,
  depth: u32,
) -> SourceResult {
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(_)
    | Expression::NullLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::BigIntLiteral(_) => SourceResult::Stable,
    Expression::Identifier(identifier) if identifier.name == "undefined" => SourceResult::Stable,
    Expression::UnaryExpression(unary)
      if matches!(unary.operator, oxc_ast::ast::UnaryOperator::Void) =>
    {
      SourceResult::Stable
    }
    Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
      SourceResult::Stable
    }
    Expression::StaticMemberExpression(member) => classify_member_result(
      semantic,
      index,
      cache,
      &member.object,
      member.property.name.as_str(),
      function_id,
      depth,
    ),
    Expression::ComputedMemberExpression(member) => {
      let property = match member.expression.get_inner_expression() {
        Expression::StringLiteral(literal) => literal.value.as_str(),
        _ => {
          return if has_subscribing_op(
            index,
            function_id,
            member.object.get_inner_expression().span(),
          ) {
            SourceResult::Changing
          } else {
            SourceResult::Unknown
          };
        }
      };
      classify_member_result(semantic, index, cache, &member.object, property, function_id, depth)
    }
    _ => SourceResult::Unknown,
  }
}

fn classify_member_result(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  cache: &mut HashMap<NodeId, ClassifyState>,
  object: &Expression<'_>,
  property: &str,
  function_id: NodeId,
  depth: u32,
) -> SourceResult {
  let subscribed = has_subscribing_op(index, function_id, object.get_inner_expression().span());
  if property == "value"
    && let Some(identifier) = object.get_inner_expression().get_identifier_reference()
    && let Some(symbol) = referenced_symbol(semantic, identifier)
    && let Some(binding) = index.reactive_bindings.get(&symbol)
  {
    if !subscribed {
      return SourceResult::Unknown;
    }
    return match binding.kind {
      super::index::ReactiveKind::Computed => {
        let Some(getter) = binding.value_getter else {
          return SourceResult::Unknown;
        };
        index.work.add_computed_edges(1);
        classify_function_result(semantic, index, cache, getter, depth.saturating_add(1))
      }
      super::index::ReactiveKind::Ref | super::index::ReactiveKind::Proxy => SourceResult::Changing,
    };
  }
  if subscribed { SourceResult::Changing } else { SourceResult::Unknown }
}

fn has_subscribing_op(index: &LifetimeIndex, function_id: NodeId, span: Span) -> bool {
  let ops = index.tracked_ops_by_fn.get(&function_id).map_or(&[][..], Vec::as_slice);
  index.work.add_references(ops.len());
  ops.iter().any(|op| op.kind.subscribes() && op.span == span)
}
