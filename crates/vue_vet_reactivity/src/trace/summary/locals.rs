//! Phase one: classify each module-local name into an [`ExportState`].
//!
//! Function / arrow bodies go through [`super::return_kind`]; declared TypeScript
//! surfaces through [`super::declared_types`]; merges follow [`super::export_lattice`].

use std::collections::BTreeMap;

use oxc_ast::{
  AstKind,
  ast::{BindingPattern, Expression},
};
use oxc_semantic::NodeId;
use vue_vet_core::{ReactiveBindingKind, ReactivityGraph, ScriptKind};

use super::super::follow::local_function_id;
use super::super::kinds::{reactive_binding_kind, resolved_vue_callee};
use super::declared_types::{
  arrow_return_type_kind, composable_shape_from_ts_type, declared_return_for_arrow,
  declared_return_for_function, declared_return_from_declarator_annotation,
  function_return_type_kind, typeof_forward_from_declarator,
};
use super::export_lattice;
use super::model::{ComposableReturn, ComposableShape, DeclaredReturn, ExportState, ValueBagEntry};
use super::return_kind::{build_returns_by_function, composable_return_with_borrowed_index};

pub(super) fn collect_local_values(
  semantic: &oxc_semantic::Semantic<'_>,
  public_graph: &ReactivityGraph,
  shape_graph: &ReactivityGraph,
  script_offset: usize,
  span_source: &str,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> BTreeMap<String, ExportState> {
  let mut locals = public_graph
    .bindings
    .iter()
    .map(|binding| (binding.name.clone(), ExportState::Known(binding.kind)))
    .collect::<BTreeMap<_, _>>();

  // Lazy: modules with no function/composable candidates must not pay a full
  // return-statement index walk (cold `trace_1k_*` synthetic modules).
  let mut returns_by_function = None;

  // `defineComponent` setup wrappers → ComponentFactory (before composable Forward).
  // Cheap source gate keeps synthetic 1k modules off the wrapper AST walk.
  if span_source.contains("defineComponent") {
    for name in super::super::render::component_factory_wrapper_locals(semantic, imported_bindings)
    {
      locals.insert(name, ExportState::ComponentFactory);
    }
  }

  // `function useX() { return { field } }` / `return ref(0)` / `(): Ref<T>`
  for node in semantic.nodes() {
    let AstKind::Function(function) = node.kind() else {
      continue;
    };
    let Some(identifier) = &function.id else {
      continue;
    };
    let name = identifier.name.to_string();
    if matches!(locals.get(&name), Some(ExportState::ComponentFactory)) {
      continue;
    }
    let index = returns_by_function.get_or_insert_with(|| build_returns_by_function(semantic));
    let function_id = function.node_id.get();
    if let Some(state) = composable_export_state(
      semantic,
      function_id,
      shape_graph,
      script_offset,
      index,
      imported_bindings,
      function_return_type_kind(function),
      || declared_return_for_function(semantic, function),
    ) {
      insert_local_export_state(&mut locals, name, state);
    }
  }

  // `const useX = () => ({ … })` / `export declare const useX: () => T`
  for node in semantic.nodes() {
    let AstKind::VariableDeclarator(declarator) = node.kind() else {
      continue;
    };
    let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
      continue;
    };
    let name = identifier.name.to_string();
    // Keep graph-seeded `ref`/`computed`/… bindings; do not overwrite with call markers.
    if matches!(locals.get(&name), Some(ExportState::ComponentFactory | ExportState::Known(_))) {
      continue;
    }
    let state = match &declarator.init {
      Some(Expression::ArrowFunctionExpression(arrow)) => {
        let index = returns_by_function.get_or_insert_with(|| build_returns_by_function(semantic));
        composable_export_state(
          semantic,
          arrow.node_id.get(),
          shape_graph,
          script_offset,
          index,
          imported_bindings,
          arrow_return_type_kind(arrow),
          || {
            declared_return_for_arrow(semantic, arrow)
              .or_else(|| declared_return_from_declarator_annotation(semantic, declarator))
          },
        )
      }
      Some(Expression::FunctionExpression(function)) => {
        let index = returns_by_function.get_or_insert_with(|| build_returns_by_function(semantic));
        composable_export_state(
          semantic,
          function.node_id.get(),
          shape_graph,
          script_offset,
          index,
          imported_bindings,
          function_return_type_kind(function),
          || {
            declared_return_for_function(semantic, function)
              .or_else(|| declared_return_from_declarator_annotation(semantic, declarator))
          },
        )
      }
      // `export declare const useX: () => T` / `const useX: typeof useY` — no init.
      None => typeof_forward_from_declarator(declarator).or_else(|| {
        combine_composable_export(
          None,
          declared_return_from_declarator_annotation(semantic, declarator),
        )
      }),
      // `const api = createApi()` — import / local factory only (not `computed()` etc.).
      // VueUse `createSharedComposable(factory)` forwards the factory bag (Fn → Fn).
      Some(Expression::CallExpression(call)) => vueuse_shared_composable_export_state(
        semantic,
        call,
        shape_graph,
        script_offset,
        imported_bindings,
        &mut returns_by_function,
      )
      .or_else(|| {
        call.callee.get_identifier_reference().and_then(|callee| {
          let callee_name = callee.name.as_str();
          if local_function_id(semantic, callee).is_some() {
            return Some(ExportState::ValueFactoryCall(callee_name.to_owned()));
          }
          if !imported_bindings.contains_key(callee_name) {
            return None;
          }
          // Vue / `#imports` primitives seed [`ExportState::Known`] via the graph.
          if resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Script)
            .is_some_and(|name| reactive_binding_kind(&name).is_some())
          {
            return None;
          }
          Some(ExportState::ValueFactoryCall(callee_name.to_owned()))
        })
      }),
      // `export const x = cond ? computed(...) : useStorage(...)` — both arms ref-like.
      Some(Expression::ConditionalExpression(cond)) => {
        known_export_from_ref_like_ternary(semantic, cond, imported_bindings)
      }
      // Keep the `ref()` cold path tiny: never build the return index until a function init.
      Some(_) => continue,
    };
    if let Some(state) = state {
      insert_local_export_state(&mut locals, name, state);
    }
  }

  // Same-file fixpoint only when forwards / value factories are present.
  let needs_fixpoint = locals.values().any(|state| {
    matches!(
      state,
      ExportState::ForwardReturn(_)
        | ExportState::ValueFactory(_)
        | ExportState::ValueFactoryCall(_)
    )
  });
  if needs_fixpoint {
    for _ in 0..8 {
      let mut changed = false;
      let forwards: Vec<(String, String)> = locals
        .iter()
        .filter_map(|(name, state)| match state {
          ExportState::ForwardReturn(callee) => Some((name.clone(), callee.clone())),
          _ => None,
        })
        .collect();
      for (name, callee) in forwards {
        let Some(resolved) = locals.get(&callee).cloned() else {
          continue;
        };
        if !matches!(
          resolved,
          ExportState::Composable(_)
            | ExportState::Factory(_)
            | ExportState::ValueFactory(_)
            | ExportState::ComponentFactory
        ) {
          continue;
        }
        if locals.get(&name) != Some(&resolved) {
          locals.insert(name, resolved);
          changed = true;
        }
      }
      let factory_calls: Vec<(String, String)> = locals
        .iter()
        .filter_map(|(name, state)| match state {
          ExportState::ValueFactoryCall(callee) => Some((name.clone(), callee.clone())),
          _ => None,
        })
        .collect();
      for (name, callee) in factory_calls {
        let Some(ExportState::ValueFactory(bag)) = locals.get(&callee).cloned() else {
          continue;
        };
        let next = ExportState::ValueBag(bag);
        if locals.get(&name) != Some(&next) {
          locals.insert(name, next);
          changed = true;
        }
      }
      if !changed {
        break;
      }
    }
  }

  // `const { useInject: useX } = createContext<Ctx>(…)` — after value factories exist.
  collect_generic_method_instantiations(semantic, &mut locals);

  locals
}

