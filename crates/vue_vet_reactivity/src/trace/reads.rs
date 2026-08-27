//! Tracking-scope reads: collection, guards, branch coverage, and classification.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::{
  AstKind,
  ast::{Argument, Expression, FunctionBody, IdentifierReference, Statement},
};
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};
use vue_vet_core::{
  ReactiveBindingFact, ReactiveGuardFact, ReactiveGuardRole, ReactiveReadFact, ReactiveReadKind,
  ScriptKind,
};

use super::{
  ComposableShapeMap,
  bindings::AmbientCallHandles,
  branch_hygiene,
  context::ScopeNodeIndex,
  follow::{
    FileTraceIndex, FollowOutside, follow_local_callees, innermost_function_id,
    local_helper_calls_by_owner,
  },
  kinds::{reference_resolves_to_binding, resolved_vue_callee, source_span, span_contains},
  writes::function_body_of,
};

#[derive(Clone, Debug)]
pub(super) struct RawReactiveRead {
  node_id: NodeId,
  binding: String,
  property: Option<String>,
  span: Span,
  outside_tracking: bool,
  /// Follow hops from this read out to the tracking scope. Each hop is the
  /// `CallExpression` set in that caller. Empty for reads already in the scope.
  caller_hops: Vec<Vec<NodeId>>,
}

impl RawReactiveRead {
  fn local(
    node_id: NodeId,
    binding: impl Into<String>,
    property: Option<String>,
    span: Span,
    outside_tracking: bool,
  ) -> Self {
    Self {
      node_id,
      binding: binding.into(),
      property,
      span,
      outside_tracking,
      caller_hops: Vec::new(),
    }
  }
}

#[derive(Debug)]
pub(super) struct RawGuard {
  read: RawReactiveRead,
  role: ReactiveGuardRole,
}

#[expect(
  clippy::too_many_arguments,
  reason = "file trace index is one extra arg on the collector surface"
)]
pub(super) fn collect_scope_reads(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  imported_bindings: &BTreeMap<String, (String, String)>,
  ambient_call_handles: &AmbientCallHandles,
  script_offset: usize,
  index: &FileTraceIndex,
) -> Vec<RawReactiveRead> {
  let mut visiting = BTreeSet::new();
  visiting.insert(scope_id);
  collect_scope_reads_bounded(
    semantic,
    scope_id,
    reactive_bindings,
    composable_instances,
    imported_bindings,
    ambient_call_handles,
    script_offset,
    0,
    &mut visiting,
    index,
  )
}

#[expect(clippy::too_many_arguments, reason = "bounded collector threads scope + visit state")]
pub(super) fn collect_scope_reads_bounded(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  imported_bindings: &BTreeMap<String, (String, String)>,
  ambient_call_handles: &AmbientCallHandles,
  script_offset: usize,
  depth: u32,
  visiting: &mut BTreeSet<NodeId>,
  index: &FileTraceIndex,
) -> Vec<RawReactiveRead> {
  let mut reads = collect_scope_reads_local(
    semantic,
    scope_id,
    reactive_bindings,
    composable_instances,
    imported_bindings,
    ambient_call_handles,
    script_offset,
    index.nodes(),
  );

  // Same-file zero-arg helpers contribute ambient tracking reads (Vue's
  // activeEffect). `then()` / `nextTick`-only calls stay outside-tracking.
  follow_local_callees(
    index.callees(),
    scope_id,
    depth,
    visiting,
    FollowOutside::Mark,
    |callee_id, call_outside, next_depth, call_sites, visiting| {
      let mut nested = collect_scope_reads_bounded(
        semantic,
        callee_id,
        reactive_bindings,
        composable_instances,
        imported_bindings,
        ambient_call_handles,
        script_offset,
        next_depth,
        visiting,
        index,
      );
      for read in &mut nested {
        if call_outside {
          read.outside_tracking = true;
        }
        read.caller_hops.push(call_sites.to_vec());
      }
      reads.extend(nested);
    },
  );

  reads.sort_by_key(|read| read.span.start);
  reads
}

