//! Soft evidence: unclassified accesses inside tracking scopes and watch sources.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::{
  AstKind,
  ast::{Argument, Expression, FunctionBody, IdentifierReference},
};
use oxc_semantic::NodeId;
use oxc_span::Span;
use vue_vet_core::{ReactiveBindingFact, ReactiveReadFact, ReactiveReadKind, ScriptKind};

use super::{
  ComposableShapeMap, DEEP_WATCH_PROPERTY,
  bindings::AmbientCallHandles,
  context::{is_sync_hof_callback_param, scope_context},
  follow::{FollowOutside, follow_local_callees},
  kinds::{reference_resolves_to_binding, resolved_vue_callee, source_span},
  reads::{classify_scope_reads, collect_scope_reads},
};

/// Soft evidence inside a scope: reactivity-shaped accesses we could not classify.
///
/// Surfaced so absence rules can say `(maybe: name)` after trying to find hard edges.
/// Shares [`follow_local_callees`] with hard reads so `computed(() => load())`
/// cannot disagree with an inlined getter.
pub(super) fn collect_uncertain_scope_accesses(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_offset: usize,
) -> Vec<String> {
  let mut visiting = BTreeSet::new();
  visiting.insert(scope_id);
  collect_uncertain_scope_accesses_bounded(
    semantic,
    scope_id,
    reactive_bindings,
    composable_instances,
    imported_bindings,
    script_offset,
    0,
    &mut visiting,
  )
  .into_iter()
  .collect()
}

#[expect(clippy::too_many_arguments, reason = "bounded collector threads scope + visit state")]
pub(super) fn collect_uncertain_scope_accesses_bounded(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_offset: usize,
  depth: u32,
  visiting: &mut BTreeSet<NodeId>,
) -> BTreeSet<String> {
  let mut names = collect_uncertain_scope_accesses_local(
    semantic,
    scope_id,
    reactive_bindings,
    composable_instances,
    imported_bindings,
    script_offset,
  );
  follow_local_callees(
    semantic,
    scope_id,
    imported_bindings,
    depth,
    visiting,
    FollowOutside::Skip,
    |callee_id, _, next_depth, visiting| {
      names.extend(collect_uncertain_scope_accesses_bounded(
        semantic,
        callee_id,
        reactive_bindings,
        composable_instances,
        imported_bindings,
        script_offset,
        next_depth,
        visiting,
      ));
    },
  );
  names
}

pub(super) fn collect_uncertain_scope_accesses_local(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_offset: usize,
) -> BTreeSet<String> {
  let mut names = BTreeSet::new();
  for (node_id, node) in semantic.nodes().iter_enumerated() {
    let Some((name, span)) = uncertain_access_at(
      semantic,
      node.kind(),
      reactive_bindings,
      composable_instances,
      imported_bindings,
      script_offset,
    ) else {
      continue;
    };
    if scope_context(semantic, scope_id, node_id, span, imported_bindings).is_none() {
      continue;
    }
    names.insert(name);
  }
  names
}

pub(super) fn uncertain_access_at(
  semantic: &oxc_semantic::Semantic<'_>,
  kind: AstKind<'_>,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_offset: usize,
) -> Option<(String, Span)> {
  match kind {
    AstKind::StaticMemberExpression(member) if member.property.name.as_str() == "value" => {
      let root = member_expression_root_identifier(&member.object)?;
      let known_binding = reactive_bindings.iter().any(|binding| {
        binding.name == root.name.as_str()
          && reference_resolves_to_binding(semantic, root, binding, script_offset)
      });
      let known_bag = composable_instances.contains_key(root.name.as_str());
      if known_binding || known_bag {
        return None;
      }
      // Sync Array/String HOF callback params almost always use `.value` as a
      // plain data field (`OPTIONS.map(o => o.value)`), not a Ref unwrap.
      // Typed Ref formals still classify via `known_binding` above.
      if is_sync_hof_callback_param(semantic, root) {
        return None;
      }
      Some((root.name.to_string(), member.span))
    }
    AstKind::CallExpression(call) => {
      let callee =
        resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Setup)?;
      if !matches!(callee.as_str(), "unref" | "toValue") {
        return None;
      }
      let argument = call.arguments.first().and_then(Argument::as_expression)?;
      let identifier = argument.get_identifier_reference()?;
      let known = reactive_bindings.iter().any(|binding| {
        binding.name == identifier.name.as_str()
          && reference_resolves_to_binding(semantic, identifier, binding, script_offset)
          && binding.kind.is_ref_like()
      });
      (!known).then(|| (identifier.name.to_string(), call.span))
    }
    _ => None,
  }
}

