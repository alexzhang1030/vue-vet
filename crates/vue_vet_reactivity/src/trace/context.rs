//! Whether a member/call sits inside a tracking scope (HOF / toValue / deferred).
//!
//! [`scope_context`] (reads / uncertain) and [`sync_tracking_owns_node`] (writes)
//! share HOF / `toValue` / deferred classification so they cannot disagree on
//! whether a nested callback still runs in the parent tracking flush.

use std::collections::BTreeMap;

use oxc_ast::{
  AstKind,
  ast::{Argument, Expression, IdentifierReference},
};
use oxc_semantic::NodeId;
use oxc_span::{GetSpan, Span};
use vue_vet_core::ScriptKind;

use super::kinds::{resolved_vue_callee, span_contains};

/// True when `function_id` is a callback argument to a known **synchronously**
/// invoked higher-order method (Array extras, etc.).
///
/// Callback argument **index** is callee-specific: prototype HOF methods take the
/// callback at index 0; `replace`/`replaceAll`/`Array.from`/`JSON.parse` take it
/// at index 1. Matching any-argument would invent deps (e.g. `Array.from(() => x)`).
pub(super) fn is_sync_hof_callback(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
) -> bool {
  let function_span = match semantic.nodes().kind(function_id) {
    AstKind::ArrowFunctionExpression(callback) => callback.span,
    AstKind::Function(function) => function.span,
    _ => return false,
  };
  for ancestor_id in semantic.nodes().ancestor_ids(function_id) {
    let AstKind::CallExpression(call) = semantic.nodes().kind(ancestor_id) else {
      if matches!(
        semantic.nodes().kind(ancestor_id),
        AstKind::ArrowFunctionExpression(_) | AstKind::Function(_)
      ) {
        return false;
      }
      continue;
    };
    let Some(arg_index) = call.arguments.iter().position(|argument| match argument {
      Argument::ArrowFunctionExpression(callback) => callback.span == function_span,
      Argument::FunctionExpression(function) => function.span == function_span,
      _ => false,
    }) else {
      continue;
    };
    return is_sync_hof_at_arg(&call.callee, arg_index);
  }
  false
}

/// True when `reference` resolves to a formal parameter of a sync HOF callback.
pub(super) fn is_sync_hof_callback_param(
  semantic: &oxc_semantic::Semantic<'_>,
  reference: &IdentifierReference<'_>,
) -> bool {
  let Some(reference_id) = reference.reference_id.get() else {
    return false;
  };
  let Some(symbol_id) = semantic.scoping().get_reference(reference_id).symbol_id() else {
    return false;
  };
  let decl = semantic.symbol_declaration(symbol_id);
  let mut saw_formal = false;
  for ancestor_id in std::iter::once(decl.id()).chain(semantic.nodes().ancestor_ids(decl.id())) {
    match semantic.nodes().kind(ancestor_id) {
      AstKind::FormalParameter(_) => {
        saw_formal = true;
      }
      AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) if saw_formal => {
        return is_sync_hof_callback(semantic, ancestor_id);
      }
      AstKind::VariableDeclarator(_) | AstKind::VariableDeclaration(_) if !saw_formal => {
        return false;
      }
      _ => {}
    }
  }
  false
}

/// Whether a function at `arg_index` of a call with this `callee` runs synchronously
/// during the parent tracking flush.
pub(super) fn is_sync_hof_at_arg(callee: &Expression<'_>, arg_index: usize) -> bool {
  // Prototype methods: callback is the first argument (index 0).
  // `reduce`/`reduceRight` also place the reducer at 0 (init is optional 2nd).
  const PROTO_CALLBACK_AT_0: &[&str] = &[
    "filter",
    "map",
    "forEach",
    "reduce",
    "reduceRight",
    "some",
    "every",
    "find",
    "findIndex",
    "findLast",
    "findLastIndex",
    "flatMap",
    "sort",
    "toSorted",
    "toSpliced",
  ];
  // Replacer / mapFn / reviver is the *second* argument (index 1).
  const CALLBACK_AT_1: &[&str] = &["replace", "replaceAll"];

  match callee {
    Expression::StaticMemberExpression(member) => {
      let method = member.property.name.as_str();
      if PROTO_CALLBACK_AT_0.contains(&method) {
        return arg_index == 0;
      }
      if CALLBACK_AT_1.contains(&method) {
        return arg_index == 1;
      }
      // Well-known statics only — bare `.from` / `.parse` on unknown receivers may be async.
      // mapFn / reviver is always the second argument.
      if arg_index == 1
        && let Expression::Identifier(object) = &member.object
      {
        return matches!((object.name.as_str(), method), ("Array", "from") | ("JSON", "parse"));
      }
      false
    }
    Expression::ComputedMemberExpression(member) => member
      .static_property_name()
      .is_some_and(|name| PROTO_CALLBACK_AT_0.contains(&name.as_str()) && arg_index == 0),
    _ => false,
  }
}

