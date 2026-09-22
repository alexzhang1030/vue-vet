use std::collections::{HashMap, HashSet};

use oxc_ast::{
  AstKind,
  ast::{
    Argument, ArrayExpressionElement, AssignmentOperator, AssignmentTarget,
    AssignmentTargetMaybeDefault, AssignmentTargetProperty, BindingPattern, CallExpression,
    Declaration, Expression, IdentifierReference, ObjectPropertyKind, SimpleAssignmentTarget,
    Statement, UnaryOperator, VariableDeclarator,
  },
};
use oxc_semantic::{IsGlobalReference, NodeId, SymbolFlags, SymbolId};
use oxc_span::{GetSpan, Span};
use vue_vet_core::WatcherApiKind;

use super::resolve::{
  FunctionRef, FunctionResolver, argument_expression, call_has_spread, callback_argument_index,
  enclosing_function, export_local_symbol_id, is_global_host, is_proven_promise, referenced_symbol,
  uncertain_to_function, vue_callee_export, watcher_api,
};
use super::stats::WorkCounter;

#[derive(Default)]
pub(super) struct LifetimeIndex {
  pub enclosing: HashMap<NodeId, Option<NodeId>>,
  pub first_await: HashMap<NodeId, Span>,
  pub watchers: Vec<WatcherSite>,
  pub cleanups_by_fn: HashMap<NodeId, Vec<CleanupSite>>,
  pub deferred_cleanups_by_parent: HashMap<NodeId, Vec<DeferredCleanup>>,
  pub disposes_by_fn: HashMap<NodeId, Vec<DisposeSite>>,
  pub run_callbacks: HashMap<NodeId, Vec<ProvenRun>>,
  pub deferred_parents: HashMap<NodeId, (Span, NodeId)>,
  pub unproven_scopes: HashSet<SymbolId>,
  pub reentered_functions: HashSet<NodeId>,
  pub registers_by_fn: HashMap<NodeId, Vec<RegisterSite>>,
  pub selected_runs: HashMap<NodeId, Option<ProvenRun>>,
  pub incomplete_registration: HashSet<NodeId>,
  pub identity: super::cleanup_identity::CleanupIdentityIndex,
  pub reactive_bindings: HashMap<SymbolId, ReactiveBinding>,
  pub tracked_ops_by_fn: HashMap<NodeId, Vec<TrackedOp>>,
  pub detached_scopes: Vec<DetachedScopeSite>,
  pub stopped_scopes: HashSet<SymbolId>,
  pub scope_use_fns: HashMap<SymbolId, HashSet<NodeId>>,
  pub watch_handles: HashMap<SymbolId, NodeId>,
  pub stopped_watchers: HashSet<NodeId>,
  pub scope_toggles: Vec<ScopeToggle>,
  pub watchers_by_node: HashMap<NodeId, usize>,
  pub blocks: HashMap<NodeId, BlockFacts>,
  pub current_scope_calls: Vec<CurrentScopeCall>,
  pub stop_candidates: Vec<StopCandidate>,
  pub work: WorkCounter,
}

#[derive(Clone)]
pub(super) struct BlockFacts {
  pub first_exit: usize,
  pub starts: Vec<u32>,
  pub ends: Vec<u32>,
  pub pure: Vec<bool>,
  pub next_impure: Vec<usize>,
}

#[derive(Clone, Copy)]
pub(super) struct CurrentScopeCall {
  pub function_id: NodeId,
  pub node_id: NodeId,
  pub span: Span,
  pub unused: bool,
}

#[derive(Clone, Copy)]
pub(super) struct StopCandidate {
  pub watcher_id: NodeId,
  pub stop_id: NodeId,
}

#[derive(Clone, Copy)]
pub(super) struct RegisterSite {
  pub span: Span,
  pub identity: Option<NodeId>,
  pub callee: Option<SymbolId>,
  pub vue_cleanup: bool,
  pub explicit_owner: bool,
}

#[derive(Clone, Copy, Default)]
pub(super) struct WatcherOptions {
  pub unknown: bool,
  pub once: Option<bool>,
  pub immediate: Option<bool>,
  pub has_scheduler: bool,
}

impl WatcherOptions {
  /// Vue respects `once` only on `watch(source, callback, options)`.
  const fn once_applies(self, api: WatcherApiKind) -> bool {
    matches!(api, WatcherApiKind::Watch) && matches!(self.once, Some(true))
  }

  pub(super) const fn is_repeatable(self, api: WatcherApiKind) -> bool {
    !self.unknown && !self.has_scheduler && !self.once_applies(api)
  }

  pub(super) const fn is_exhausted_subscription(self, api: WatcherApiKind) -> bool {
    !self.unknown && self.once_applies(api) && matches!(self.immediate, Some(true))
  }
}

#[derive(Clone, Copy)]
pub(super) enum ReactiveKind {
  Ref,
  Computed,
  Proxy,
}