/// `cond ? computed(...) : useStorage(...)` / `cond ? ref(a) : shallowRef(b)`.
///
/// Both arms must be ref-like call results so the binding is safe to seed as
/// [`ExportState::Known`]. Mixed plain arms stay quiet (under-approx).
fn known_export_from_ref_like_ternary(
  semantic: &oxc_semantic::Semantic<'_>,
  cond: &oxc_ast::ast::ConditionalExpression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> Option<ExportState> {
  let left = ref_like_kind_from_value_expression(semantic, &cond.consequent, imported_bindings)?;
  let right = ref_like_kind_from_value_expression(semantic, &cond.alternate, imported_bindings)?;
  export_lattice::known_from_ref_like_kinds(left, right)
}

fn ref_like_kind_from_value_expression(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> Option<ReactiveBindingKind> {
  let mut current = expression;
  for _ in 0..4 {
    match current {
      Expression::ParenthesizedExpression(paren) => current = &paren.expression,
      Expression::TSAsExpression(assertion) => current = &assertion.expression,
      Expression::TSTypeAssertion(assertion) => current = &assertion.expression,
      Expression::TSNonNullExpression(non_null) => current = &non_null.expression,
      Expression::CallExpression(call) => {
        if let Some(vue) =
          resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Script)
        {
          let kind = reactive_binding_kind(&vue)?;
          return kind.is_ref_like().then_some(kind);
        }
        // Bare / imported factory call (`useStorage`, `useNow`, …) — result is
        // treated as Ref-like so SSR `computed` + client storage ternaries export.
        let callee = call.callee.get_identifier_reference()?;
        let name = callee.name.as_str();
        if reactive_binding_kind(name).is_some() {
          return None;
        }
        return Some(ReactiveBindingKind::Ref);
      }
      _ => return None,
    }
  }
  None
}

/// Insert / merge a local export per the A6 lattice ([`export_lattice`]).
fn insert_local_export_state(
  locals: &mut BTreeMap<String, ExportState>,
  name: String,
  state: ExportState,
) {
  let merged = export_lattice::merge_local(locals.get(&name), state);
  locals.insert(name, merged);
}

#[expect(
  clippy::too_many_arguments,
  reason = "summary return classification reuses the canonical import index instead of rebuilding it"
)]
fn composable_export_state(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  shape_graph: &ReactivityGraph,
  script_offset: usize,
  returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  declared_return_kind: Option<ReactiveBindingKind>,
  declared_return: impl FnOnce() -> Option<DeclaredReturn>,
) -> Option<ExportState> {
  match composable_return_with_borrowed_index(
    semantic,
    function_id,
    shape_graph,
    script_offset,
    returns_by_function,
    imported_bindings,
  ) {
    Some(ComposableReturn::Object(shape)) => Some(ExportState::Composable(shape)),
    Some(ComposableReturn::ValueBag(bag)) => Some(ExportState::ValueFactory(bag)),
    Some(ComposableReturn::Factory(kind)) => Some(ExportState::Factory(kind)),
    Some(ComposableReturn::Forward(callee)) => Some(ExportState::ForwardReturn(callee)),
    Some(ComposableReturn::GenericParam(_)) => None,
    Some(ComposableReturn::UnwrappedState) => match declared_return() {
      Some(DeclaredReturn::PlainObject) => {
        Some(ExportState::Factory(ReactiveBindingKind::Reactive))
      }
      _ => Some(ExportState::BodyUnwrappedState),
    },
    None => {
      if let Some(kind) = declared_return_kind {
        return Some(ExportState::Factory(kind));
      }
      combine_composable_export(None, declared_return())
    }
  }
}

