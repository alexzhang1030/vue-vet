//! Whether a member/call sits inside a tracking scope (HOF / toValue / deferred).
//!
//! [`scope_context`] (reads / uncertain) and [`sync_tracking_owns_node`] (writes)
//! share HOF / `toValue` / deferred classification so they cannot disagree on
//! whether a nested callback still runs in the parent tracking flush.
//!
//! [`ScopeNodeIndex`] is the file-level inverse: one walk, two maps. Do not
//! unify them — deferred callbacks stay in context (outside) and drop from sync.

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
#[cfg(test)]
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

#[cfg(test)]
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

/// Inverse of [`scope_context`]: first Function/Arrow that is not a sync HOF /
/// `toValue` getter. Deferred ancestors set `outside_tracking` and continue.
/// A node on an assignment left has no owner.
pub(super) fn tracking_context_owner(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  span: Span,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> Option<(NodeId, bool)> {
  let mut outside_tracking = false;
  let mut write_only = false;
  for ancestor_id in semantic.nodes().ancestor_ids(node_id) {
    match semantic.nodes().kind(ancestor_id) {
      AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
        if is_deferred_callback_container(semantic, ancestor_id) {
          outside_tracking = true;
          continue;
        }
        if is_sync_hof_callback(semantic, ancestor_id)
          || is_to_value_getter_callback(semantic, ancestor_id, imported_bindings)
        {
          continue;
        }
        if write_only {
          return None;
        }
        return Some((ancestor_id, outside_tracking));
      }
      AstKind::AssignmentExpression(assignment)
        if assignment.operator.is_assign() && span_contains(assignment.left.span(), span) =>
      {
        write_only = true;
      }
      _ => {}
    }
  }
  None
}

/// Inverse of [`sync_tracking_owns_node`]: first Function/Arrow that is not a
/// sync HOF / `toValue` getter. Deferred ancestors have no sync owner.
pub(super) fn tracking_sync_owner(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> Option<NodeId> {
  for ancestor_id in semantic.nodes().ancestor_ids(node_id) {
    match semantic.nodes().kind(ancestor_id) {
      AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
        if is_deferred_callback_container(semantic, ancestor_id) {
          return None;
        }
        if is_sync_hof_callback(semantic, ancestor_id)
          || is_to_value_getter_callback(semantic, ancestor_id, imported_bindings)
        {
          continue;
        }
        return Some(ancestor_id);
      }
      _ => {}
    }
  }
  None
}

/// One node in a tracking (or helper) function, with the deferred-callback flag
/// [`scope_context`] would have returned.
#[derive(Clone, Copy)]
pub(super) struct OwnedNode {
  pub id: NodeId,
  pub outside: bool,
}

/// File-level local-walk index. Members / idents / calls use the context map;
/// assignment / update writes use the sync map. Await / pause IR stay separate.
pub(super) struct ScopeNodeIndex {
  members: BTreeMap<NodeId, Vec<OwnedNode>>,
  idents: BTreeMap<NodeId, Vec<OwnedNode>>,
  calls: BTreeMap<NodeId, Vec<OwnedNode>>,
  writes: BTreeMap<NodeId, Vec<NodeId>>,
}

impl ScopeNodeIndex {
  pub(super) fn build(
    semantic: &oxc_semantic::Semantic<'_>,
    imported_bindings: &BTreeMap<String, (String, String)>,
  ) -> Self {
    let mut members: BTreeMap<NodeId, Vec<OwnedNode>> = BTreeMap::new();
    let mut idents: BTreeMap<NodeId, Vec<OwnedNode>> = BTreeMap::new();
    let mut calls: BTreeMap<NodeId, Vec<OwnedNode>> = BTreeMap::new();
    let mut writes: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    for (node_id, node) in semantic.nodes().iter_enumerated() {
      match node.kind() {
        AstKind::StaticMemberExpression(member) => {
          if let Some((owner, outside)) =
            tracking_context_owner(semantic, node_id, member.span, imported_bindings)
          {
            members.entry(owner).or_default().push(OwnedNode { id: node_id, outside });
          }
        }
        AstKind::ComputedMemberExpression(member) => {
          if let Some((owner, outside)) =
            tracking_context_owner(semantic, node_id, member.span, imported_bindings)
          {
            members.entry(owner).or_default().push(OwnedNode { id: node_id, outside });
          }
        }
        AstKind::IdentifierReference(identifier) => {
          if let Some((owner, outside)) =
            tracking_context_owner(semantic, node_id, identifier.span, imported_bindings)
          {
            idents.entry(owner).or_default().push(OwnedNode { id: node_id, outside });
          }
        }
        AstKind::CallExpression(call) => {
          if let Some((owner, outside)) =
            tracking_context_owner(semantic, node_id, call.span, imported_bindings)
          {
            calls.entry(owner).or_default().push(OwnedNode { id: node_id, outside });
          }
        }
        AstKind::AssignmentExpression(_) | AstKind::UpdateExpression(_) => {
          if let Some(owner) = tracking_sync_owner(semantic, node_id, imported_bindings) {
            writes.entry(owner).or_default().push(node_id);
          }
        }
        _ => {}
      }
    }
    Self { members, idents, calls, writes }
  }

