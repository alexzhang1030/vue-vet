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
  context::scope_context,
  follow::{FollowOutside, follow_local_callees},
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

pub(super) fn collect_scope_reads(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  imported_bindings: &BTreeMap<String, (String, String)>,
  ambient_call_handles: &AmbientCallHandles,
  script_offset: usize,
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
) -> Vec<RawReactiveRead> {
  let mut reads = collect_scope_reads_local(
    semantic,
    scope_id,
    reactive_bindings,
    composable_instances,
    imported_bindings,
    ambient_call_handles,
    script_offset,
  );

  // Same-file zero-arg helpers contribute ambient tracking reads (Vue's
  // activeEffect). `then()` / `nextTick`-only calls stay outside-tracking.
  follow_local_callees(
    semantic,
    scope_id,
    imported_bindings,
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

pub(super) fn collect_scope_reads_local(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  imported_bindings: &BTreeMap<String, (String, String)>,
  ambient_call_handles: &AmbientCallHandles,
  script_offset: usize,
) -> Vec<RawReactiveRead> {
  let mut reads = semantic
    .nodes()
    .iter_enumerated()
    .filter_map(|(member_id, member_node)| {
      // Nested composable instance: bag.field.value
      if let AstKind::StaticMemberExpression(outer) = member_node.kind()
        && outer.property.name.as_str() == "value"
        && let Expression::StaticMemberExpression(inner) = &outer.object
        && let Some(instance) = inner.object.get_identifier_reference()
        && let Some(shape) = composable_instances.get(instance.name.as_str())
        && let Some(kind) = shape.get(inner.property.name.as_str())
        && kind.is_ref_like()
      {
        let (_, outside_tracking) =
          scope_context(semantic, scope_id, member_id, outer.span, imported_bindings)?;
        return Some(RawReactiveRead::local(
          member_id,
          inner.property.name.to_string(),
          Some("value".into()),
          outer.span,
          outside_tracking,
        ));
      }

      // Nested composable instance: bag.field for non-ref-like kinds
      if let AstKind::StaticMemberExpression(member) = member_node.kind()
        && let Some(instance) = member.object.get_identifier_reference()
        && let Some(shape) = composable_instances.get(instance.name.as_str())
        && let Some(kind) = shape.get(member.property.name.as_str())
        && !kind.is_ref_like()
      {
        let (_, outside_tracking) =
          scope_context(semantic, scope_id, member_id, member.span, imported_bindings)?;
        return Some(RawReactiveRead::local(
          member_id,
          member.property.name.to_string(),
          Some(member.property.name.to_string()),
          member.span,
          outside_tracking,
        ));
      }

      // `unref(x)` / `toValue(x)` track ref-like bindings (runtime reads `.value`).
      // `toValue(() => …)` is handled via `is_to_value_getter_callback` so nested
      // member reads stay in the parent tracking scope.
      if let AstKind::CallExpression(call) = member_node.kind()
        && let Some(callee) =
          resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Setup)
        && matches!(callee.as_str(), "unref" | "toValue")
        && let Some(argument) = call.arguments.first().and_then(Argument::as_expression)
        && let Some(identifier) = argument.get_identifier_reference()
      {
        let (_, outside_tracking) =
          scope_context(semantic, scope_id, member_id, call.span, imported_bindings)?;
        let binding = reactive_bindings.iter().find(|binding| {
          binding.name == identifier.name.as_str()
            && reference_resolves_to_binding(semantic, identifier, binding, script_offset)
            && binding.kind.is_ref_like()
        })?;
        return Some(RawReactiveRead::local(
          member_id,
          binding.name.clone(),
          Some("value".into()),
          call.span,
          outside_tracking,
        ));
      }

      // vue-i18n `t`/`d`/`n`/`rt`/`te` from `useI18n()` — wrapWithDeps tracks
      // composer ambient refs. Handled after the member loop (multi-read inject).

      let (object, property, member_span) = match member_node.kind() {
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

      let (_, outside_tracking) =
        scope_context(semantic, scope_id, member_id, member_span, imported_bindings)?;

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
    })
    .collect::<Vec<_>>();

  // Bare identifier reads of Reactive / ShallowReactive bindings (Vue 3.5 props
  // destructure, `reactive()` locals). Ref-like still require `.value` / unref /
  // toValue above. Skip identifiers that are the object of a member expression —
  // those already contributed a member read.
  for (ident_id, ident_node) in semantic.nodes().iter_enumerated() {
    let AstKind::IdentifierReference(identifier) = ident_node.kind() else {
      continue;
    };
    if identifier_is_member_object(semantic, ident_id) {
      continue;
    }
    let Some(binding) = reactive_bindings.iter().find(|binding| {
      binding.name == identifier.name.as_str()
        && reference_resolves_to_binding(semantic, identifier, binding, script_offset)
        && !binding.kind.is_ref_like()
    }) else {
      continue;
    };
    let Some((_, outside_tracking)) =
      scope_context(semantic, scope_id, ident_id, identifier.span, imported_bindings)
    else {
      continue;
    };
    reads.push(RawReactiveRead::local(
      ident_id,
      binding.name.clone(),
      None,
      identifier.span,
      outside_tracking,
    ));
  }

  // Named API bag methods (`const { t } = useI18n()`): inject precomputed ambient reads.
  for (call_id, call_node) in semantic.nodes().iter_enumerated() {
    let AstKind::CallExpression(call) = call_node.kind() else {
      continue;
    };
    let Some(identifier) = call.callee.get_identifier_reference() else {
      continue;
    };
    let Some(ambient) = resolve_ambient_call_handle(semantic, identifier, ambient_call_handles)
    else {
      continue;
    };
    let Some((_, outside_tracking)) =
      scope_context(semantic, scope_id, call_id, call.span, imported_bindings)
    else {
      continue;
    };
    for (binding, property) in ambient {
      reads.push(RawReactiveRead::local(
        call_id,
        binding.clone(),
        property.clone(),
        call.span,
        outside_tracking,
      ));
    }
  }

  reads
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

/// Per-tracking-scope control-flow facts built once, then used to classify reads.
pub(super) struct TrackingScopeIR {
  /// Ends of top-level `await` expressions owned by this scope (sorted ascending).
  await_ends: Vec<u32>,
  /// `(call_end, is_pause)` pause/resume events owned by this scope (sorted by end).
  pause_events: Vec<(u32, bool)>,
}

pub(super) fn build_tracking_scope_ir(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> TrackingScopeIR {
  let mut await_ends = Vec::new();
  let mut pause_events = Vec::new();
  for (node_id, node) in semantic.nodes().iter_enumerated() {
    match node.kind() {
      AstKind::AwaitExpression(await_expression) => {
        if scope_owns_await(semantic, scope_id, node_id) {
          await_ends.push(await_expression.span.end);
        }
      }
      AstKind::CallExpression(call) => {
        if !scope_owns_pause_call(semantic, scope_id, call.node_id.get()) {
          continue;
        }
        if is_pause_tracking_call(semantic, call, imported_bindings) {
          pause_events.push((call.span.end, true));
        } else if is_resume_tracking_call(semantic, call, imported_bindings) {
          pause_events.push((call.span.end, false));
        }
      }
      _ => {}
    }
  }
  await_ends.sort_unstable();
  pause_events.sort_by_key(|(end, _)| *end);
  TrackingScopeIR { await_ends, pause_events }
}

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

pub(super) fn scope_owns_pause_call(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  call_id: NodeId,
) -> bool {
  for ancestor_id in semantic.nodes().ancestor_ids(call_id) {
    if ancestor_id == scope_id {
      return true;
    }
    if matches!(
      semantic.nodes().kind(ancestor_id),
      AstKind::ArrowFunctionExpression(_) | AstKind::Function(_)
    ) {
      return false;
    }
  }
  false
}

pub(super) fn is_after_top_level_await_ir(ir: &TrackingScopeIR, read: &RawReactiveRead) -> bool {
  // Any await that fully precedes the read.
  let index = ir.await_ends.partition_point(|&end| end <= read.span.start);
  index > 0
}

pub(super) fn is_after_pause_tracking_ir(ir: &TrackingScopeIR, read: &RawReactiveRead) -> bool {
  let mut paused = false;
  for (end, is_pause) in &ir.pause_events {
    if *end > read.span.start {
      break;
    }
    paused = *is_pause;
  }
  paused
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
}

pub(super) fn classify_scope_reads(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  body: Option<&FunctionBody<'_>>,
  raw_reads: &[RawReactiveRead],
  sfc_source: &str,
  script_offset: usize,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> Vec<ReactiveReadFact> {
  if raw_reads.is_empty() {
    return Vec::new();
  }
  let ir = build_tracking_scope_ir(semantic, scope_id, imported_bindings);
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
        ir: &ir,
      })
    })
    .collect()
}

fn owning_function_id(semantic: &Semantic<'_>, node_id: NodeId) -> Option<NodeId> {
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

/// Guards on the read inside its owning function, plus caller guards at each
/// follow hop. A hop with any unguarded (or both-arm-covered) call site adds
/// nothing — that hop does not constrain the read.
fn classify_path_guards(input: &ClassifyRead<'_>) -> Vec<RawGuard> {
  let local_scope =
    owning_function_id(input.semantic, input.read.node_id).unwrap_or(input.scope_id);
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
  let owner = owning_function_id(semantic, first).unwrap_or(first);
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

pub(super) fn classify_read(input: &ClassifyRead<'_>) -> ReactiveReadFact {
  let outside = input.read.outside_tracking || is_after_pause_tracking_ir(input.ir, input.read);
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
