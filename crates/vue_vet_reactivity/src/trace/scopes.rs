//! Tracking-scope and render-scope assembly.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::{
  AstKind,
  ast::{BindingPattern, Expression, FunctionBody},
};
use oxc_semantic::NodeId;
use vue_vet_core::{
  ReactiveBindingFact, ReactiveReadKind, ScriptKind, TrackingScopeFact, TrackingScopeKind,
};

use super::{
  ComposableShapeMap,
  bindings::AmbientCallHandles,
  follow::FileTraceIndex,
  kinds::{resolved_vue_callee, source_span},
  reads::{ScopeIrIndex, classify_scope_reads, collect_scope_reads},
  render,
  uncertain::{
    collect_uncertain_scope_accesses, collect_uncertain_watch_sources, collect_watch_source_gaps,
    collect_watch_source_reads,
  },
  writes::{
    callback_parts, collect_scope_writes, is_assignment_only_followed, tracking_callback_parts,
  },
};

/// Locals assigned from `effectScope()` (Vue import / `#imports` / namespace).
pub(super) fn effect_scope_instance_locals(
  semantic: &oxc_semantic::Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> BTreeSet<String> {
  let mut locals = BTreeSet::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) =
      resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Script)
    else {
      continue;
    };
    if callee != "effectScope" {
      continue;
    }
    if let Some(name) = assigned_binding_name(semantic, call.node_id.get()) {
      locals.insert(name);
    }
  }
  locals
}

pub(super) struct ScopeBuild<'a> {
  kind: TrackingScopeKind,
  callee: String,
  span: vue_vet_core::SourceSpan,
  binding: Option<String>,
  semantic: &'a oxc_semantic::Semantic<'a>,
  scope_id: NodeId,
  body: Option<&'a FunctionBody<'a>>,
  reactive_bindings: &'a [ReactiveBindingFact],
  composable_instances: &'a ComposableShapeMap,
  imported_bindings: &'a BTreeMap<String, (String, String)>,
  ambient_call_handles: &'a AmbientCallHandles,
  sfc_source: &'a str,
  script_offset: usize,
  force_outside_tracking: bool,
  index: &'a FileTraceIndex,
  ir: &'a ScopeIrIndex,
}

pub(super) fn finish_scope(build: ScopeBuild<'_>) -> TrackingScopeFact {
  let raw_reads = collect_scope_reads(
    build.semantic,
    build.scope_id,
    build.reactive_bindings,
    build.composable_instances,
    build.imported_bindings,
    build.ambient_call_handles,
    build.script_offset,
    build.index,
  );
  let mut reads = classify_scope_reads(
    build.semantic,
    build.scope_id,
    build.body,
    &raw_reads,
    build.sfc_source,
    build.script_offset,
    build.ir,
  );
  if build.force_outside_tracking {
    for read in &mut reads {
      read.kind = ReactiveReadKind::OutsideTracking;
      read.guards.clear();
      read.guarded_by = None;
    }
  }
  let writes = collect_scope_writes(
    build.semantic,
    build.scope_id,
    build.reactive_bindings,
    build.composable_instances,
    build.sfc_source,
    build.script_offset,
    build.index,
  );
  let uncertain_accesses = collect_uncertain_scope_accesses(
    build.semantic,
    build.scope_id,
    build.reactive_bindings,
    build.composable_instances,
    build.imported_bindings,
    build.script_offset,
    build.index,
  );
  let mut assignment_visiting = BTreeSet::new();
  assignment_visiting.insert(build.scope_id);
  let gaps = super::follow::collect_analysis_gaps(
    build.semantic,
    build.index,
    build.scope_id,
    build.imported_bindings,
  );
  TrackingScopeFact {
    kind: build.kind,
    callee: build.callee,
    span: build.span,
    reads,
    writes,
    assignment_only: is_assignment_only_followed(
      build.semantic,
      build.body,
      0,
      &mut assignment_visiting,
    ),
    binding: build.binding,
    uncertain_accesses,
    unknown_calls: gaps.unknown_calls,
    follow_truncated: gaps.truncated,
  }
}

