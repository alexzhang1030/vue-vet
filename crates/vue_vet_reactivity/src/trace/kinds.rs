//! Vue callee resolution, binding-kind classification, imports, and spans.
//!
//! Shared by the single-file tracer, binding collectors, and cross-module prepare.

use std::{cell::RefCell, collections::BTreeMap, sync::Arc};

use oxc_ast::{
  AstKind,
  ast::{
    BindingPattern, Expression, IdentifierReference, ImportDeclarationSpecifier, ModuleExportName,
  },
};
use oxc_semantic::Semantic;
use oxc_span::Span;
use vue_vet_core::{
  LineIndex, ReactiveBindingFact, ReactiveBindingKind, ScriptKind, SourceSpan, TrackingScopeKind,
};

thread_local! {
  /// Installed for one trace from a shared [`vue_vet_core::SourceContext`] line index.
  static TRACE_LINE_INDEX: RefCell<Option<Arc<LineIndex>>> = const { RefCell::new(None) };
}

pub(super) fn install_trace_line_index(index: Arc<LineIndex>) {
  TRACE_LINE_INDEX.with(|slot| {
    *slot.borrow_mut() = Some(index);
  });
}

pub(super) fn clear_trace_line_index() {
  TRACE_LINE_INDEX.with(|slot| {
    *slot.borrow_mut() = None;
  });
}

pub(super) fn reference_resolves_to_span(
  semantic: &Semantic<'_>,
  reference: &IdentifierReference<'_>,
  def_span: Span,
) -> bool {
  let Some(reference_id) = reference.reference_id.get() else {
    return false;
  };
  semantic
    .scoping()
    .get_reference(reference_id)
    .symbol_id()
    .is_some_and(|symbol_id| semantic.scoping().symbol_span(symbol_id) == def_span)
}

pub(super) fn collect_imported_bindings(
  semantic: &Semantic<'_>,
) -> BTreeMap<String, (String, String)> {
  let mut imported_bindings = BTreeMap::new();
  for node in semantic.nodes() {
    let AstKind::ImportDeclaration(declaration) = node.kind() else {
      continue;
    };
    let Some(specifiers) = &declaration.specifiers else {
      continue;
    };
    let source = declaration.source.value.to_string();
    for specifier in specifiers {
      let (imported, local) = match specifier {
        ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
          (module_export_name(&specifier.imported), specifier.local.name.to_string())
        }
        ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
          ("default".into(), specifier.local.name.to_string())
        }
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
          ("*".into(), specifier.local.name.to_string())
        }
      };
      imported_bindings.insert(local, (source.clone(), imported));
    }
  }
  imported_bindings
}

pub(super) fn module_export_name(name: &ModuleExportName<'_>) -> String {
  match name {
    ModuleExportName::IdentifierName(name) => name.name.to_string(),
    ModuleExportName::IdentifierReference(name) => name.name.to_string(),
    ModuleExportName::StringLiteral(name) => name.value.to_string(),
  }
}

pub(super) fn resolved_vue_callee(
  semantic: &Semantic<'_>,
  callee: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  kind: ScriptKind,
) -> Option<String> {
  if let Some(identifier) = callee.get_identifier_reference() {
    let local = identifier.name.as_str();
    if matches!(local, "defineModel" | "defineModels" | "defineProps" | "withDefaults")
      && kind == ScriptKind::Setup
      && !imported_bindings.contains_key(local)
    {
      return Some(local.into());
    }
    if let Some((source, imported)) = imported_bindings.get(local) {
      return known_reactivity_export(source, imported).then(|| imported.clone());
    }
    // Nuxt / unplugin-auto-import: bare `ref()` / `watchEffect()` with no local binding.
    // Compiler macros stay setup-only (handled above); do not invent them in ordinary scripts.
    if !matches!(local, "defineModel" | "defineModels" | "defineProps" | "withDefaults")
      && known_reactivity_export("vue", local)
      && identifier_reference_is_unresolved(semantic, identifier)
    {
      return Some(local.into());
    }
    return None;
  }

  let (namespace, property) = match callee {
    Expression::StaticMemberExpression(member) => {
      (member.object.get_identifier_reference()?.name.as_str(), member.property.name.to_string())
    }
    Expression::ComputedMemberExpression(member) => (
      member.object.get_identifier_reference()?.name.as_str(),
      member.static_property_name()?.to_string(),
    ),
    _ => return None,
  };
  imported_bindings.get(namespace).and_then(|(source, imported)| {
    if imported == "*" && matches!(source.as_str(), "vue" | "#imports") {
      known_reactivity_export("vue", &property).then_some(property)
    } else {
      None
    }
  })
}

