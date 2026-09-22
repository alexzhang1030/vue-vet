//! Vue identity and value-shape vocabulary for source-contract facts.

use oxc_ast::{
  AstKind,
  ast::{
    CallExpression, Expression, IdentifierReference, ImportDeclarationSpecifier,
    ImportOrExportKind, UnaryOperator,
  },
};
use oxc_semantic::{NodeId, Reference, ReferenceFlags, SymbolId};
use oxc_span::{GetSpan, Span};
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
pub(super) enum VueUseImport {
  Named(&'static str),
  NamespaceCore,
  NamespaceShared,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PrimitiveKind {
  Unknown,
  Number,
  String,
  Boolean,
  BigInt,
  Nullish,
}

impl PrimitiveKind {
  pub(super) const fn to_fact(self) -> Option<vue_vet_core::PrimitiveValueKind> {
    match self {
      Self::Unknown => None,
      Self::Number => Some(vue_vet_core::PrimitiveValueKind::Number),
      Self::String => Some(vue_vet_core::PrimitiveValueKind::String),
      Self::Boolean => Some(vue_vet_core::PrimitiveValueKind::Boolean),
      Self::BigInt => Some(vue_vet_core::PrimitiveValueKind::Bigint),
      Self::Nullish => Some(vue_vet_core::PrimitiveValueKind::Nullish),
    }
  }
}

/// Closed literal payload for until timeout unmatched-demand proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Scalar {
  Number(u64),
  Str(u64),
  Bool(bool),
  Nullish,
}

impl Scalar {
  pub(super) const fn kind(self) -> NativeKind {
    match self {
      Self::Number(_) => NativeKind::Number,
      Self::Str(_) => NativeKind::String,
      Self::Bool(_) => NativeKind::Boolean,
      Self::Nullish => NativeKind::Nullish,
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NativeKind {
  Number,
  String,
  Boolean,
  Nullish,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Literal {
  Bool(bool),
  Number(i64),
  String,
  Null,
  Undefined,
}

/// One options-object read. `Absent` is the only case that takes a default.
/// A non-literal value or a prop hidden by a spread or computed key is `Unknown`
/// and the caller abstains.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OptionValue<T> {
  Absent,
  Known(T),
  Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Shape {
  Unknown,
  /// `Nullish` is `Primitive(PrimitiveKind::Nullish)`. `Unknown` stays the outer variant.
  Primitive(PrimitiveKind),
  PlainRecord,
  Collection,
  Function,
  RefLike,
  DeepProxy,
  ShallowProxy,
  ReadonlyProxy,
}

impl Shape {
  pub(super) const fn is_non_nullish_primitive(self) -> bool {
    matches!(
      self,
      Self::Primitive(
        PrimitiveKind::Number
          | PrimitiveKind::String
          | PrimitiveKind::Boolean
          | PrimitiveKind::BigInt
      )
    )
  }

  pub(super) const fn is_non_ref_trigger_target(self) -> bool {
    matches!(
      self,
      Self::Primitive(_)
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
    matches!(self, Self::Primitive(_))
  }

  pub(super) const fn is_deep_mutable_proxy(self) -> bool {
    matches!(self, Self::DeepProxy)
  }

  pub(super) const fn toref_ignored_key_reason(self) -> Option<ToRefIgnoredKeyReason> {
    match self {
      Self::RefLike => Some(ToRefIgnoredKeyReason::Ref),
      Self::Function => Some(ToRefIgnoredKeyReason::Function),
      Self::Primitive(_) => Some(ToRefIgnoredKeyReason::Primitive),
      _ => None,
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ShapeHint {
  Unknown,
  Primitive(PrimitiveKind),
  Nullish,
  PlainRecord,
  Function,
  Identifier(Option<SymbolId>, bool),
  Call(Span),
  New(Span),
}

/// What a shape hint contributes to a primitive-kind walk.
///
/// Symbol policy stays in the lane: cached, injection, and filter do not
/// follow an identifier the same way.
pub(super) enum HintClass {
  Kind(PrimitiveKind),
  Follow(SymbolId),
  Other,
}

impl ShapeHint {
  pub(super) const fn classify_primitive(self) -> HintClass {
    match self {
      Self::Primitive(kind) => HintClass::Kind(kind),
      Self::Nullish | Self::Identifier(_, true) => HintClass::Kind(PrimitiveKind::Nullish),
      Self::Identifier(Some(symbol_id), false) => HintClass::Follow(symbol_id),
      _ => HintClass::Other,
    }
  }
}

/// Closed primitive payload used to prove a write actually changed.
/// Interned `Str`/`BigInt` ids live on `Indexes.interned`. Computed-identity
/// keeps a separate owned `atom::PrimitiveAtom`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PrimitiveAtom {
  Bool(bool),
  Number { bits: u64, nan: bool },
  Str(u32),
  BigInt(u32),
  Null,
  Undefined,
}

impl PrimitiveAtom {
  /// Vue watcher delivery uses `Object.is`.
  #[expect(
    clippy::match_same_arms,
    reason = "NaN Object.is and interned string/bigint equality are distinct JS cases"
  )]
  pub(super) fn object_is(self, other: Self, interned: &[String]) -> bool {
    match (self, other) {
      (Self::Bool(left), Self::Bool(right)) => left == right,
      (Self::Number { bits: left, nan: false }, Self::Number { bits: right, nan: false }) => {
        left == right
      }
      (Self::Number { nan: true, .. }, Self::Number { nan: true, .. }) => true,
      (Self::Str(left), Self::Str(right)) | (Self::BigInt(left), Self::BigInt(right)) => {
        interned_eq(interned, left, right)
      }
      (Self::Null, Self::Null) | (Self::Undefined, Self::Undefined) => true,
      _ => false,
    }
  }

  /// Guard operators `===` / `!==` use JavaScript strict equality (`+0 === -0`, `NaN !== NaN`).
  #[expect(clippy::match_same_arms, reason = "NaN !== NaN must stay explicit against the wildcard")]
  pub(super) fn js_strict_eq(self, other: Self, interned: &[String]) -> bool {
    match (self, other) {
      (Self::Bool(left), Self::Bool(right)) => left == right,
      (Self::Number { nan: true, .. }, _) | (_, Self::Number { nan: true, .. }) => false,
      (Self::Number { bits: left, nan: false }, Self::Number { bits: right, nan: false }) => {
        f64::from_bits(left) == f64::from_bits(right)
      }
      (Self::Str(left), Self::Str(right)) | (Self::BigInt(left), Self::BigInt(right)) => {
        interned_eq(interned, left, right)
      }
      (Self::Null, Self::Null) | (Self::Undefined, Self::Undefined) => true,
      _ => false,
    }
  }

  pub(super) const fn is_falsy_guard(self) -> bool {
    matches!(
      self,
      Self::Bool(false) | Self::Number { bits: 0, nan: false } | Self::Null | Self::Undefined
    )
  }
}

fn interned_eq(interned: &[String], left: u32, right: u32) -> bool {
  interned.get(left as usize) == interned.get(right as usize)
}

pub(super) fn primitive_atom(expression: &Expression<'_>) -> Option<PrimitiveAtom> {
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(literal) => Some(PrimitiveAtom::Bool(literal.value)),
    Expression::NumericLiteral(literal) => {
      Some(PrimitiveAtom::Number { bits: literal.value.to_bits(), nan: literal.value.is_nan() })
    }
    Expression::NullLiteral(_) => Some(PrimitiveAtom::Null),
    Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::UnaryNegation => {
      match primitive_atom(&unary.argument) {
        Some(PrimitiveAtom::Number { bits, nan: false }) => {
          Some(PrimitiveAtom::Number { bits: (-f64::from_bits(bits)).to_bits(), nan: false })
        }
        other => other,
      }
    }
    Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::UnaryPlus => {
      primitive_atom(&unary.argument)
    }
    Expression::Identifier(identifier) if identifier.name.as_str() == "undefined" => {
      Some(PrimitiveAtom::Undefined)
    }
    _ => None,
  }
}

/// Fact-producing Vue / `@vueuse` API sinks collected into `SourceContractFacts`.
///
/// Eligibility preflight and the collector walk share this table through
/// [`contract_sink`]. `watch` feeds both the ordinary source collector and
/// watch-family option/signature facts. Ordinary `watch` also feeds
/// callback-contract collectors; `watch*Effect` does not. Named
/// [`ContractSink::WatchEffectFamily`] imports keep source indexes empty when
/// every resolved reference is a proven call with fewer than two arguments and
/// no spread. Ordinary sinks and namespace imports keep full indexing.
/// `toRef`, `effectScope`, and `customRef` are additional sinks so a named
/// import of any still admits collection. `computed` admits computed-only
/// imports so identity facts keep forced-full parity without a second
/// Vue-import pass. `syncRef` is admitted only from `@vueuse/shared` /
/// `@vueuse/core`. `computedAsync` is admitted only from `@vueuse/core`.
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
  Computed,
  SyncRef,
  ComputedAsync,
  OnMounted,
}

impl ContractSink {
  const fn requires_full_index(self) -> bool {
    !matches!(self, Self::WatchEffectFamily)
  }
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
    "effect" => Some("effect"),
    "defineProps" => Some("defineProps"),
    "ref" => Some("ref"),
    "shallowRef" => Some("shallowRef"),
    "customRef" => Some("customRef"),
    "computed" => Some("computed"),
    "toRef" => Some("toRef"),
    "effectScope" => Some("effectScope"),
    "useTemplateRef" => Some("useTemplateRef"),
    "defineModel" => Some("defineModel"),
    "syncRef" => Some("syncRef"),
    "onScopeDispose" => Some("onScopeDispose"),
    "nextTick" => Some("nextTick"),
    "computedAsync" | "asyncComputed" => Some("computedAsync"),
    "provide" => Some("provide"),
    "inject" => Some("inject"),
    "defineExpose" => Some("defineExpose"),
    "onMounted" => Some("onMounted"),
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
    "@vueuse/core" => Some("@vueuse/core"),
    "@vueuse/shared" => Some("@vueuse/shared"),
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
    "computed" => Some(ContractSink::Computed),
    "syncRef" => Some(ContractSink::SyncRef),
    "computedAsync" => Some(ContractSink::ComputedAsync),
    "onMounted" => Some(ContractSink::OnMounted),
    _ => None,
  }
}

pub(super) fn is_watch_effect_api(api: &str) -> bool {
  matches!(api, "watchEffect" | "watchSyncEffect" | "watchPostEffect" | "effect")
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

pub(super) fn is_vueuse_core_source(source: &str) -> bool {
  source == "@vueuse/core"
}

pub(super) fn is_vueuse_shared_source(source: &str) -> bool {
  source == "@vueuse/shared"
}

pub(super) fn intern_vueuse_api(name: &str, core: bool, shared: bool) -> Option<&'static str> {
  match name {
    "useMemoize" if core => Some("useMemoize"),
    "computedWithControl" if core || shared => Some("computedWithControl"),
    "controlledComputed" if core || shared => Some("controlledComputed"),
    "computedAsync" | "asyncComputed" if core => Some("computedAsync"),
    "until" if core || shared => Some("until"),
    "watchIgnorable" if core || shared => Some("watchIgnorable"),
    "ignorableWatch" if core || shared => Some("ignorableWatch"),
    "createSharedComposable" if core || shared => Some("createSharedComposable"),
    "createGlobalState" if core || shared => Some("createGlobalState"),
    "useDebounceFn" if core || shared => Some("useDebounceFn"),
    "useCloned" if core => Some("useCloned"),
    "useManualRefHistory" if core => Some("useManualRefHistory"),
    _ => None,
  }
}

pub(super) fn native_callable(kind: PrimitiveKind, method: &str) -> Option<bool> {
  if kind == PrimitiveKind::Unknown {
    return None;
  }
  match method {
    "toUpperCase" | "toLowerCase" | "charAt" | "charCodeAt" | "concat" | "includes" | "indexOf"
    | "lastIndexOf" | "slice" | "substring" | "split" | "trim" | "trimStart" | "trimEnd"
    | "padStart" | "padEnd" | "repeat" | "startsWith" | "endsWith" | "match" | "replace"
    | "search" | "localeCompare" | "normalize" | "at" | "codePointAt" | "replaceAll"
    | "matchAll" | "isWellFormed" | "toWellFormed" | "trimLeft" | "trimRight" | "substr" => {
      Some(kind == PrimitiveKind::String)
    }
    "toFixed" | "toExponential" | "toPrecision" => Some(kind == PrimitiveKind::Number),
    _ => None,
  }
}

pub(super) fn native_return_kind(kind: PrimitiveKind, method: &str) -> PrimitiveKind {
  if native_callable(kind, method) != Some(true) {
    return PrimitiveKind::Unknown;
  }
  match method {
    "charCodeAt" | "indexOf" | "lastIndexOf" | "search" | "localeCompare" | "codePointAt" => {
      PrimitiveKind::Number
    }
    "includes" | "startsWith" | "endsWith" | "isWellFormed" => PrimitiveKind::Boolean,
    "match" | "matchAll" | "split" => PrimitiveKind::Unknown,
    _ => PrimitiveKind::String,
  }
}

pub(super) fn is_vueuse_sync_ref_source(source: &str) -> bool {
  matches!(source, "@vueuse/core" | "@vueuse/shared")
}

pub(super) fn is_ref_api(api: &str) -> bool {
  matches!(
    api,
    "ref"
      | "shallowRef"
      | "customRef"
      | "computed"
      | "toRef"
      | "useTemplateRef"
      | "defineModel"
      | "computedAsync"
  )
}

pub(super) fn collect_vue_imports(
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
) -> (HashMap<SymbolId, VueImport>, bool) {
  let mut imports = HashMap::new();
  let mut requires_full_index = false;
  let mut has_effect_family = false;
  for node in semantic.nodes() {
    work.add_nodes(1);
    if let AstKind::IdentifierReference(identifier) = node.kind()
      && identifier.name.as_str() == "defineModel"
    {
      // Compiler macro: not a ContractSink and not a Vue import, but model-default
      // facts still need a full index. `defineExpose` / `onMounted` ride along
      // once `defineModel` or an imported `onMounted` sink already opted in.
      requires_full_index = true;
    }
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
    let vueuse = is_vueuse_sync_ref_source(source) || is_vueuse_core_source(source);
    if !runtime && !auto && !vueuse {
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
          let computed_async = matches!(imported, "computedAsync" | "asyncComputed");
          let sync_ref = imported == "syncRef";
          if vueuse {
            let core = is_vueuse_core_source(source);
            if !(sync_ref || (computed_async && core)) {
              continue;
            }
          } else if sync_ref || computed_async {
            continue;
          }
          if let Some(api) = intern_api(imported)
            && let Some(symbol_id) = specifier.local.symbol_id.get()
          {
            match contract_sink(api) {
              Some(sink) if !sink.requires_full_index() => has_effect_family = true,
              Some(_) => requires_full_index = true,
              None if matches!(api, "provide" | "inject") => requires_full_index = true,
              None => {}
            }
            imports.insert(symbol_id, VueImport::Named(api, source));
          }
        }
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) if runtime => {
          if let Some(symbol_id) = specifier.local.symbol_id.get() {
            imports.insert(symbol_id, VueImport::Namespace(source));
            requires_full_index = true;
          }
        }
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(_)
        | ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => {}
      }
    }
  }
  let needs_index =
    requires_full_index || (has_effect_family && effect_family_may_emit(semantic, &imports, work));
  (imports, needs_index)
}

