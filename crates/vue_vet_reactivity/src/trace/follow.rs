//! Same-file zero-arg helper follow (reads / uncertain / writes / `assignment_only`).
//!
//! One callee set and one depth cap so tracking-scope collectors cannot disagree
//! on which local helpers are in the walk. Identifier getters (`computed(load)`)
//! are scope discovery in [`super::writes::local_getter_parts`], not a second
//! follow walk.
//!
//! [`LocalCalleeIndex`] walks the file once. Nested hops and watch-source getters
//! look up `full_callees(F)` and skip ids already in `visiting`. Do not cache a
//! slice keyed only on `scope_id` — that drops the visiting filter.
//!
//! [`FileTraceIndex`] also holds [`super::context::ScopeNodeIndex`] so local
//! member / ident / write / uncertain walks do not scan every node again.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::{
  AstKind,
  ast::{Expression, IdentifierReference},
};
use oxc_semantic::{NodeId, Semantic};

use super::context::{ScopeNodeIndex, is_sync_hof_at_arg, tracking_context_owner};
use super::expr::peel_parens;
use super::kinds::{identifier_reference_is_unresolved, resolved_vue_callee};
use vue_vet_core::ScriptKind;

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

/// File-level `full_callees(F)`: zero-arg local helpers owned by function `F`.
///
/// Owner is the inverse of [`super::context::scope_context`]: first ancestor
/// Function/Arrow that is not a sync HOF / `toValue` getter. Deferred
/// (`then` / `nextTick`) ancestors set `call_outside` and continue. A call
/// on an assignment left has no owner.
///
/// `effective(F, visiting) = full_callees(F).filter(|c| !visiting.contains(c.id))`.
/// Apply that filter at use time — do not store a slice keyed only on `F`.
pub(super) struct LocalCalleeIndex {
  by_scope: BTreeMap<NodeId, Vec<LocalCallee>>,
}

impl LocalCalleeIndex {
  /// One node walk. Call from the single-file tracer before assembling scopes.
  pub(super) fn build(
    semantic: &Semantic<'_>,
    imported_bindings: &BTreeMap<String, (String, String)>,
  ) -> Self {
    let mut by_scope: BTreeMap<NodeId, Vec<LocalCallee>> = BTreeMap::new();
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
      let Some((owner, call_outside)) =
        tracking_context_owner(semantic, call_id, call.span, imported_bindings)
      else {
        continue;
      };
      let callees = by_scope.entry(owner).or_default();
      if let Some(existing) = callees.iter_mut().find(|callee| callee.id == callee_id) {
        existing.call_outside = existing.call_outside && call_outside;
        existing.call_sites.push(call_id);
        continue;
      }
      callees.push(LocalCallee { id: callee_id, call_outside, call_sites: vec![call_id] });
    }
    Self { by_scope }
  }

  pub(super) fn for_scope(&self, scope_id: NodeId) -> &[LocalCallee] {
    self.by_scope.get(&scope_id).map_or(&[], Vec::as_slice)
  }
}

/// Callee follow plus local member / ident / write / uncertain buckets.
///
/// Built once per file. Collectors look up; they do not walk `semantic.nodes()`
/// to rediscover ownership.
pub(super) struct FileTraceIndex {
  callees: LocalCalleeIndex,
  nodes: ScopeNodeIndex,
}

impl FileTraceIndex {
  pub(super) fn build(
    semantic: &Semantic<'_>,
    imported_bindings: &BTreeMap<String, (String, String)>,
  ) -> Self {
    Self {
      callees: LocalCalleeIndex::build(semantic, imported_bindings),
      nodes: ScopeNodeIndex::build(semantic, imported_bindings),
    }
  }

  pub(super) const fn callees(&self) -> &LocalCalleeIndex {
    &self.callees
  }

