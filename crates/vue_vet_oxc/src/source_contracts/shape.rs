//! Vue identity and value-shape vocabulary for source-contract facts.

use oxc_ast::{
  AstKind,
  ast::{Expression, IdentifierReference, ImportDeclarationSpecifier, ImportOrExportKind},
};
use oxc_semantic::SymbolId;
use oxc_span::Span;
use std::collections::HashMap;
use vue_vet_core::{ScriptKind, ToRefIgnoredKeyReason};

use super::stats::WorkCounter;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum VueImport {
  Named(&'static str, &'static str),
  Namespace(&'static str),
}

impl VueImport {
  pub(super) const fn source(self) -> &'static str {
    match self {
      Self::Named(_, source) | Self::Namespace(source) => source,
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Shape {
  Unknown,
  Primitive,
  Nullish,
  PlainRecord,
  Collection,
  Function,
  RefLike,
  DeepProxy,
  ShallowProxy,
  ReadonlyProxy,
}

impl Shape {
  pub(super) const fn is_non_ref_trigger_target(self) -> bool {
    matches!(
      self,
      Self::Primitive
        | Self::Nullish
        | Self::PlainRecord
        | Self::Collection
        | Self::DeepProxy
        | Self::ShallowProxy
        | Self::ReadonlyProxy
    )
  }

  pub(super) const fn is_plain_torefs_target(self) -> bool {
    matches!(self, Self::PlainRecord)
  }

  pub(super) const fn is_primitive_reactive_target(self) -> bool {
    matches!(self, Self::Primitive | Self::Nullish)
  }

  pub(super) const fn is_deep_mutable_proxy(self) -> bool {
    matches!(self, Self::DeepProxy)
  }

  pub(super) const fn toref_ignored_key_reason(self) -> Option<ToRefIgnoredKeyReason> {
    match self {
      Self::RefLike => Some(ToRefIgnoredKeyReason::Ref),
      Self::Function => Some(ToRefIgnoredKeyReason::Function),
      Self::Primitive | Self::Nullish => Some(ToRefIgnoredKeyReason::Primitive),
      _ => None,
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ShapeHint {
  Unknown,
  Primitive,
  Nullish,
  PlainRecord,
  Function,
  Identifier(Option<SymbolId>, bool),
  Call(Span),
  New(Span),
}

/// Fact-producing Vue API sinks collected into `SourceContractFacts`.
///
/// Eligibility preflight and the collector walk share this table through
/// [`contract_sink`]. `watch` still feeds both the ordinary source collector
/// and watch-family option/signature facts. Ordinary `watch` also
/// feeds callback-contract collectors; `watch*Effect` does not.
/// `toRef`, `effectScope`, and `customRef` are additional sinks so a named
/// import of any still admits collection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CollectionCtor {
  Map,
  Set,
  WeakMap,
  WeakSet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CollectionKind {
  Map,
  Set,
  Array,
}

impl CollectionKind {
  pub(super) const fn as_str(self) -> &'static str {
    match self {
      Self::Map => "Map",
      Self::Set => "Set",
      Self::Array => "Array",
    }
  }

  pub(super) fn accepts_method(self, method: &str) -> bool {
    match self {
      Self::Map => matches!(method, "get" | "set" | "has"),
      Self::Set => matches!(method, "has" | "add"),
      Self::Array => matches!(method, "map" | "includes" | "push"),
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractSink {
  TriggerRef,
  ToRefs,
  ProxyConstructor,
  Watch,
  WatchEffectFamily,
  ToRef,
  EffectScope,
  CustomRef,
}

pub(super) fn intern_api(name: &str) -> Option<&'static str> {
  match name {
    "triggerRef" => Some("triggerRef"),
    "toRefs" => Some("toRefs"),
    "reactive" => Some("reactive"),
    "readonly" => Some("readonly"),
    "shallowReactive" => Some("shallowReactive"),
    "shallowReadonly" => Some("shallowReadonly"),
    "watch" => Some("watch"),
    "watchEffect" => Some("watchEffect"),
    "watchPostEffect" => Some("watchPostEffect"),
    "watchSyncEffect" => Some("watchSyncEffect"),
    "defineProps" => Some("defineProps"),
    "ref" => Some("ref"),
    "shallowRef" => Some("shallowRef"),
    "customRef" => Some("customRef"),
    "computed" => Some("computed"),
    "toRef" => Some("toRef"),
    "effectScope" => Some("effectScope"),
    "useTemplateRef" => Some("useTemplateRef"),
    "defineModel" => Some("defineModel"),
    _ => None,
  }
}

pub(super) fn intern_tracked_source(source: &str) -> Option<&'static str> {
  match source {
    "vue" => Some("vue"),
    "vue-demi" => Some("vue-demi"),
    "@vue/runtime-core" => Some("@vue/runtime-core"),
    "@vue/runtime-dom" => Some("@vue/runtime-dom"),
    "@vue/reactivity" => Some("@vue/reactivity"),
    "#imports" => Some("#imports"),
    _ => None,
  }
}

pub fn contract_sink(api: &str) -> Option<ContractSink> {
  match api {
    "triggerRef" => Some(ContractSink::TriggerRef),
    "toRefs" => Some(ContractSink::ToRefs),
    "reactive" | "readonly" | "shallowReactive" | "shallowReadonly" => {
      Some(ContractSink::ProxyConstructor)
    }
    "watch" => Some(ContractSink::Watch),
    "watchEffect" | "watchPostEffect" | "watchSyncEffect" => Some(ContractSink::WatchEffectFamily),
    "toRef" => Some(ContractSink::ToRef),
    "effectScope" => Some(ContractSink::EffectScope),
    "customRef" => Some(ContractSink::CustomRef),
    _ => None,
  }
}

pub(super) fn is_vue_runtime_source(source: &str) -> bool {
  matches!(
    source,
    "vue" | "vue-demi" | "@vue/runtime-core" | "@vue/runtime-dom" | "@vue/reactivity"
  )
}

pub(super) fn is_proxy_allocating_api(api: &str) -> bool {
  matches!(api, "reactive" | "readonly" | "shallowReactive" | "shallowReadonly")
}

/// Packages whose named/namespace constructors allocate an actual Proxy in Vue 3.
/// `vue-demi` stays out: Vue 2 mode returns an observed plain object.
/// Named `#imports` stays out until a project-origin fact proves the export.
pub(super) fn is_actual_proxy_runtime_source(source: &str) -> bool {
  matches!(source, "vue" | "@vue/runtime-core" | "@vue/runtime-dom" | "@vue/reactivity")
}

pub(super) fn intern_extractable_method(name: &str) -> Option<&'static str> {
  match name {
    "get" => Some("get"),
    "set" => Some("set"),
    "has" => Some("has"),
    "add" => Some("add"),
    "map" => Some("map"),
    "includes" => Some("includes"),
    "push" => Some("push"),
    _ => None,
  }
}

pub(super) fn is_known_receiver_method(name: &str) -> bool {
  matches!(
    name,
    "get"
      | "set"
      | "has"
      | "add"
      | "delete"
      | "clear"
      | "forEach"
      | "keys"
      | "values"
      | "entries"
      | "map"
      | "includes"
      | "push"
      | "pop"
      | "shift"
      | "unshift"
      | "splice"
      | "slice"
      | "filter"
      | "reduce"
      | "reduceRight"
      | "find"
      | "findIndex"
      | "findLast"
      | "findLastIndex"
      | "some"
      | "every"
      | "concat"
      | "join"
      | "indexOf"
      | "lastIndexOf"
      | "at"
      | "flat"
      | "flatMap"
      | "reverse"
      | "sort"
      | "fill"
      | "copyWithin"
      | "toSorted"
      | "toReversed"
      | "toSpliced"
      | "with"
  )
}

pub(super) fn intern_native_ctor(name: &str) -> Option<&'static str> {
  match name {
    "Map" => Some("Map"),
    "Set" => Some("Set"),
    "Array" => Some("Array"),
    _ => None,
  }
}

pub(super) fn is_named_auto_import_source(source: &str) -> bool {
  source == "#imports"
}

pub(super) fn is_ref_api(api: &str) -> bool {
  matches!(
    api,
    "ref" | "shallowRef" | "customRef" | "computed" | "toRef" | "useTemplateRef" | "defineModel"
  )
}

pub(super) fn collect_vue_imports(
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
) -> (HashMap<SymbolId, VueImport>, bool) {
  let mut imports = HashMap::new();
  let mut has_contract_sink = false;
  for node in semantic.nodes() {
    work.add_nodes(1);
    let AstKind::ImportDeclaration(declaration) = node.kind() else {
      continue;
    };
    if declaration.import_kind == ImportOrExportKind::Type {
      continue;
    }
    let Some(source) = intern_tracked_source(declaration.source.value.as_str()) else {
      continue;
    };
    let runtime = is_vue_runtime_source(source);
    let auto = is_named_auto_import_source(source);
    if !runtime && !auto {
      continue;
    }
    let Some(specifiers) = &declaration.specifiers else {
      continue;
    };
    for specifier in specifiers {
      work.add_references(1);
      match specifier {
        ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
          if specifier.import_kind == ImportOrExportKind::Type {
            continue;
          }
          let imported = match &specifier.imported {
            oxc_ast::ast::ModuleExportName::IdentifierName(name) => name.name.as_str(),
            oxc_ast::ast::ModuleExportName::IdentifierReference(name) => name.name.as_str(),
            oxc_ast::ast::ModuleExportName::StringLiteral(name) => name.value.as_str(),
          };
          if let Some(api) = intern_api(imported)
            && let Some(symbol_id) = specifier.local.symbol_id.get()
          {
            if contract_sink(api).is_some() {
              has_contract_sink = true;
            }
            imports.insert(symbol_id, VueImport::Named(api, source));
          }
        }
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) if runtime => {
          if let Some(symbol_id) = specifier.local.symbol_id.get() {
            imports.insert(symbol_id, VueImport::Namespace(source));
            has_contract_sink = true;
          }
        }
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(_)
        | ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => {}
      }
    }
  }
  (imports, has_contract_sink)
}

pub(super) fn hint_of(
  expression: &Expression<'_>,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
) -> ShapeHint {
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::BigIntLiteral(_) => ShapeHint::Primitive,
    Expression::TemplateLiteral(literal) if literal.expressions.is_empty() => ShapeHint::Primitive,
    Expression::NullLiteral(_) => ShapeHint::Nullish,
    Expression::Identifier(identifier) => {
      let symbol = symbol_of(identifier);
      ShapeHint::Identifier(symbol, identifier.name.as_str() == "undefined" && symbol.is_none())
    }
    Expression::ObjectExpression(_) | Expression::ArrayExpression(_) => ShapeHint::PlainRecord,
    Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
      ShapeHint::Function
    }
    Expression::NewExpression(expression) => ShapeHint::New(expression.span),
    Expression::CallExpression(call) => ShapeHint::Call(call.span),
    _ => ShapeHint::Unknown,
  }
}