fn combine_composable_export(
  body: Option<ComposableReturn>,
  declared: Option<DeclaredReturn>,
) -> Option<ExportState> {
  match (body, declared) {
    (Some(ComposableReturn::Object(shape)), _)
    | (None, Some(DeclaredReturn::Composable(shape))) => Some(ExportState::Composable(shape)),
    (Some(ComposableReturn::ValueBag(bag)), _) => Some(ExportState::ValueFactory(bag)),
    (Some(ComposableReturn::Factory(kind)), _) | (None, Some(DeclaredReturn::Factory(kind))) => {
      Some(ExportState::Factory(kind))
    }
    (Some(ComposableReturn::Forward(callee)), Some(DeclaredReturn::Composable(shape))) => {
      // Declared shape wins over unresolved forward (e.g. `.d.ts` + thin body).
      let _ = callee;
      Some(ExportState::Composable(shape))
    }
    (Some(ComposableReturn::Forward(callee)), _) => Some(ExportState::ForwardReturn(callee)),
    (Some(ComposableReturn::UnwrappedState), Some(DeclaredReturn::PlainObject)) => {
      Some(ExportState::Factory(ReactiveBindingKind::Reactive))
    }
    (Some(ComposableReturn::UnwrappedState), _) => Some(ExportState::BodyUnwrappedState),
    (Some(ComposableReturn::GenericParam(_)), _) | (None, None) => None,
    (None, Some(DeclaredReturn::PlainObject)) => Some(ExportState::DeclaredPlainObjectFactory),
  }
}