#[derive(Clone, Copy)]
pub(super) struct ReactiveBinding {
  pub kind: ReactiveKind,
  pub value_getter: Option<NodeId>,
  pub fresh: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum TrackedOpKind {
  Read,
  Write,
  Delete,
  Update,
}

impl TrackedOpKind {
  pub(super) const fn subscribes(self) -> bool {
    matches!(self, Self::Read | Self::Update)
  }
}

#[derive(Clone, Copy)]
pub(super) struct TrackedOp {
  pub symbol: SymbolId,
  pub span: Span,
  pub node_id: NodeId,
  pub kind: TrackedOpKind,
}

#[derive(Clone, Copy)]
pub(super) struct ScopeToggle {
  pub symbol: SymbolId,
  pub function_id: NodeId,
  pub node_id: NodeId,
  pub span: Span,
  pub on: bool,
}

#[derive(Clone, Copy)]
pub(super) struct WatcherSite {
  pub span: Span,
  pub api: WatcherApiKind,
  pub callback: Option<FunctionRef>,
  pub unused: bool,
  pub node_id: NodeId,
  pub options: WatcherOptions,
  /// Bare-identifier `watch` source for ownership proofs (no spread, no wrapper).
  pub source_symbol: Option<SymbolId>,
  /// Inner-expression `watch` source identity for cleanup-identity proofs.
  pub identity_source_symbol: Option<SymbolId>,
  pub schedule: super::cleanup_identity::WatchSchedule,
  pub handle_symbol: Option<SymbolId>,
  pub owner: Option<NodeId>,
  pub source_span: Option<Span>,
  pub source_getter: Option<NodeId>,
}

#[derive(Clone, Copy)]
pub(super) struct DetachedScopeSite {
  pub span: Span,
  pub symbol: SymbolId,
}

#[derive(Clone, Copy)]
pub(super) struct CleanupSite {
  pub node_id: NodeId,
  pub span: Span,
  pub suppressed: bool,
}

#[derive(Clone, Copy)]
pub(super) struct DeferredCleanup {
  pub site: CleanupSite,
  pub boundary: Span,
}

#[derive(Clone, Copy)]
pub(super) struct DisposeSite {
  pub node_id: NodeId,
  pub span: Span,
  pub suppressed: bool,
}

#[derive(Clone, Copy)]
pub(super) struct ProvenRun {
  pub node_id: NodeId,
  pub run_span: Span,
  pub owner_span: Span,
  pub callback: FunctionRef,
  pub scope_symbol: Option<SymbolId>,
}

pub(super) fn observe_node(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  kind: AstKind<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  resolver: &mut FunctionResolver<'_, '_>,
  index: &mut LifetimeIndex,
) {
  super::cleanup_identity::observe(semantic, node_id, kind, vue_exports, resolver, index);
  match kind {
    AstKind::BlockStatement(block) => index.record_block(node_id, block.body.as_slice()),
    AstKind::FunctionBody(body) => index.record_block(node_id, body.statements.as_slice()),
    AstKind::Program(program) => index.record_block(node_id, program.body.as_slice()),
    AstKind::CallExpression(call) => {
      record_scope_escape(semantic, call, vue_exports, index);
      record_scope_member_use(semantic, node_id, call, vue_exports, index);
      index_call(semantic, node_id, call, vue_exports, resolver, index);
    }
    AstKind::NewExpression(expression) => {
      for argument in &expression.arguments {
        let Some(value) = argument_expression(argument) else {
          continue;
        };
        mark_value_escape(semantic, value, vue_exports, index);
      }
    }
    AstKind::TaggedTemplateExpression(tagged) => {
      mark_value_escape(semantic, &tagged.tag, vue_exports, index);
      for expression in &tagged.quasi.expressions {
        mark_value_escape(semantic, expression, vue_exports, index);
      }
    }
    AstKind::ForInStatement(statement) => {
      if let Some(target) = statement.left.as_assignment_target() {
        mark_capability_mutation_target(semantic, target, index);
      }
    }
    AstKind::ForOfStatement(statement) => {
      if let Some(target) = statement.left.as_assignment_target() {
        mark_capability_mutation_target(semantic, target, index);
      }
    }
    AstKind::AwaitExpression(await_expression) => {
      let Some(function_id) = enclosing_function(semantic, node_id, &mut index.enclosing) else {
        return;
      };
      if uncertain_to_function(semantic, node_id, function_id) {
        return;
      }
      let span = await_expression.span;
      let entry = index.first_await.entry(function_id).or_insert(span);
      if span.start < entry.start {
        *entry = span;
      }
    }
    AstKind::AssignmentExpression(assignment) => {
      mark_capability_mutation_target(semantic, &assignment.left, index);
      mark_value_escape(semantic, &assignment.right, vue_exports, index);
    }
    AstKind::UpdateExpression(update) => match &update.argument {
      SimpleAssignmentTarget::StaticMemberExpression(member) => {
        invalidate_scope_receiver(semantic, &member.object, index);
      }
      SimpleAssignmentTarget::ComputedMemberExpression(member) => {
        invalidate_scope_receiver(semantic, &member.object, index);
      }
      _ => {}
    },
    AstKind::VariableDeclarator(declarator) => {
      if let Some(init) = &declarator.init {
        mark_value_escape(semantic, init, vue_exports, index);
      }
      record_declarator_bindings(semantic, declarator, vue_exports, resolver, index);
    }
    AstKind::IdentifierReference(identifier) => {
      record_identifier_use(semantic, node_id, identifier, vue_exports, index);
    }
    AstKind::ReturnStatement(ret) => {
      if let Some(argument) = &ret.argument {
        mark_value_escape(semantic, argument, vue_exports, index);
      }
    }
    AstKind::ExportNamedDeclaration(declaration) if declaration.source.is_none() => {
      for specifier in &declaration.specifiers {
        if let Some(symbol_id) = export_local_symbol_id(semantic, &specifier.local)
          && is_effect_scope_binding(semantic, symbol_id, vue_exports)
        {
          index.unproven_scopes.insert(symbol_id);
        }
      }
      if let Some(inner) = &declaration.declaration {
        unprove_exported_declaration(semantic, inner, vue_exports, index);
      }
    }
    AstKind::ExportDefaultDeclaration(declaration) => {
      if let Some(expression) = declaration.declaration.as_expression() {
        mark_value_escape(semantic, expression, vue_exports, index);
      }
    }
    AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::Delete => {
      mark_delete_run(semantic, &unary.argument, index);
    }
    AstKind::ArrayExpression(array) => {
      for element in &array.elements {
        if let Some(expression) = element.as_expression() {
          mark_value_escape(semantic, expression, vue_exports, index);
        }
      }
    }
    AstKind::ObjectProperty(property) => {
      mark_value_escape(semantic, &property.value, vue_exports, index);
    }
    AstKind::SpreadElement(spread) => {
      mark_value_escape(semantic, &spread.argument, vue_exports, index);
    }
    _ => {}
  }
}

fn index_call(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  call: &CallExpression<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  resolver: &mut FunctionResolver<'_, '_>,
  index: &mut LifetimeIndex,
) {
  record_register(semantic, node_id, call, vue_exports, resolver, index);

  if let Some(api) = watcher_api(semantic, call, vue_exports) {
    let callback = if call_has_spread(call) {
      None
    } else {
      call
        .arguments
        .get(callback_argument_index(api))
        .and_then(argument_expression)
        .and_then(|expression| resolver.resolve_expression(expression))
        .filter(|function| !function.is_generator)
    };
    let (identity_symbol, schedule, handle_symbol) =
      if api == WatcherApiKind::Watch && callback.is_some() {
        super::cleanup_identity::watch_source_and_schedule(semantic, node_id, call)
      } else {
        (None, super::cleanup_identity::WatchSchedule::DEFAULT, None)
      };
    let owner = enclosing_function(semantic, node_id, &mut index.enclosing);
    let (slot_symbol, source_span, source_getter) =
      watch_source_slots(semantic, call, api, resolver);
    index.work.add_watchers(1);
    index.watchers_by_node.insert(node_id, index.watchers.len());
    index.watchers.push(WatcherSite {
      node_id,
      span: call.span,
      api,
      callback,
      unused: call_result_unused(semantic, node_id),
      options: watcher_options(call, api),
      source_symbol: slot_symbol,
      identity_source_symbol: identity_symbol,
      schedule,
      handle_symbol,
      owner,
      source_span,
      source_getter,
    });
    if let Some(symbol) = const_call_binding(semantic, node_id) {
      index.watch_handles.insert(symbol, node_id);
    }
  }

  record_current_scope_call(semantic, node_id, call, vue_exports, index);

  record_stopped_handle(semantic, node_id, call, index);

  if vue_callee_export(semantic, &call.callee, vue_exports) == Some("onWatcherCleanup") {
    let site = CleanupSite { node_id, span: call.span, suppressed: cleanup_call_suppressed(call) };
    if let Some(function_id) = enclosing_function(semantic, node_id, &mut index.enclosing) {
      if let Some(&(boundary, parent)) = index.deferred_parents.get(&function_id) {
        index
          .deferred_cleanups_by_parent
          .entry(parent)
          .or_default()
          .push(DeferredCleanup { site, boundary });
      } else {
        index.cleanups_by_fn.entry(function_id).or_default().push(site);
      }
    }
  }

  if vue_callee_export(semantic, &call.callee, vue_exports) == Some("onScopeDispose")
    && let Some(function_id) = enclosing_function(semantic, node_id, &mut index.enclosing)
  {
    index.disposes_by_fn.entry(function_id).or_default().push(DisposeSite {
      node_id,
      span: call.span,
      suppressed: fail_silently_true(call) || call_has_spread(call),
    });
  }

  if let Some(run) = proven_run_call(semantic, node_id, call, vue_exports, resolver, index) {
    index.run_callbacks.entry(run.callback.node_id).or_default().push(run);
  }

  record_deferred_callback(semantic, node_id, call, vue_exports, resolver, index);
}

fn record_register(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  call: &CallExpression<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  resolver: &mut FunctionResolver<'_, '_>,
  index: &mut LifetimeIndex,
) {
  if watcher_api(semantic, call, vue_exports).is_some()
    || vue_callee_export(semantic, &call.callee, vue_exports) == Some("effectScope")
  {
    return;
  }
  let Some(function_id) = enclosing_function(semantic, node_id, &mut index.enclosing) else {
    return;
  };
  let vue_cleanup =
    vue_callee_export(semantic, &call.callee, vue_exports) == Some("onWatcherCleanup");
  let callee = call
    .callee
    .get_inner_expression()
    .get_identifier_reference()
    .and_then(|identifier| referenced_symbol(semantic, identifier));
  let explicit_owner = call.arguments.len() >= 3
    || call.arguments.iter().skip(2).any(|argument| matches!(argument, Argument::SpreadElement(_)));
  let (identity, incomplete) = register_argument_identity(call, resolver);
  if incomplete && (vue_cleanup || callee.is_some()) {
    index.incomplete_registration.insert(function_id);
  }
  if identity.is_none() && !vue_cleanup && callee.is_none() && !explicit_owner && !incomplete {
    return;
  }
  if explicit_owner && identity.is_none() {
    index.incomplete_registration.insert(function_id);
  }
  index.registers_by_fn.entry(function_id).or_default().push(RegisterSite {
    span: call.span,
    identity,
    callee,
    vue_cleanup,
    explicit_owner,
  });
}

fn register_argument_identity(
  call: &CallExpression<'_>,
  resolver: &mut FunctionResolver<'_, '_>,
) -> (Option<NodeId>, bool) {
  let Some(argument) = call.arguments.first() else {
    return (None, false);
  };
  match argument {
    Argument::SpreadElement(spread) => match spread.argument.get_inner_expression() {
      Expression::ArrayExpression(array) if array.elements.len() == 1 => {
        let Some(expression) =
          array.elements.first().and_then(oxc_ast::ast::ArrayExpressionElement::as_expression)
        else {
          return (None, true);
        };
        (
          resolver
            .resolve_expression(expression.get_inner_expression())
            .map(|function| function.node_id),
          false,
        )
      }
      _ => (None, true),
    },
    other => argument_expression(other).map_or((None, true), |expression| {
      (resolver.resolve_expression(expression).map(|function| function.node_id), false)
    }),
  }
}

fn cleanup_call_suppressed(call: &CallExpression<'_>) -> bool {
  if call_has_spread(call) {
    return true;
  }
  if call.arguments.len() >= 3 {
    return true;
  }
  match call.arguments.get(1).and_then(argument_expression) {
    None => false,
    Some(Expression::BooleanLiteral(literal)) => literal.value,
    Some(_) => true,
  }
}

fn fail_silently_true(call: &CallExpression<'_>) -> bool {
  matches!(
    call.arguments.get(1).and_then(argument_expression),
    Some(Expression::BooleanLiteral(literal)) if literal.value
  )
}

fn proven_run_call(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  call: &CallExpression<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  resolver: &mut FunctionResolver<'_, '_>,
  index: &LifetimeIndex,
) -> Option<ProvenRun> {
  if call_has_spread(call) {
    return None;
  }
  let (object, property) = run_member(&call.callee)?;
  if property != "run" {
    return None;
  }
  let (owner_span, scope_symbol) =
    proven_scope_owner(semantic, object, vue_exports, resolver, index)?;
  if scope_symbol.is_some_and(|symbol| index.unproven_scopes.contains(&symbol)) {
    return None;
  }
  let callback = argument_expression(call.arguments.first()?)
    .and_then(|expression| resolver.resolve_expression(expression))?;
  if callback.is_generator {
    return None;
  }
  Some(ProvenRun { node_id, run_span: call.span, owner_span, callback, scope_symbol })
}

fn run_member<'a>(callee: &'a Expression<'a>) -> Option<(&'a Expression<'a>, &'a str)> {
  match callee.get_inner_expression() {
    Expression::StaticMemberExpression(member) => {
      Some((&member.object, member.property.name.as_str()))
    }
    Expression::ComputedMemberExpression(member) => {
      let Expression::StringLiteral(literal) = member.expression.get_inner_expression() else {
        return None;
      };
      Some((&member.object, literal.value.as_str()))
    }
    _ => None,
  }
}

