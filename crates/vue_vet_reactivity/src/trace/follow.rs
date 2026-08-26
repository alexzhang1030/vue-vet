//! Same-file zero-arg helper follow (reads / uncertain / writes / `assignment_only`).
//!
//! One callee set and one depth cap so tracking-scope collectors cannot disagree
//! on which local helpers are in the walk. Identifier getters (`computed(load)`)
//! are scope discovery in [`super::writes::local_getter_parts`], not a second
//! follow walk.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::{
  AstKind,
  ast::{Expression, IdentifierReference},
};
use oxc_semantic::{NodeId, Semantic};

use super::context::scope_context;

/// Max hops when following same-file zero-arg helpers from a tracking scope.
/// Vue tracks sync reads inside callees; we under-approx with a small bound.
pub(super) const MAX_LOCAL_CALLEE_FOLLOW_DEPTH: u32 = 2;

/// One same-file zero-arg helper reached from a tracking (or helper) scope.
pub(super) struct LocalCallee {
  id: NodeId,
  /// True only when *every* in-scope call site is outside tracking.
  call_outside: bool,
  /// `CallExpression` nodes in `scope_id` that invoke this helper.
  call_sites: Vec<NodeId>,
}

/// Same-file zero-arg local helpers called from `scope_id`.
///
/// [`finish_scope`] computes this once for the tracking root and hands the
/// same slice to reads / uncertain / writes. Nested hops still rediscover.
/// `assignment_only` walks statements but uses the same [`local_function_id`]
/// + async skip.
pub(super) fn local_zero_arg_callees_in_scope(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  imported_bindings: &BTreeMap<String, (String, String)>,
  visiting: &BTreeSet<NodeId>,
) -> Vec<LocalCallee> {
  // Collect callee ids first so callers do not hold a nodes borrow across recursion.
  // `call_outside` is true only when *every* in-scope call site is outside tracking
  // (any in-tracking call keeps ambient deps / soft evidence).
  let mut callees: Vec<LocalCallee> = Vec::new();
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
    if let Some(existing) = callees.iter_mut().find(|callee| callee.id == callee_id) {
      existing.call_outside = existing.call_outside && call_outside;
      existing.call_sites.push(call_id);
      continue;
    }
    callees.push(LocalCallee { id: callee_id, call_outside, call_sites: vec![call_id] });
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
///
/// `root_callees` skips rediscovery at the tracking-scope root. Nested hops
/// pass `None` and walk nodes again (visiting set differs).
pub(super) fn follow_local_callees(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  imported_bindings: &BTreeMap<String, (String, String)>,
  depth: u32,
  visiting: &mut BTreeSet<NodeId>,
  outside: FollowOutside,
  root_callees: Option<&[LocalCallee]>,
  mut visit: impl FnMut(NodeId, bool, u32, &[NodeId], &mut BTreeSet<NodeId>),
) {
  if depth >= MAX_LOCAL_CALLEE_FOLLOW_DEPTH {
    return;
  }
  let discovered;
  let callees = if let Some(ready) = root_callees {
    ready
  } else {
    discovered = local_zero_arg_callees_in_scope(semantic, scope_id, imported_bindings, visiting);
    &discovered
  };
  for callee in callees {
    if matches!(outside, FollowOutside::Skip) && callee.call_outside {
      continue;
    }
    if !visiting.insert(callee.id) {
      continue;
    }
    visit(callee.id, callee.call_outside, depth.saturating_add(1), &callee.call_sites, visiting);
    visiting.remove(&callee.id);
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

/// Innermost `function` / arrow containing `node_id` (or the node itself).
pub(super) fn innermost_function_id(semantic: &Semantic<'_>, node_id: NodeId) -> Option<NodeId> {
  if matches!(
    semantic.nodes().kind(node_id),
    AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
  ) {
    return Some(node_id);
  }
  semantic.nodes().ancestor_ids(node_id).find(|&ancestor_id| {
    matches!(
      semantic.nodes().kind(ancestor_id),
      AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
    )
  })
}

/// Zero-arg same-file helper calls grouped by the function that contains them.
///
/// Pause/resume is process-global in Vue; classify projects a callee's last
/// pause event onto each call end so later sibling reads in the caller see it.
pub(super) fn local_helper_calls_by_owner(
  semantic: &Semantic<'_>,
) -> BTreeMap<NodeId, Vec<(u32, NodeId)>> {
  let mut calls: BTreeMap<NodeId, Vec<(u32, NodeId)>> = BTreeMap::new();
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
    let Some(callee_id) = local_function_id(semantic, identifier) else {
      continue;
    };
    if is_async_or_generator_function(semantic, callee_id) {
      continue;
    }
    let Some(owner) = innermost_function_id(semantic, call_id) else {
      continue;
    };
    calls.entry(owner).or_default().push((call.span.end, callee_id));
  }
  calls
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
