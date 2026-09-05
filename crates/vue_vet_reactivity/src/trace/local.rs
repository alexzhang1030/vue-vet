//! Same-file composable / factory usage: defs, instance bags, and destructure seeds.

use std::collections::BTreeMap;

use oxc_ast::{
  AstKind,
  ast::{BindingPattern, Expression},
};
use oxc_semantic::Semantic;
use vue_vet_core::{ReactiveBindingFact, ReactiveBindingKind, ReactivityGraph};

use super::kinds::{collect_binding_identifiers, reference_resolves_to_span, source_span};
use super::{ComposableShapeMap, LocalComposableDefs, LocalComposableExport, summary};

/// Local composable defs + instance/destructure/factory calls in the same file.
///
/// Returns `(instances, seeded_bindings, composable_defs_with_spans)`.
pub(super) fn collect_local_composable_usage(
  semantic: &Semantic<'_>,
  shape_graph: &ReactivityGraph,
  sfc_source: &str,
  script_offset: usize,
) -> (ComposableShapeMap, Vec<ReactiveBindingFact>, LocalComposableDefs) {
  let mut composables = LocalComposableDefs::new();
  // Lazy index — skip full return walks when the file has no function candidates.
  let mut returns_by_function = None;

  // `function useX() { return { field: ref(0) } }` / `return ref(0)`
  for node in semantic.nodes() {
    let AstKind::Function(function) = node.kind() else {
      continue;
    };
    let Some(identifier) = &function.id else {
      continue;
    };
    let index =
      returns_by_function.get_or_insert_with(|| summary::build_returns_by_function(semantic));
    let Some(export) = local_composable_export_for(
      semantic,
      function.node_id.get(),
      shape_graph,
      script_offset,
      index,
      summary::function_return_type_kind(function),
      || summary::function_return_type_shape(semantic, function),
    ) else {
      continue;
    };
    composables.insert(identifier.name.to_string(), (identifier.span, export));
  }

  // `const useX = () => ({ … })` / `const useX = () => ref(0)` / `(): Ref`
  for node in semantic.nodes() {
    let AstKind::VariableDeclarator(declarator) = node.kind() else {
      continue;
    };
    let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
      continue;
    };
    let Some(init) = &declarator.init else {
      continue;
    };
    // Do not build the return index / declared shapes for `const x = ref(0)`.
    let export = match init {
      Expression::ArrowFunctionExpression(arrow) => {
        let index =
          returns_by_function.get_or_insert_with(|| summary::build_returns_by_function(semantic));
        local_composable_export_for(
          semantic,
          arrow.node_id.get(),
          shape_graph,
          script_offset,
          index,
          summary::arrow_return_type_kind(arrow),
          || summary::arrow_return_type_shape(semantic, arrow),
        )
      }
      Expression::FunctionExpression(function) => {
        let index =
          returns_by_function.get_or_insert_with(|| summary::build_returns_by_function(semantic));
        local_composable_export_for(
          semantic,
          function.node_id.get(),
          shape_graph,
          script_offset,
          index,
          summary::function_return_type_kind(function),
          || summary::function_return_type_shape(semantic, function),
        )
      }
      _ => continue,
    };
    let Some(export) = export else {
      continue;
    };
    composables.insert(identifier.name.to_string(), (identifier.span, export));
  }

  let mut instances = BTreeMap::new();
  let mut seeded = Vec::new();
  let mut value_bags = BTreeMap::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = call.callee.get_identifier_reference() else {
      continue;
    };
    let Some((_, export)) = composables
      .get(callee.name.as_str())
      .filter(|(def_span, _)| reference_resolves_to_span(semantic, callee, *def_span))
    else {
      continue;
    };
    let AstKind::VariableDeclarator(declarator) = semantic.nodes().parent_kind(call.node_id.get())
    else {
      continue;
    };
    match (&declarator.id, export) {
      (BindingPattern::BindingIdentifier(identifier), LocalComposableExport::Bag(shape)) => {
        // `const bag = useX()` — bag.field via composable_instances only.
        instances.insert(identifier.name.to_string(), shape.fields.clone());
      }
      (BindingPattern::BindingIdentifier(identifier), LocalComposableExport::Factory(kind)) => {
        // `const flag = useFlag()` — scalar factory seeds a local reactive binding.
        if !seeded
          .iter()
          .any(|binding: &ReactiveBindingFact| binding.name == identifier.name.as_str())
        {
          seeded.push(ReactiveBindingFact {
            name: identifier.name.to_string(),
            kind: *kind,
            initialized_with_null: false,
            alias_of: None,
            span: source_span(sfc_source, script_offset, identifier.span),
          });
        }
      }
      (BindingPattern::BindingIdentifier(identifier), LocalComposableExport::ValueFactory(bag)) => {
        value_bags.insert(identifier.name.to_string(), bag.clone());
      }
      (BindingPattern::ObjectPattern(pattern), LocalComposableExport::Bag(shape)) => {
        // `const { field } = useX()` — seed each known field as a local binding.
        for property in &pattern.properties {
          let Some(exported) = property.key.static_name() else {
            continue;
          };
          let Some(kind) = shape.kind_for_destructure(exported.as_ref()) else {
            continue;
          };
          let mut identifiers = Vec::new();
          collect_binding_identifiers(&property.value, &mut identifiers);
          for (local, span) in identifiers {
            if seeded.iter().any(|binding: &ReactiveBindingFact| binding.name == local) {
              continue;
            }
            seeded.push(ReactiveBindingFact {
              name: local,
              kind,
              initialized_with_null: false,
              alias_of: None,
              span: source_span(sfc_source, script_offset, span),
            });
          }
        }
      }
      _ => {}
    }
  }
  // `api.maps.useX()` member destructures against local value bags.
  if !value_bags.is_empty() {
    seed_local_member_calls(
      semantic,
      &value_bags,
      &mut instances,
      &mut seeded,
      sfc_source,
      script_offset,
    );
  }
  (instances, seeded, composables)
}