fn proven_scope_owner(
  semantic: &oxc_semantic::Semantic<'_>,
  object: &Expression<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  resolver: &mut FunctionResolver<'_, '_>,
  index: &LifetimeIndex,
) -> Option<(Span, Option<SymbolId>)> {
  match object.get_inner_expression() {
    Expression::Identifier(identifier) => {
      let symbol_id = referenced_symbol(semantic, identifier)?;
      if index.unproven_scopes.contains(&symbol_id) {
        return None;
      }
      if resolver.symbol_has_write(symbol_id) {
        return None;
      }
      effect_scope_init_span(semantic, symbol_id, vue_exports).map(|span| (span, Some(symbol_id)))
    }
    Expression::CallExpression(call)
      if vue_callee_export(semantic, &call.callee, vue_exports) == Some("effectScope") =>
    {
      Some((call.span, None))
    }
    _ => None,
  }
}

fn effect_scope_init_span(
  semantic: &oxc_semantic::Semantic<'_>,
  symbol_id: SymbolId,
  vue_exports: &HashMap<SymbolId, String>,
) -> Option<Span> {
  let declaration_id = semantic.scoping().symbol_declaration(symbol_id);
  let AstKind::VariableDeclarator(declarator) = semantic.nodes().kind(declaration_id) else {
    return None;
  };
  let init = declarator.init.as_ref()?;
  let Expression::CallExpression(call) = init.get_inner_expression() else {
    return None;
  };
  if vue_callee_export(semantic, &call.callee, vue_exports) != Some("effectScope") {
    return None;
  }
  Some(call.span)
}

fn unprove_exported_declaration(
  semantic: &oxc_semantic::Semantic<'_>,
  declaration: &Declaration<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  index: &mut LifetimeIndex,
) {
  let Declaration::VariableDeclaration(variable) = declaration else {
    return;
  };
  for declarator in &variable.declarations {
    unprove_exported_pattern(semantic, &declarator.id, vue_exports, index);
  }
}

fn unprove_exported_pattern(
  semantic: &oxc_semantic::Semantic<'_>,
  pattern: &BindingPattern<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  index: &mut LifetimeIndex,
) {
  match pattern {
    BindingPattern::BindingIdentifier(identifier) => {
      if let Some(symbol_id) = identifier.symbol_id.get()
        && is_effect_scope_binding(semantic, symbol_id, vue_exports)
      {
        index.unproven_scopes.insert(symbol_id);
      }
    }
    BindingPattern::AssignmentPattern(assignment) => {
      unprove_exported_pattern(semantic, &assignment.left, vue_exports, index);
    }
    BindingPattern::ObjectPattern(object) => {
      for property in &object.properties {
        unprove_exported_pattern(semantic, &property.value, vue_exports, index);
      }
    }
    BindingPattern::ArrayPattern(array) => {
      for element in array.elements.iter().flatten() {
        unprove_exported_pattern(semantic, element, vue_exports, index);
      }
    }
  }
}

fn is_effect_scope_binding(
  semantic: &oxc_semantic::Semantic<'_>,
  symbol_id: SymbolId,
  vue_exports: &HashMap<SymbolId, String>,
) -> bool {
  effect_scope_init_span(semantic, symbol_id, vue_exports).is_some()
}

fn record_deferred_callback(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  call: &CallExpression<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  resolver: &mut FunctionResolver<'_, '_>,
  index: &mut LifetimeIndex,
) {
  if call_has_spread(call) {
    return;
  }
  let Some(slots) = proven_deferred_slots(semantic, call, vue_exports) else {
    return;
  };
  let Some(parent) = enclosing_function(semantic, node_id, &mut index.enclosing) else {
    return;
  };
  for slot in slots {
    let Some(expression) = call.arguments.get(*slot).and_then(argument_expression) else {
      continue;
    };
    if !matches!(
      expression,
      Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
    ) {
      continue;
    }
    let Some(function) = resolver.resolve_expression(expression) else {
      continue;
    };
    index.deferred_parents.insert(function.node_id, (call.span, parent));
  }
}