  pub(super) const fn nodes(&self) -> &ScopeNodeIndex {
    &self.nodes
  }
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
/// Looks up `full_callees(scope_id)` and skips ids already in `visiting`.
/// Nested hops pass the same index — they do not walk nodes again.
pub(super) fn follow_local_callees(
  callees: &LocalCalleeIndex,
  scope_id: NodeId,
  depth: u32,
  visiting: &mut BTreeSet<NodeId>,
  outside: FollowOutside,
  mut visit: impl FnMut(NodeId, bool, u32, &[NodeId], &mut BTreeSet<NodeId>),
) {
  if depth >= MAX_LOCAL_CALLEE_FOLLOW_DEPTH {
    return;
  }
  for callee in callees.for_scope(scope_id) {
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

/// Identifier / member callees the bounded tracer did not fully follow.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct AnalysisGaps {
  pub unknown_calls: Vec<String>,
  pub truncated: bool,
}

impl AnalysisGaps {
  pub(super) fn merge(&mut self, other: Self) {
    let mut calls = BTreeSet::from_iter(std::mem::take(&mut self.unknown_calls));
    calls.extend(other.unknown_calls);
    self.unknown_calls = calls.into_iter().collect();
    self.truncated |= other.truncated;
  }
}

/// Walk calls owned by `scope_id` (and followed local helpers).
///
/// Local zero-arg helpers use [`LocalCalleeIndex`] (one edge per callee, not
/// per call site). Imports, member/dynamic callees, argumented helpers, and
/// async/generator become `unknown_calls`. Modeled Vue APIs and proven builtin
/// HOFs stay covered; lookalike methods (`api.map`) stay unknown even with an
/// inline callback. Hitting the depth cap or a recursive callee sets `truncated`.
pub(super) fn collect_analysis_gaps(
  semantic: &Semantic<'_>,
  index: &FileTraceIndex,
  scope_id: NodeId,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> AnalysisGaps {
  let mut unknown = BTreeSet::new();
  let mut truncated = false;
  let mut done = BTreeSet::new();
  let mut stack = BTreeSet::from([scope_id]);
  collect_analysis_gaps_walk(
    semantic,
    index,
    imported_bindings,
    scope_id,
    0,
    &mut done,
    &mut stack,
    &mut unknown,
    &mut truncated,
  );
  AnalysisGaps { unknown_calls: unknown.into_iter().collect(), truncated }
}

#[expect(
  clippy::too_many_arguments,
  reason = "gap walk threads index, visit sets, and accumulators"
)]
fn collect_analysis_gaps_walk(
  semantic: &Semantic<'_>,
  index: &FileTraceIndex,
  imported_bindings: &BTreeMap<String, (String, String)>,
  scope_id: NodeId,
  depth: u32,
  done: &mut BTreeSet<NodeId>,
  stack: &mut BTreeSet<NodeId>,
  unknown: &mut BTreeSet<String>,
  truncated: &mut bool,
) {
  if !done.insert(scope_id) {
    return;
  }
  for callee in index.callees().for_scope(scope_id) {
    if callee.call_outside {
      continue;
    }
    if depth >= MAX_LOCAL_CALLEE_FOLLOW_DEPTH || stack.contains(&callee.id) {
      *truncated = true;
      continue;
    }
    stack.insert(callee.id);
    collect_analysis_gaps_walk(
      semantic,
      index,
      imported_bindings,
      callee.id,
      depth.saturating_add(1),
      done,
      stack,
      unknown,
      truncated,
    );
    stack.remove(&callee.id);
  }
  for owned in index.nodes().calls(scope_id) {
    if owned.outside {
      continue;
    }
    let AstKind::CallExpression(call) = semantic.nodes().kind(owned.id) else {
      continue;
    };
    record_call_gaps(semantic, call, imported_bindings, unknown);
  }
}

fn record_call_gaps(
  semantic: &Semantic<'_>,
  call: &oxc_ast::ast::CallExpression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  unknown: &mut BTreeSet<String>,
) {
  let vue_api =
    resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Script).is_some();
  if let Some(identifier) = call.callee.get_identifier_reference() {
    if let Some(callee_id) = local_function_id(semantic, identifier)
      && !is_async_or_generator_function(semantic, callee_id)
      && call.arguments.is_empty()
    {
      return;
    }
    if !vue_api {
      unknown.insert(identifier.name.to_string());
    }
    record_unfollowed_argument_callees(semantic, call, imported_bindings, unknown);
    return;
  }
  // Proven builtin HOF: keep the callee covered and still scan arguments
  // (identifier callbacks such as `[1].map(readCount)`). Lookalike
  // `api.map(() => …)` records the callee as unknown; inline-callback
  // *reads* still come from tracking ownership, not this gap walk.
  if vue_api || is_modeled_sync_hof_call(semantic, call) {
    record_unfollowed_argument_callees(semantic, call, imported_bindings, unknown);
    return;
  }
  unknown.insert(unfollowed_call_label(&call.callee));
  record_unfollowed_argument_callees(semantic, call, imported_bindings, unknown);
}