pub(super) fn seed_local_member_calls(
  semantic: &Semantic<'_>,
  value_bags: &BTreeMap<String, summary::ValueBag>,
  instances: &mut ComposableShapeMap,
  seeded: &mut Vec<ReactiveBindingFact>,
  sfc_source: &str,
  script_offset: usize,
) {
  use summary::ValueBagEntry;
  debug_assert!(!value_bags.is_empty(), "caller gates empty value bags");
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some((root, path)) = summary::static_member_call_path(&call.callee) else {
      continue;
    };
    let Some(bag) = value_bags.get(&root) else {
      continue;
    };
    let Some(entry) = bag.resolve_path(&path) else {
      continue;
    };
    let AstKind::VariableDeclarator(declarator) = semantic.nodes().parent_kind(call.node_id.get())
    else {
      continue;
    };
    match (entry, &declarator.id) {
      (ValueBagEntry::Method(shape), BindingPattern::ObjectPattern(pattern)) => {
        for property in &pattern.properties {
          let Some(exported) = property.key.static_name() else {
            continue;
          };
          let Some(kind) = shape.kind_for_destructure(exported.as_ref()) else {
            continue;
          };
          let mut identifiers = Vec::new();
          collect_binding_identifiers(&property.value, &mut identifiers);
          for (local, span) in identifiers {
            if seeded.iter().any(|binding| binding.name == local) {
              continue;
            }
            seeded.push(ReactiveBindingFact {
              name: local,
              kind,
              initialized_with_null: false,
              alias_of: None,
              span: source_span(sfc_source, script_offset, span),
            });
          }
        }
      }
      (ValueBagEntry::Method(shape), BindingPattern::BindingIdentifier(identifier)) => {
        instances.insert(identifier.name.to_string(), shape.fields.clone());
      }
      (ValueBagEntry::MethodFactory(kind), BindingPattern::BindingIdentifier(identifier)) => {
        if seeded.iter().any(|binding| binding.name == identifier.name.as_str()) {
          continue;
        }
        seeded.push(ReactiveBindingFact {
          name: identifier.name.to_string(),
          kind: *kind,
          initialized_with_null: false,
          alias_of: None,
          span: source_span(sfc_source, script_offset, identifier.span),
        });
      }
      _ => {}
    }
  }
}

pub(super) fn local_composable_export_for(
  semantic: &Semantic<'_>,
  function_id: oxc_semantic::NodeId,
  shape_graph: &ReactivityGraph,
  script_offset: usize,
  returns_by_function: &BTreeMap<oxc_semantic::NodeId, Vec<oxc_semantic::NodeId>>,
  declared_return_kind: Option<ReactiveBindingKind>,
  declared_return_shape: impl FnOnce() -> summary::ComposableShape,
) -> Option<LocalComposableExport> {
  // One return walk — do not call shape/value-bag/factory helpers separately.
  match summary::composable_return_with_index(
    semantic,
    function_id,
    shape_graph,
    script_offset,
    returns_by_function,
  ) {
    Some(summary::ComposableReturn::Object(shape)) if !shape.is_empty() => {
      return Some(LocalComposableExport::Bag(shape));
    }
    Some(summary::ComposableReturn::ValueBag(bag)) if !bag.is_empty() => {
      return Some(LocalComposableExport::ValueFactory(bag));
    }
    Some(summary::ComposableReturn::Factory(kind)) => {
      return Some(LocalComposableExport::Factory(kind));
    }
    Some(
      summary::ComposableReturn::Object(_)
      | summary::ComposableReturn::ValueBag(_)
      | summary::ComposableReturn::UnwrappedState
      | summary::ComposableReturn::Forward(_)
      | summary::ComposableReturn::GenericParam(_),
    )
    | None => {}
  }
  let declared_shape = declared_return_shape();
  if declared_shape.is_empty() {
    declared_return_kind.map(LocalComposableExport::Factory)
  } else {
    Some(LocalComposableExport::Bag(declared_shape))
  }
}