fn proven_deferred_slots(
  semantic: &oxc_semantic::Semantic<'_>,
  call: &CallExpression<'_>,
  vue_exports: &HashMap<SymbolId, String>,
) -> Option<&'static [usize]> {
  if vue_callee_export(semantic, &call.callee, vue_exports) == Some("nextTick") {
    return Some(&[0]);
  }
  match call.callee.get_inner_expression() {
    Expression::Identifier(identifier) => {
      if !identifier.is_global_reference(semantic.scoping()) {
        return None;
      }
      match identifier.name.as_str() {
        "queueMicrotask" | "setTimeout" | "setInterval" | "requestAnimationFrame" => Some(&[0]),
        _ => None,
      }
    }
    Expression::StaticMemberExpression(member) => {
      let property = member.property.name.as_str();
      if matches!(property, "setTimeout" | "setInterval" | "requestAnimationFrame") {
        return is_global_host(semantic, &member.object).then_some(&[0][..]);
      }
      if !is_proven_promise(semantic, &member.object) {
        return None;
      }
      match property {
        "then" => Some(&[0, 1]),
        "catch" | "finally" => Some(&[0]),
        _ => None,
      }
    }
    _ => None,
  }
}

fn record_scope_member_use(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  call: &CallExpression<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  index: &mut LifetimeIndex,
) {
  let Some((object, property)) = run_member(&call.callee) else {
    return;
  };
  if property == "stop" {
    if let Some(identifier) = object.get_inner_expression().get_identifier_reference()
      && let Some(symbol_id) = referenced_symbol(semantic, identifier)
      && is_effect_scope_binding(semantic, symbol_id, vue_exports)
    {
      index.stopped_scopes.insert(symbol_id);
    }
    return;
  }
  if !matches!(property, "on" | "off") {
    return;
  }
  let Some(identifier) = object.get_inner_expression().get_identifier_reference() else {
    return;
  };
  let Some(symbol_id) = referenced_symbol(semantic, identifier) else {
    return;
  };
  if !is_effect_scope_binding(semantic, symbol_id, vue_exports) {
    return;
  }
  let Some(function_id) = enclosing_function(semantic, node_id, &mut index.enclosing) else {
    return;
  };
  index.reentered_functions.insert(function_id);
  index.scope_toggles.push(ScopeToggle {
    symbol: symbol_id,
    function_id,
    node_id,
    span: call.span,
    on: property == "on",
  });
}

fn record_scope_escape(
  semantic: &oxc_semantic::Semantic<'_>,
  call: &CallExpression<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  index: &mut LifetimeIndex,
) {
  if let Some((object, property)) = run_member(&call.callee)
    && matches!(property, "run" | "stop" | "on" | "off")
    && object
      .get_inner_expression()
      .get_identifier_reference()
      .and_then(|identifier| referenced_symbol(semantic, identifier))
      .is_some_and(|symbol| is_effect_scope_binding(semantic, symbol, vue_exports))
  {
    return;
  }
  for argument in &call.arguments {
    let Some(expression) = argument_expression(argument) else {
      continue;
    };
    mark_value_escape(semantic, expression, vue_exports, index);
  }
}

fn mark_value_escape(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  index: &mut LifetimeIndex,
) {
  match expression.get_inner_expression() {
    Expression::Identifier(identifier) => {
      if let Some(symbol_id) = referenced_symbol(semantic, identifier)
        && is_effect_scope_binding(semantic, symbol_id, vue_exports)
      {
        index.unproven_scopes.insert(symbol_id);
      }
    }
    Expression::ConditionalExpression(conditional) => {
      mark_value_escape(semantic, &conditional.consequent, vue_exports, index);
      mark_value_escape(semantic, &conditional.alternate, vue_exports, index);
      mark_value_escape(semantic, &conditional.test, vue_exports, index);
    }
    Expression::SequenceExpression(sequence) => {
      for expression in &sequence.expressions {
        mark_value_escape(semantic, expression, vue_exports, index);
      }
    }
    Expression::LogicalExpression(logical) => {
      mark_value_escape(semantic, &logical.left, vue_exports, index);
      mark_value_escape(semantic, &logical.right, vue_exports, index);
    }
    Expression::AssignmentExpression(assignment) => {
      mark_value_escape(semantic, &assignment.right, vue_exports, index);
    }
    Expression::ParenthesizedExpression(inner) => {
      mark_value_escape(semantic, &inner.expression, vue_exports, index);
    }
    Expression::TSAsExpression(inner) => {
      mark_value_escape(semantic, &inner.expression, vue_exports, index);
    }
    Expression::TSSatisfiesExpression(inner) => {
      mark_value_escape(semantic, &inner.expression, vue_exports, index);
    }
    Expression::TSNonNullExpression(inner) => {
      mark_value_escape(semantic, &inner.expression, vue_exports, index);
    }
    Expression::TSTypeAssertion(inner) => {
      mark_value_escape(semantic, &inner.expression, vue_exports, index);
    }
    Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) => {
      if let Some((_object, property)) = run_member(expression)
        && matches!(property, "run" | "stop" | "on" | "off")
      {
        return;
      }
      if let Expression::StaticMemberExpression(member) = expression.get_inner_expression() {
        mark_value_escape(semantic, &member.object, vue_exports, index);
      }
      if let Expression::ComputedMemberExpression(member) = expression.get_inner_expression() {
        mark_value_escape(semantic, &member.object, vue_exports, index);
      }
    }
    _ => {}
  }
}

fn mark_capability_mutation_target(
  semantic: &oxc_semantic::Semantic<'_>,
  target: &AssignmentTarget<'_>,
  index: &mut LifetimeIndex,
) {
  match target {
    AssignmentTarget::StaticMemberExpression(member) => {
      invalidate_scope_receiver(semantic, &member.object, index);
    }
    AssignmentTarget::ComputedMemberExpression(member) => {
      invalidate_scope_receiver(semantic, &member.object, index);
    }
    AssignmentTarget::PrivateFieldExpression(member) => {
      invalidate_scope_receiver(semantic, &member.object, index);
    }
    AssignmentTarget::ArrayAssignmentTarget(array) => {
      for element in array.elements.iter().flatten() {
        mark_capability_mutation_maybe_default(semantic, element, index);
      }
      if let Some(rest) = &array.rest {
        mark_capability_mutation_target(semantic, &rest.target, index);
      }
    }
    AssignmentTarget::ObjectAssignmentTarget(object) => {
      for property in &object.properties {
        if let AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) = property {
          mark_capability_mutation_maybe_default(semantic, &property.binding, index);
        }
      }
      if let Some(rest) = &object.rest {
        mark_capability_mutation_target(semantic, &rest.target, index);
      }
    }
    AssignmentTarget::TSAsExpression(inner) => {
      mark_capability_mutation_from_expression(semantic, &inner.expression, index);
    }
    AssignmentTarget::TSSatisfiesExpression(inner) => {
      mark_capability_mutation_from_expression(semantic, &inner.expression, index);
    }
    AssignmentTarget::TSNonNullExpression(inner) => {
      mark_capability_mutation_from_expression(semantic, &inner.expression, index);
    }
    AssignmentTarget::TSTypeAssertion(inner) => {
      mark_capability_mutation_from_expression(semantic, &inner.expression, index);
    }
    AssignmentTarget::AssignmentTargetIdentifier(_) => {}
  }
}

fn mark_capability_mutation_maybe_default(
  semantic: &oxc_semantic::Semantic<'_>,
  target: &AssignmentTargetMaybeDefault<'_>,
  index: &mut LifetimeIndex,
) {
  match target {
    AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(with_default) => {
      mark_capability_mutation_target(semantic, &with_default.binding, index);
    }
    other => {
      if let Some(target) = other.as_assignment_target() {
        mark_capability_mutation_target(semantic, target, index);
      }
    }
  }
}

