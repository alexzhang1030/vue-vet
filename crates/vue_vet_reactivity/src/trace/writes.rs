//! Tracking-scope writes and assignment-only body classification.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::ast::PropertyKind;
use oxc_ast::{
  AstKind,
  ast::{
    Argument, AssignmentTarget, Expression, FunctionBody, IdentifierReference, ObjectPropertyKind,
    PropertyKey, SimpleAssignmentTarget, Statement,
  },
};
use oxc_semantic::NodeId;
use oxc_span::Span;
use vue_vet_core::{ReactiveBindingFact, ReactiveWriteFact};

use super::{
  expr,
  follow::{
    FollowOutside, MAX_LOCAL_CALLEE_FOLLOW_DEPTH, follow_local_callees,
    is_async_or_generator_function, local_function_id,
  },
  kinds::{reference_resolves_to_binding, source_span},
};

pub(super) fn callback_parts<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  argument: &'a Argument<'a>,
) -> Option<(NodeId, Option<&'a FunctionBody<'a>>)> {
  match argument {
    Argument::ArrowFunctionExpression(callback) => {
      Some((callback.node_id.get(), Some(&*callback.body)))
    }
    Argument::FunctionExpression(callback) => {
      Some((callback.node_id.get(), callback.body.as_deref()))
    }
    other => other.as_expression().and_then(|expression| local_getter_parts(semantic, expression)),
  }
}

/// Function/arrow body for a tracking scope, including `computed({ get, set })`.
pub(super) fn tracking_callback_parts<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  argument: &'a Argument<'a>,
) -> Option<(NodeId, Option<&'a FunctionBody<'a>>)> {
  if let Some(parts) = callback_parts(semantic, argument) {
    return Some(parts);
  }
  let expression = argument.as_expression()?;
  let Expression::ObjectExpression(object) = expression else {
    return None;
  };
  for property in &object.properties {
    let ObjectPropertyKind::ObjectProperty(property) = property else {
      continue;
    };
    // Prefer explicit getters; also accept `get: () => …` / `get() { … }` / `get: load`.
    let is_get = property.kind == PropertyKind::Get || property_key_is_name(&property.key, "get");
    if !is_get {
      continue;
    }
    return match &property.value {
      Expression::FunctionExpression(callback) => {
        Some((callback.node_id.get(), callback.body.as_deref()))
      }
      Expression::ArrowFunctionExpression(callback) => {
        Some((callback.node_id.get(), Some(&*callback.body)))
      }
      other => local_getter_parts(semantic, other),
    };
  }
  None
}

/// Same-file `function f` / `const f = () =>` passed by reference to a Vue
/// tracking API (`computed(load)`, `watchEffect(load)`, `watch(load)`,
/// `computed({ get: load })`). Imports, methods, and async/generator stay quiet.
pub(super) fn local_getter_parts<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  expression: &'a Expression<'a>,
) -> Option<(NodeId, Option<&'a FunctionBody<'a>>)> {
  let identifier = expr::peel_parens(expression).get_identifier_reference()?;
  let function_id = local_function_id(semantic, identifier)?;
  if is_async_or_generator_function(semantic, function_id) {
    return None;
  }
  Some((function_id, function_body_of(semantic, function_id)))
}

pub(super) fn property_key_is_name(key: &PropertyKey<'_>, name: &str) -> bool {
  match key {
    PropertyKey::StaticIdentifier(identifier) => identifier.name.as_str() == name,
    PropertyKey::StringLiteral(literal) => literal.value.as_str() == name,
    _ => false,
  }
}

pub(super) fn is_assignment_only_body(body: Option<&FunctionBody<'_>>) -> bool {
  let Some(body) = body else {
    return false;
  };
  if body.statements.is_empty() {
    return false;
  }
  body.statements.iter().all(|statement| match statement {
    Statement::ExpressionStatement(expression) => {
      matches!(
        expr::peel_parens(&expression.expression),
        Expression::AssignmentExpression(_) | Expression::UpdateExpression(_)
      )
    }
    Statement::EmptyStatement(_) => true,
    _ => false,
  })
}

pub(super) fn function_body_of<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  function_id: NodeId,
) -> Option<&'a FunctionBody<'a>> {
  match semantic.nodes().kind(function_id) {
    AstKind::Function(function) => function.body.as_deref(),
    AstKind::ArrowFunctionExpression(arrow) => Some(&*arrow.body),
    _ => None,
  }
}

/// Dual-path with [`is_assignment_only_body`]: a body is assignment-only when
/// every statement is an assignment, empty, or a same-file zero-arg helper
/// whose body is itself assignment-only (depth-capped).
pub(super) fn is_assignment_only_followed(
  semantic: &oxc_semantic::Semantic<'_>,
  body: Option<&FunctionBody<'_>>,
  depth: u32,
  visiting: &mut BTreeSet<NodeId>,
) -> bool {
  if is_assignment_only_body(body) {
    return true;
  }
  let Some(body) = body else {
    return false;
  };
  if body.statements.is_empty() || depth >= MAX_LOCAL_CALLEE_FOLLOW_DEPTH {
    return false;
  }
  body.statements.iter().all(|statement| match statement {
    Statement::EmptyStatement(_) => true,
    Statement::ExpressionStatement(expression) => {
      statement_is_assignment_or_followed_helper(semantic, &expression.expression, depth, visiting)
    }
    _ => false,
  })
}