pub(super) fn member_expression_root_identifier<'a>(
  expression: &'a Expression<'a>,
) -> Option<&'a IdentifierReference<'a>> {
  match expression {
    Expression::Identifier(identifier) => Some(identifier),
    Expression::ParenthesizedExpression(paren) => {
      member_expression_root_identifier(&paren.expression)
    }
    Expression::StaticMemberExpression(member) => member_expression_root_identifier(&member.object),
    Expression::ChainExpression(chain) => match &chain.expression {
      oxc_ast::ast::ChainElement::StaticMemberExpression(member) => {
        member_expression_root_identifier(&member.object)
      }
      _ => None,
    },
    _ => None,
  }
}

/// Soft evidence in `watch` sources: unclassified `.value` / bare unknown idents.
pub(super) fn collect_uncertain_watch_sources(
  semantic: &oxc_semantic::Semantic<'_>,
  argument: &Argument<'_>,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_offset: usize,
) -> Vec<String> {
  let mut names = BTreeSet::new();
  collect_uncertain_watch_argument(
    semantic,
    argument,
    reactive_bindings,
    composable_instances,
    imported_bindings,
    script_offset,
    &mut names,
  );
  names.into_iter().collect()
}

pub(super) fn collect_uncertain_watch_argument(
  semantic: &oxc_semantic::Semantic<'_>,
  argument: &Argument<'_>,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_offset: usize,
  names: &mut BTreeSet<String>,
) {
  match argument {
    Argument::ArrowFunctionExpression(callback) => {
      names.extend(collect_uncertain_scope_accesses(
        semantic,
        callback.node_id.get(),
        reactive_bindings,
        composable_instances,
        imported_bindings,
        script_offset,
      ));
    }
    Argument::FunctionExpression(callback) => {
      names.extend(collect_uncertain_scope_accesses(
        semantic,
        callback.node_id.get(),
        reactive_bindings,
        composable_instances,
        imported_bindings,
        script_offset,
      ));
    }
    Argument::ArrayExpression(array) => {
      for element in &array.elements {
        let Some(expression) = element.as_expression() else {
          continue;
        };
        collect_uncertain_watch_expression(
          semantic,
          expression,
          reactive_bindings,
          composable_instances,
          imported_bindings,
          script_offset,
          names,
        );
      }
    }
    other => {
      if let Some(expression) = other.as_expression() {
        collect_uncertain_watch_expression(
          semantic,
          expression,
          reactive_bindings,
          composable_instances,
          imported_bindings,
          script_offset,
          names,
        );
      }
    }
  }
}