#[expect(clippy::too_many_arguments, reason = "local reads look up the file node index by kind")]
pub(super) fn collect_scope_reads_local(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  imported_bindings: &BTreeMap<String, (String, String)>,
  ambient_call_handles: &AmbientCallHandles,
  script_offset: usize,
  nodes: &ScopeNodeIndex,
) -> Vec<RawReactiveRead> {
  let mut reads = Vec::new();
  for owned in nodes.members(scope_id) {
    if let Some(read) = local_member_read(
      semantic,
      owned.id,
      owned.outside,
      reactive_bindings,
      composable_instances,
      script_offset,
    ) {
      reads.push(read);
    }
  }
  for owned in nodes.calls(scope_id) {
    if let Some(read) = local_unwrap_read(
      semantic,
      owned.id,
      owned.outside,
      reactive_bindings,
      imported_bindings,
      script_offset,
    ) {
      reads.push(read);
    }
  }

  // Bare identifier reads of Reactive / ShallowReactive bindings (Vue 3.5 props
  // destructure, `reactive()` locals). Ref-like still require `.value` / unref /
  // toValue above. Skip identifiers that are the object of a member expression —
  // those already contributed a member read.
  for owned in nodes.idents(scope_id) {
    let AstKind::IdentifierReference(identifier) = semantic.nodes().kind(owned.id) else {
      continue;
    };
    if identifier_is_member_object(semantic, owned.id) {
      continue;
    }
    let Some(binding) = reactive_bindings.iter().find(|binding| {
      binding.name == identifier.name.as_str()
        && reference_resolves_to_binding(semantic, identifier, binding, script_offset)
        && !binding.kind.is_ref_like()
    }) else {
      continue;
    };
    reads.push(RawReactiveRead::local(
      owned.id,
      binding.name.clone(),
      None,
      identifier.span,
      owned.outside,
    ));
  }

  // Named API bag methods (`const { t } = useI18n()`): inject precomputed ambient reads.
  for owned in nodes.calls(scope_id) {
    let AstKind::CallExpression(call) = semantic.nodes().kind(owned.id) else {
      continue;
    };
    let Some(identifier) = call.callee.get_identifier_reference() else {
      continue;
    };
    let Some(ambient) = resolve_ambient_call_handle(semantic, identifier, ambient_call_handles)
    else {
      continue;
    };
    for (binding, property) in ambient {
      reads.push(RawReactiveRead::local(
        owned.id,
        binding.clone(),
        property.clone(),
        call.span,
        owned.outside,
      ));
    }
  }

  reads
}

fn local_member_read(
  semantic: &oxc_semantic::Semantic<'_>,
  member_id: NodeId,
  outside_tracking: bool,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  script_offset: usize,
) -> Option<RawReactiveRead> {
  let kind = semantic.nodes().kind(member_id);
  // Nested composable instance: bag.field.value
  if let AstKind::StaticMemberExpression(outer) = kind
    && outer.property.name.as_str() == "value"
    && let Expression::StaticMemberExpression(inner) = &outer.object
    && let Some(instance) = inner.object.get_identifier_reference()
    && let Some(shape) = composable_instances.get(instance.name.as_str())
    && let Some(kind) = shape.get(inner.property.name.as_str())
    && kind.is_ref_like()
  {
    return Some(RawReactiveRead::local(
      member_id,
      inner.property.name.to_string(),
      Some("value".into()),
      outer.span,
      outside_tracking,
    ));
  }

  // Nested composable instance: bag.field for non-ref-like kinds
  if let AstKind::StaticMemberExpression(member) = kind
    && let Some(instance) = member.object.get_identifier_reference()
    && let Some(shape) = composable_instances.get(instance.name.as_str())
    && let Some(kind) = shape.get(member.property.name.as_str())
    && !kind.is_ref_like()
  {
    return Some(RawReactiveRead::local(
      member_id,
      member.property.name.to_string(),
      Some(member.property.name.to_string()),
      member.span,
      outside_tracking,
    ));
  }

  let (object, property, member_span) = match kind {
    AstKind::StaticMemberExpression(member) => (
      member.object.get_identifier_reference()?,
      Some(member.property.name.to_string()),
      member.span,
    ),
    AstKind::ComputedMemberExpression(member) => (
      member.object.get_identifier_reference()?,
      member.static_property_name().map(|name| name.to_string()),
      member.span,
    ),
    _ => return None,
  };
  let binding = reactive_bindings.iter().find(|binding| {
    binding.name == object.name.as_str()
      && reference_resolves_to_binding(semantic, object, binding, script_offset)
      && (!binding.kind.is_ref_like() || property.as_deref() == Some("value"))
  })?;
  Some(RawReactiveRead::local(
    member_id,
    binding.name.clone(),
    property,
    member_span,
    outside_tracking,
  ))
}