fn effect_family_may_emit(
  semantic: &oxc_semantic::Semantic<'_>,
  imports: &HashMap<SymbolId, VueImport>,
  work: &WorkCounter,
) -> bool {
  for (symbol_id, import) in imports {
    work.add_queries(1);
    let VueImport::Named(api, _) = *import else {
      continue;
    };
    if contract_sink(api) != Some(ContractSink::WatchEffectFamily) {
      continue;
    }
    for reference in semantic.symbol_references(*symbol_id) {
      work.add_references(1);
      if effect_reference_may_emit(semantic, reference, work) {
        return true;
      }
    }
  }
  false
}

fn effect_reference_may_emit(
  semantic: &oxc_semantic::Semantic<'_>,
  reference: &Reference,
  work: &WorkCounter,
) -> bool {
  let flags = reference.flags();
  if flags.contains(ReferenceFlags::Type) && !flags.is_value() {
    return false;
  }
  let ident_id = reference.node_id();
  for ancestor in semantic.nodes().ancestors(ident_id) {
    work.add_nodes(1);
    if let AstKind::CallExpression(call) = ancestor.kind() {
      return effect_call_may_emit(semantic, call, ident_id, work);
    }
    if !is_identity_wrapper(ancestor.kind()) {
      return true;
    }
  }
  true
}