pub(super) fn identifier_reference_is_unresolved(
  semantic: &Semantic<'_>,
  identifier: &IdentifierReference<'_>,
) -> bool {
  let Some(reference_id) = identifier.reference_id.get() else {
    return false;
  };
  semantic.scoping().get_reference(reference_id).symbol_id().is_none()
}

/// Packages/exports the tracer treats as reactivity APIs (under-approx allowlist).
pub(super) fn known_reactivity_export(source: &str, imported: &str) -> bool {
  match source {
    "vue" | "#imports" => {
      reactive_binding_kind(imported).is_some()
        || TrackingScopeKind::from_vue_callee(imported).is_some()
        || matches!(
          imported,
          "storeToRefs"
            | "useRoute"
            | "useRouter"
            | "pauseTracking"
            | "enableTracking"
            | "resetTracking"
            | "unref"
            | "toValue"
            | "withDefaults"
            | "provide"
            | "inject"
        )
    }
    "pinia" => matches!(imported, "storeToRefs"),
    "vue-router" => matches!(imported, "useRoute" | "useRouter"),
    _ => false,
  }
}

pub(super) fn reactive_binding_kind(callee: &str) -> Option<ReactiveBindingKind> {
  match callee {
    "ref" => Some(ReactiveBindingKind::Ref),
    "shallowRef" => Some(ReactiveBindingKind::ShallowRef),
    "computed" => Some(ReactiveBindingKind::Computed),
    // defineProps / useRoute / useRouter expose reactive objects (member reads, not .value).
    "reactive" | "defineProps" | "useRoute" | "useRouter" => Some(ReactiveBindingKind::Reactive),
    "shallowReactive" => Some(ReactiveBindingKind::ShallowReactive),
    "readonly" => Some(ReactiveBindingKind::Readonly),
    "shallowReadonly" => Some(ReactiveBindingKind::ShallowReadonly),
    "customRef" => Some(ReactiveBindingKind::CustomRef),
    "toRef" | "toRefs" | "storeToRefs" => Some(ReactiveBindingKind::ToRef),
    "useTemplateRef" => Some(ReactiveBindingKind::TemplateRef),
    // `defineModel` is the Vue compiler macro; `defineModels` is Vue Macros' multi-model
    // form (`const { modelValue } = defineModels<{…}>()`), each local is a writable ref.
    "defineModel" | "defineModels" => Some(ReactiveBindingKind::ModelRef),
    _ => None,
  }
}

pub(super) fn collect_binding_identifiers(
  pattern: &BindingPattern<'_>,
  identifiers: &mut Vec<(String, Span)>,
) {
  match pattern {
    BindingPattern::BindingIdentifier(identifier) => {
      identifiers.push((identifier.name.to_string(), identifier.span));
    }
    BindingPattern::ObjectPattern(object) => {
      for property in &object.properties {
        collect_binding_identifiers(&property.value, identifiers);
      }
      if let Some(rest) = &object.rest {
        collect_binding_identifiers(&rest.argument, identifiers);
      }
    }
    BindingPattern::ArrayPattern(array) => {
      for element in array.elements.iter().flatten() {
        collect_binding_identifiers(element, identifiers);
      }
      if let Some(rest) = &array.rest {
        collect_binding_identifiers(&rest.argument, identifiers);
      }
    }
    BindingPattern::AssignmentPattern(assignment) => {
      collect_binding_identifiers(&assignment.left, identifiers);
    }
  }
}