pub(super) fn is_fresh_allocation(
  expression: &Expression<'_>,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
) -> bool {
  match expression.get_inner_expression() {
    Expression::ObjectExpression(_) | Expression::ArrayExpression(_) => true,
    Expression::NewExpression(expression) => {
      is_unresolved_collection(&expression.callee, symbol_of)
    }
    _ => false,
  }
}

pub(super) fn is_unresolved_collection(
  callee: &Expression<'_>,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
) -> bool {
  unresolved_collection_kind(callee, symbol_of).is_some()
}

pub(super) fn unresolved_collection_kind(
  callee: &Expression<'_>,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
) -> Option<CollectionCtor> {
  let identifier = callee.get_inner_expression().get_identifier_reference()?;
  if symbol_of(identifier).is_some() {
    return None;
  }
  match identifier.name.as_str() {
    "Map" => Some(CollectionCtor::Map),
    "Set" => Some(CollectionCtor::Set),
    "WeakMap" => Some(CollectionCtor::WeakMap),
    "WeakSet" => Some(CollectionCtor::WeakSet),
    _ => None,
  }
}

pub(super) fn resolve_vue_api(
  callee: &Expression<'_>,
  imports: &HashMap<SymbolId, VueImport>,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
  kind: ScriptKind,
) -> Option<&'static str> {
  let callee = callee.get_inner_expression();
  if let Some(identifier) = callee.get_identifier_reference() {
    if let Some(symbol_id) = symbol_of(identifier) {
      return match imports.get(&symbol_id) {
        Some(VueImport::Named(api, _)) => Some(*api),
        _ => None,
      };
    }
    let name = identifier.name.as_str();
    if matches!(name, "defineProps" | "defineModel") && kind == ScriptKind::Setup {
      return intern_api(name);
    }
    return None;
  }
  let Expression::StaticMemberExpression(member) = callee else {
    return None;
  };
  let object = member.object.get_inner_expression().get_identifier_reference()?;
  let symbol_id = symbol_of(object)?;
  if !matches!(imports.get(&symbol_id), Some(VueImport::Namespace(_))) {
    return None;
  }
  intern_api(member.property.name.as_str())
}