#[expect(
  clippy::too_many_arguments,
  reason = "scope assembly threads the file trace index with existing collectors"
)]
pub(super) fn collect_tracking_scopes(
  semantic: &oxc_semantic::Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  ambient_call_handles: &AmbientCallHandles,
  sfc_source: &str,
  script_offset: usize,
  index: &FileTraceIndex,
  ir: &ScopeIrIndex,
) -> Vec<TrackingScopeFact> {
  // Only treat `.run(cb)` as an effect-scope body when the receiver was assigned
  // from Vue's `effectScope()` — never invent edges for arbitrary `.run` APIs.
  let effect_scope_locals = effect_scope_instance_locals(semantic, imported_bindings);
  let mut scopes = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };

    if let Expression::StaticMemberExpression(member) = &call.callee
      && member.property.name.as_str() == "run"
      && let Some(receiver) = member.object.get_identifier_reference()
      && effect_scope_locals.contains(receiver.name.as_str())
      && let Some(argument) = call.arguments.first()
      && let Some((scope_id, body)) = callback_parts(semantic, argument)
    {
      scopes.push(finish_scope(ScopeBuild {
        kind: TrackingScopeKind::EffectScope,
        callee: "effectScope.run".into(),
        span: source_span(sfc_source, script_offset, call.span),
        binding: None,
        semantic,
        scope_id,
        body,
        reactive_bindings,
        composable_instances,
        imported_bindings,
        ambient_call_handles,
        sfc_source,
        script_offset,
        force_outside_tracking: false,
        index,
        ir,
      }));
      continue;
    }

    let Some(callee) =
      resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Script)
    else {
      continue;
    };
    let Some(scope_kind) = TrackingScopeKind::from_vue_callee(&callee) else {
      continue;
    };

    match scope_kind {
      TrackingScopeKind::WatchEffect
      | TrackingScopeKind::WatchPostEffect
      | TrackingScopeKind::WatchSyncEffect
      | TrackingScopeKind::Computed
      | TrackingScopeKind::OnScopeDispose => {
        let Some(argument) = call.arguments.first() else {
          continue;
        };
        // `computed` accepts a getter function or `{ get, set }` object form.
        let Some((scope_id, body)) = (if scope_kind == TrackingScopeKind::Computed {
          tracking_callback_parts(semantic, argument)
        } else {
          callback_parts(semantic, argument)
        }) else {
          continue;
        };
        let binding = if scope_kind == TrackingScopeKind::Computed {
          assigned_binding_name(semantic, call.node_id.get())
        } else {
          None
        };
        scopes.push(finish_scope(ScopeBuild {
          kind: scope_kind,
          callee,
          span: source_span(sfc_source, script_offset, call.span),
          binding,
          semantic,
          scope_id,
          body,
          reactive_bindings,
          composable_instances,
          imported_bindings,
          ambient_call_handles,
          sfc_source,
          script_offset,
          force_outside_tracking: scope_kind == TrackingScopeKind::OnScopeDispose,
          index,
          ir,
        }));
      }
      TrackingScopeKind::EffectScope => {
        // effectScope(fn) or const s = effectScope(); s.run(fn)
        if let Some(argument) = call.arguments.first()
          && let Some((scope_id, body)) = callback_parts(semantic, argument)
        {
          scopes.push(finish_scope(ScopeBuild {
            kind: TrackingScopeKind::EffectScope,
            callee: callee.clone(),
            span: source_span(sfc_source, script_offset, call.span),
            binding: assigned_binding_name(semantic, call.node_id.get()),
            semantic,
            scope_id,
            body,
            reactive_bindings,
            composable_instances,
            imported_bindings,
            ambient_call_handles,
            sfc_source,
            script_offset,
            force_outside_tracking: false,
            index,
            ir,
          }));
        }
        // Also capture `.run(callback)` on effectScope instances via member call below.
      }
      TrackingScopeKind::WatchSources => {
        let Some(source_argument) = call.arguments.first() else {
          continue;
        };
        let call_span = source_span(sfc_source, script_offset, call.span);
        let reads = collect_watch_source_reads(
          semantic,
          source_argument,
          reactive_bindings,
          composable_instances,
          imported_bindings,
          ambient_call_handles,
          sfc_source,
          script_offset,
          index,
          ir,
        );
        let uncertain_accesses = collect_uncertain_watch_sources(
          semantic,
          source_argument,
          reactive_bindings,
          composable_instances,
          imported_bindings,
          script_offset,
          index,
        );
        let gaps = collect_watch_source_gaps(semantic, source_argument, imported_bindings, index);
        scopes.push(TrackingScopeFact {
          kind: TrackingScopeKind::WatchSources,
          callee: callee.clone(),
          span: call_span,
          reads,
          writes: Vec::new(),
          assignment_only: false,
          binding: None,
          uncertain_accesses,
          unknown_calls: gaps.unknown_calls,
          follow_truncated: gaps.truncated,
        });

        if let Some(callback_argument) = call.arguments.get(1)
          && let Some((scope_id, body)) = callback_parts(semantic, callback_argument)
        {
          scopes.push(finish_scope(ScopeBuild {
            kind: TrackingScopeKind::WatchCallback,
            callee,
            span: call_span,
            binding: None,
            semantic,
            scope_id,
            body,
            reactive_bindings,
            composable_instances,
            imported_bindings,
            ambient_call_handles,
            sfc_source,
            script_offset,
            force_outside_tracking: true,
            index,
            ir,
          }));
        }
      }
      TrackingScopeKind::WatchCallback | TrackingScopeKind::Render => {}
    }
  }
  scopes
}

pub(super) fn assigned_binding_name(
  semantic: &oxc_semantic::Semantic<'_>,
  call_id: NodeId,
) -> Option<String> {
  let AstKind::VariableDeclarator(declarator) = semantic.nodes().parent_kind(call_id) else {
    return None;
  };
  match &declarator.id {
    BindingPattern::BindingIdentifier(identifier) => Some(identifier.name.to_string()),
    _ => None,
  }
}

#[expect(
  clippy::too_many_arguments,
  reason = "scope assembly threads the file trace index with existing collectors"
)]
pub(super) fn collect_render_scopes(
  semantic: &oxc_semantic::Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  ambient_call_handles: &AmbientCallHandles,
  sfc_source: &str,
  script_offset: usize,
  index: &FileTraceIndex,
  ir: &ScopeIrIndex,
) -> Vec<TrackingScopeFact> {
  let mut scopes = Vec::new();
  for body in render::collect_render_bodies(semantic, imported_bindings) {
    scopes.push(finish_scope(ScopeBuild {
      kind: TrackingScopeKind::Render,
      callee: "render".into(),
      span: source_span(sfc_source, script_offset, body.span),
      binding: None,
      semantic,
      scope_id: body.scope_id,
      body: body.body,
      reactive_bindings,
      composable_instances,
      imported_bindings,
      ambient_call_handles,
      sfc_source,
      script_offset,
      force_outside_tracking: false,
      index,
      ir,
    }));
  }
  scopes
}
