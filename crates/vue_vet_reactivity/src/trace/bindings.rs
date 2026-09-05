//! Reactive binding collectors: Vue APIs, typed annotations, props, aliases, routes.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::{
  AstKind,
  ast::{Argument, BindingPattern, Expression, ObjectPropertyKind, PropertyKey},
};
use oxc_semantic::{NodeId, Semantic};
use oxc_span::Span;
use vue_vet_core::{ReactiveBindingFact, ReactiveBindingKind, ScriptKind};

use super::{
  expr,
  kinds::{
    collect_binding_identifiers, identifier_reference_is_unresolved, reactive_binding_kind,
    reference_resolves_to_binding, resolved_vue_callee, source_span,
  },
  plugin::{NamedApiBag, is_named_api_bag_callee, named_api_bag},
  render, summary,
};

/// Seed bindings from TypeScript ref-like annotations on parameters / declarators.
///
/// Example: `function useDetail(type: ComputedRef<T>)` so `type.value` classifies
/// instead of becoming `uncertain_accesses` / `(maybe: type)`.
pub(super) fn collect_typed_reactive_bindings(
  semantic: &oxc_semantic::Semantic<'_>,
  sfc_source: &str,
  script_offset: usize,
) -> Vec<ReactiveBindingFact> {
  let mut bindings = Vec::new();
  for node in semantic.nodes() {
    match node.kind() {
      AstKind::FormalParameter(parameter) => {
        let Some(annotation) = parameter.type_annotation.as_ref() else {
          continue;
        };
        let Some(kind) = summary::ts_type_reactive_kind(&annotation.type_annotation) else {
          continue;
        };
        let BindingPattern::BindingIdentifier(identifier) = &parameter.pattern else {
          continue;
        };
        bindings.push(ReactiveBindingFact {
          name: identifier.name.to_string(),
          kind,
          initialized_with_null: false,
          alias_of: None,
          span: source_span(sfc_source, script_offset, identifier.span),
        });
      }
      AstKind::VariableDeclarator(declarator) => {
        // `const x: Ref<T> = …` or `const x = useVModel(…) as Ref<T>`.
        let kind = declarator
          .type_annotation
          .as_ref()
          .and_then(|annotation| summary::ts_type_reactive_kind(&annotation.type_annotation))
          .or_else(|| {
            declarator.init.as_ref().and_then(|init| match init {
              Expression::TSAsExpression(assertion) => {
                summary::ts_type_reactive_kind(&assertion.type_annotation)
              }
              Expression::TSTypeAssertion(assertion) => {
                summary::ts_type_reactive_kind(&assertion.type_annotation)
              }
              _ => None,
            })
          });
        let Some(kind) = kind else {
          continue;
        };
        let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
          continue;
        };
        bindings.push(ReactiveBindingFact {
          name: identifier.name.to_string(),
          kind,
          initialized_with_null: false,
          alias_of: None,
          span: source_span(sfc_source, script_offset, identifier.span),
        });
      }
      _ => {}
    }
  }
  bindings
}