fn local_unwrap_read(
  semantic: &oxc_semantic::Semantic<'_>,
  call_id: NodeId,
  outside_tracking: bool,
  reactive_bindings: &[ReactiveBindingFact],
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_offset: usize,
) -> Option<RawReactiveRead> {
  // `unref(x)` / `toValue(x)` track ref-like bindings (runtime reads `.value`).
  // `toValue(() => …)` is handled via `is_to_value_getter_callback` so nested
  // member reads stay in the parent tracking scope.
  let AstKind::CallExpression(call) = semantic.nodes().kind(call_id) else {
    return None;
  };
  let callee = resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Setup)?;
  if !matches!(callee.as_str(), "unref" | "toValue") {
    return None;
  }
  let argument = call.arguments.first().and_then(Argument::as_expression)?;
  let identifier = argument.get_identifier_reference()?;
  let binding = reactive_bindings.iter().find(|binding| {
    binding.name == identifier.name.as_str()
      && reference_resolves_to_binding(semantic, identifier, binding, script_offset)
      && binding.kind.is_ref_like()
  })?;
  Some(RawReactiveRead::local(
    call_id,
    binding.name.clone(),
    Some("value".into()),
    call.span,
    outside_tracking,
  ))
}

/// Resolve a bare call to a registered ambient-on-call method handle.
pub(super) fn resolve_ambient_call_handle<'a>(
  semantic: &oxc_semantic::Semantic<'_>,
  identifier: &IdentifierReference<'_>,
  ambient_call_handles: &'a AmbientCallHandles,
) -> Option<&'a [(String, Option<String>)]> {
  let handles = ambient_call_handles.get(identifier.name.as_str())?;
  let reference_id = identifier.reference_id.get()?;
  let symbol_id = semantic.scoping().get_reference(reference_id).symbol_id()?;
  let site_decl = symbol_declaration_site_decl(semantic, symbol_id)?;
  handles
    .iter()
    .find(|handle| handle.site_decl == site_decl)
    .map(|handle| handle.ambient.as_slice())
}

/// Anchor node for a binding symbol — `VariableDeclarator` when oxc surfaces that
/// for object-pattern locals (shared by the whole destructure).
pub(super) fn symbol_declaration_site_decl(
  semantic: &oxc_semantic::Semantic<'_>,
  symbol_id: oxc_semantic::SymbolId,
) -> Option<NodeId> {
  let decl = semantic.symbol_declaration(symbol_id);
  match decl.kind() {
    AstKind::VariableDeclarator(_) => Some(decl.id()),
    AstKind::BindingIdentifier(_) => semantic
      .nodes()
      .ancestor_ids(decl.id())
      .find(|&id| matches!(semantic.nodes().kind(id), AstKind::VariableDeclarator(_))),
    _ => None,
  }
}

/// True when this identifier is `obj` in `obj.prop` / `obj[prop]` (already covered
/// by member-expression reads). Computed keys (`obj[key]`) stay bare reads.
pub(super) fn identifier_is_member_object(
  semantic: &oxc_semantic::Semantic<'_>,
  ident_id: NodeId,
) -> bool {
  let AstKind::IdentifierReference(identifier) = semantic.nodes().kind(ident_id) else {
    return false;
  };
  match semantic.nodes().parent_kind(ident_id) {
    AstKind::StaticMemberExpression(member) => {
      member.object.get_identifier_reference().is_some_and(|object| object.span == identifier.span)
    }
    AstKind::ComputedMemberExpression(member) => {
      member.object.get_identifier_reference().is_some_and(|object| object.span == identifier.span)
    }
    _ => false,
  }
}

pub(super) fn push_guards_in_span(
  guards: &mut Vec<RawGuard>,
  reads: &[RawReactiveRead],
  span: Span,
  role: ReactiveGuardRole,
) {
  for read in reads.iter().filter(|read| span_contains(span, read.span) && !read.outside_tracking) {
    if !guards.iter().any(|guard| {
      guard.read.binding == read.binding
        && guard.read.property == read.property
        && guard.read.span.start == read.span.start
        && guard.read.span.end == read.span.end
    }) {
      guards.push(RawGuard { read: read.clone(), role });
    }
  }
}