pub(super) fn span_key(span: Span) -> u64 {
  (u64::from(span.start) << 32) | u64::from(span.end)
}

pub(super) fn classify_vue_result(api: &str, inner: Shape, has_spread: bool) -> Shape {
  if has_spread {
    return Shape::Unknown;
  }
  if is_ref_api(api) {
    return Shape::RefLike;
  }
  if api == "toRefs" {
    return Shape::Unknown;
  }
  match api {
    "reactive" => match inner {
      Shape::RefLike => Shape::RefLike,
      Shape::Primitive | Shape::Nullish => inner,
      Shape::PlainRecord | Shape::Collection | Shape::DeepProxy => Shape::DeepProxy,
      _ => Shape::Unknown,
    },
    "shallowReactive" => match inner {
      Shape::RefLike => Shape::RefLike,
      Shape::Primitive | Shape::Nullish => inner,
      Shape::PlainRecord | Shape::Collection => Shape::ShallowProxy,
      _ => Shape::Unknown,
    },
    "readonly" | "shallowReadonly" => match inner {
      Shape::RefLike => Shape::RefLike,
      Shape::Primitive | Shape::Nullish => inner,
      Shape::PlainRecord | Shape::Collection | Shape::DeepProxy | Shape::ShallowProxy => {
        Shape::ReadonlyProxy
      }
      _ => Shape::Unknown,
    },
    "defineProps" => Shape::DeepProxy,
    _ => Shape::Unknown,
  }
}