/// Seed the props parameter of component factories / `setup` as a reactive bag.
///
/// Covers `defineComponent((props) => …)` and `defineComponent({ setup(props) })`.
/// Same-file identity / setup-forward wrappers and seeded cross-module
/// `ComponentFactory` imports also seed; opaque helpers stay quiet.
pub(super) fn collect_component_props_bindings(
  semantic: &oxc_semantic::Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  sfc_source: &str,
  script_offset: usize,
  seeded_factories: &BTreeSet<String>,
) -> Vec<ReactiveBindingFact> {
  let mut factories = render::component_factories_including_bare(semantic, imported_bindings);
  if sfc_source.contains("defineComponent") {
    factories.extend(render::component_factory_wrapper_locals(semantic, imported_bindings));
  }
  factories.extend(seeded_factories.iter().cloned());
  let mut bindings = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    if !is_component_factory_callee(&call.callee, &factories) {
      continue;
    }
    let Some(first) = call.arguments.first().and_then(Argument::as_expression) else {
      continue;
    };
    match first {
      Expression::ArrowFunctionExpression(arrow) => {
        seed_first_formal_as_reactive(
          arrow.params.items.first(),
          sfc_source,
          script_offset,
          &mut bindings,
        );
      }
      Expression::FunctionExpression(function) => {
        seed_first_formal_as_reactive(
          function.params.items.first(),
          sfc_source,
          script_offset,
          &mut bindings,
        );
      }
      Expression::ObjectExpression(object) => {
        for property in &object.properties {
          let ObjectPropertyKind::ObjectProperty(property) = property else {
            continue;
          };
          let is_setup = match &property.key {
            PropertyKey::StaticIdentifier(identifier) => identifier.name == "setup",
            PropertyKey::StringLiteral(literal) => literal.value == "setup",
            _ => false,
          };
          if !is_setup {
            continue;
          }
          match &property.value {
            Expression::ArrowFunctionExpression(arrow) => {
              seed_first_formal_as_reactive(
                arrow.params.items.first(),
                sfc_source,
                script_offset,
                &mut bindings,
              );
            }
            Expression::FunctionExpression(function) => {
              seed_first_formal_as_reactive(
                function.params.items.first(),
                sfc_source,
                script_offset,
                &mut bindings,
              );
            }
            _ => {}
          }
        }
      }
      _ => {}
    }
  }
  bindings
}

pub(super) fn is_component_factory_callee(
  callee: &Expression<'_>,
  factories: &BTreeSet<String>,
) -> bool {
  callee
    .get_identifier_reference()
    .is_some_and(|identifier| factories.contains(identifier.name.as_str()))
}

pub(super) fn seed_first_formal_as_reactive(
  parameter: Option<&oxc_ast::ast::FormalParameter<'_>>,
  sfc_source: &str,
  script_offset: usize,
  bindings: &mut Vec<ReactiveBindingFact>,
) {
  let Some(parameter) = parameter else {
    return;
  };
  let BindingPattern::BindingIdentifier(identifier) = &parameter.pattern else {
    return;
  };
  if bindings.iter().any(|binding| {
    binding.name == identifier.name.as_str()
      && binding.span.offset == source_span(sfc_source, script_offset, identifier.span).offset
  }) {
    return;
  }
  bindings.push(ReactiveBindingFact {
    name: identifier.name.to_string(),
    kind: ReactiveBindingKind::Reactive,
    initialized_with_null: false,
    alias_of: None,
    span: source_span(sfc_source, script_offset, identifier.span),
  });
}

/// Propagate known reactive bindings through `const alias = known` (under-approx).
///
/// Only bare identifier initials; `alias = known.value` / calls stay quiet.
pub(super) fn extend_with_reactive_aliases(
  semantic: &oxc_semantic::Semantic<'_>,
  bindings: &mut Vec<ReactiveBindingFact>,
  sfc_source: &str,
  script_offset: usize,
) {
  loop {
    let mut added = Vec::new();
    for node in semantic.nodes() {
      let AstKind::VariableDeclarator(declarator) = node.kind() else {
        continue;
      };
      let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
        continue;
      };
      if bindings.iter().any(|binding| binding.name == identifier.name.as_str()) {
        continue;
      }
      let Some(init) = &declarator.init else {
        continue;
      };
      let Some(reference) = init.get_identifier_reference() else {
        continue;
      };
      let Some(source) = bindings.iter().find(|binding| {
        binding.name == reference.name.as_str()
          && reference_resolves_to_binding(semantic, reference, binding, script_offset)
      }) else {
        continue;
      };
      added.push(ReactiveBindingFact {
        name: identifier.name.to_string(),
        kind: source.kind,
        initialized_with_null: source.initialized_with_null,
        alias_of: Some(source.alias_of.clone().unwrap_or_else(|| source.name.clone())),
        span: source_span(sfc_source, script_offset, identifier.span),
      });
    }
    if added.is_empty() {
      break;
    }
    for binding in added {
      if !bindings.iter().any(|existing| existing.name == binding.name) {
        bindings.push(binding);
      }
    }
  }
}