pub(super) fn is_early_return(statement: &Statement<'_>) -> bool {
  match statement {
    Statement::ReturnStatement(_) | Statement::ThrowStatement(_) => true,
    Statement::BlockStatement(block) => match block.body.as_slice() {
      [only] => is_early_return(only),
      _ => false,
    },
    _ => false,
  }
}

pub(super) fn collect_prefix_early_exits(
  statements: &[Statement<'_>],
  read_start: u32,
  reads: &[RawReactiveRead],
  guards: &mut Vec<RawGuard>,
) {
  for statement in statements {
    if statement.span().start >= read_start {
      break;
    }
    match statement {
      // Only statements fully before the read can guard it. Reads inside the
      // `if` test itself remain unconditional relative to that early exit.
      Statement::IfStatement(guard)
        if guard.span.end <= read_start
          && guard.alternate.is_none()
          && is_early_return(&guard.consequent) =>
      {
        push_guards_in_span(guards, reads, guard.test.span(), ReactiveGuardRole::EarlyExit);
      }
      Statement::BlockStatement(block) => {
        collect_prefix_early_exits(&block.body, read_start, reads, guards);
      }
      Statement::TryStatement(try_statement) => {
        collect_prefix_early_exits(&try_statement.block.body, read_start, reads, guards);
        if let Some(handler) = &try_statement.handler {
          collect_prefix_early_exits(&handler.body.body, read_start, reads, guards);
        }
        if let Some(finalizer) = &try_statement.finalizer {
          collect_prefix_early_exits(&finalizer.body, read_start, reads, guards);
        }
      }
      _ => {}
    }
  }
}

pub(super) fn path_guards(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  body: Option<&FunctionBody<'_>>,
  reads: &[RawReactiveRead],
  read: &RawReactiveRead,
) -> Vec<RawGuard> {
  let mut guards = Vec::new();

  if let Some(body) = body {
    collect_prefix_early_exits(&body.statements, read.span.start, reads, &mut guards);
  }

  for ancestor_id in semantic.nodes().ancestor_ids(read.node_id) {
    if ancestor_id == scope_id {
      break;
    }
    match semantic.nodes().kind(ancestor_id) {
      AstKind::IfStatement(statement) => {
        let in_branch = span_contains(statement.consequent.span(), read.span)
          || statement
            .alternate
            .as_ref()
            .is_some_and(|alternate| span_contains(alternate.span(), read.span));
        if in_branch {
          // Both if/else arms read the same binding+property → always reached.
          if branch_pair_covers_read(
            reads,
            read,
            statement.consequent.span(),
            statement.alternate.as_ref().map(oxc_span::GetSpan::span),
          ) {
            continue;
          }
          push_guards_in_span(
            &mut guards,
            reads,
            statement.test.span(),
            ReactiveGuardRole::BranchTest,
          );
        }
      }
      AstKind::ConditionalExpression(expression) => {
        if span_contains(expression.consequent.span(), read.span)
          || span_contains(expression.alternate.span(), read.span)
        {
          // `cond ? x.value : x.value` — `x` is a reliable dependency.
          if branch_pair_covers_read(
            reads,
            read,
            expression.consequent.span(),
            Some(expression.alternate.span()),
          ) {
            continue;
          }
          push_guards_in_span(
            &mut guards,
            reads,
            expression.test.span(),
            ReactiveGuardRole::BranchTest,
          );
        }
      }
      AstKind::LogicalExpression(expression)
        if span_contains(expression.right.span(), read.span) =>
      {
        push_guards_in_span(
          &mut guards,
          reads,
          expression.left.span(),
          ReactiveGuardRole::ShortCircuit,
        );
      }
      AstKind::SwitchCase(case) if span_contains(case.span, read.span) => {
        let switch_id = semantic.nodes().parent_id(ancestor_id);
        if let AstKind::SwitchStatement(switch_statement) = semantic.nodes().kind(switch_id) {
          push_guards_in_span(
            &mut guards,
            reads,
            switch_statement.discriminant.span(),
            ReactiveGuardRole::SwitchDiscriminant,
          );
        }
      }
      _ => {}
    }
  }

  guards.sort_by_key(|guard| guard.read.span.start);
  guards
}

impl branch_hygiene::BranchReadView for RawReactiveRead {
  fn binding(&self) -> &str {
    self.binding.as_str()
  }
  fn property(&self) -> Option<&str> {
    self.property.as_deref()
  }
  fn span_start(&self) -> u32 {
    self.span.start
  }
  fn span_end(&self) -> u32 {
    self.span.end
  }
  fn outside_tracking(&self) -> bool {
    self.outside_tracking
  }
}

/// True when both arms of a branch pair contain a same-identity read as `read`.
///
/// Pure contract: [`branch_hygiene::branch_pair_covers_read`].
pub(super) fn branch_pair_covers_read(
  reads: &[RawReactiveRead],
  read: &RawReactiveRead,
  left: Span,
  right: Option<Span>,
) -> bool {
  branch_hygiene::branch_pair_covers_read(
    reads,
    read.binding.as_str(),
    read.property.as_deref(),
    branch_hygiene::SpanRange { start: left.start, end: left.end },
    right.map(|span| branch_hygiene::SpanRange { start: span.start, end: span.end }),
  )
}

/// Per-tracking-scope await facts. Pause events live in [`ScopeIrIndex`]
/// so helper bodies are not compared against the caller by file offset.
pub(super) struct TrackingScopeIR {
  /// Ends of top-level `await` expressions owned by this scope (sorted ascending).
  await_ends: Vec<u32>,
}

/// File-level await + pause IR. Built once; [`classify_scope_reads`] looks up.
///
/// Await ownership is a third rule — first Function/Arrow, but `if` / ternary /
/// `&&` drop the await (top-level only). Do not unify with [`super::context::ScopeNodeIndex`].
pub(super) struct ScopeIrIndex {
  awaits: BTreeMap<NodeId, Vec<u32>>,
  pause_irs: BTreeMap<NodeId, Vec<(u32, bool)>>,
}

impl ScopeIrIndex {
  pub(super) fn build(
    semantic: &Semantic<'_>,
    imported_bindings: &BTreeMap<String, (String, String)>,
  ) -> Self {
    Self {
      awaits: index_await_ends(semantic),
      pause_irs: build_function_pause_irs(semantic, imported_bindings),
    }
  }

  pub(super) fn tracking_ir(&self, scope_id: NodeId) -> TrackingScopeIR {
    TrackingScopeIR { await_ends: self.awaits.get(&scope_id).cloned().unwrap_or_default() }
  }

  pub(super) const fn pause_irs(&self) -> &BTreeMap<NodeId, Vec<(u32, bool)>> {
    &self.pause_irs
  }
}

fn index_await_ends(semantic: &Semantic<'_>) -> BTreeMap<NodeId, Vec<u32>> {
  let mut awaits: BTreeMap<NodeId, Vec<u32>> = BTreeMap::new();
  for (node_id, node) in semantic.nodes().iter_enumerated() {
    let AstKind::AwaitExpression(await_expression) = node.kind() else {
      continue;
    };
    let Some(owner) = tracking_await_owner(semantic, node_id) else {
      continue;
    };
    awaits.entry(owner).or_default().push(await_expression.span.end);
  }
  for ends in awaits.values_mut() {
    ends.sort_unstable();
  }
  awaits
}

/// Inverse of [`scope_owns_await`]: first Function/Arrow, unless an `if` /
/// ternary / logical sits in between.
fn tracking_await_owner(semantic: &Semantic<'_>, await_id: NodeId) -> Option<NodeId> {
  for ancestor_id in semantic.nodes().ancestor_ids(await_id) {
    match semantic.nodes().kind(ancestor_id) {
      AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => return Some(ancestor_id),
      AstKind::IfStatement(_)
      | AstKind::ConditionalExpression(_)
      | AstKind::LogicalExpression(_) => return None,
      _ => {}
    }
  }
  None
}

#[cfg(test)]
pub(super) fn build_tracking_scope_ir(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
) -> TrackingScopeIR {
  let mut await_ends = Vec::new();
  for (node_id, node) in semantic.nodes().iter_enumerated() {
    if let AstKind::AwaitExpression(await_expression) = node.kind()
      && scope_owns_await(semantic, scope_id, node_id)
    {
      await_ends.push(await_expression.span.end);
    }
  }
  await_ends.sort_unstable();
  TrackingScopeIR { await_ends }
}

#[cfg(test)]
pub(super) fn scope_owns_await(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  await_id: NodeId,
) -> bool {
  for ancestor_id in semantic.nodes().ancestor_ids(await_id) {
    if ancestor_id == scope_id {
      return true;
    }
    match semantic.nodes().kind(ancestor_id) {
      AstKind::ArrowFunctionExpression(_)
      | AstKind::Function(_)
      | AstKind::IfStatement(_)
      | AstKind::ConditionalExpression(_)
      | AstKind::LogicalExpression(_) => return false,
      _ => {}
    }
  }
  false
}

pub(super) fn is_after_top_level_await_ir(ir: &TrackingScopeIR, read: &RawReactiveRead) -> bool {
  // Any await that fully precedes the read.
  let index = ir.await_ends.partition_point(|&end| end <= read.span.start);
  index > 0
}

/// Last pause/resume before `pos` in one function. No events keeps `inherit`.
/// Vue's `shouldTrack` is a stack, not a counter — this is still "last event",
/// the same fold as the committed `pause-tracking-window` cases.
fn pause_state_at(events: &[(u32, bool)], pos: u32, inherit: bool) -> bool {
  let mut paused = inherit;
  for (end, is_pause) in events {
    if *end > pos {
      break;
    }
    paused = *is_pause;
  }
  paused
}

fn pause_events_for(
  irs: &BTreeMap<NodeId, Vec<(u32, bool)>>,
  function_id: NodeId,
) -> &[(u32, bool)] {
  irs.get(&function_id).map_or(&[], Vec::as_slice)
}

/// Per-function pause/resume, plus a synthetic event at each same-file helper
/// call end when that callee's last event is pause or resume (leak).
fn build_function_pause_irs(
  semantic: &Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> BTreeMap<NodeId, Vec<(u32, bool)>> {
  let mut raw: BTreeMap<NodeId, Vec<(u32, bool)>> = BTreeMap::new();
  for (_, node) in semantic.nodes().iter_enumerated() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let is_pause = if is_pause_tracking_call(semantic, call, imported_bindings) {
      true
    } else if is_resume_tracking_call(semantic, call, imported_bindings) {
      false
    } else {
      continue;
    };
    let Some(owner) = innermost_function_id(semantic, call.node_id.get()) else {
      continue;
    };
    raw.entry(owner).or_default().push((call.span.end, is_pause));
  }
  let helper_calls = local_helper_calls_by_owner(semantic);
  let mut owners: BTreeSet<NodeId> = raw.keys().copied().collect();
  owners.extend(helper_calls.keys().copied());
  let mut memo = BTreeMap::new();
  let mut visiting = BTreeSet::new();
  for owner in owners {
    enrich_function_pause_events(owner, &raw, &helper_calls, &mut memo, &mut visiting);
  }
  memo
}

fn enrich_function_pause_events(
  function_id: NodeId,
  raw: &BTreeMap<NodeId, Vec<(u32, bool)>>,
  helper_calls: &BTreeMap<NodeId, Vec<(u32, NodeId)>>,
  memo: &mut BTreeMap<NodeId, Vec<(u32, bool)>>,
  visiting: &mut BTreeSet<NodeId>,
) -> Vec<(u32, bool)> {
  if let Some(events) = memo.get(&function_id) {
    return events.clone();
  }
  if !visiting.insert(function_id) {
    let mut events = raw.get(&function_id).cloned().unwrap_or_default();
    events.sort_by_key(|(end, _)| *end);
    return events;
  }
  let mut events = raw.get(&function_id).cloned().unwrap_or_default();
  if let Some(calls) = helper_calls.get(&function_id) {
    for &(end, callee_id) in calls {
      let nested = enrich_function_pause_events(callee_id, raw, helper_calls, memo, visiting);
      if let Some((_, is_pause)) = nested.last() {
        events.push((end, *is_pause));
      }
    }
  }
  events.sort_by_key(|(end, _)| *end);
  visiting.remove(&function_id);
  memo.insert(function_id, events.clone());
  events
}

pub(super) fn is_pause_tracking_call(
  semantic: &Semantic<'_>,
  call: &oxc_ast::ast::CallExpression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> bool {
  resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Script)
    .is_some_and(|callee| matches!(callee.as_str(), "pauseTracking"))
}

pub(super) fn is_resume_tracking_call(
  semantic: &Semantic<'_>,
  call: &oxc_ast::ast::CallExpression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> bool {
  resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Script)
    .is_some_and(|callee| matches!(callee.as_str(), "enableTracking" | "resetTracking"))
}

pub(super) struct ClassifyRead<'a> {
  semantic: &'a oxc_semantic::Semantic<'a>,
  scope_id: NodeId,
  body: Option<&'a FunctionBody<'a>>,
  raw_reads: &'a [RawReactiveRead],
  read: &'a RawReactiveRead,
  sfc_source: &'a str,
  script_offset: usize,
  ir: &'a TrackingScopeIR,
  pause_irs: &'a BTreeMap<NodeId, Vec<(u32, bool)>>,
}