fn mark_capability_mutation_from_expression(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  index: &mut LifetimeIndex,
) {
  match expression.get_inner_expression() {
    Expression::StaticMemberExpression(member) => {
      invalidate_scope_receiver(semantic, &member.object, index);
    }
    Expression::ComputedMemberExpression(member) => {
      invalidate_scope_receiver(semantic, &member.object, index);
    }
    Expression::PrivateFieldExpression(member) => {
      invalidate_scope_receiver(semantic, &member.object, index);
    }
    _ => {}
  }
}

fn mark_delete_run(
  semantic: &oxc_semantic::Semantic<'_>,
  argument: &Expression<'_>,
  index: &mut LifetimeIndex,
) {
  match argument.get_inner_expression() {
    Expression::StaticMemberExpression(member) => {
      invalidate_scope_receiver(semantic, &member.object, index);
    }
    Expression::ComputedMemberExpression(member) => {
      invalidate_scope_receiver(semantic, &member.object, index);
    }
    Expression::PrivateFieldExpression(member) => {
      invalidate_scope_receiver(semantic, &member.object, index);
    }
    _ => {}
  }
}

fn invalidate_scope_receiver(
  semantic: &oxc_semantic::Semantic<'_>,
  object: &Expression<'_>,
  index: &mut LifetimeIndex,
) {
  let Some(identifier) = object.get_inner_expression().get_identifier_reference() else {
    return;
  };
  if let Some(symbol_id) = referenced_symbol(semantic, identifier) {
    index.unproven_scopes.insert(symbol_id);
  }
}

impl LifetimeIndex {
  fn record_block(&mut self, block_id: NodeId, statements: &[Statement<'_>]) {
    if self.blocks.contains_key(&block_id) {
      return;
    }
    let mut first_exit = statements.len();
    let mut starts = Vec::with_capacity(statements.len());
    let mut ends = Vec::with_capacity(statements.len());
    let mut pure = Vec::with_capacity(statements.len());
    for (index, statement) in statements.iter().enumerate() {
      self.work.add_statements(1);
      starts.push(statement.span().start);
      ends.push(statement.span().end);
      if first_exit == statements.len() && statement_is_unconditional_exit(statement) {
        first_exit = index;
      }
      pure.push(false);
    }
    self
      .blocks
      .insert(block_id, BlockFacts { first_exit, starts, ends, pure, next_impure: Vec::new() });
  }

  pub(super) fn preceding_exit(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: NodeId,
    function_id: NodeId,
  ) -> bool {
    let node_start = semantic.nodes().kind(node_id).span().start;
    let mut current = node_id;
    loop {
      let parent = semantic.nodes().parent_id(current);
      if parent == current {
        return false;
      }
      self.work.add_statements(1);
      if let Some(block) = self.blocks.get(&parent)
        && block.ends.get(block.first_exit).is_some_and(|end| *end <= node_start)
      {
        return true;
      }
      if parent == function_id {
        return false;
      }
      current = parent;
    }
  }

  fn statement_pos(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    mut node_id: NodeId,
  ) -> Option<(NodeId, usize)> {
    let node_span = semantic.nodes().kind(node_id).span();
    loop {
      let parent = semantic.nodes().parent_id(node_id);
      if parent == node_id {
        return None;
      }
      if let Some(block) = self.blocks.get(&parent) {
        let index = self.work.partition_point(&block.starts, |start| *start <= node_span.start);
        let index = index.saturating_sub(1);
        if block.starts.get(index).is_some_and(|start| *start <= node_span.start)
          && block.ends.get(index).is_some_and(|end| *end >= node_span.end)
        {
          return Some((parent, index));
        }
      }
      node_id = parent;
    }
  }

  fn stop_follows_watch_through_pure_reads(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    watch_id: NodeId,
    stop_id: NodeId,
  ) -> bool {
    let Some((watch_block, watch_index)) = self.statement_pos(semantic, watch_id) else {
      return false;
    };
    let Some((stop_block, stop_index)) = self.statement_pos(semantic, stop_id) else {
      return false;
    };
    if watch_block != stop_block || stop_index <= watch_index {
      return false;
    }
    let Some(block) = self.blocks.get(&watch_block) else {
      return false;
    };
    self.work.add_statements(1);
    let start = watch_index.saturating_add(1);
    let next = block.next_impure.get(start).copied().unwrap_or(block.pure.len());
    next >= stop_index
  }

  pub(super) fn watcher_for_call(&self, call: &CallExpression<'_>) -> Option<&WatcherSite> {
    self.work.add_watchers(1);
    let node_id = call.node_id.get();
    self.watchers_by_node.get(&node_id).and_then(|index| self.watchers.get(*index))
  }

  pub(super) fn finalize(&mut self, semantic: &oxc_semantic::Semantic<'_>) -> usize {
    self.finalize_block_purity(semantic);
    self.prove_stopped_handles(semantic);
    self.drop_dead_tracked_ops(semantic);
    self.escape_current_scope_capability(semantic);
    let mut work = 0usize;
    let unproven = &self.unproven_scopes;
    let mut selected = HashMap::new();
    for (function_id, runs) in &self.run_callbacks {
      work = work.saturating_add(runs.len());
      self.work.add_watchers(runs.len());
      let chosen = runs
        .iter()
        .filter(|run| run.scope_symbol.is_none_or(|symbol| !unproven.contains(&symbol)))
        .min_by_key(|run| run.run_span.start)
        .copied();
      selected.insert(*function_id, chosen);
    }
    self.selected_runs = selected;
    work
  }

  fn drop_dead_tracked_ops(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    let function_ids: Vec<NodeId> = self.tracked_ops_by_fn.keys().copied().collect();
    for function_id in function_ids {
      let ops = self.tracked_ops_by_fn.get(&function_id).cloned().unwrap_or_default();
      let live: Vec<TrackedOp> = ops
        .into_iter()
        .filter(|op| {
          self.work.add_references(1);
          !self.preceding_exit(semantic, op.node_id, function_id)
        })
        .collect();
      self.tracked_ops_by_fn.insert(function_id, live);
    }
  }

  fn finalize_block_purity(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    let block_ids: Vec<NodeId> = self.blocks.keys().copied().collect();
    for block_id in block_ids {
      let Some(statements) = statements_of(semantic, block_id) else {
        continue;
      };
      let mut pure = Vec::with_capacity(statements.len());
      for statement in statements {
        self.work.add_statements(1);
        pure.push(statement_is_pure_read(semantic, self, statement));
      }
      let count = pure.len();
      let mut next_impure = vec![count; count];
      let mut next = count;
      for index in (0..count).rev() {
        self.work.add_statements(1);
        if !pure.get(index).copied().unwrap_or(true) {
          next = index;
        }
        if let Some(slot) = next_impure.get_mut(index) {
          *slot = next;
        }
      }
      if let Some(block) = self.blocks.get_mut(&block_id) {
        block.pure = pure;
        block.next_impure = next_impure;
      }
    }
  }

  fn prove_stopped_handles(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    let candidates = self.stop_candidates.clone();
    for candidate in candidates {
      self.work.add_watchers(1);
      if self.stopped_watchers.contains(&candidate.watcher_id) {
        continue;
      }
      if self.stop_follows_watch_through_pure_reads(
        semantic,
        candidate.watcher_id,
        candidate.stop_id,
      ) {
        self.stopped_watchers.insert(candidate.watcher_id);
      }
    }
  }

  fn escape_current_scope_capability(&mut self, semantic: &oxc_semantic::Semantic<'_>) {
    let calls = self.current_scope_calls.clone();
    let mut eligible = HashSet::new();
    for call in &calls {
      self.work.add_references(1);
      match self.current_scope_capability(semantic, call) {
        CurrentScopeCapability::Absent => {}
        CurrentScopeCapability::MayEscape | CurrentScopeCapability::Escaped => {
          eligible.insert(call.function_id);
        }
      }
    }
    let mut escaped = HashSet::new();
    for function_id in eligible {
      let Some(runs) = self.run_callbacks.get(&function_id) else {
        continue;
      };
      self.work.add_references(runs.len());
      for run in runs {
        if let Some(symbol) = run.scope_symbol {
          escaped.insert(symbol);
        }
      }
    }
    self.unproven_scopes.extend(escaped);
  }

  fn current_scope_capability(
    &self,
    semantic: &oxc_semantic::Semantic<'_>,
    call: &CurrentScopeCall,
  ) -> CurrentScopeCapability {
    if call.unused || self.preceding_exit(semantic, call.node_id, call.function_id) {
      return CurrentScopeCapability::Absent;
    }
    if let Some(await_span) = self.first_await.get(&call.function_id)
      && call.span.start >= await_span.end
    {
      return CurrentScopeCapability::Absent;
    }
    if uncertain_to_function(semantic, call.node_id, call.function_id) {
      CurrentScopeCapability::MayEscape
    } else {
      CurrentScopeCapability::Escaped
    }
  }
}

const fn statement_is_unconditional_exit(statement: &Statement<'_>) -> bool {
  matches!(statement, Statement::ReturnStatement(_) | Statement::ThrowStatement(_))
}

fn statements_of<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  node_id: NodeId,
) -> Option<&'a [Statement<'a>]> {
  match semantic.nodes().kind(node_id) {
    AstKind::BlockStatement(block) => Some(block.body.as_slice()),
    AstKind::FunctionBody(body) => Some(body.statements.as_slice()),
    AstKind::Program(program) => Some(program.body.as_slice()),
    _ => None,
  }
}