/// Result of walking reactive binding seeds in one script.
pub(super) struct CollectedBindings {
  pub(super) bindings: Vec<ReactiveBindingFact>,
  /// Locals that are ambient-on-call methods of a named API bag (`t` from `useI18n`).
  pub(super) ambient_call_handles: AmbientCallHandles,
}

/// One local method handle: calling it injects the listed ambient reads.
#[derive(Clone, Debug)]
pub(super) struct AmbientCallHandle {
  /// Owning `VariableDeclarator` node (shared by a destructure pattern).
  pub(super) site_decl: NodeId,
  pub(super) ambient: Vec<(String, Option<String>)>,
}

/// Local name → handles (usually one; multi-site same name is rare).
pub(super) type AmbientCallHandles = BTreeMap<String, Vec<AmbientCallHandle>>;

pub(super) fn collect_reactive_bindings(
  semantic: &oxc_semantic::Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
  include_nested: bool,
  named_api_bags: &[NamedApiBag],
) -> CollectedBindings {
  let mut reactive_bindings = Vec::new();
  let mut ambient_call_handles = AmbientCallHandles::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = resolved_binding_callee(
      semantic,
      &call.callee,
      imported_bindings,
      script_kind,
      named_api_bags,
    ) else {
      continue;
    };
    let Some(declarator) = variable_declarator_for_call(semantic, call.node_id.get()) else {
      continue;
    };
    if !include_nested && expr::is_nested_in_function(semantic, call.node_id.get()) {
      continue;
    }

    // Named API bags from plugins: field seeds + ambient-on-call methods.
    if let Some(api) = named_api_bag(named_api_bags, &callee) {
      if let BindingPattern::ObjectPattern(pattern) = &declarator.id {
        seed_named_api_bag_destructure(
          api,
          pattern,
          call,
          // Parent of the call is the declarator (or await→declarator).
          variable_declarator_node_id(semantic, call.node_id.get()),
          sfc_source,
          script_offset,
          &mut reactive_bindings,
          &mut ambient_call_handles,
        );
      }
      continue;
    }

    // `const props = withDefaults(defineProps(...), defaults)` — binding is the
    // outer call's assignee; peel defineProps for the reactive kind.
    let binding_kind = if callee == "withDefaults" {
      let Some(inner) = call.arguments.first().and_then(Argument::as_expression) else {
        continue;
      };
      let Expression::CallExpression(inner_call) = inner else {
        continue;
      };
      let Some(inner_callee) = resolved_binding_callee(
        semantic,
        &inner_call.callee,
        imported_bindings,
        script_kind,
        named_api_bags,
      ) else {
        continue;
      };
      if inner_callee != "defineProps" {
        continue;
      }
      ReactiveBindingKind::Reactive
    } else {
      let Some(kind) = reactive_binding_kind(&callee) else {
        continue;
      };
      kind
    };

    let mut identifiers = Vec::new();
    if matches!(callee.as_str(), "toRefs" | "storeToRefs" | "defineModels") {
      // `const { count, name } = storeToRefs(store)` / `toRefs(obj)` /
      // `const { modelValue } = defineModels<{…}>()` → each local is ref-like.
      if matches!(&declarator.id, BindingPattern::ObjectPattern(_)) {
        collect_binding_identifiers(&declarator.id, &mut identifiers);
      }
    } else if matches!(callee.as_str(), "defineProps")
      || (callee == "withDefaults" && binding_kind == ReactiveBindingKind::Reactive)
    {
      // Vue 3.5+ reactive props destructure:
      // `const { account } = defineProps<{ account: Account }>()` and
      // `const { title, ...rest } = withDefaults(defineProps<…>(), …)`.
      // Each local tracks like a Reactive bag field (bare id, not `.value`).
      // Pre-3.5 projects still get `no-nonreactive-props-destructure`.
      match &declarator.id {
        BindingPattern::ObjectPattern(_) => {
          collect_binding_identifiers(&declarator.id, &mut identifiers);
        }
        BindingPattern::BindingIdentifier(identifier) => {
          identifiers.push((identifier.name.to_string(), identifier.span));
        }
        _ => {}
      }
    } else if let BindingPattern::BindingIdentifier(identifier) = &declarator.id {
      identifiers.push((identifier.name.to_string(), identifier.span));
    }

    let initialized_with_null =
      call.arguments.first().is_some_and(|argument| matches!(argument, Argument::NullLiteral(_)));
    for (name, span) in identifiers {
      reactive_bindings.push(ReactiveBindingFact {
        name,
        kind: binding_kind,
        initialized_with_null,
        alias_of: None,
        span: source_span(sfc_source, script_offset, span),
      });
    }
  }

  // `const params = useRoute().params` / `const params = route.params`.
  collect_route_slice_bindings(
    semantic,
    imported_bindings,
    script_kind,
    include_nested,
    sfc_source,
    script_offset,
    named_api_bags,
    &mut reactive_bindings,
  );

  // `const x = cond ? ref(false) : computed(() => …)` — both arms same reactive kind.
  collect_conditional_init_bindings(
    semantic,
    imported_bindings,
    script_kind,
    include_nested,
    sfc_source,
    script_offset,
    named_api_bags,
    &mut reactive_bindings,
  );

  CollectedBindings { bindings: reactive_bindings, ambient_call_handles }
}