pub(super) fn classify_scope_reads(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  body: Option<&FunctionBody<'_>>,
  raw_reads: &[RawReactiveRead],
  sfc_source: &str,
  script_offset: usize,
  ir: &ScopeIrIndex,
) -> Vec<ReactiveReadFact> {
  if raw_reads.is_empty() {
    return Vec::new();
  }
  let tracking = ir.tracking_ir(scope_id);
  raw_reads
    .iter()
    .map(|read| {
      classify_read(&ClassifyRead {
        semantic,
        scope_id,
        body,
        raw_reads,
        read,
        sfc_source,
        script_offset,
        ir: &tracking,
        pause_irs: ir.pause_irs(),
      })
    })
    .collect()
}

/// Guards on the read inside its owning function, plus caller guards at each
/// follow hop. A hop with any unguarded (or both-arm-covered) call site adds
/// nothing — that hop does not constrain the read.
fn classify_path_guards(input: &ClassifyRead<'_>) -> Vec<RawGuard> {
  let local_scope =
    innermost_function_id(input.semantic, input.read.node_id).unwrap_or(input.scope_id);
  let local_body = function_body_of(input.semantic, local_scope).or(input.body);
  let mut guards =
    path_guards(input.semantic, local_scope, local_body, input.raw_reads, input.read);
  for hop in &input.read.caller_hops {
    guards.extend(caller_hop_guards(input.semantic, input.raw_reads, input.read, hop));
  }
  guards.sort_by_key(|guard| (guard.read.span.start, guard.role as u8));
  guards.dedup_by(|left, right| {
    left.role == right.role
      && left.read.binding == right.read.binding
      && left.read.property == right.read.property
      && left.read.span == right.read.span
  });
  guards
}