pub(super) fn collect_uncertain_watch_expression(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_offset: usize,
  names: &mut BTreeSet<String>,
) {
  match expression {
    Expression::ArrowFunctionExpression(callback) => {
      names.extend(collect_uncertain_scope_accesses(
        semantic,
        callback.node_id.get(),
        reactive_bindings,
        composable_instances,
        imported_bindings,
        script_offset,
      ));
    }
    Expression::FunctionExpression(callback) => {
      names.extend(collect_uncertain_scope_accesses(
        semantic,
        callback.node_id.get(),
        reactive_bindings,
        composable_instances,
        imported_bindings,
        script_offset,
      ));
    }
    Expression::Identifier(identifier) => {
      let known = reactive_bindings.iter().any(|binding| {
        binding.name == identifier.name.as_str()
          && reference_resolves_to_binding(semantic, identifier, binding, script_offset)
      });
      if !known {
        names.insert(identifier.name.to_string());
      }
    }
    Expression::ParenthesizedExpression(paren) => {
      collect_uncertain_watch_expression(
        semantic,
        &paren.expression,
        reactive_bindings,
        composable_instances,
        imported_bindings,
        script_offset,
        names,
      );
    }
    Expression::StaticMemberExpression(member) if member.property.name.as_str() == "value" => {
      let Some(root) = member_expression_root_identifier(&member.object) else {
        return;
      };
      let known_ref = reactive_bindings.iter().any(|binding| {
        binding.name == root.name.as_str()
          && reference_resolves_to_binding(semantic, root, binding, script_offset)
          && binding.kind.is_ref_like()
      });
      let known_bag = composable_instances.contains_key(root.name.as_str());
      if !known_ref && !known_bag && !is_sync_hof_callback_param(semantic, root) {
        names.insert(root.name.to_string());
      }
    }
    Expression::CallExpression(call) => {
      let Some(callee) =
        resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Setup)
      else {
        return;
      };
      if !matches!(callee.as_str(), "unref" | "toValue") {
        return;
      }
      let Some(argument) = call.arguments.first().and_then(Argument::as_expression) else {
        return;
      };
      let Some(identifier) = argument.get_identifier_reference() else {
        return;
      };
      let known = reactive_bindings.iter().any(|binding| {
        binding.name == identifier.name.as_str()
          && reference_resolves_to_binding(semantic, identifier, binding, script_offset)
          && binding.kind.is_ref_like()
      });
      if !known {
        names.insert(identifier.name.to_string());
      }
    }
    _ => {}
  }
}

#[expect(clippy::too_many_arguments, reason = "watch sources share scope-read context fields")]
pub(super) fn collect_watch_source_reads(
  semantic: &oxc_semantic::Semantic<'_>,
  argument: &Argument<'_>,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  imported_bindings: &BTreeMap<String, (String, String)>,
  ambient_call_handles: &AmbientCallHandles,
  sfc_source: &str,
  script_offset: usize,
) -> Vec<ReactiveReadFact> {
  let ctx = WatchSourceCtx {
    semantic,
    reactive_bindings,
    composable_instances,
    imported_bindings,
    ambient_call_handles,
    sfc_source,
    script_offset,
  };
  match argument {
    Argument::ArrowFunctionExpression(callback) => {
      collect_watch_getter_reads(&ctx, callback.node_id.get(), Some(&*callback.body))
    }
    Argument::FunctionExpression(callback) => {
      collect_watch_getter_reads(&ctx, callback.node_id.get(), callback.body.as_deref())
    }
    Argument::ArrayExpression(array) => {
      // `watch([a, () => b.value, () => c.value])` — each element is a source.
      let mut reads = Vec::new();
      for element in &array.elements {
        let Some(expression) = element.as_expression() else {
          continue;
        };
        match expression {
          Expression::ArrowFunctionExpression(callback) => {
            reads.extend(collect_watch_getter_reads(
              &ctx,
              callback.node_id.get(),
              Some(&*callback.body),
            ));
          }
          Expression::FunctionExpression(callback) => {
            reads.extend(collect_watch_getter_reads(
              &ctx,
              callback.node_id.get(),
              callback.body.as_deref(),
            ));
          }
          other => {
            collect_expression_source_reads(
              ctx.semantic,
              other,
              ctx.reactive_bindings,
              ctx.sfc_source,
              ctx.script_offset,
              &mut reads,
            );
          }
        }
      }
      reads.sort_by_key(|read| read.span.offset);
      reads
    }
    argument => {
      let mut reads = Vec::new();
      if let Some(expression) = argument.as_expression() {
        match expression {
          Expression::ArrowFunctionExpression(callback) => {
            return collect_watch_getter_reads(&ctx, callback.node_id.get(), Some(&*callback.body));
          }
          Expression::FunctionExpression(callback) => {
            return collect_watch_getter_reads(
              &ctx,
              callback.node_id.get(),
              callback.body.as_deref(),
            );
          }
          other => {
            collect_expression_source_reads(
              ctx.semantic,
              other,
              ctx.reactive_bindings,
              ctx.sfc_source,
              ctx.script_offset,
              &mut reads,
            );
          }
        }
      }
      reads
    }
  }
}