pub(super) fn push_binding_by_span(
  bindings: &mut Vec<ReactiveBindingFact>,
  binding: ReactiveBindingFact,
) {
  if !bindings
    .iter()
    .any(|local| local.name == binding.name && local.span.offset == binding.span.offset)
  {
    bindings.push(binding);
  }
}

/// Source may contain a typed Ref-like annotation worth walking the AST for.
#[inline]
pub(super) fn source_may_have_typed_ref_annotations(source: &str) -> bool {
  // Forms recognized by `ts_type_reactive_kind` (not the `ref()` runtime API).
  source.contains("Ref")
    || source.contains("Computed")
    || source.contains("Readonly")
    || source.contains("ToRef")
    // Structural duck `{ value?: T }` (optional sole `value`).
    || source.contains("value?")
}

/// Source may call a component factory that seeds a props bag.
#[inline]
pub(super) fn source_may_have_component_props_factory(source: &str) -> bool {
  source.contains("defineComponent")
}

pub const DEEP_WATCH_PROPERTY: &str = "*";

pub(super) const fn span_contains(outer: Span, inner: Span) -> bool {
  outer.start <= inner.start && outer.end >= inner.end
}

pub(super) fn reference_resolves_to_binding(
  semantic: &oxc_semantic::Semantic<'_>,
  reference: &IdentifierReference<'_>,
  binding: &ReactiveBindingFact,
  script_offset: usize,
) -> bool {
  let Some(reference_id) = reference.reference_id.get() else {
    return false;
  };
  let Some(symbol_id) = semantic.scoping().get_reference(reference_id).symbol_id() else {
    // Bare Nuxt/Vite auto-import of an exported ref/computed (`currentUser`) has
    // no local symbol. Match by name only when this module also has no local
    // symbol of that name — otherwise nested `const signal = ref()` would
    // attach free `signal.value` reads via scope_bindings (include_nested).
    if reference.name.as_str() != binding.name {
      return false;
    }
    return !module_has_local_symbol_named(semantic, binding.name.as_str());
  };
  if semantic.scoping().symbol_name(symbol_id) != binding.name {
    return false;
  }
  let symbol_span = semantic.scoping().symbol_span(symbol_id);
  let relative = usize::try_from(symbol_span.start).unwrap_or(usize::MAX);
  let absolute = script_offset.saturating_add(relative);
  // Exact absolute match (local facts and correctly offset seeds).
  if absolute == binding.span.offset {
    return true;
  }
  // Seeds historically/occasionally store script-relative spans even when the
  // module re-trace uses a non-zero SFC offset — accept the relative match too.
  script_offset > 0 && relative == binding.span.offset
}

pub(super) fn module_has_local_symbol_named(
  semantic: &oxc_semantic::Semantic<'_>,
  name: &str,
) -> bool {
  let scoping = semantic.scoping();
  scoping.symbol_ids().any(|symbol_id| scoping.symbol_name(symbol_id) == name)
}

pub(super) fn source_span(source: &str, base: usize, span: Span) -> SourceSpan {
  let offset = base.saturating_add(usize::try_from(span.start).unwrap_or(usize::MAX));
  let end = base.saturating_add(usize::try_from(span.end).unwrap_or(usize::MAX));
  let (line, column) = TRACE_LINE_INDEX.with(|slot| {
    slot.borrow().as_ref().map_or_else(
      || vue_vet_core::LineIndex::new(source).byte_to_line_column(offset),
      |index| index.as_ref().byte_to_line_column(offset),
    )
  });
  SourceSpan { offset, length: end.saturating_sub(offset), line, column }
}
