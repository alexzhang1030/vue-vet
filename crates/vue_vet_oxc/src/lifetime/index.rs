use std::collections::{HashMap, HashSet};

use oxc_ast::{
  AstKind,
  ast::{
    Argument, AssignmentTarget, AssignmentTargetMaybeDefault, AssignmentTargetProperty,
    BindingPattern, CallExpression, Declaration, Expression, UnaryOperator,
  },
};
use oxc_semantic::{IsGlobalReference, NodeId, SymbolId};
use oxc_span::Span;
use vue_vet_core::WatcherApiKind;

use super::resolve::{
  FunctionRef, FunctionResolver, argument_expression, call_has_spread, callback_argument_index,
  enclosing_function, export_local_symbol_id, is_global_host, is_proven_promise, referenced_symbol,
  uncertain_to_function, vue_callee_export, watcher_api,
};

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
}

#[derive(Clone, Copy)]
pub(super) struct RegisterSite {
  pub span: Span,
  pub identity: Option<NodeId>,
  pub callee: Option<SymbolId>,
  pub vue_cleanup: bool,
  pub explicit_owner: bool,
}

#[derive(Clone, Copy)]
pub(super) struct WatcherSite {
  pub span: Span,
  pub api: WatcherApiKind,
  pub callback: Option<FunctionRef>,
  pub unused: bool,
  pub node_id: NodeId,
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
  match kind {
    AstKind::CallExpression(call) => {
      record_scope_escape(semantic, call, vue_exports, index);
      record_scope_member_use(semantic, node_id, call, vue_exports, index);
      index_call(semantic, node_id, call, vue_exports, resolver, index);
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
      mark_run_mutation_target(semantic, &assignment.left, index);
      mark_value_escape(semantic, &assignment.right, vue_exports, index);
    }
    AstKind::VariableDeclarator(declarator) => {
      if let Some(init) = &declarator.init {
        mark_value_escape(semantic, init, vue_exports, index);
      }
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
    index.watchers.push(WatcherSite {
      node_id,
      span: call.span,
      api,
      callback,
      unused: call_result_unused(semantic, node_id),
    });
  }

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

  if let Some(run) = proven_run_call(semantic, call, vue_exports, resolver, index) {
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
  Some(ProvenRun { run_span: call.span, owner_span, callback, scope_symbol })
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

pub(super) fn is_effect_scope_binding(
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

fn mark_run_mutation_target(
  semantic: &oxc_semantic::Semantic<'_>,
  target: &AssignmentTarget<'_>,
  index: &mut LifetimeIndex,
) {
  match target {
    AssignmentTarget::StaticMemberExpression(member) if member.property.name.as_str() == "run" => {
      invalidate_scope_receiver(semantic, &member.object, index);
    }
    AssignmentTarget::ComputedMemberExpression(member) => {
      let Expression::StringLiteral(literal) = member.expression.get_inner_expression() else {
        return;
      };
      if literal.value.as_str() == "run" {
        invalidate_scope_receiver(semantic, &member.object, index);
      }
    }
    AssignmentTarget::ArrayAssignmentTarget(array) => {
      for element in array.elements.iter().flatten() {
        mark_run_mutation_maybe_default(semantic, element, index);
      }
      if let Some(rest) = &array.rest {
        mark_run_mutation_target(semantic, &rest.target, index);
      }
    }
    AssignmentTarget::ObjectAssignmentTarget(object) => {
      for property in &object.properties {
        if let AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) = property {
          mark_run_mutation_maybe_default(semantic, &property.binding, index);
        }
      }
      if let Some(rest) = &object.rest {
        mark_run_mutation_target(semantic, &rest.target, index);
      }
    }
    AssignmentTarget::TSAsExpression(inner) => {
      mark_run_mutation_from_expression(semantic, &inner.expression, index);
    }
    AssignmentTarget::TSSatisfiesExpression(inner) => {
      mark_run_mutation_from_expression(semantic, &inner.expression, index);
    }
    AssignmentTarget::TSNonNullExpression(inner) => {
      mark_run_mutation_from_expression(semantic, &inner.expression, index);
    }
    AssignmentTarget::TSTypeAssertion(inner) => {
      mark_run_mutation_from_expression(semantic, &inner.expression, index);
    }
    _ => {}
  }
}

fn mark_run_mutation_maybe_default(
  semantic: &oxc_semantic::Semantic<'_>,
  target: &AssignmentTargetMaybeDefault<'_>,
  index: &mut LifetimeIndex,
) {
  match target {
    AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(with_default) => {
      mark_run_mutation_target(semantic, &with_default.binding, index);
    }
    other => {
      if let Some(target) = other.as_assignment_target() {
        mark_run_mutation_target(semantic, target, index);
      }
    }
  }
}

fn mark_run_mutation_from_expression(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  index: &mut LifetimeIndex,
) {
  match expression.get_inner_expression() {
    Expression::StaticMemberExpression(member) if member.property.name.as_str() == "run" => {
      invalidate_scope_receiver(semantic, &member.object, index);
    }
    Expression::ComputedMemberExpression(member) => {
      if let Expression::StringLiteral(literal) = member.expression.get_inner_expression()
        && literal.value.as_str() == "run"
      {
        invalidate_scope_receiver(semantic, &member.object, index);
      }
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
    Expression::StaticMemberExpression(member) if member.property.name.as_str() == "run" => {
      invalidate_scope_receiver(semantic, &member.object, index);
    }
    Expression::ComputedMemberExpression(member) => {
      let Expression::StringLiteral(literal) = member.expression.get_inner_expression() else {
        return;
      };
      if literal.value.as_str() == "run" {
        invalidate_scope_receiver(semantic, &member.object, index);
      }
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
  pub(super) fn finalize_selected_runs(&mut self) -> usize {
    let mut work = 0usize;
    let unproven = &self.unproven_scopes;
    let mut selected = HashMap::new();
    for (function_id, runs) in &self.run_callbacks {
      work = work.saturating_add(runs.len());
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
}

fn call_result_unused(semantic: &oxc_semantic::Semantic<'_>, node_id: NodeId) -> bool {
  match semantic.nodes().parent_kind(node_id) {
    AstKind::ExpressionStatement(_) => true,
    AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::Void => true,
    _ => false,
  }
}