pub(super) fn statement_is_assignment_or_followed_helper(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  depth: u32,
  visiting: &mut BTreeSet<NodeId>,
) -> bool {
  match expr::peel_parens(expression) {
    Expression::AssignmentExpression(_) | Expression::UpdateExpression(_) => true,
    Expression::CallExpression(call) if call.arguments.is_empty() => {
      let Some(identifier) = call.callee.get_identifier_reference() else {
        return false;
      };
      let Some(callee_id) = local_function_id(semantic, identifier) else {
        return false;
      };
      if is_async_or_generator_function(semantic, callee_id) || !visiting.insert(callee_id) {
        return false;
      }
      let ok = is_assignment_only_followed(
        semantic,
        function_body_of(semantic, callee_id),
        depth.saturating_add(1),
        visiting,
      );
      visiting.remove(&callee_id);
      ok
    }
    _ => false,
  }
}

pub(super) fn collect_scope_writes(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  reactive_bindings: &[ReactiveBindingFact],
  imported_bindings: &BTreeMap<String, (String, String)>,
  sfc_source: &str,
  script_offset: usize,
) -> Vec<ReactiveWriteFact> {
  let mut visiting = BTreeSet::new();
  visiting.insert(scope_id);
  let mut writes = collect_scope_writes_bounded(
    semantic,
    scope_id,
    reactive_bindings,
    imported_bindings,
    sfc_source,
    script_offset,
    0,
    &mut visiting,
  );
  writes.sort_by_key(|write| write.span.offset);
  writes
}

#[expect(clippy::too_many_arguments, reason = "bounded collector threads scope + visit state")]
pub(super) fn collect_scope_writes_bounded(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  reactive_bindings: &[ReactiveBindingFact],
  imported_bindings: &BTreeMap<String, (String, String)>,
  sfc_source: &str,
  script_offset: usize,
  depth: u32,
  visiting: &mut BTreeSet<NodeId>,
) -> Vec<ReactiveWriteFact> {
  let mut writes =
    collect_scope_writes_local(semantic, scope_id, reactive_bindings, sfc_source, script_offset);
  follow_local_callees(
    semantic,
    scope_id,
    imported_bindings,
    depth,
    visiting,
    FollowOutside::Skip,
    |callee_id, _, next_depth, _, visiting| {
      writes.extend(collect_scope_writes_bounded(
        semantic,
        callee_id,
        reactive_bindings,
        imported_bindings,
        sfc_source,
        script_offset,
        next_depth,
        visiting,
      ));
    },
  );
  writes
}

pub(super) fn collect_scope_writes_local(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  reactive_bindings: &[ReactiveBindingFact],
  sfc_source: &str,
  script_offset: usize,
) -> Vec<ReactiveWriteFact> {
  let mut writes = Vec::new();
  for node in semantic.nodes() {
    let Some((object, property, write_span)) = write_target_from_node(node.kind()) else {
      continue;
    };

    let mut reached_scope = false;
    let mut nested_function = false;
    for ancestor_id in semantic.nodes().ancestor_ids(node.id()) {
      if ancestor_id == scope_id {
        reached_scope = true;
        break;
      }
      if matches!(
        semantic.nodes().kind(ancestor_id),
        AstKind::ArrowFunctionExpression(_) | AstKind::Function(_)
      ) {
        nested_function = true;
        break;
      }
    }
    if !reached_scope || nested_function {
      continue;
    }

    let Some(binding) = reactive_bindings.iter().find(|binding| {
      binding.name == object.name.as_str()
        && reference_resolves_to_binding(semantic, object, binding, script_offset)
        && (!binding.kind.is_ref_like() || property.as_deref() == Some("value"))
    }) else {
      continue;
    };
    writes.push(ReactiveWriteFact {
      binding: binding.name.clone(),
      property,
      span: source_span(sfc_source, script_offset, write_span),
    });
  }
  writes
}

/// `=` / `+=` / `++` member writes. Logical `&&=` / `||=` / `??=` stay quiet
/// (they may not write).
fn write_target_from_node(
  kind: AstKind<'_>,
) -> Option<(&IdentifierReference<'_>, Option<String>, Span)> {
  match kind {
    AstKind::AssignmentExpression(assignment) if !assignment.operator.is_logical() => {
      member_write_from_assignment_target(&assignment.left)
    }
    AstKind::UpdateExpression(update) => member_write_from_simple_target(&update.argument),
    _ => None,
  }
}

fn member_write_from_assignment_target<'a>(
  target: &'a AssignmentTarget<'a>,
) -> Option<(&'a IdentifierReference<'a>, Option<String>, Span)> {
  match target {
    AssignmentTarget::StaticMemberExpression(member) => Some((
      member.object.get_identifier_reference()?,
      Some(member.property.name.to_string()),
      member.span,
    )),
    AssignmentTarget::ComputedMemberExpression(member) => Some((
      member.object.get_identifier_reference()?,
      member.static_property_name().map(|name| name.to_string()),
      member.span,
    )),
    _ => None,
  }
}

fn member_write_from_simple_target<'a>(
  target: &'a SimpleAssignmentTarget<'a>,
) -> Option<(&'a IdentifierReference<'a>, Option<String>, Span)> {
  match target {
    SimpleAssignmentTarget::StaticMemberExpression(member) => Some((
      member.object.get_identifier_reference()?,
      Some(member.property.name.to_string()),
      member.span,
    )),
    SimpleAssignmentTarget::ComputedMemberExpression(member) => Some((
      member.object.get_identifier_reference()?,
      member.static_property_name().map(|name| name.to_string()),
      member.span,
    )),
    _ => None,
  }
}