  pub(super) fn members(&self, scope_id: NodeId) -> &[OwnedNode] {
    self.members.get(&scope_id).map_or(&[], Vec::as_slice)
  }

  pub(super) fn idents(&self, scope_id: NodeId) -> &[OwnedNode] {
    self.idents.get(&scope_id).map_or(&[], Vec::as_slice)
  }

  pub(super) fn calls(&self, scope_id: NodeId) -> &[OwnedNode] {
    self.calls.get(&scope_id).map_or(&[], Vec::as_slice)
  }

  pub(super) fn writes(&self, scope_id: NodeId) -> &[NodeId] {
    self.writes.get(&scope_id).map_or(&[], Vec::as_slice)
  }
}

#[cfg(test)]
mod node_index_equiv {
  use oxc_allocator::Allocator;
  use oxc_parser::Parser;
  use oxc_semantic::SemanticBuilder;
  use oxc_span::SourceType;

  use super::*;
  use crate::trace::kinds::collect_imported_bindings;

  fn is_transparent_callback(
    semantic: &oxc_semantic::Semantic<'_>,
    function_id: NodeId,
    imported_bindings: &BTreeMap<String, (String, String)>,
  ) -> bool {
    is_deferred_callback_container(semantic, function_id)
      || is_sync_hof_callback(semantic, function_id)
      || is_to_value_getter_callback(semantic, function_id, imported_bindings)
  }

  #[test]
  fn file_node_index_matches_scope_walks() {
    let source = "\
import { ref, computed, toValue } from 'vue';
const x = ref(0);
const bag = { field: x };
const items = ref([1]);
function inner() { x.value = 1; return x.value; }
function load() { return inner(); }
const c = computed(() => {
  items.value.map(() => { x.value += 1; return load(); });
  Promise.resolve().then(() => { x.value = 2; return x.value; });
  toValue(() => x.value);
  const n = x.value;
  return n;
});
void c.value;
void bag.field.value;
";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let built = SemanticBuilder::new().with_build_nodes(true).build(&parsed.program);
    assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
    let semantic = &built.semantic;
    let imported = collect_imported_bindings(semantic);
    let index = ScopeNodeIndex::build(semantic, &imported);

    let mut function_ids = Vec::new();
    for (id, node) in semantic.nodes().iter_enumerated() {
      if matches!(node.kind(), AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)) {
        function_ids.push(id);
      }
    }

    for &scope_id in &function_ids {
      if is_transparent_callback(semantic, scope_id, &imported) {
        continue;
      }

      let mut walked_members = Vec::new();
      let mut walked_idents = Vec::new();
      let mut walked_calls = Vec::new();
      let mut walked_writes = Vec::new();
      for (node_id, node) in semantic.nodes().iter_enumerated() {
        match node.kind() {
          AstKind::StaticMemberExpression(member) => {
            if let Some((_, outside)) =
              scope_context(semantic, scope_id, node_id, member.span, &imported)
            {
              walked_members.push((node_id, outside));
            }
          }
          AstKind::ComputedMemberExpression(member) => {
            if let Some((_, outside)) =
              scope_context(semantic, scope_id, node_id, member.span, &imported)
            {
              walked_members.push((node_id, outside));
            }
          }
          AstKind::IdentifierReference(identifier) => {
            if let Some((_, outside)) =
              scope_context(semantic, scope_id, node_id, identifier.span, &imported)
            {
              walked_idents.push((node_id, outside));
            }
          }
          AstKind::CallExpression(call) => {
            if let Some((_, outside)) =
              scope_context(semantic, scope_id, node_id, call.span, &imported)
            {
              walked_calls.push((node_id, outside));
            }
          }
          AstKind::AssignmentExpression(_) | AstKind::UpdateExpression(_)
            if sync_tracking_owns_node(semantic, scope_id, node_id, &imported) =>
          {
            walked_writes.push(node_id);
          }
          _ => {}
        }
      }

      let indexed_members: Vec<(NodeId, bool)> =
        index.members(scope_id).iter().map(|node| (node.id, node.outside)).collect();
      let indexed_idents: Vec<(NodeId, bool)> =
        index.idents(scope_id).iter().map(|node| (node.id, node.outside)).collect();
      let indexed_calls: Vec<(NodeId, bool)> =
        index.calls(scope_id).iter().map(|node| (node.id, node.outside)).collect();
      let indexed_writes: Vec<NodeId> = index.writes(scope_id).to_vec();

      assert_eq!(walked_members, indexed_members, "members scope={scope_id:?}");
      assert_eq!(walked_idents, indexed_idents, "idents scope={scope_id:?}");
      assert_eq!(walked_calls, indexed_calls, "calls scope={scope_id:?}");
      assert_eq!(walked_writes, indexed_writes, "writes scope={scope_id:?}");
    }
  }
}