fn caller_hop_guards(
  semantic: &Semantic<'_>,
  raw_reads: &[RawReactiveRead],
  read: &RawReactiveRead,
  call_sites: &[NodeId],
) -> Vec<RawGuard> {
  let Some(first) = call_sites.first().copied() else {
    return Vec::new();
  };
  let owner = innermost_function_id(semantic, first).unwrap_or(first);
  let body = function_body_of(semantic, owner);
  let proxies: Vec<RawReactiveRead> = call_sites
    .iter()
    .map(|&call_id| {
      RawReactiveRead::local(
        call_id,
        read.binding.clone(),
        read.property.clone(),
        semantic.nodes().kind(call_id).span(),
        false,
      )
    })
    .collect();
  let mut combined = raw_reads.to_vec();
  combined.extend(proxies.iter().cloned());
  let mut merged = Vec::new();
  for proxy in &proxies {
    let site = path_guards(semantic, owner, body, &combined, proxy);
    if site.is_empty() {
      return Vec::new();
    }
    merged.extend(site);
  }
  merged
}

fn hop_all_paused(input: &ClassifyRead<'_>, hop: &[NodeId], inherit: bool) -> bool {
  if hop.is_empty() {
    return inherit;
  }
  hop.iter().all(|&call_id| {
    let owner = innermost_function_id(input.semantic, call_id).unwrap_or(input.scope_id);
    let start = input.semantic.nodes().kind(call_id).span().start;
    pause_state_at(pause_events_for(input.pause_irs, owner), start, inherit)
  })
}