/// `const { prop: local } = factory<Ctx>(…)` → pending / immediate generic instantiate.
fn collect_generic_method_instantiations(
  semantic: &oxc_semantic::Semantic<'_>,
  locals: &mut BTreeMap<String, ExportState>,
) {
  for node in semantic.nodes() {
    let AstKind::VariableDeclarator(declarator) = node.kind() else {
      continue;
    };
    let BindingPattern::ObjectPattern(pattern) = &declarator.id else {
      continue;
    };
    let Some(Expression::CallExpression(call)) = &declarator.init else {
      continue;
    };
    let Some(type_args) = call.type_arguments.as_ref() else {
      continue;
    };
    let Some(callee) = call.callee.get_identifier_reference() else {
      continue;
    };
    let type_arg_shapes: Vec<ComposableShape> = type_args
      .params
      .iter()
      .map(|ts_type| composable_shape_from_ts_type(semantic, ts_type))
      .collect();
    if type_arg_shapes.iter().all(ComposableShape::is_empty) {
      continue;
    }
    let callee_name = callee.name.as_str();
    for property in &pattern.properties {
      let Some(exported) = property.key.static_name() else {
        continue;
      };
      let BindingPattern::BindingIdentifier(identifier) = &property.value else {
        continue;
      };
      let local = identifier.name.to_string();
      if matches!(
        locals.get(&local),
        Some(ExportState::ComponentFactory | ExportState::Known(_) | ExportState::Composable(_))
      ) {
        continue;
      }
      let property = exported.into_owned();
      // Same-file: instantiate immediately when the factory bag is already known.
      if let Some(ExportState::ValueFactory(bag)) = locals.get(callee_name)
        && let Some(ValueBagEntry::MethodGeneric(index)) = bag.entries.get(&property)
        && let Some(shape) = type_arg_shapes.get(*index as usize).filter(|shape| !shape.is_empty())
      {
        locals.insert(local, ExportState::Composable(shape.clone()));
        continue;
      }
      locals.insert(
        local,
        ExportState::GenericMethodInstantiate {
          callee: callee_name.to_owned(),
          property,
          type_arg_shapes: type_arg_shapes.clone(),
        },
      );
    }
  }
}

/// `VueUse` identity wrappers: `createSharedComposable` / `createGlobalState` return `Fn`.
///
/// See <https://vueuse.org/shared/createSharedComposable/> — the export keeps the
/// factory's return bag so consumers can destructure seeded fields.
fn is_vueuse_identity_wrapper(
  imported_bindings: &BTreeMap<String, (String, String)>,
  local_name: &str,
) -> bool {
  let Some((source, imported)) = imported_bindings.get(local_name) else {
    return false;
  };
  let vueuse = source == "@vueuse/core"
    || source == "@vueuse/shared"
    || source.starts_with("@vueuse/core/")
    || source.starts_with("@vueuse/shared/");
  vueuse && matches!(imported.as_str(), "createSharedComposable" | "createGlobalState")
}

fn vueuse_shared_composable_export_state(
  semantic: &oxc_semantic::Semantic<'_>,
  call: &oxc_ast::ast::CallExpression<'_>,
  shape_graph: &ReactivityGraph,
  script_offset: usize,
  imported_bindings: &BTreeMap<String, (String, String)>,
  returns_by_function: &mut Option<BTreeMap<NodeId, Vec<NodeId>>>,
) -> Option<ExportState> {
  let callee = call.callee.get_identifier_reference()?;
  if !is_vueuse_identity_wrapper(imported_bindings, callee.name.as_str()) {
    return None;
  }
  let first = call.arguments.first()?.as_expression()?;
  let index = returns_by_function.get_or_insert_with(|| build_returns_by_function(semantic));
  match first {
    Expression::ArrowFunctionExpression(arrow) => composable_export_state(
      semantic,
      arrow.node_id.get(),
      shape_graph,
      script_offset,
      index,
      imported_bindings,
      arrow_return_type_kind(arrow),
      || declared_return_for_arrow(semantic, arrow),
    ),
    Expression::FunctionExpression(function) => composable_export_state(
      semantic,
      function.node_id.get(),
      shape_graph,
      script_offset,
      index,
      imported_bindings,
      function_return_type_kind(function),
      || declared_return_for_function(semantic, function),
    ),
    Expression::Identifier(identifier) => local_function_id(semantic, identifier).map_or_else(
      || Some(ExportState::ForwardReturn(identifier.name.to_string())),
      |callee_id| match semantic.nodes().kind(callee_id) {
        AstKind::Function(function) => composable_export_state(
          semantic,
          callee_id,
          shape_graph,
          script_offset,
          index,
          imported_bindings,
          function_return_type_kind(function),
          || declared_return_for_function(semantic, function),
        ),
        AstKind::ArrowFunctionExpression(arrow) => composable_export_state(
          semantic,
          callee_id,
          shape_graph,
          script_offset,
          index,
          imported_bindings,
          arrow_return_type_kind(arrow),
          || declared_return_for_arrow(semantic, arrow),
        ),
        _ => Some(ExportState::ForwardReturn(identifier.name.to_string())),
      },
    ),
    _ => None,
  }
}