fn statement_is_pure_read(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  statement: &Statement<'_>,
) -> bool {
  match statement {
    Statement::EmptyStatement(_) => true,
    Statement::ExpressionStatement(expression) => {
      is_pure_read_expr(semantic, index, &expression.expression)
    }
    Statement::VariableDeclaration(declaration)
      if matches!(
        declaration.kind,
        oxc_ast::ast::VariableDeclarationKind::Const
          | oxc_ast::ast::VariableDeclarationKind::Let
          | oxc_ast::ast::VariableDeclarationKind::Var
      ) =>
    {
      declaration.declarations.iter().all(|declarator| {
        matches!(declarator.id, BindingPattern::BindingIdentifier(_))
          && declarator.init.as_ref().is_none_or(|init| is_pure_read_expr(semantic, index, init))
      })
    }
    _ => false,
  }
}

fn is_pure_read_expr(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  expression: &Expression<'_>,
) -> bool {
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(_)
    | Expression::NullLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::Identifier(_)
    | Expression::ThisExpression(_) => true,
    Expression::TemplateLiteral(template) => template.expressions.is_empty(),
    Expression::StaticMemberExpression(member) => {
      is_proven_vue_ref_value(semantic, index, &member.object, member.property.name.as_str())
    }
    Expression::ComputedMemberExpression(member) => {
      matches!(
        member.expression.get_inner_expression(),
        Expression::StringLiteral(literal) if literal.value.as_str() == "value"
      ) && is_proven_vue_ref_value(semantic, index, &member.object, "value")
    }
    Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::Void => {
      is_pure_read_expr(semantic, index, &unary.argument)
    }
    Expression::ParenthesizedExpression(inner) => {
      is_pure_read_expr(semantic, index, &inner.expression)
    }
    Expression::TSAsExpression(inner) => is_pure_read_expr(semantic, index, &inner.expression),
    Expression::TSSatisfiesExpression(inner) => {
      is_pure_read_expr(semantic, index, &inner.expression)
    }
    Expression::TSNonNullExpression(inner) => is_pure_read_expr(semantic, index, &inner.expression),
    Expression::TSTypeAssertion(inner) => is_pure_read_expr(semantic, index, &inner.expression),
    _ => false,
  }
}

fn is_proven_vue_ref_value(
  semantic: &oxc_semantic::Semantic<'_>,
  index: &LifetimeIndex,
  object: &Expression<'_>,
  property: &str,
) -> bool {
  if property != "value" {
    return false;
  }
  let Some(identifier) = object.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  let Some(symbol) = referenced_symbol(semantic, identifier) else {
    return false;
  };
  index
    .reactive_bindings
    .get(&symbol)
    .is_some_and(|binding| matches!(binding.kind, ReactiveKind::Ref) && binding.fresh)
}

fn call_result_unused(semantic: &oxc_semantic::Semantic<'_>, node_id: NodeId) -> bool {
  match semantic.nodes().parent_kind(node_id) {
    AstKind::ExpressionStatement(_) => true,
    AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::Void => true,
    _ => false,
  }
}

fn record_declarator_bindings(
  semantic: &oxc_semantic::Semantic<'_>,
  declarator: &VariableDeclarator<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  resolver: &mut FunctionResolver<'_, '_>,
  index: &mut LifetimeIndex,
) {
  let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
    return;
  };
  let Some(symbol_id) = identifier.symbol_id.get() else {
    return;
  };
  if !semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable) {
    return;
  }
  let Some(init) = &declarator.init else {
    return;
  };
  let Expression::CallExpression(call) = init.get_inner_expression() else {
    return;
  };
  match vue_callee_export(semantic, &call.callee, vue_exports) {
    Some("ref" | "shallowRef") => {
      index.reactive_bindings.insert(
        symbol_id,
        ReactiveBinding {
          kind: ReactiveKind::Ref,
          value_getter: None,
          fresh: is_fresh_plain_ref_argument(call),
        },
      );
    }
    Some("computed") => {
      let value_getter = computed_getter_id(call, resolver);
      index.reactive_bindings.insert(
        symbol_id,
        ReactiveBinding { kind: ReactiveKind::Computed, value_getter, fresh: false },
      );
    }
    Some("reactive" | "shallowReactive") => {
      index.reactive_bindings.insert(
        symbol_id,
        ReactiveBinding { kind: ReactiveKind::Proxy, value_getter: None, fresh: false },
      );
    }
    Some("effectScope") if is_literal_detached_scope(call) => {
      index.detached_scopes.push(DetachedScopeSite { span: call.span, symbol: symbol_id });
    }
    _ => {}
  }
}

fn is_literal_detached_scope(call: &CallExpression<'_>) -> bool {
  if call_has_spread(call) || call.arguments.len() != 1 {
    return false;
  }
  matches!(
    call.arguments.first().and_then(argument_expression),
    Some(Expression::BooleanLiteral(literal)) if literal.value
  )
}

fn is_fresh_plain_ref_argument(call: &CallExpression<'_>) -> bool {
  if call_has_spread(call) {
    return false;
  }
  let Some(argument) = call.arguments.first() else {
    return true;
  };
  let Some(expression) = argument_expression(argument) else {
    return false;
  };
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(_)
    | Expression::NullLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::ObjectExpression(_)
    | Expression::ArrayExpression(_) => true,
    Expression::TemplateLiteral(template) => template.expressions.is_empty(),
    Expression::Identifier(identifier) if identifier.name == "undefined" => true,
    Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::Void => true,
    _ => false,
  }
}