/// Pause on the owning function, caller hops, and (when the read is in-scope
/// rather than followed) the tracking-scope IR so inline HOF callbacks still
/// see `pauseTracking()` in the parent. Followed reads never compare helper
/// spans against caller events.
fn read_is_after_pause(input: &ClassifyRead<'_>) -> bool {
  let owner = innermost_function_id(input.semantic, input.read.node_id).unwrap_or(input.scope_id);
  if input.read.caller_hops.is_empty() {
    let mut paused = pause_state_at(
      pause_events_for(input.pause_irs, input.scope_id),
      input.read.span.start,
      false,
    );
    if owner != input.scope_id {
      paused =
        pause_state_at(pause_events_for(input.pause_irs, owner), input.read.span.start, paused);
    }
    return paused;
  }
  let mut paused = false;
  for hop in input.read.caller_hops.iter().rev() {
    paused = hop_all_paused(input, hop, paused);
  }
  pause_state_at(pause_events_for(input.pause_irs, owner), input.read.span.start, paused)
}

pub(super) fn classify_read(input: &ClassifyRead<'_>) -> ReactiveReadFact {
  let outside = input.read.outside_tracking || read_is_after_pause(input);
  let guards = if outside { Vec::new() } else { classify_path_guards(input) };
  let kind = if outside {
    ReactiveReadKind::OutsideTracking
  } else if is_after_top_level_await_ir(input.ir, input.read) {
    ReactiveReadKind::AfterAwait
  } else if guards.is_empty() {
    ReactiveReadKind::Unconditional
  } else {
    ReactiveReadKind::Conditional
  };
  let guarded_by = guards.first().map(|guard| guard.read.binding.clone());
  ReactiveReadFact {
    binding: input.read.binding.clone(),
    property: input.read.property.clone(),
    kind,
    guards: guards
      .into_iter()
      .map(|guard| ReactiveGuardFact {
        binding: guard.read.binding,
        property: guard.read.property,
        span: source_span(input.sfc_source, input.script_offset, guard.read.span),
        role: guard.role,
      })
      .collect(),
    guarded_by,
    span: source_span(input.sfc_source, input.script_offset, input.read.span),
  }
}