const fn is_identity_wrapper(kind: AstKind<'_>) -> bool {
  matches!(
    kind,
    AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSInstantiationExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_)
  )
}

fn effect_call_may_emit(
  semantic: &oxc_semantic::Semantic<'_>,
  call: &CallExpression<'_>,
  ident_id: NodeId,
  work: &WorkCounter,
) -> bool {
  let Expression::Identifier(callee) = inner_expression(&call.callee, work) else {
    return true;
  };
  let AstKind::IdentifierReference(ident) = semantic.nodes().kind(ident_id) else {
    return true;
  };
  if callee.span() != ident.span() {
    return true;
  }
  if call.arguments.len() >= 2 {
    return true;
  }
  for argument in &call.arguments {
    work.add_queries(1);
    if argument.is_spread() {
      return true;
    }
  }
  false
}

fn inner_expression<'a>(expression: &'a Expression<'a>, work: &WorkCounter) -> &'a Expression<'a> {
  let mut current = expression;
  loop {
    current = match current {
      Expression::ParenthesizedExpression(inner) => {
        work.add_nodes(1);
        &inner.expression
      }
      Expression::TSAsExpression(inner) => {
        work.add_nodes(1);
        &inner.expression
      }
      Expression::TSSatisfiesExpression(inner) => {
        work.add_nodes(1);
        &inner.expression
      }
      Expression::TSInstantiationExpression(inner) => {
        work.add_nodes(1);
        &inner.expression
      }
      Expression::TSNonNullExpression(inner) => {
        work.add_nodes(1);
        &inner.expression
      }
      Expression::TSTypeAssertion(inner) => {
        work.add_nodes(1);
        &inner.expression
      }
      _ => return current,
    };
  }
}