/// Array/string literals, or global `Array.from` / `JSON.parse` whose name is
/// not a local binding. Ordinary objects (`api.map`) are not modeled.
fn is_modeled_sync_hof_call(
  semantic: &Semantic<'_>,
  call: &oxc_ast::ast::CallExpression<'_>,
) -> bool {
  (is_sync_hof_at_arg(&call.callee, 0) || is_sync_hof_at_arg(&call.callee, 1))
    && is_proven_hof_receiver(semantic, &call.callee)
}

fn is_proven_hof_receiver(semantic: &Semantic<'_>, callee: &Expression<'_>) -> bool {
  match peel_parens(callee) {
    Expression::StaticMemberExpression(member) => {
      let object = peel_parens(&member.object);
      if matches!(
        object,
        Expression::ArrayExpression(_)
          | Expression::StringLiteral(_)
          | Expression::TemplateLiteral(_)
      ) {
        return true;
      }
      let Expression::Identifier(identifier) = object else {
        return false;
      };
      matches!(
        (identifier.name.as_str(), member.property.name.as_str()),
        ("Array", "from") | ("JSON", "parse")
      ) && identifier_reference_is_unresolved(semantic, identifier)
    }
    Expression::ComputedMemberExpression(member) => {
      matches!(peel_parens(&member.object), Expression::ArrayExpression(_))
    }
    _ => false,
  }
}

const fn argument_is_inline_function(argument: &oxc_ast::ast::Argument<'_>) -> bool {
  matches!(
    argument,
    oxc_ast::ast::Argument::ArrowFunctionExpression(_)
      | oxc_ast::ast::Argument::FunctionExpression(_)
  )
}

fn record_unfollowed_argument_callees(
  semantic: &Semantic<'_>,
  call: &oxc_ast::ast::CallExpression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  unknown: &mut BTreeSet<String>,
) {
  for argument in &call.arguments {
    if argument_is_inline_function(argument) {
      continue;
    }
    record_expression_callee(semantic, argument, imported_bindings, unknown);
  }
}

fn record_expression_callee(
  semantic: &Semantic<'_>,
  argument: &oxc_ast::ast::Argument<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  unknown: &mut BTreeSet<String>,
) {
  let Some(expression) = argument.as_expression() else {
    return;
  };
  let expression = super::expr::peel_parens(expression);
  if let Some(identifier) = expression.get_identifier_reference() {
    let name = identifier.name.to_string();
    if local_function_id(semantic, identifier).is_some() || imported_bindings.contains_key(&name) {
      unknown.insert(name);
    }
    return;
  }
  if !matches!(
    expression,
    Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_)
  ) {
    return;
  }
  unknown.insert(unfollowed_call_label(expression));
}

fn unfollowed_call_label(callee: &Expression<'_>) -> String {
  if let Some(identifier) = callee.get_identifier_reference() {
    return identifier.name.to_string();
  }
  match callee {
    Expression::StaticMemberExpression(member) => {
      member.object.get_identifier_reference().map_or_else(
        || member.property.name.to_string(),
        |object| format!("{}.{}", object.name, member.property.name),
      )
    }
    Expression::ComputedMemberExpression(member) => {
      let object = member
        .object
        .get_identifier_reference()
        .map_or_else(|| "?".to_owned(), |identifier| identifier.name.to_string());
      member
        .static_property_name()
        .map_or_else(|| format!("{object}.*"), |property| format!("{object}.{property}"))
    }
    _ => "<dynamic>".into(),
  }
}