/// True when `function_id` is the first argument to Vue `toValue(...)`.
///
/// Runtime `toValue` calls function sources immediately (`isFunction(source) ?
/// source() : unref(source)`), so reactive reads inside the getter stay in the
/// parent tracking scope.
pub(super) fn is_to_value_getter_callback(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> bool {
  let function_span = match semantic.nodes().kind(function_id) {
    AstKind::ArrowFunctionExpression(callback) => callback.span,
    AstKind::Function(function) => function.span,
    _ => return false,
  };
  for ancestor_id in semantic.nodes().ancestor_ids(function_id) {
    let AstKind::CallExpression(call) = semantic.nodes().kind(ancestor_id) else {
      if matches!(
        semantic.nodes().kind(ancestor_id),
        AstKind::ArrowFunctionExpression(_) | AstKind::Function(_)
      ) {
        return false;
      }
      continue;
    };
    let is_first_argument = call.arguments.first().is_some_and(|argument| match argument {
      Argument::ArrowFunctionExpression(callback) => callback.span == function_span,
      Argument::FunctionExpression(function) => function.span == function_span,
      _ => false,
    });
    if !is_first_argument {
      continue;
    }
    return resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Setup)
      .as_deref()
      == Some("toValue");
  }
  false
}

pub(super) fn is_deferred_callback_container(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
) -> bool {
  let function_span = match semantic.nodes().kind(function_id) {
    AstKind::ArrowFunctionExpression(callback) => callback.span,
    AstKind::Function(function) => function.span,
    _ => return false,
  };
  for ancestor_id in semantic.nodes().ancestor_ids(function_id) {
    let AstKind::CallExpression(call) = semantic.nodes().kind(ancestor_id) else {
      if matches!(
        semantic.nodes().kind(ancestor_id),
        AstKind::ArrowFunctionExpression(_) | AstKind::Function(_)
      ) {
        return false;
      }
      continue;
    };
    let is_argument = call.arguments.iter().any(|argument| match argument {
      Argument::ArrowFunctionExpression(callback) => callback.span == function_span,
      Argument::FunctionExpression(function) => function.span == function_span,
      _ => false,
    });
    if !is_argument {
      continue;
    }
    return match &call.callee {
      Expression::StaticMemberExpression(member) => {
        matches!(member.property.name.as_str(), "then" | "catch" | "finally" | "nextTick")
      }
      Expression::Identifier(identifier) => {
        matches!(identifier.name.as_str(), "nextTick" | "queueMicrotask" | "setTimeout")
      }
      _ => false,
    };
  }
  false
}

/// True when `node_id` sits in `scope_id`'s synchronous tracking body.
///
/// Dual-path with [`scope_context`]: sync Array/String/`toValue` callbacks
/// stay in; `then` / `nextTick` / `setTimeout` and other nested functions
/// drop. Helper follow covers same-file zero-arg locals separately.
pub(super) fn sync_tracking_owns_node(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  node_id: NodeId,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> bool {
  for ancestor_id in semantic.nodes().ancestor_ids(node_id) {
    if ancestor_id == scope_id {
      return true;
    }
    match semantic.nodes().kind(ancestor_id) {
      AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
        if is_deferred_callback_container(semantic, ancestor_id) {
          return false;
        }
        if is_sync_hof_callback(semantic, ancestor_id)
          || is_to_value_getter_callback(semantic, ancestor_id, imported_bindings)
        {
          continue;
        }
        return false;
      }
      _ => {}
    }
  }
  false
}

pub(super) fn scope_context(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  member_id: NodeId,
  member_span: Span,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> Option<(bool, bool)> {
  let mut reached_scope = false;
  let mut outside_tracking = false;
  let mut write_only = false;
  for ancestor_id in semantic.nodes().ancestor_ids(member_id) {
    if ancestor_id == scope_id {
      reached_scope = true;
      break;
    }
    match semantic.nodes().kind(ancestor_id) {
      AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
        if is_deferred_callback_container(semantic, ancestor_id) {
          outside_tracking = true;
          continue;
        }
        // Sync higher-order callbacks (Array#filter/map/…) run during the parent
        // tracking flush, so Vue still tracks their reactive reads.
        if is_sync_hof_callback(semantic, ancestor_id) {
          continue;
        }
        // `toValue(() => count.value)` invokes the getter synchronously.
        if is_to_value_getter_callback(semantic, ancestor_id, imported_bindings) {
          continue;
        }
        return None;
      }
      AstKind::AssignmentExpression(assignment)
        if assignment.operator.is_assign()
          && span_contains(assignment.left.span(), member_span) =>
      {
        write_only = true;
      }
      _ => {}
    }
  }
  if !reached_scope || write_only {
    return None;
  }
  Some((reached_scope, outside_tracking))
}