pub(super) struct WatchSourceCtx<'a> {
  semantic: &'a oxc_semantic::Semantic<'a>,
  reactive_bindings: &'a [ReactiveBindingFact],
  composable_instances: &'a ComposableShapeMap,
  imported_bindings: &'a BTreeMap<String, (String, String)>,
  ambient_call_handles: &'a AmbientCallHandles,
  sfc_source: &'a str,
  script_offset: usize,
}

/// Reads collected from a `watch` source getter function body.
pub(super) fn collect_watch_getter_reads(
  ctx: &WatchSourceCtx<'_>,
  scope_id: NodeId,
  body: Option<&FunctionBody<'_>>,
) -> Vec<ReactiveReadFact> {
  let raw_reads = collect_scope_reads(
    ctx.semantic,
    scope_id,
    ctx.reactive_bindings,
    ctx.composable_instances,
    ctx.imported_bindings,
    ctx.ambient_call_handles,
    ctx.script_offset,
  );
  classify_scope_reads(
    ctx.semantic,
    scope_id,
    body,
    &raw_reads,
    ctx.sfc_source,
    ctx.script_offset,
    ctx.imported_bindings,
  )
}

pub(super) fn collect_expression_source_reads(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  reactive_bindings: &[ReactiveBindingFact],
  sfc_source: &str,
  script_offset: usize,
  reads: &mut Vec<ReactiveReadFact>,
) {
  match expression {
    Expression::Identifier(identifier) => {
      if let Some(binding) = reactive_bindings.iter().find(|binding| {
        binding.name == identifier.name.as_str()
          && reference_resolves_to_binding(semantic, identifier, binding, script_offset)
      }) {
        // Vue's `watch(ref)` / `watch([ref])` tracks the ref's `.value` dep key.
        // Bare `watch(reactiveObj)` deep-tracks many keys at runtime; emit a single
        // deep-root sentinel `property: "*"` rather than inventing nested fields.
        if binding.kind.is_ref_like() {
          reads.push(ReactiveReadFact {
            binding: binding.name.clone(),
            property: Some("value".into()),
            kind: ReactiveReadKind::Unconditional,
            guards: Vec::new(),
            guarded_by: None,
            span: source_span(sfc_source, script_offset, identifier.span),
          });
        } else if binding.kind.is_deep_watch_source() {
          reads.push(ReactiveReadFact {
            binding: binding.name.clone(),
            property: Some(DEEP_WATCH_PROPERTY.into()),
            kind: ReactiveReadKind::Unconditional,
            guards: Vec::new(),
            guarded_by: None,
            span: source_span(sfc_source, script_offset, identifier.span),
          });
        }
      }
    }
    Expression::StaticMemberExpression(member) => {
      if let Some(object) = member.object.get_identifier_reference()
        && let Some(binding) = reactive_bindings.iter().find(|binding| {
          binding.name == object.name.as_str()
            && reference_resolves_to_binding(semantic, object, binding, script_offset)
            && (!binding.kind.is_ref_like() || member.property.name.as_str() == "value")
        })
      {
        reads.push(ReactiveReadFact {
          binding: binding.name.clone(),
          property: Some(member.property.name.to_string()),
          kind: ReactiveReadKind::Unconditional,
          guards: Vec::new(),
          guarded_by: None,
          span: source_span(sfc_source, script_offset, member.span),
        });
      }
    }
    Expression::ComputedMemberExpression(member) => {
      if let Some(object) = member.object.get_identifier_reference()
        && let Some(property) = member.static_property_name()
        && let Some(binding) = reactive_bindings.iter().find(|binding| {
          binding.name == object.name.as_str()
            && reference_resolves_to_binding(semantic, object, binding, script_offset)
            && (!binding.kind.is_ref_like() || property == "value")
        })
      {
        reads.push(ReactiveReadFact {
          binding: binding.name.clone(),
          property: Some(property.to_string()),
          kind: ReactiveReadKind::Unconditional,
          guards: Vec::new(),
          guarded_by: None,
          span: source_span(sfc_source, script_offset, member.span),
        });
      }
    }
    Expression::ArrayExpression(array) => {
      for element in &array.elements {
        if let Some(inner) = element.as_expression() {
          collect_expression_source_reads(
            semantic,
            inner,
            reactive_bindings,
            sfc_source,
            script_offset,
            reads,
          );
        }
      }
    }
    _ => {}
  }
}
