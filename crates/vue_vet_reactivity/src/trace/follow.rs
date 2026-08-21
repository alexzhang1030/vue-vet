//! Same-file zero-arg helper follow (reads / uncertain / writes / `assignment_only`).
//!
//! One callee set and one depth cap so tracking-scope collectors cannot disagree
//! on which local helpers are in the walk.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::{
  AstKind,
  ast::{Expression, IdentifierReference},
};
use oxc_semantic::NodeId;

use super::scope_context;

/// Max hops when following same-file zero-arg helpers from a tracking scope.
/// Vue tracks sync reads inside callees; we under-approx with a small bound.
pub(super) const MAX_LOCAL_CALLEE_FOLLOW_DEPTH: u32 = 2;

/// Same-file zero-arg local helpers called from `scope_id`.
///
/// Each entry is `(callee_id, all_in_scope_call_sites_are_outside_tracking)`.
/// [`follow_local_callees`] is the only consumer so reads / uncertain / writes
/// cannot disagree on the callee set. `assignment_only` walks statements but
/// uses the same [`local_function_id`] + async skip.
fn local_zero_arg_callees_in_scope(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  imported_bindings: &BTreeMap<String, (String, String)>,
  visiting: &BTreeSet<NodeId>,
) -> Vec<(NodeId, bool)> {
  // Collect callee ids first so callers do not hold a nodes borrow across recursion.
  // `call_outside` is true only when *every* in-scope call site is outside tracking
  // (any in-tracking call keeps ambient deps / soft evidence).
  let mut callees: Vec<(NodeId, bool)> = Vec::new();
  for (call_id, call_node) in semantic.nodes().iter_enumerated() {
    let AstKind::CallExpression(call) = call_node.kind() else {
      continue;
    };
    if !call.arguments.is_empty() {
      continue;
    }
    let Some(identifier) = call.callee.get_identifier_reference() else {
      continue;
    };
    let Some((_, call_outside)) =
      scope_context(semantic, scope_id, call_id, call.span, imported_bindings)
    else {
      continue;
    };
    let Some(callee_id) = local_function_id(semantic, identifier) else {
      continue;
    };
    if is_async_or_generator_function(semantic, callee_id) {
      continue;
    }
    if visiting.contains(&callee_id) {
      continue;
    }
    if let Some((_, existing_outside)) = callees.iter_mut().find(|(id, _)| *id == callee_id) {
      *existing_outside = *existing_outside && call_outside;
      continue;
    }
    callees.push((callee_id, call_outside));
  }
  callees
}

/// What to do with helpers whose *every* in-scope call is outside tracking
/// (`then()` / `nextTick`). Reads mark nested facts; writes / uncertain skip
/// so we do not invent computed side-effects or maybe-deps.
#[derive(Clone, Copy)]
pub(super) enum FollowOutside {
  Mark,
  Skip,
}

/// Shared walk for hard reads, `uncertain_accesses`, and writes.
pub(super) fn follow_local_callees(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  imported_bindings: &BTreeMap<String, (String, String)>,
  depth: u32,
  visiting: &mut BTreeSet<NodeId>,
  outside: FollowOutside,
  mut visit: impl FnMut(NodeId, bool, u32, &mut BTreeSet<NodeId>),
) {
  if depth >= MAX_LOCAL_CALLEE_FOLLOW_DEPTH {
    return;
  }
  let callees = local_zero_arg_callees_in_scope(semantic, scope_id, imported_bindings, visiting);
  for (callee_id, call_outside) in callees {
    if matches!(outside, FollowOutside::Skip) && call_outside {
      continue;
    }
    if !visiting.insert(callee_id) {
      continue;
    }
    visit(callee_id, call_outside, depth.saturating_add(1), visiting);
    visiting.remove(&callee_id);
  }
}

/// Resolve a same-file local `function f` / `const f = () =>` / `function` expr
/// from an identifier reference. Imports and non-function bindings return `None`.
pub(super) fn local_function_id(
  semantic: &oxc_semantic::Semantic<'_>,
  reference: &IdentifierReference<'_>,
) -> Option<NodeId> {
  let reference_id = reference.reference_id.get()?;
  let symbol_id = semantic.scoping().get_reference(reference_id).symbol_id()?;
  let decl = semantic.symbol_declaration(symbol_id);
  match decl.kind() {
    AstKind::Function(function) => Some(function.node_id.get()),
    AstKind::VariableDeclarator(declarator) => match &declarator.init {
      Some(Expression::ArrowFunctionExpression(arrow)) => Some(arrow.node_id.get()),
      Some(Expression::FunctionExpression(function)) => Some(function.node_id.get()),
      _ => None,
    },
    // `function useX()` binds on the Function node; some paths surface the id binding.
    AstKind::BindingIdentifier(_) => {
      for ancestor_id in semantic.nodes().ancestor_ids(decl.id()) {
        match semantic.nodes().kind(ancestor_id) {
          AstKind::Function(function) => return Some(function.node_id.get()),
          AstKind::VariableDeclarator(declarator) => {
            return match &declarator.init {
              Some(Expression::ArrowFunctionExpression(arrow)) => Some(arrow.node_id.get()),
              Some(Expression::FunctionExpression(function)) => Some(function.node_id.get()),
              _ => None,
            };
          }
          _ => {}
        }
      }
      None
    }
    _ => None,
  }
}

pub(super) fn is_async_or_generator_function(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
) -> bool {
  match semantic.nodes().kind(function_id) {
    AstKind::Function(function) => function.r#async || function.generator,
    AstKind::ArrowFunctionExpression(arrow) => arrow.r#async,
    // Unknown shape: refuse to follow (under-approx).
    _ => true,
  }
}