/// Seed object-destructure of a [`NamedApiBag`]: reactive fields + ambient-on-call methods.
#[expect(clippy::too_many_arguments, reason = "seed needs call site + both output maps")]
pub(super) fn seed_named_api_bag_destructure(
  api: &NamedApiBag,
  pattern: &oxc_ast::ast::ObjectPattern<'_>,
  call: &oxc_ast::ast::CallExpression<'_>,
  site_decl: Option<NodeId>,
  sfc_source: &str,
  script_offset: usize,
  reactive_bindings: &mut Vec<ReactiveBindingFact>,
  ambient_call_handles: &mut AmbientCallHandles,
) {
  let Some(site_decl) = site_decl else {
    return;
  };

  // export_key → local binding names (and spans) for reactive fields.
  let mut field_locals: BTreeMap<String, Vec<(String, Span)>> = BTreeMap::new();
  let mut method_locals: Vec<String> = Vec::new();

  for property in &pattern.properties {
    let Some(key) = property.key.static_name() else {
      continue;
    };
    let key = key.into_owned();
    let mut identifiers = Vec::new();
    collect_binding_identifiers(&property.value, &mut identifiers);

    if api.ambient_methods.contains(&key.as_str()) {
      for (name, _) in &identifiers {
        if !method_locals.iter().any(|existing| existing == name) {
          method_locals.push(name.clone());
        }
      }
      // Methods are not reactive bindings.
      continue;
    }

    let Some(kind) = api.field_kind_of(&key) else {
      continue;
    };
    for (name, span) in identifiers {
      field_locals.entry(key.clone()).or_default().push((name.clone(), span));
      reactive_bindings.push(ReactiveBindingFact {
        name,
        kind,
        initialized_with_null: false,
        alias_of: None,
        span: source_span(sfc_source, script_offset, span),
      });
    }
  }

  if method_locals.is_empty() || api.ambient_fields.is_empty() {
    return;
  }

  // Resolve ambient reads for method calls.
  // Prefer co-destructured ambient field locals (under-approx: if the user
  // took `locale` but not `messages`, we only attribute `locale` — enough for
  // presence; missing edges stay quiet). When *no* ambient field was taken,
  // seed a site bag for the full ambient field set.
  let mut ambient: Vec<(String, Option<String>)> = Vec::new();
  for field in api.ambient_fields {
    if let Some(locals) = field_locals.get(*field) {
      for (local, _) in locals {
        // Field kinds from the contract are ref-like Computed → track `.value`.
        ambient.push((local.clone(), Some("value".into())));
      }
    }
  }

  if ambient.is_empty() {
    let call_span = source_span(sfc_source, script_offset, call.span);
    let site_name = api_site_binding_name(api.callee, call_span.offset);
    if !reactive_bindings.iter().any(|b| b.name == site_name) {
      reactive_bindings.push(ReactiveBindingFact {
        name: site_name.clone(),
        kind: ReactiveBindingKind::Reactive,
        initialized_with_null: false,
        alias_of: None,
        span: call_span,
      });
    }
    for field in api.ambient_fields {
      ambient.push((site_name.clone(), Some((*field).into())));
    }
  }

  if ambient.is_empty() {
    return;
  }
  for method in method_locals {
    ambient_call_handles
      .entry(method)
      .or_default()
      .push(AmbientCallHandle { site_decl, ambient: ambient.clone() });
  }
}