fn record_identifier_use(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  identifier: &IdentifierReference<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  index: &mut LifetimeIndex,
) {
  let Some(symbol_id) = referenced_symbol(semantic, identifier) else {
    return;
  };
  let Some(function_id) = enclosing_function(semantic, node_id, &mut index.enclosing) else {
    return;
  };
  if is_effect_scope_binding(semantic, symbol_id, vue_exports) {
    index.scope_use_fns.entry(symbol_id).or_default().insert(function_id);
  }
  if is_type_position(semantic, node_id) {
    return;
  }
  let Some(&binding) = index.reactive_bindings.get(&symbol_id) else {
    return;
  };
  let Some(kind) = classify_tracked_op(semantic, node_id, identifier.span, binding.kind) else {
    return;
  };
  if uncertain_to_function(semantic, node_id, function_id) {
    return;
  }
  if let Some(await_span) = index.first_await.get(&function_id)
    && identifier.span.start >= await_span.end
  {
    return;
  }
  index.work.add_references(1);
  index.tracked_ops_by_fn.entry(function_id).or_default().push(TrackedOp {
    symbol: symbol_id,
    span: identifier.span,
    node_id,
    kind,
  });
}

fn watch_source_slots(
  semantic: &oxc_semantic::Semantic<'_>,
  call: &CallExpression<'_>,
  api: WatcherApiKind,
  resolver: &mut FunctionResolver<'_, '_>,
) -> (Option<SymbolId>, Option<Span>, Option<NodeId>) {
  if api != WatcherApiKind::Watch || call_has_spread(call) {
    return (None, None, None);
  }
  let Some(expression) = call.arguments.first().and_then(argument_expression) else {
    return (None, None, None);
  };
  match expression {
    Expression::Identifier(identifier) => {
      let symbol = referenced_symbol(semantic, identifier);
      (symbol, symbol.map(|_| identifier.span), None)
    }
    Expression::ArrowFunctionExpression(arrow) => (None, None, Some(arrow.node_id.get())),
    Expression::FunctionExpression(function) => (None, None, Some(function.node_id.get())),
    other => resolver
      .resolve_expression(other)
      .map_or((None, None, None), |function| (None, None, Some(function.node_id))),
  }
}

fn watcher_options(call: &CallExpression<'_>, api: WatcherApiKind) -> WatcherOptions {
  if call_has_spread(call) {
    return WatcherOptions { unknown: true, ..WatcherOptions::default() };
  }
  let slot = match api {
    WatcherApiKind::Watch => 2,
    WatcherApiKind::WatchEffect
    | WatcherApiKind::WatchPostEffect
    | WatcherApiKind::WatchSyncEffect => 1,
  };
  let Some(argument) = call.arguments.get(slot) else {
    return WatcherOptions::default();
  };
  let Some(expression) = argument_expression(argument) else {
    return WatcherOptions { unknown: true, ..WatcherOptions::default() };
  };
  let Expression::ObjectExpression(object) = expression else {
    return WatcherOptions { unknown: true, ..WatcherOptions::default() };
  };
  let mut options = WatcherOptions::default();
  for property in &object.properties {
    match property {
      ObjectPropertyKind::SpreadProperty(_) => options.unknown = true,
      ObjectPropertyKind::ObjectProperty(property) => {
        let Some(name) = property.key.static_name() else {
          options.unknown = true;
          continue;
        };
        match name.as_ref() {
          "once" => match property.value.get_inner_expression() {
            Expression::BooleanLiteral(literal) => options.once = Some(literal.value),
            _ => options.unknown = true,
          },
          "immediate" => match property.value.get_inner_expression() {
            Expression::BooleanLiteral(literal) => options.immediate = Some(literal.value),
            _ => options.unknown = true,
          },
          "scheduler" => options.has_scheduler = true,
          _ => {}
        }
      }
    }
  }
  options
}

fn const_call_binding(
  semantic: &oxc_semantic::Semantic<'_>,
  mut node_id: NodeId,
) -> Option<SymbolId> {
  loop {
    let parent = semantic.nodes().parent_id(node_id);
    if parent == node_id {
      return None;
    }
    match semantic.nodes().kind(parent) {
      AstKind::VariableDeclarator(declarator) => {
        let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
          return None;
        };
        let symbol_id = identifier.symbol_id.get()?;
        if semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable) {
          return Some(symbol_id);
        }
        return None;
      }
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_) => node_id = parent,
      _ => return None,
    }
  }
}

fn record_stopped_handle(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  call: &CallExpression<'_>,
  index: &mut LifetimeIndex,
) {
  let Some(identifier) = call.callee.get_inner_expression().get_identifier_reference() else {
    return;
  };
  let Some(symbol_id) = referenced_symbol(semantic, identifier) else {
    return;
  };
  let Some(&watcher_id) = index.watch_handles.get(&symbol_id) else {
    return;
  };
  let function_id = enclosing_function(semantic, node_id, &mut index.enclosing);
  let watch_fn = enclosing_function(semantic, watcher_id, &mut index.enclosing);
  if function_id != watch_fn {
    return;
  }
  if function_id.is_some_and(|function_id| uncertain_to_function(semantic, node_id, function_id)) {
    return;
  }
  index.work.add_watchers(1);
  index.stop_candidates.push(StopCandidate { watcher_id, stop_id: node_id });
}

fn is_type_position(semantic: &oxc_semantic::Semantic<'_>, node_id: NodeId) -> bool {
  matches!(
    semantic.nodes().parent_kind(node_id),
    AstKind::TSTypeQuery(_)
      | AstKind::TSTypeReference(_)
      | AstKind::TSTypeAnnotation(_)
      | AstKind::TSTypeAliasDeclaration(_)
      | AstKind::TSInterfaceDeclaration(_)
      | AstKind::TSTypeParameterInstantiation(_)
      | AstKind::TSTypeParameter(_)
      | AstKind::TSQualifiedName(_)
  )
}

fn computed_getter_id(
  call: &CallExpression<'_>,
  resolver: &mut FunctionResolver<'_, '_>,
) -> Option<NodeId> {
  let expression = call.arguments.first().and_then(argument_expression)?;
  match expression {
    Expression::ArrowFunctionExpression(arrow) => Some(arrow.node_id.get()),
    Expression::FunctionExpression(function) => Some(function.node_id.get()),
    other => resolver.resolve_expression(other).map(|function| function.node_id),
  }
}

fn record_current_scope_call(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  call: &CallExpression<'_>,
  vue_exports: &HashMap<SymbolId, String>,
  index: &mut LifetimeIndex,
) {
  if vue_callee_export(semantic, &call.callee, vue_exports) != Some("getCurrentScope") {
    return;
  }
  let Some(function_id) = enclosing_function(semantic, node_id, &mut index.enclosing) else {
    return;
  };
  index.work.add_references(1);
  index.current_scope_calls.push(CurrentScopeCall {
    function_id,
    node_id,
    span: call.span,
    unused: call_result_unused(semantic, node_id),
  });
}

fn classify_tracked_op(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  ident_span: Span,
  kind: ReactiveKind,
) -> Option<TrackedOpKind> {
  let member_id = semantic.nodes().parent_id(node_id);
  if member_id == node_id || !is_tracked_member(semantic, member_id, ident_span, kind) {
    return None;
  }
  classify_member_role(semantic, member_id)
}

fn is_tracked_member(
  semantic: &oxc_semantic::Semantic<'_>,
  member_id: NodeId,
  ident_span: Span,
  kind: ReactiveKind,
) -> bool {
  match semantic.nodes().kind(member_id) {
    AstKind::StaticMemberExpression(member) => {
      if member.object.get_inner_expression().span() != ident_span {
        return false;
      }
      match kind {
        ReactiveKind::Ref | ReactiveKind::Computed => member.property.name.as_str() == "value",
        ReactiveKind::Proxy => true,
      }
    }
    AstKind::ComputedMemberExpression(member) => {
      if member.object.get_inner_expression().span() != ident_span {
        return false;
      }
      match kind {
        ReactiveKind::Ref | ReactiveKind::Computed => {
          matches!(
            member.expression.get_inner_expression(),
            Expression::StringLiteral(literal) if literal.value.as_str() == "value"
          )
        }
        ReactiveKind::Proxy => true,
      }
    }
    _ => false,
  }
}