/// Walk-based discovery. Production uses [`LocalCalleeIndex`]. Kept so tests
/// can prove `for_scope` + a visiting filter matches this walk.
#[cfg(test)]
pub(super) fn local_zero_arg_callees_in_scope(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  imported_bindings: &BTreeMap<String, (String, String)>,
  visiting: &BTreeSet<NodeId>,
) -> Vec<LocalCallee> {
  use super::context::scope_context;

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

#[cfg(test)]
mod index_equiv {
  use std::collections::{BTreeMap, BTreeSet};

  use oxc_allocator::Allocator;
  use oxc_ast::AstKind;
  use oxc_parser::Parser;
  use oxc_semantic::{NodeId, Semantic, SemanticBuilder};
  use oxc_span::SourceType;

  use super::{LocalCallee, LocalCalleeIndex, local_zero_arg_callees_in_scope};
  use crate::trace::context::{
    is_deferred_callback_container, is_sync_hof_callback, is_to_value_getter_callback,
  };
  use crate::trace::kinds::collect_imported_bindings;

  fn callee_snap(callees: &[LocalCallee]) -> Vec<(NodeId, bool, Vec<NodeId>)> {
    callees
      .iter()
      .map(|callee| (callee.id, callee.call_outside, callee.call_sites.clone()))
      .collect()
  }

  fn is_transparent_callback(
    semantic: &Semantic<'_>,
    function_id: NodeId,
    imported_bindings: &BTreeMap<String, (String, String)>,
  ) -> bool {
    is_deferred_callback_container(semantic, function_id)
      || is_sync_hof_callback(semantic, function_id)
      || is_to_value_getter_callback(semantic, function_id, imported_bindings)
  }

  #[test]
  fn file_callee_index_matches_scope_walk() {
    let source = "\
import { ref, computed, toValue } from 'vue';
const x = ref(0);
const items = ref([1]);
function inner() { return x.value; }
function load() { return inner(); }
function cycle() { return cycle(); }
const c = computed(() => {
  items.value.map(() => load());
  Promise.resolve().then(() => load());
  toValue(() => load());
  return cycle();
});
void c.value;
";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let built = SemanticBuilder::new().with_build_nodes(true).build(&parsed.program);
    assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
    let semantic = &built.semantic;
    let imported = collect_imported_bindings(semantic);
    let index = LocalCalleeIndex::build(semantic, &imported);

    let mut function_ids = Vec::new();
    for (id, node) in semantic.nodes().iter_enumerated() {
      if matches!(node.kind(), AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)) {
        function_ids.push(id);
      }
    }
    assert!(function_ids.len() >= 4, "expected helpers + computed/HOF/then/toValue arrows");

    let mut visiting_sets = vec![BTreeSet::new()];
    for &id in &function_ids {
      visiting_sets.push(BTreeSet::from([id]));
    }
    if let (Some(&a), Some(&b)) = (function_ids.first(), function_ids.get(1)) {
      visiting_sets.push(BTreeSet::from([a, b]));
    }

    for &scope_id in &function_ids {
      if is_transparent_callback(semantic, scope_id, &imported) {
        continue;
      }
      for visiting in &visiting_sets {
        let walked = local_zero_arg_callees_in_scope(semantic, scope_id, &imported, visiting);
        let indexed: Vec<&LocalCallee> = index
          .for_scope(scope_id)
          .iter()
          .filter(|callee| !visiting.contains(&callee.id))
          .collect();
        let indexed_owned: Vec<LocalCallee> = indexed
          .iter()
          .map(|callee| LocalCallee {
            id: callee.id,
            call_outside: callee.call_outside,
            call_sites: callee.call_sites.clone(),
          })
          .collect();
        assert_eq!(
          callee_snap(&walked),
          callee_snap(&indexed_owned),
          "scope={scope_id:?} visiting={visiting:?}"
        );
      }
    }
  }
}