#[cfg(test)]
mod scope_ir_equiv {
  use oxc_allocator::Allocator;
  use oxc_ast::AstKind;
  use oxc_parser::Parser;
  use oxc_semantic::SemanticBuilder;
  use oxc_span::SourceType;

  use super::{
    ScopeIrIndex, build_tracking_scope_ir, is_pause_tracking_call, is_resume_tracking_call,
  };
  use crate::trace::follow::innermost_function_id;
  use crate::trace::kinds::collect_imported_bindings;

  #[test]
  fn file_scope_ir_matches_await_walk() {
    let source = "\
import { ref, computed, watchEffect, toValue, pauseTracking, enableTracking } from 'vue';
const x = ref(0);
const items = ref([1]);
async function inner() { await Promise.resolve(); return x.value; }
async function load() { return inner(); }
const c = computed(async () => {
  const before = x.value;
  await Promise.resolve();
  if (before) { await Promise.resolve(); }
  before ? await Promise.resolve() : 0;
  before && await Promise.resolve();
  items.value.map(async () => { await Promise.resolve(); return load(); });
  Promise.resolve().then(async () => { await Promise.resolve(); });
  toValue(async () => { await Promise.resolve(); return x.value; });
  return x.value;
});
function paused() { pauseTracking(); return x.value; }
watchEffect(() => {
  pauseTracking();
  void paused();
  enableTracking();
  void x.value;
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
    let index = ScopeIrIndex::build(semantic, &imported);

    let mut function_ids = Vec::new();
    for (id, node) in semantic.nodes().iter_enumerated() {
      if matches!(node.kind(), AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)) {
        function_ids.push(id);
      }
    }
    assert!(
      function_ids.len() >= 6,
      "expected helpers + computed/HOF/then/toValue arrows, got {}",
      function_ids.len()
    );

    let mut saw_await_owner = false;
    for &scope_id in &function_ids {
      let walked = build_tracking_scope_ir(semantic, scope_id);
      let indexed = index.tracking_ir(scope_id);
      assert_eq!(walked.await_ends, indexed.await_ends, "await ends scope={scope_id:?}");
      if !walked.await_ends.is_empty() {
        saw_await_owner = true;
      }
    }
    assert!(saw_await_owner, "fixture must own at least one top-level await");

    for (_, node) in semantic.nodes().iter_enumerated() {
      let AstKind::CallExpression(call) = node.kind() else {
        continue;
      };
      let is_pause = if is_pause_tracking_call(semantic, call, &imported) {
        true
      } else if is_resume_tracking_call(semantic, call, &imported) {
        false
      } else {
        continue;
      };
      let Some(owner) = innermost_function_id(semantic, call.node_id.get()) else {
        continue;
      };
      assert!(
        index
          .pause_irs()
          .get(&owner)
          .is_some_and(|events| events.contains(&(call.span.end, is_pause))),
        "pause/resume at {} missing from owner={owner:?}",
        call.span.end
      );
    }
  }
}