pub(super) fn api_site_binding_name(callee: &str, call_offset: usize) -> String {
  format!("{callee}@{call_offset}")
}

pub(super) fn variable_declarator_node_id(
  semantic: &oxc_semantic::Semantic<'_>,
  call_id: NodeId,
) -> Option<NodeId> {
  let parent = semantic.nodes().parent_id(call_id);
  match semantic.nodes().kind(parent) {
    AstKind::VariableDeclarator(_) => Some(parent),
    AstKind::AwaitExpression(_) => {
      let declarator = semantic.nodes().parent_id(parent);
      matches!(semantic.nodes().kind(declarator), AstKind::VariableDeclarator(_))
        .then_some(declarator)
    }
    _ => None,
  }
}

/// `const local = cond ? ref(a) : shallowRef(b)` when both arms share a reactive kind.
///
/// Under-approx: only Vue primitive callees (not unknown helpers). Missing one arm
/// stays quiet rather than inventing a binding from a single branch.
#[expect(clippy::too_many_arguments, reason = "conditional init shares binding-collection context")]
pub(super) fn collect_conditional_init_bindings(
  semantic: &oxc_semantic::Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_kind: ScriptKind,
  include_nested: bool,
  sfc_source: &str,
  script_offset: usize,
  named_api_bags: &[NamedApiBag],
  reactive_bindings: &mut Vec<ReactiveBindingFact>,
) {
  for node in semantic.nodes() {
    let AstKind::VariableDeclarator(declarator) = node.kind() else {
      continue;
    };
    if !include_nested && expr::is_nested_in_function(semantic, node.id()) {
      continue;
    }
    let Some(Expression::ConditionalExpression(cond)) = &declarator.init else {
      continue;
    };
    let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
      continue;
    };
    let Some(kind) = compatible_vue_binding_kind_from_arms(
      semantic,
      &cond.consequent,
      &cond.alternate,
      imported_bindings,
      script_kind,
      named_api_bags,
    ) else {
      continue;
    };
    let span = source_span(sfc_source, script_offset, identifier.span);
    if reactive_bindings
      .iter()
      .any(|binding| binding.name == identifier.name.as_str() && binding.span.offset == span.offset)
    {
      continue;
    }
    reactive_bindings.push(ReactiveBindingFact {
      name: identifier.name.to_string(),
      kind,
      initialized_with_null: false,
      alias_of: None,
      span,
    });
  }
}

pub(super) fn compatible_vue_binding_kind_from_arms(
  semantic: &Semantic<'_>,
  left: &Expression<'_>,
  right: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_kind: ScriptKind,
  named_api_bags: &[NamedApiBag],
) -> Option<ReactiveBindingKind> {
  let left_kind =
    vue_call_binding_kind(semantic, left, imported_bindings, script_kind, named_api_bags)?;
  let right_kind =
    vue_call_binding_kind(semantic, right, imported_bindings, script_kind, named_api_bags)?;
  if left_kind == right_kind {
    return Some(left_kind);
  }
  // Ref-like arms (ref vs computed vs shallowRef) all track via `.value`.
  // Distinct kinds merge to Ref (same contract as ternary export Known).
  if left_kind.is_ref_like() && right_kind.is_ref_like() {
    return Some(left_kind.merge_ref_like(right_kind));
  }
  None
}

pub(super) fn vue_call_binding_kind(
  semantic: &Semantic<'_>,
  expression: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_kind: ScriptKind,
  named_api_bags: &[NamedApiBag],
) -> Option<ReactiveBindingKind> {
  let mut current = expression;
  for _ in 0..4 {
    match current {
      Expression::ParenthesizedExpression(paren) => current = &paren.expression,
      Expression::TSAsExpression(assertion) => current = &assertion.expression,
      Expression::TSTypeAssertion(assertion) => current = &assertion.expression,
      Expression::TSNonNullExpression(non_null) => current = &non_null.expression,
      Expression::CallExpression(call) => {
        let callee = resolved_binding_callee(
          semantic,
          &call.callee,
          imported_bindings,
          script_kind,
          named_api_bags,
        )?;
        return reactive_binding_kind(&callee);
      }
      _ => return None,
    }
  }
  None
}