fn classify_member_role(
  semantic: &oxc_semantic::Semantic<'_>,
  mut node_id: NodeId,
) -> Option<TrackedOpKind> {
  loop {
    let parent = semantic.nodes().parent_id(node_id);
    if parent == node_id {
      return Some(TrackedOpKind::Read);
    }
    match semantic.nodes().kind(parent) {
      AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::Delete => {
        return Some(TrackedOpKind::Delete);
      }
      AstKind::UpdateExpression(_) => return Some(TrackedOpKind::Update),
      AstKind::AssignmentExpression(assignment) => {
        let node_span = semantic.nodes().kind(node_id).span();
        if span_covers(assignment.left.span(), node_span) {
          return Some(if assignment.operator == AssignmentOperator::Assign {
            TrackedOpKind::Write
          } else {
            TrackedOpKind::Update
          });
        }
        return Some(TrackedOpKind::Read);
      }
      AstKind::AssignmentTargetWithDefault(target) => {
        let node_span = semantic.nodes().kind(node_id).span();
        if span_covers(target.init.span(), node_span) {
          return match default_activation(semantic, parent) {
            DefaultActivation::Activated => Some(TrackedOpKind::Read),
            DefaultActivation::Skipped | DefaultActivation::Unknown => None,
          };
        }
        node_id = parent;
      }
      AstKind::AssignmentTargetPropertyIdentifier(property) => {
        let node_span = semantic.nodes().kind(node_id).span();
        if property.init.as_ref().is_some_and(|init| span_covers(init.span(), node_span)) {
          return match default_activation(semantic, parent) {
            DefaultActivation::Activated => Some(TrackedOpKind::Read),
            DefaultActivation::Skipped | DefaultActivation::Unknown => None,
          };
        }
        node_id = parent;
      }
      AstKind::AssignmentTargetPropertyProperty(property) => {
        let node_span = semantic.nodes().kind(node_id).span();
        if span_covers(property.name.span(), node_span) {
          return Some(TrackedOpKind::Read);
        }
        node_id = parent;
      }
      AstKind::ArrayAssignmentTarget(_)
      | AstKind::ObjectAssignmentTarget(_)
      | AstKind::ForInStatement(_)
      | AstKind::ForOfStatement(_) => return Some(TrackedOpKind::Write),
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_)
      | AstKind::ChainExpression(_) => node_id = parent,
      _ => return Some(TrackedOpKind::Read),
    }
  }
}

const fn span_covers(outer: Span, inner: Span) -> bool {
  outer.start <= inner.start && outer.end >= inner.end
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CurrentScopeCapability {
  Absent,
  MayEscape,
  Escaped,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum DefaultActivation {
  Activated,
  Skipped,
  Unknown,
}

enum PatternSlot {
  Object(String),
  Array(usize),
}

fn default_activation(
  semantic: &oxc_semantic::Semantic<'_>,
  mut node_id: NodeId,
) -> DefaultActivation {
  let mut slots = Vec::new();
  loop {
    let parent = semantic.nodes().parent_id(node_id);
    if parent == node_id {
      return DefaultActivation::Unknown;
    }
    match semantic.nodes().kind(parent) {
      AstKind::AssignmentTargetPropertyProperty(property) => {
        if property.computed {
          return DefaultActivation::Unknown;
        }
        let Some(name) = property.name.static_name() else {
          return DefaultActivation::Unknown;
        };
        slots.push(PatternSlot::Object(name.to_string()));
        node_id = parent;
      }
      AstKind::AssignmentTargetPropertyIdentifier(property) => {
        slots.push(PatternSlot::Object(property.binding.name.to_string()));
        node_id = parent;
      }
      AstKind::ArrayAssignmentTarget(array) => {
        let node_span = semantic.nodes().kind(node_id).span();
        let Some(index) = array
          .elements
          .iter()
          .position(|element| element.as_ref().is_some_and(|el| span_covers(el.span(), node_span)))
        else {
          return DefaultActivation::Unknown;
        };
        slots.push(PatternSlot::Array(index));
        node_id = parent;
      }
      AstKind::AssignmentExpression(assignment) => {
        if slots.is_empty() {
          return DefaultActivation::Unknown;
        }
        return activation_from_rhs(&assignment.right, &slots);
      }
      AstKind::AssignmentTargetWithDefault(_)
      | AstKind::ObjectAssignmentTarget(_)
      | AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_)
      | AstKind::ChainExpression(_) => {
        node_id = parent;
      }
      _ => return DefaultActivation::Unknown,
    }
  }
}

fn activation_from_rhs(expression: &Expression<'_>, slots: &[PatternSlot]) -> DefaultActivation {
  let mut current = expression;
  for slot in slots.iter().rev() {
    match (current.get_inner_expression(), slot) {
      (Expression::ObjectExpression(object), PatternSlot::Object(key)) => {
        match object_property_value(object, key) {
          ObjectSlot::Missing => return DefaultActivation::Activated,
          ObjectSlot::Unknown => return DefaultActivation::Unknown,
          ObjectSlot::Value(value) => current = value,
        }
      }
      (Expression::ArrayExpression(array), PatternSlot::Array(index)) => {
        match array_element_value(array, *index) {
          ObjectSlot::Missing => return DefaultActivation::Activated,
          ObjectSlot::Unknown => return DefaultActivation::Unknown,
          ObjectSlot::Value(value) => current = value,
        }
      }
      (Expression::Identifier(identifier), _) if identifier.name == "undefined" => {
        return DefaultActivation::Activated;
      }
      (Expression::UnaryExpression(unary), _) if matches!(unary.operator, UnaryOperator::Void) => {
        return DefaultActivation::Activated;
      }
      _ => return DefaultActivation::Unknown,
    }
  }
  match current.get_inner_expression() {
    Expression::Identifier(identifier) if identifier.name == "undefined" => {
      DefaultActivation::Activated
    }
    Expression::UnaryExpression(unary) if matches!(unary.operator, UnaryOperator::Void) => {
      DefaultActivation::Activated
    }
    Expression::BooleanLiteral(_)
    | Expression::NullLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::ObjectExpression(_)
    | Expression::ArrayExpression(_) => DefaultActivation::Skipped,
    Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
      DefaultActivation::Skipped
    }
    _ => DefaultActivation::Unknown,
  }
}

enum ObjectSlot<'a> {
  Missing,
  Value(&'a Expression<'a>),
  Unknown,
}

fn object_property_value<'a>(
  object: &'a oxc_ast::ast::ObjectExpression<'a>,
  key: &str,
) -> ObjectSlot<'a> {
  let mut found = None;
  for property in &object.properties {
    match property {
      ObjectPropertyKind::SpreadProperty(_) => return ObjectSlot::Unknown,
      ObjectPropertyKind::ObjectProperty(property) => {
        if property.computed {
          return ObjectSlot::Unknown;
        }
        let Some(name) = property.key.static_name() else {
          return ObjectSlot::Unknown;
        };
        if name == key {
          found = Some(&property.value);
        }
      }
    }
  }
  found.map_or(ObjectSlot::Missing, ObjectSlot::Value)
}

fn array_element_value<'a>(
  array: &'a oxc_ast::ast::ArrayExpression<'a>,
  index: usize,
) -> ObjectSlot<'a> {
  if array
    .elements
    .iter()
    .any(|element| matches!(element, ArrayExpressionElement::SpreadElement(_)))
  {
    return ObjectSlot::Unknown;
  }
  match array.elements.get(index) {
    None | Some(ArrayExpressionElement::Elision(_)) => ObjectSlot::Missing,
    Some(ArrayExpressionElement::SpreadElement(_)) => ObjectSlot::Unknown,
    Some(element) => element.as_expression().map_or(ObjectSlot::Unknown, ObjectSlot::Value),
  }
}