pub(super) fn collect_vueuse_imports(
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
) -> HashMap<SymbolId, VueUseImport> {
  let mut imports = HashMap::new();
  for node in semantic.nodes() {
    work.add_nodes(1);
    let AstKind::ImportDeclaration(declaration) = node.kind() else {
      continue;
    };
    if declaration.import_kind == ImportOrExportKind::Type {
      continue;
    }
    let source = declaration.source.value.as_str();
    let core = is_vueuse_core_source(source);
    let shared = is_vueuse_shared_source(source);
    if !core && !shared {
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
          if let Some(api) = intern_vueuse_api(imported, core, shared)
            && let Some(symbol_id) = specifier.local.symbol_id.get()
          {
            imports.insert(symbol_id, VueUseImport::Named(api));
          }
        }
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
          if let Some(symbol_id) = specifier.local.symbol_id.get() {
            imports.insert(
              symbol_id,
              if core { VueUseImport::NamespaceCore } else { VueUseImport::NamespaceShared },
            );
          }
        }
        ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => {}
      }
    }
  }
  imports
}

pub(super) fn hint_of(
  expression: &Expression<'_>,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
) -> ShapeHint {
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(_) => ShapeHint::Primitive(PrimitiveKind::Boolean),
    Expression::NumericLiteral(_) => ShapeHint::Primitive(PrimitiveKind::Number),
    Expression::StringLiteral(_) => ShapeHint::Primitive(PrimitiveKind::String),
    Expression::BigIntLiteral(_) => ShapeHint::Primitive(PrimitiveKind::BigInt),
    Expression::TemplateLiteral(literal) if literal.expressions.is_empty() => {
      ShapeHint::Primitive(PrimitiveKind::String)
    }
    Expression::NullLiteral(_) => ShapeHint::Nullish,
    Expression::UnaryExpression(unary)
      if matches!(unary.operator, UnaryOperator::UnaryNegation | UnaryOperator::UnaryPlus) =>
    {
      match unary.argument.get_inner_expression() {
        Expression::NumericLiteral(_) => ShapeHint::Primitive(PrimitiveKind::Number),
        Expression::BigIntLiteral(_) => ShapeHint::Primitive(PrimitiveKind::BigInt),
        _ => ShapeHint::Unknown,
      }
    }
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

pub(super) fn is_unresolved_date(
  callee: &Expression<'_>,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
) -> bool {
  let Some(identifier) = callee.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  identifier.name.as_str() == "Date" && symbol_of(identifier).is_none()
}

pub(super) fn literal_of(expression: &Expression<'_>) -> Option<Literal> {
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(literal) => Some(Literal::Bool(literal.value)),
    Expression::NumericLiteral(literal) if literal.value.fract() == 0.0 =>
    {
      #[expect(clippy::cast_possible_truncation, reason = "option capacity is a small integer")]
      Some(Literal::Number(literal.value as i64))
    }
    Expression::StringLiteral(_) => Some(Literal::String),
    Expression::NullLiteral(_) => Some(Literal::Null),
    Expression::Identifier(identifier) if identifier.name.as_str() == "undefined" => {
      Some(Literal::Undefined)
    }
    _ => None,
  }
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
    if matches!(name, "defineProps" | "defineModel" | "defineExpose") && kind == ScriptKind::Setup {
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

pub(super) fn resolve_vueuse_api(
  callee: &Expression<'_>,
  imports: &HashMap<SymbolId, VueUseImport>,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
) -> Option<&'static str> {
  let callee = callee.get_inner_expression();
  if let Some(identifier) = callee.get_identifier_reference() {
    let symbol_id = symbol_of(identifier)?;
    return match imports.get(&symbol_id) {
      Some(VueUseImport::Named(api)) => Some(*api),
      _ => None,
    };
  }
  let Expression::StaticMemberExpression(member) = callee else {
    return None;
  };
  let object = member.object.get_inner_expression().get_identifier_reference()?;
  let symbol_id = symbol_of(object)?;
  match imports.get(&symbol_id) {
    Some(VueUseImport::NamespaceCore) => {
      intern_vueuse_api(member.property.name.as_str(), true, false)
    }
    Some(VueUseImport::NamespaceShared) => {
      intern_vueuse_api(member.property.name.as_str(), false, true)
    }
    _ => None,
  }
}

pub(super) fn scalar_of(expression: &Expression<'_>) -> Option<Scalar> {
  match expression.get_inner_expression() {
    Expression::NumericLiteral(literal) => Some(Scalar::Number(literal.value.to_bits())),
    Expression::BooleanLiteral(literal) => Some(Scalar::Bool(literal.value)),
    Expression::StringLiteral(literal) => Some(Scalar::Str(fnv1a(literal.value.as_str()))),
    Expression::TemplateLiteral(literal) if literal.expressions.is_empty() => {
      let cooked = literal.quasis.first()?.value.cooked.as_ref()?;
      Some(Scalar::Str(fnv1a(cooked.as_str())))
    }
    Expression::NullLiteral(_) => Some(Scalar::Nullish),
    Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::UnaryNegation => {
      let Expression::NumericLiteral(literal) = unary.argument.get_inner_expression() else {
        return None;
      };
      Some(Scalar::Number((-literal.value).to_bits()))
    }
    _ => None,
  }
}

pub(super) const SYNC_FLUSH: u64 = fnv1a("sync");

#[expect(clippy::indexing_slicing, reason = "const FNV-1a walks a compile-time string")]
pub(super) const fn fnv1a(text: &str) -> u64 {
  let mut hash = 0xcbf2_9ce4_8422_2325;
  let bytes = text.as_bytes();
  let mut index = 0;
  while index < bytes.len() {
    hash ^= bytes[index] as u64;
    hash = hash.wrapping_mul(0x0100_0000_01b3);
    index += 1;
  }
  hash
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
      Shape::Primitive(_) => inner,
      Shape::PlainRecord | Shape::Collection | Shape::DeepProxy => Shape::DeepProxy,
      _ => Shape::Unknown,
    },
    "shallowReactive" => match inner {
      Shape::RefLike => Shape::RefLike,
      Shape::Primitive(_) => inner,
      Shape::PlainRecord | Shape::Collection => Shape::ShallowProxy,
      _ => Shape::Unknown,
    },
    "readonly" | "shallowReadonly" => match inner {
      Shape::RefLike => Shape::RefLike,
      Shape::Primitive(_) => inner,
      Shape::PlainRecord | Shape::Collection | Shape::DeepProxy | Shape::ShallowProxy => {
        Shape::ReadonlyProxy
      }
      _ => Shape::Unknown,
    },
    "defineProps" => Shape::DeepProxy,
    _ => Shape::Unknown,
  }
}