/// Parent `VariableDeclarator` for a call, peeling `await` (`const x = await useAsyncData()`).
pub(super) fn variable_declarator_for_call<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  call_id: NodeId,
) -> Option<&'a oxc_ast::ast::VariableDeclarator<'a>> {
  match semantic.nodes().parent_kind(call_id) {
    AstKind::VariableDeclarator(declarator) => Some(declarator),
    AstKind::AwaitExpression(_) => {
      // call → await → declarator
      let await_id = semantic.nodes().parent_id(call_id);
      match semantic.nodes().parent_kind(await_id) {
        AstKind::VariableDeclarator(declarator) => Some(declarator),
        _ => None,
      }
    }
    _ => None,
  }
}

/// Vue primitives plus bare auto-import helpers from the plugin API-bag catalog.
pub(super) fn resolved_binding_callee(
  semantic: &oxc_semantic::Semantic<'_>,
  callee: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  kind: ScriptKind,
  named_api_bags: &[NamedApiBag],
) -> Option<String> {
  if let Some(name) = resolved_vue_callee(semantic, callee, imported_bindings, kind) {
    return Some(name);
  }
  let identifier = callee.get_identifier_reference()?;
  let local = identifier.name.as_str();
  if !is_named_api_bag_callee(named_api_bags, local) {
    return None;
  }
  if !identifier_reference_is_unresolved(semantic, identifier)
    && !imported_bindings.contains_key(local)
  {
    // Local binding of the same name (rare) — leave quiet.
    return None;
  }
  // Bare auto-import or explicit import of a plugin-registered API bag.
  if imported_bindings.contains_key(local)
    || identifier_reference_is_unresolved(semantic, identifier)
  {
    return Some(local.into());
  }
  None
}

/// Seed `const params = useRoute().params` (and `route.params` when `route` is Reactive).
#[expect(clippy::too_many_arguments, reason = "route slice shares binding-collection context")]
pub(super) fn collect_route_slice_bindings(
  semantic: &oxc_semantic::Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_kind: ScriptKind,
  include_nested: bool,
  sfc_source: &str,
  script_offset: usize,
  named_api_bags: &[NamedApiBag],
  reactive_bindings: &mut Vec<ReactiveBindingFact>,
) {
  for (node_id, node) in semantic.nodes().iter_enumerated() {
    let AstKind::VariableDeclarator(declarator) = node.kind() else {
      continue;
    };
    if !include_nested && expr::is_nested_in_function(semantic, node_id) {
      continue;
    }
    let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
      continue;
    };
    if reactive_bindings.iter().any(|binding| binding.name == identifier.name.as_str()) {
      continue;
    }
    let Some(init) = &declarator.init else {
      continue;
    };
    let Expression::StaticMemberExpression(member) = init else {
      continue;
    };
    let property = member.property.name.as_str();
    if !matches!(property, "params" | "query" | "meta") {
      continue;
    }
    let from_use_route = match &member.object {
      Expression::CallExpression(call) => resolved_binding_callee(
        semantic,
        &call.callee,
        imported_bindings,
        script_kind,
        named_api_bags,
      )
      .is_some_and(|name| name == "useRoute"),
      Expression::Identifier(object) => reactive_bindings.iter().any(|binding| {
        binding.name == object.name.as_str()
          && matches!(
            binding.kind,
            ReactiveBindingKind::Reactive | ReactiveBindingKind::ShallowReactive
          )
      }),
      _ => false,
    };
    if !from_use_route {
      continue;
    }
    reactive_bindings.push(ReactiveBindingFact {
      name: identifier.name.to_string(),
      kind: ReactiveBindingKind::Reactive,
      initialized_with_null: false,
      alias_of: None,
      span: source_span(sfc_source, script_offset, identifier.span),
    });
  }
}
