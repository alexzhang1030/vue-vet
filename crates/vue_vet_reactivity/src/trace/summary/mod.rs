use std::{
  collections::{BTreeMap, BTreeSet, btree_map::Entry},
  sync::Arc,
};

use oxc_allocator::Allocator;
use oxc_ast::{
  AstKind,
  ast::{
    BindingPattern, Declaration, ExportDefaultDeclarationKind, Expression,
    ImportDeclarationSpecifier, ObjectPropertyKind,
  },
};
use oxc_parser::Parser;
use oxc_semantic::{NodeId, Semantic, SemanticBuilder};
use oxc_span::{SourceType, Span};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vue_vet_core::{ModuleId, ReactiveBindingKind, ReactivityGraph, ScriptKind};

use super::{
  TraceSeeds, collect_binding_identifiers, collect_imported_bindings, collect_inject_sites,
  collect_provide_sites, collect_reactive_bindings, module_export_name, reactive_binding_kind,
  reference_resolves_to_binding, resolved_vue_callee, trace_reactivity_seeded,
};
use oxc_ast::ast::Argument;

/// One script surface to analyze — standalone JS/TS or an extracted SFC block.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModuleSource {
  /// Stable module identity used in [`ModuleLink`] and result ordering.
  pub id: ModuleId,
  /// Text parsed by Oxc (extracted `<script>` body for SFCs).
  pub source: Arc<str>,
  /// Language hint (`js`, `ts`, `jsx`, `tsx`, …).
  pub language: String,
  pub kind: ScriptKind,
  /// Byte offset of [`Self::source`] within [`Self::span_source`].
  #[serde(default)]
  pub source_offset: usize,
  /// Full original file used for absolute line/column (SFC source). When empty,
  /// spans are computed against [`Self::source`] (standalone modules).
  #[serde(default)]
  pub span_source: Arc<str>,
  /// Module semantic IR extracted by the Oxc adapter during its first parse.
  #[serde(skip)]
  module_summary: Option<std::sync::Arc<ModuleSummary>>,
}

impl PartialEq for ModuleSource {
  fn eq(&self, other: &Self) -> bool {
    // `span_source` is excluded: style-only SFC edits change the wrapper file
    // without invalidating script body IR when `source` + `source_offset` match.
    self.id == other.id
      && self.source == other.source
      && self.language == other.language
      && self.kind == other.kind
      && self.source_offset == other.source_offset
  }
}

impl Eq for ModuleSource {}

impl ModuleSource {
  /// Standalone JS/TS module (offset 0, spans against `source`).
  #[must_use]
  pub fn standalone(
    id: impl Into<ModuleId>,
    source: impl Into<Arc<str>>,
    language: impl Into<String>,
    kind: ScriptKind,
  ) -> Self {
    Self {
      id: id.into(),
      source: source.into(),
      language: language.into(),
      kind,
      source_offset: 0,
      span_source: Arc::from(""),
      module_summary: None,
    }
  }

  /// Extracted SFC script block with absolute span mapping into the original file.
  #[must_use]
  pub fn sfc_script(
    id: impl Into<ModuleId>,
    script_source: impl Into<Arc<str>>,
    language: impl Into<String>,
    kind: ScriptKind,
    source_offset: usize,
    sfc_source: impl Into<Arc<str>>,
  ) -> Self {
    Self {
      id: id.into(),
      source: script_source.into(),
      language: language.into(),
      kind,
      source_offset,
      span_source: sfc_source.into(),
      module_summary: None,
    }
  }

  /// Attach module semantic IR produced from the same Oxc parse as script facts.
  #[must_use]
  pub fn with_module_summary(mut self, module_summary: impl Into<Arc<ModuleSummary>>) -> Self {
    self.module_summary = Some(module_summary.into());
    self
  }

  /// Borrow the attached module semantic IR, when present.
  #[must_use]
  pub fn module_summary(&self) -> Option<Arc<ModuleSummary>> {
    self.module_summary.as_ref().map(Arc::clone)
  }

  /// Compatibility alias for [`Self::with_module_summary`].
  #[must_use]
  pub fn with_prepared_trace(self, prepared_trace: PreparedModuleTrace) -> Self {
    self.with_module_summary(prepared_trace)
  }

  #[must_use]
  pub(super) fn span_origin(&self) -> &str {
    if self.span_source.is_empty() { self.source.as_ref() } else { self.span_source.as_ref() }
  }
}

/// Already-resolved import edge between two [`ModuleSource::id`] values.
///
/// This crate does not open the filesystem or resolve bare specifiers; the
/// caller (for example Vue Vet's project graph) must supply concrete targets.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleLink {
  pub from: ModuleId,
  pub specifier: String,
  pub to: ModuleId,
}

/// Per-module reactivity graph produced by [`trace_modules`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleReactivity {
  pub id: ModuleId,
  pub graph: std::sync::Arc<ReactivityGraph>,
}

/// Failures while parsing, linking, or tracing a module set.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TraceModulesError {
  #[error("duplicate reactivity module id `{0}`")]
  DuplicateModule(ModuleId),
  #[error("module `{module}` uses unsupported language `{language}`")]
  UnsupportedLanguage { module: ModuleId, language: String },
  #[error("could not parse reactivity module `{module}`: {message}")]
  Parse { module: ModuleId, message: String },
  #[error("could not build semantics for reactivity module `{module}`: {message}")]
  Semantic { module: ModuleId, message: String },
  #[error("reactivity module link {from} -> {to} references an unknown module")]
  UnknownLink { from: ModuleId, to: ModuleId },
  #[error("reactivity module `{from}` resolves `{specifier}` to multiple targets")]
  AmbiguousLink { from: ModuleId, specifier: String },
  #[error("reactivity module worker pool could not complete tracing")]
  WorkerDisconnected,
}

impl TraceModulesError {
  /// Module most directly responsible for this issue, when one exists.
  #[must_use]
  pub const fn module_id(&self) -> Option<&ModuleId> {
    match self {
      Self::DuplicateModule(module)
      | Self::UnsupportedLanguage { module, .. }
      | Self::Parse { module, .. }
      | Self::Semantic { module, .. } => Some(module),
      Self::UnknownLink { from, .. } | Self::AmbiguousLink { from, .. } => Some(from),
      Self::WorkerDisconnected => None,
    }
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ImportSummary {
  local: String,
  imported: String,
  source: String,
  span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ExportSummary {
  Local { local: String, exported: String },
  Reexport { source: String, imported: String, exported: String },
  Star { source: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DestructuredCallBinding {
  imported_local: String,
  property: String,
  local: String,
  span: Span,
}

/// `const bag = useFoo()` — whole-object composable call used via member access.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct InstanceCallBinding {
  imported_local: String,
  local: String,
  span: Span,
}

/// Synthetic [`ModuleLink`] specifier prefix for bare Nuxt auto-import calls.
///
/// Kept in sync with `vue_vet_project::conventions::NUXT_IMPORTS_SPECIFIER_PREFIX`.
pub(super) const NUXT_IMPORTS_SPECIFIER_PREFIX: &str = "#nuxt-imports:";
/// Exclusive end for [`BTreeMap::range`] over `#nuxt-imports:…` keys (`';` follows `:`).
pub(super) const NUXT_IMPORTS_RANGE_END: &str = "#nuxt-imports;";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ExportState {
  /// Imported local is itself a reactive binding (`import { count } from './x'`).
  Known(ReactiveBindingKind),
  /// Calling the export returns a statically keyed object bag.
  Composable(BTreeMap<String, ReactiveBindingKind>),
  /// Calling the export returns a scalar reactive value (`return ref(0)` / `(): Ref<T>`).
  Factory(ReactiveBindingKind),
  /// Declared `() => PlainObject` (no Ref fields) — needs body evidence for Reactive factory.
  DeclaredPlainObjectFactory,
  /// Body unwraps a state ref (e.g. `return useState(...).value`) — needs plain-object declaration.
  BodyUnwrappedState,
  Ambiguous,
}

/// Under-approx classification of a composable/factory function return.
#[derive(Clone, Debug, Eq, PartialEq)]
enum ComposableReturn {
  Object(BTreeMap<String, ReactiveBindingKind>),
  Factory(ReactiveBindingKind),
  /// Body unwraps a state ref (`return useState(...).value`, unresolved / `#imports`).
  UnwrappedState,
}

/// Declared TypeScript return surface for factory/composable exports.
#[derive(Clone, Debug, Eq, PartialEq)]
enum DeclaredReturn {
  Factory(ReactiveBindingKind),
  Composable(BTreeMap<String, ReactiveBindingKind>),
  /// Object-shaped type with ≥1 property and no Ref-like fields.
  PlainObject,
}

/// Export-resolution payload only — no source body, no owned reactivity graph.
/// Shares [`ModuleSummary`] across the seed barrier instead of cloning its vectors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ModuleExportFacts {
  pub(super) id: ModuleId,
  pub(super) summary: Arc<ModuleSummary>,
}

/// Stable module semantic IR extracted from an existing Oxc semantic.
///
/// Cross-file linking consumes this summary instead of parser ASTs. It is
/// intentionally not disk-serializable: callers retain it only for the current
/// analysis lifecycle, and Oxc nodes never cross the adapter boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleSummary {
  imports: Vec<ImportSummary>,
  exports: Vec<ExportSummary>,
  locals: BTreeMap<String, ExportState>,
  provides: Vec<super::ProvideSite>,
  injects: Vec<super::InjectSite>,
  local_graph: std::sync::Arc<ReactivityGraph>,
}

impl ModuleSummary {
  /// Specifiers this module imports or re-exports (for external follow).
  #[must_use]
  pub fn follow_specifiers(&self) -> Vec<String> {
    let mut specifiers = BTreeSet::new();
    for import in &self.imports {
      specifiers.insert(import.source.clone());
    }
    for export in &self.exports {
      match export {
        ExportSummary::Reexport { source, .. } | ExportSummary::Star { source } => {
          specifiers.insert(source.clone());
        }
        ExportSummary::Local { .. } => {}
      }
    }
    specifiers.into_iter().collect()
  }

  /// Whether any export local is a finished Factory/Composable/Known seed.
  #[must_use]
  pub fn has_reactivity_export_seeds(&self) -> bool {
    self.locals.values().any(|state| {
      matches!(state, ExportState::Factory(_) | ExportState::Composable(_) | ExportState::Known(_))
    })
  }

  /// Whether a companion implementation file may still complete provisional seeds.
  ///
  /// Only provisional declaration/body halves need a merge. Do **not** treat "no
  /// finished Factory/Composable seeds" as incomplete — that would parse every
  /// unrelated package's companion `.js` (e.g. multi‑MB `typescript.js`).
  #[must_use]
  pub fn needs_implementation_merge(&self) -> bool {
    self.locals.values().any(|state| {
      matches!(state, ExportState::DeclaredPlainObjectFactory | ExportState::BodyUnwrappedState)
    })
  }

  /// Replace locals after merging a declaration file with its implementation body.
  #[must_use]
  fn with_locals(mut self, locals: BTreeMap<String, ExportState>) -> Self {
    self.locals = locals;
    self
  }
}

/// Merge `.d.ts` declaration locals with companion implementation locals.
///
/// `DeclaredPlainObjectFactory` + `BodyUnwrappedState` → `Factory(Reactive)`.
#[must_use]
pub fn merge_declaration_implementation_summary(
  declaration: ModuleSummary,
  implementation: &ModuleSummary,
) -> ModuleSummary {
  let mut merged = declaration.locals.clone();
  for (name, impl_state) in &implementation.locals {
    match (merged.get(name), impl_state) {
      (Some(ExportState::DeclaredPlainObjectFactory), ExportState::BodyUnwrappedState)
      | (Some(ExportState::BodyUnwrappedState), ExportState::DeclaredPlainObjectFactory) => {
        merged.insert(name.clone(), ExportState::Factory(ReactiveBindingKind::Reactive));
      }
      (Some(ExportState::DeclaredPlainObjectFactory), ExportState::Factory(kind))
        if *kind == ReactiveBindingKind::Reactive =>
      {
        merged.insert(name.clone(), ExportState::Factory(ReactiveBindingKind::Reactive));
      }
      (
        None | Some(ExportState::DeclaredPlainObjectFactory | ExportState::BodyUnwrappedState),
        state,
      ) if matches!(
        state,
        ExportState::Factory(_) | ExportState::Composable(_) | ExportState::Known(_)
      ) =>
      {
        merged.insert(name.clone(), state.clone());
      }
      (None, ExportState::BodyUnwrappedState | ExportState::DeclaredPlainObjectFactory) => {
        merged.insert(name.clone(), impl_state.clone());
      }
      _ => {}
    }
  }
  declaration.with_locals(merged)
}

/// Parse a standalone module and attach its [`ModuleSummary`] (external seed path).
///
/// # Errors
///
/// Returns parse/semantic errors for invalid sources or unsupported languages.
pub fn prepare_standalone_module_source(
  id: impl Into<ModuleId>,
  source: impl Into<Arc<str>>,
  language: impl Into<String>,
) -> Result<ModuleSource, TraceModulesError> {
  let module = ModuleSource::standalone(id, source, language, ScriptKind::Script);
  let phase = analyze_module_phase_one(&module)?;
  Ok(module.with_module_summary(phase.facts.summary))
}

/// Compatibility alias for [`ModuleSummary`].
pub type PreparedModuleTrace = ModuleSummary;

pub fn prepare_module_summary(
  semantic: &Semantic<'_>,
  span_source: &str,
  source_offset: usize,
  kind: ScriptKind,
  local_graph: impl Into<Arc<ReactivityGraph>>,
) -> ModuleSummary {
  let local_graph = local_graph.into();
  let imports = collect_imports(semantic);
  let exports = collect_exports(semantic);
  let shape_graph = ReactivityGraph {
    bindings: collect_reactive_bindings(
      semantic,
      &collect_imported_bindings(semantic),
      span_source,
      source_offset,
      kind,
      true,
    ),
    ..ReactivityGraph::default()
  };
  let locals = collect_local_values(semantic, &local_graph, &shape_graph, source_offset);
  let imported_bindings = collect_imported_bindings(semantic);
  let provides = collect_provide_sites(
    semantic,
    &imported_bindings,
    &local_graph.bindings,
    &local_graph.composable_instances,
    &BTreeMap::new(),
    kind,
  );
  let injects = collect_inject_sites(semantic, &imported_bindings, &local_graph.bindings, kind);
  ModuleSummary { imports, exports, locals, provides, injects, local_graph }
}

/// Compatibility alias for [`prepare_module_summary`].
#[must_use]
pub fn prepare_module_trace(
  semantic: &Semantic<'_>,
  span_source: &str,
  source_offset: usize,
  kind: ScriptKind,
  local_graph: impl Into<Arc<ReactivityGraph>>,
) -> PreparedModuleTrace {
  prepare_module_summary(semantic, span_source, source_offset, kind, local_graph)
}

pub(super) struct ModulePhaseOne {
  pub(super) facts: ModuleExportFacts,
  pub(super) local_graph: Arc<ReactivityGraph>,
}

pub(super) fn analyze_module_phase_one_cached(
  module: &ModuleSource,
  cached: Option<(&ModuleSource, &Arc<ModuleSummary>)>,
) -> Result<ModulePhaseOne, TraceModulesError> {
  if let Some(summary) = &module.module_summary {
    return Ok(phase_one_from_summary(module, summary));
  }
  if let Some((source, summary)) = cached
    && source == module
  {
    return Ok(phase_one_from_summary(module, summary));
  }
  analyze_module_phase_one(module)
}

pub(super) fn analyze_module_phase_one(
  module: &ModuleSource,
) -> Result<ModulePhaseOne, TraceModulesError> {
  if let Some(summary) = &module.module_summary {
    return Ok(phase_one_from_summary(module, summary));
  }

  let allocator = Allocator::default();
  let source_type = source_type(module)?;
  let parsed = Parser::new(&allocator, module.source.as_ref(), source_type).parse();
  if !parsed.errors.is_empty() {
    return Err(TraceModulesError::Parse {
      module: module.id.clone(),
      message: join_errors(&parsed.errors),
    });
  }
  let built = SemanticBuilder::new().with_check_syntax_error(true).build(&parsed.program);
  if !built.errors.is_empty() {
    return Err(TraceModulesError::Semantic {
      module: module.id.clone(),
      message: join_errors(&built.errors),
    });
  }
  let semantic = built.semantic;

  let empty = TraceSeeds::default();
  let local_graph = Arc::new(trace_reactivity_seeded(
    &semantic,
    module.span_origin(),
    module.source_offset,
    module.kind,
    &empty,
  ));
  let summary = Arc::new(prepare_module_summary(
    &semantic,
    module.span_origin(),
    module.source_offset,
    module.kind,
    Arc::clone(&local_graph),
  ));
  Ok(phase_one_from_summary(module, &summary))
}

pub(super) fn phase_one_from_summary(
  module: &ModuleSource,
  summary: &Arc<ModuleSummary>,
) -> ModulePhaseOne {
  ModulePhaseOne {
    facts: ModuleExportFacts { id: module.id.clone(), summary: Arc::clone(summary) },
    local_graph: Arc::clone(&summary.local_graph),
  }
}

pub(super) fn source_type(module: &ModuleSource) -> Result<SourceType, TraceModulesError> {
  match module.language.as_str() {
    "js" | "javascript" => Ok(SourceType::mjs()),
    "jsx" => Ok(SourceType::jsx()),
    "ts" | "typescript" => Ok(SourceType::ts()),
    "tsx" => Ok(SourceType::tsx()),
    "d.ts" | "dts" => Ok(SourceType::d_ts()),
    language => Err(TraceModulesError::UnsupportedLanguage {
      module: module.id.clone(),
      language: language.into(),
    }),
  }
}

pub(super) fn collect_imports(semantic: &oxc_semantic::Semantic<'_>) -> Vec<ImportSummary> {
  let mut imports = Vec::new();
  for node in semantic.nodes() {
    let AstKind::ImportDeclaration(declaration) = node.kind() else {
      continue;
    };
    let Some(specifiers) = &declaration.specifiers else {
      continue;
    };
    let source = declaration.source.value.to_string();
    for specifier in specifiers {
      let (local, imported, span) = match specifier {
        ImportDeclarationSpecifier::ImportSpecifier(specifier) => (
          specifier.local.name.to_string(),
          module_export_name(&specifier.imported),
          specifier.local.span,
        ),
        ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
          (specifier.local.name.to_string(), "default".into(), specifier.local.span)
        }
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
          (specifier.local.name.to_string(), "*".into(), specifier.local.span)
        }
      };
      imports.push(ImportSummary { local, imported, source: source.clone(), span });
    }
  }
  imports.sort_by_key(|import| import.span.start);
  imports
}

fn collect_local_values(
  semantic: &oxc_semantic::Semantic<'_>,
  public_graph: &ReactivityGraph,
  shape_graph: &ReactivityGraph,
  script_offset: usize,
) -> BTreeMap<String, ExportState> {
  let mut locals = public_graph
    .bindings
    .iter()
    .map(|binding| (binding.name.clone(), ExportState::Known(binding.kind)))
    .collect::<BTreeMap<_, _>>();

  // Lazy: modules with no function/composable candidates must not pay a full
  // return-statement index walk (cold `trace_1k_*` synthetic modules).
  let mut returns_by_function = None;

  // `function useX() { return { field } }` / `return ref(0)` / `(): Ref<T>`
  for node in semantic.nodes() {
    let AstKind::Function(function) = node.kind() else {
      continue;
    };
    let Some(identifier) = &function.id else {
      continue;
    };
    let index = returns_by_function.get_or_insert_with(|| build_returns_by_function(semantic));
    let function_id = function.node_id.get();
    if let Some(state) = composable_export_state(
      semantic,
      function_id,
      shape_graph,
      script_offset,
      index,
      function_return_type_kind(function),
      || declared_return_for_function(semantic, function),
    ) {
      locals.insert(identifier.name.to_string(), state);
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
    let state = match &declarator.init {
      Some(Expression::ArrowFunctionExpression(arrow)) => {
        let index = returns_by_function.get_or_insert_with(|| build_returns_by_function(semantic));
        composable_export_state(
          semantic,
          arrow.node_id.get(),
          shape_graph,
          script_offset,
          index,
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
          function_return_type_kind(function),
          || {
            declared_return_for_function(semantic, function)
              .or_else(|| declared_return_from_declarator_annotation(semantic, declarator))
          },
        )
      }
      // `export declare const useX: () => T` — no init; only then pay for annotations.
      None => combine_composable_export(
        None,
        declared_return_from_declarator_annotation(semantic, declarator),
      ),
      // Keep the CallExpression/`ref()` cold path tiny: never build the return
      // index or declared shapes until we see a real function init.
      Some(_) => continue,
    };
    if let Some(state) = state {
      locals.insert(identifier.name.to_string(), state);
    }
  }
  locals
}

fn composable_export_state(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  shape_graph: &ReactivityGraph,
  script_offset: usize,
  returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
  declared_return_kind: Option<ReactiveBindingKind>,
  declared_return: impl FnOnce() -> Option<DeclaredReturn>,
) -> Option<ExportState> {
  match composable_return_with_index(
    semantic,
    function_id,
    shape_graph,
    script_offset,
    returns_by_function,
  ) {
    Some(ComposableReturn::Object(shape)) => Some(ExportState::Composable(shape)),
    Some(ComposableReturn::Factory(kind)) => Some(ExportState::Factory(kind)),
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
    (Some(ComposableReturn::Factory(kind)), _) | (None, Some(DeclaredReturn::Factory(kind))) => {
      Some(ExportState::Factory(kind))
    }
    (Some(ComposableReturn::UnwrappedState), Some(DeclaredReturn::PlainObject)) => {
      Some(ExportState::Factory(ReactiveBindingKind::Reactive))
    }
    (Some(ComposableReturn::UnwrappedState), _) => Some(ExportState::BodyUnwrappedState),
    (None, Some(DeclaredReturn::PlainObject)) => Some(ExportState::DeclaredPlainObjectFactory),
    (None, None) => None,
  }
}

/// One-pass index: owning function/arrow → return statement node ids.
///
/// Built once per semantic so composable shape extraction is O(returns) total
/// instead of O(functions × nodes).
#[must_use]
pub fn build_returns_by_function(
  semantic: &oxc_semantic::Semantic<'_>,
) -> BTreeMap<NodeId, Vec<NodeId>> {
  let mut returns_by_function: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
  for (return_id, node) in semantic.nodes().iter_enumerated() {
    let AstKind::ReturnStatement(_) = node.kind() else {
      continue;
    };
    let Some(owner) = semantic.nodes().ancestor_ids(return_id).find(|ancestor_id| {
      matches!(
        semantic.nodes().kind(*ancestor_id),
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
      )
    }) else {
      continue;
    };
    returns_by_function.entry(owner).or_default().push(return_id);
  }
  returns_by_function
}

/// Object shape returned by a composable function / arrow (under-approx).
///
/// `script_offset` must match the offset used when materializing `graph.bindings`
/// spans (0 for standalone modules, Vize `loc.start` for SFC script bodies).
/// Prefer [`composable_return_shape_with_index`] when indexing many functions.
#[must_use]
pub fn composable_return_shape(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  graph: &ReactivityGraph,
  script_offset: usize,
) -> BTreeMap<String, ReactiveBindingKind> {
  let returns_by_function = build_returns_by_function(semantic);
  composable_return_shape_with_index(
    semantic,
    function_id,
    graph,
    script_offset,
    &returns_by_function,
  )
}

/// [`composable_return_shape`] using a prebuilt [`build_returns_by_function`] index.
#[must_use]
pub fn composable_return_shape_with_index(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  graph: &ReactivityGraph,
  script_offset: usize,
  returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
) -> BTreeMap<String, ReactiveBindingKind> {
  match composable_return_with_index(
    semantic,
    function_id,
    graph,
    script_offset,
    returns_by_function,
  ) {
    Some(ComposableReturn::Object(shape)) => shape,
    Some(ComposableReturn::Factory(_) | ComposableReturn::UnwrappedState) | None => BTreeMap::new(),
  }
}

#[expect(
  clippy::struct_excessive_bools,
  reason = "return-kind accumulator tracks independent under-approx signals"
)]
struct ReturnKindAccum {
  shape: BTreeMap<String, ReactiveBindingKind>,
  ambiguous: BTreeSet<String>,
  factory_kind: Option<ReactiveBindingKind>,
  factory_conflict: bool,
  saw_object_return: bool,
  saw_scalar_return: bool,
  /// `return <call>(...).value` — provisional until paired with a plain object declaration.
  saw_unwrapped_state: bool,
}

impl ReturnKindAccum {
  fn consider(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
    graph: &ReactivityGraph,
    imported_bindings: &BTreeMap<String, (String, String)>,
    param_names: &BTreeSet<String>,
    script_offset: usize,
  ) {
    let expression = match expression {
      Expression::ParenthesizedExpression(paren) => &paren.expression,
      other => other,
    };
    if matches!(expression, Expression::ObjectExpression(_)) {
      self.saw_object_return = true;
      merge_return_object_into_shape(
        semantic,
        expression,
        graph,
        imported_bindings,
        param_names,
        script_offset,
        &mut self.shape,
        &mut self.ambiguous,
      );
      return;
    }
    if is_unwrapped_call_return(semantic, expression, imported_bindings) {
      self.saw_scalar_return = true;
      self.saw_unwrapped_state = true;
      return;
    }
    self.saw_scalar_return = true;
    let Some(kind) = reactive_return_kind(
      semantic,
      expression,
      graph,
      imported_bindings,
      param_names,
      script_offset,
    ) else {
      self.factory_conflict = true;
      return;
    };
    match self.factory_kind {
      None => self.factory_kind = Some(kind),
      Some(existing) if existing == kind => {}
      Some(_) => self.factory_conflict = true,
    }
  }

  fn finish(self) -> Option<ComposableReturn> {
    if self.saw_object_return && self.saw_scalar_return {
      return None;
    }
    if self.saw_object_return && !self.shape.is_empty() {
      return Some(ComposableReturn::Object(self.shape));
    }
    if self.saw_scalar_return && !self.factory_conflict {
      if let Some(kind) = self.factory_kind {
        return Some(ComposableReturn::Factory(kind));
      }
      if self.saw_unwrapped_state {
        return Some(ComposableReturn::UnwrappedState);
      }
    }
    None
  }
}

/// Object bag or scalar factory return for a function/arrow (under-approx).
fn composable_return_with_index(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  graph: &ReactivityGraph,
  script_offset: usize,
  returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
) -> Option<ComposableReturn> {
  let imported_bindings = collect_imported_bindings(semantic);
  let param_names = function_param_names(semantic, function_id);
  let mut accum = ReturnKindAccum {
    shape: BTreeMap::new(),
    ambiguous: BTreeSet::new(),
    factory_kind: None,
    factory_conflict: false,
    saw_object_return: false,
    saw_scalar_return: false,
    saw_unwrapped_state: false,
  };

  // `() => ({ field: ref(0) })` / `() => ref(0)` expression body — no ReturnStatement.
  if let AstKind::ArrowFunctionExpression(arrow) = semantic.nodes().kind(function_id)
    && arrow.expression
    && let Some(statement) = arrow.body.statements.first()
    && let oxc_ast::ast::Statement::ExpressionStatement(expression) = statement
  {
    accum.consider(
      semantic,
      &expression.expression,
      graph,
      &imported_bindings,
      &param_names,
      script_offset,
    );
  }

  if let Some(return_ids) = returns_by_function.get(&function_id) {
    for &return_id in return_ids {
      let AstKind::ReturnStatement(statement) = semantic.nodes().kind(return_id) else {
        continue;
      };
      let Some(argument) = &statement.argument else {
        accum.factory_conflict = true;
        continue;
      };
      accum.consider(semantic, argument, graph, &imported_bindings, &param_names, script_offset);
    }
  }

  accum.finish()
}

/// Declared TypeScript return type on a function (`.d.ts` / annotated source).
#[must_use]
pub fn function_return_type_kind(
  function: &oxc_ast::ast::Function<'_>,
) -> Option<ReactiveBindingKind> {
  function
    .return_type
    .as_ref()
    .and_then(|annotation| ts_type_reactive_kind(&annotation.type_annotation))
}

/// Declared object-bag return shape on a function (`.d.ts` / annotated source).
///
/// Kept out of line so the `const x = ref(0)` module-export cold path does not
/// pay for TypeScript shape machinery in instruction cache.
#[must_use]
#[inline(never)]
pub fn function_return_type_shape(
  semantic: &oxc_semantic::Semantic<'_>,
  function: &oxc_ast::ast::Function<'_>,
) -> BTreeMap<String, ReactiveBindingKind> {
  let Some(annotation) = function.return_type.as_ref() else {
    return BTreeMap::new();
  };
  let mut index = None;
  ts_type_composable_shape(semantic, &annotation.type_annotation, 0, &mut index)
}

/// Declared TypeScript return type on an arrow function.
#[must_use]
pub fn arrow_return_type_kind(
  arrow: &oxc_ast::ast::ArrowFunctionExpression<'_>,
) -> Option<ReactiveBindingKind> {
  arrow
    .return_type
    .as_ref()
    .and_then(|annotation| ts_type_reactive_kind(&annotation.type_annotation))
}

/// Declared object-bag return shape on an arrow function.
#[must_use]
#[inline(never)]
pub fn arrow_return_type_shape(
  semantic: &oxc_semantic::Semantic<'_>,
  arrow: &oxc_ast::ast::ArrowFunctionExpression<'_>,
) -> BTreeMap<String, ReactiveBindingKind> {
  let Some(annotation) = arrow.return_type.as_ref() else {
    return BTreeMap::new();
  };
  let mut index = None;
  ts_type_composable_shape(semantic, &annotation.type_annotation, 0, &mut index)
}

/// Scalar factory kind from return expressions (`return ref(0)`), when consistent.
#[must_use]
pub fn composable_factory_kind_with_index(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  graph: &ReactivityGraph,
  script_offset: usize,
  returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
) -> Option<ReactiveBindingKind> {
  match composable_return_with_index(
    semantic,
    function_id,
    graph,
    script_offset,
    returns_by_function,
  ) {
    Some(ComposableReturn::Factory(kind)) => Some(kind),
    Some(ComposableReturn::Object(_) | ComposableReturn::UnwrappedState) | None => None,
  }
}

/// `return <call>(...).value` where callee is unresolved or imported from `#imports`.
///
/// Name-agnostic: pairs with a declared plain-object return to yield `Factory(Reactive)`.
fn is_unwrapped_call_return(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> bool {
  let Expression::StaticMemberExpression(member) = expression else {
    return false;
  };
  if member.property.name.as_str() != "value" {
    return false;
  }
  let Expression::CallExpression(call) = &member.object else {
    return false;
  };
  let Some(callee) = call.callee.get_identifier_reference() else {
    return false;
  };
  if let Some((source, _)) = imported_bindings.get(callee.name.as_str()) {
    return source == "#imports";
  }
  let Some(reference_id) = callee.reference_id.get() else {
    return false;
  };
  semantic.scoping().get_reference(reference_id).symbol_id().is_none()
}

fn declared_return_for_function(
  semantic: &oxc_semantic::Semantic<'_>,
  function: &oxc_ast::ast::Function<'_>,
) -> Option<DeclaredReturn> {
  if let Some(kind) = function_return_type_kind(function) {
    return Some(DeclaredReturn::Factory(kind));
  }
  let annotation = function.return_type.as_ref()?;
  classify_declared_return_type(semantic, &annotation.type_annotation)
}

fn declared_return_for_arrow(
  semantic: &oxc_semantic::Semantic<'_>,
  arrow: &oxc_ast::ast::ArrowFunctionExpression<'_>,
) -> Option<DeclaredReturn> {
  if let Some(kind) = arrow_return_type_kind(arrow) {
    return Some(DeclaredReturn::Factory(kind));
  }
  let annotation = arrow.return_type.as_ref()?;
  classify_declared_return_type(semantic, &annotation.type_annotation)
}

/// `export declare const useX: () => T` — function type on the declarator.
#[inline(never)]
fn declared_return_from_declarator_annotation(
  semantic: &oxc_semantic::Semantic<'_>,
  declarator: &oxc_ast::ast::VariableDeclarator<'_>,
) -> Option<DeclaredReturn> {
  use oxc_ast::ast::TSType;
  let annotation = declarator.type_annotation.as_ref()?;
  let ts_type = match &annotation.type_annotation {
    TSType::TSParenthesizedType(paren) => &paren.type_annotation,
    other => other,
  };
  let TSType::TSFunctionType(function_type) = ts_type else {
    return None;
  };
  classify_declared_return_type(semantic, &function_type.return_type.type_annotation)
}

#[inline(never)]
fn classify_declared_return_type(
  semantic: &oxc_semantic::Semantic<'_>,
  ts_type: &oxc_ast::ast::TSType<'_>,
) -> Option<DeclaredReturn> {
  if let Some(kind) = ts_type_reactive_kind(ts_type) {
    return Some(DeclaredReturn::Factory(kind));
  }
  let mut index = None;
  let shape = ts_type_composable_shape(semantic, ts_type, 0, &mut index);
  if !shape.is_empty() {
    return Some(DeclaredReturn::Composable(shape));
  }
  if ts_type_is_plain_object_shaped(semantic, ts_type, 0, &mut index) {
    return Some(DeclaredReturn::PlainObject);
  }
  None
}

/// Object-shaped type: ≥1 property and no Ref-like field types (under-approx).
#[inline(never)]
fn ts_type_is_plain_object_shaped<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  ts_type: &'a oxc_ast::ast::TSType<'a>,
  depth: u8,
  index: &mut Option<TypeDeclIndex<'a>>,
) -> bool {
  use oxc_ast::ast::{TSType, TSTypeName, TSTypeOperatorOperator};
  if depth > 4 {
    return false;
  }
  if ts_type_reactive_kind(ts_type).is_some() {
    return false;
  }
  match ts_type {
    TSType::TSParenthesizedType(paren) => {
      ts_type_is_plain_object_shaped(semantic, &paren.type_annotation, depth, index)
    }
    TSType::TSTypeOperatorType(operator)
      if operator.operator == TSTypeOperatorOperator::Readonly =>
    {
      ts_type_is_plain_object_shaped(semantic, &operator.type_annotation, depth, index)
    }
    TSType::TSTypeLiteral(literal) => signatures_are_plain_object_shaped(&literal.members),
    TSType::TSTypeReference(reference) => {
      let Some(name) = (match &reference.type_name {
        TSTypeName::IdentifierReference(identifier) => Some(identifier.name.as_str()),
        TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => None,
      }) else {
        return false;
      };
      let alias = {
        let decls = index.get_or_insert_with(|| TypeDeclIndex::build(semantic));
        if let Some(members) = decls.interfaces.get(name).copied() {
          return signatures_are_plain_object_shaped(members);
        }
        decls.aliases.get(name).copied()
      };
      let Some(alias) = alias else {
        return false;
      };
      ts_type_is_plain_object_shaped(semantic, alias, depth.saturating_add(1), index)
    }
    _ => false,
  }
}

fn signatures_are_plain_object_shaped(members: &[oxc_ast::ast::TSSignature<'_>]) -> bool {
  use oxc_ast::ast::TSSignature;
  let mut property_count = 0_usize;
  for member in members {
    let TSSignature::TSPropertySignature(property) = member else {
      continue;
    };
    property_count = property_count.saturating_add(1);
    let Some(annotation) = &property.type_annotation else {
      continue;
    };
    if ts_type_reactive_kind(&annotation.type_annotation).is_some() {
      return false;
    }
  }
  property_count > 0
}

/// Map a TypeScript return type surface to a reactive binding kind (under-approx).
///
/// Only recognizes Vue ref-like type names (`Ref`, `ComputedRef`, …). Full checker
/// inference and utility wrappers stay quiet.
fn ts_type_reactive_kind(ts_type: &oxc_ast::ast::TSType<'_>) -> Option<ReactiveBindingKind> {
  use oxc_ast::ast::{TSType, TSTypeName, TSTypeOperatorOperator};
  match ts_type {
    TSType::TSParenthesizedType(paren) => ts_type_reactive_kind(&paren.type_annotation),
    TSType::TSTypeOperatorType(operator)
      if operator.operator == TSTypeOperatorOperator::Readonly =>
    {
      ts_type_reactive_kind(&operator.type_annotation).map(|kind| match kind {
        ReactiveBindingKind::Ref => ReactiveBindingKind::Readonly,
        ReactiveBindingKind::ShallowRef => ReactiveBindingKind::ShallowReadonly,
        other => other,
      })
    }
    TSType::TSTypeReference(reference) => {
      let name = match &reference.type_name {
        TSTypeName::IdentifierReference(identifier) => identifier.name.as_str(),
        // `vue.Ref` / `import('vue').ShallowRef` rightmost name (qualified only).
        TSTypeName::QualifiedName(qualified) => qualified.right.name.as_str(),
        TSTypeName::ThisExpression(_) => return None,
      };
      match name {
        "Ref" => Some(ReactiveBindingKind::Ref),
        "ShallowRef" => Some(ReactiveBindingKind::ShallowRef),
        "ComputedRef" | "WritableComputedRef" => Some(ReactiveBindingKind::Computed),
        "CustomRef" => Some(ReactiveBindingKind::CustomRef),
        "ToRef" => Some(ReactiveBindingKind::ToRef),
        "Readonly" => {
          // `Readonly<Ref<T>>` — peel one type argument when present.
          let arg = reference.type_arguments.as_ref()?.params.first()?;
          ts_type_reactive_kind(arg).map(|kind| match kind {
            ReactiveBindingKind::Ref => ReactiveBindingKind::Readonly,
            ReactiveBindingKind::ShallowRef => ReactiveBindingKind::ShallowReadonly,
            other => other,
          })
        }
        _ => None,
      }
    }
    _ => None,
  }
}

/// Same-file `interface` / `type` declarations, built once per shape query.
struct TypeDeclIndex<'a> {
  interfaces: BTreeMap<&'a str, &'a [oxc_ast::ast::TSSignature<'a>]>,
  aliases: BTreeMap<&'a str, &'a oxc_ast::ast::TSType<'a>>,
}

impl<'a> TypeDeclIndex<'a> {
  fn build(semantic: &'a oxc_semantic::Semantic<'a>) -> Self {
    let mut interfaces = BTreeMap::new();
    let mut aliases = BTreeMap::new();
    for node in semantic.nodes() {
      match node.kind() {
        AstKind::TSInterfaceDeclaration(interface) => {
          interfaces.insert(interface.id.name.as_str(), interface.body.body.as_slice());
        }
        AstKind::TSTypeAliasDeclaration(alias) => {
          aliases.insert(alias.id.name.as_str(), &alias.type_annotation);
        }
        _ => {}
      }
    }
    Self { interfaces, aliases }
  }
}

/// Object-bag shape from a TypeScript return type (under-approx).
///
/// Recognizes inline `{ width: Ref<number> }`, same-file `interface` / `type`
/// aliases, and peels a single `readonly` operator. Non-reactive fields
/// (`stop: () => void`) stay out of the shape. Depth-bounded alias follow.
fn ts_type_composable_shape<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  ts_type: &'a oxc_ast::ast::TSType<'a>,
  depth: u8,
  index: &mut Option<TypeDeclIndex<'a>>,
) -> BTreeMap<String, ReactiveBindingKind> {
  use oxc_ast::ast::{TSType, TSTypeName, TSTypeOperatorOperator};
  if depth > 4 {
    return BTreeMap::new();
  }
  // Scalar Ref returns are Factory, not bags.
  if ts_type_reactive_kind(ts_type).is_some() {
    return BTreeMap::new();
  }
  match ts_type {
    TSType::TSParenthesizedType(paren) => {
      ts_type_composable_shape(semantic, &paren.type_annotation, depth, index)
    }
    TSType::TSTypeOperatorType(operator)
      if operator.operator == TSTypeOperatorOperator::Readonly =>
    {
      ts_type_composable_shape(semantic, &operator.type_annotation, depth, index)
    }
    TSType::TSTypeLiteral(literal) => shape_from_ts_signatures(&literal.members),
    TSType::TSTypeReference(reference) => {
      let Some(name) = (match &reference.type_name {
        TSTypeName::IdentifierReference(identifier) => Some(identifier.name.as_str()),
        TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => None,
      }) else {
        return BTreeMap::new();
      };
      // Resolve through a one-shot index; drop borrows before recursing into aliases.
      let alias = {
        let decls = index.get_or_insert_with(|| TypeDeclIndex::build(semantic));
        if let Some(members) = decls.interfaces.get(name).copied() {
          return shape_from_ts_signatures(members);
        }
        decls.aliases.get(name).copied()
      };
      let Some(alias) = alias else {
        return BTreeMap::new();
      };
      ts_type_composable_shape(semantic, alias, depth.saturating_add(1), index)
    }
    _ => BTreeMap::new(),
  }
}

fn shape_from_ts_signatures(
  members: &[oxc_ast::ast::TSSignature<'_>],
) -> BTreeMap<String, ReactiveBindingKind> {
  use oxc_ast::ast::TSSignature;
  let mut shape = BTreeMap::new();
  for member in members {
    let TSSignature::TSPropertySignature(property) = member else {
      continue;
    };
    let Some(exported) = property.key.static_name() else {
      continue;
    };
    let Some(annotation) = &property.type_annotation else {
      continue;
    };
    let Some(kind) = ts_type_reactive_kind(&annotation.type_annotation) else {
      continue;
    };
    shape.insert(exported.into_owned(), kind);
  }
  shape
}

#[expect(
  clippy::too_many_arguments,
  reason = "shape merge is a pure helper; packing args would obscure the call sites"
)]
fn merge_return_object_into_shape(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  graph: &ReactivityGraph,
  imported_bindings: &BTreeMap<String, (String, String)>,
  param_names: &BTreeSet<String>,
  script_offset: usize,
  shape: &mut BTreeMap<String, ReactiveBindingKind>,
  ambiguous: &mut BTreeSet<String>,
) {
  // `() => ({ field })` wraps the object in parentheses.
  let expression = match expression {
    Expression::ParenthesizedExpression(paren) => &paren.expression,
    other => other,
  };
  // `return toRefs(param)` — every static key is ToRef when the argument is a parameter.
  if let Expression::CallExpression(call) = expression
    && resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Script)
      .is_some_and(|callee| callee == "toRefs")
    && call
      .arguments
      .first()
      .and_then(Argument::as_expression)
      .and_then(Expression::get_identifier_reference)
      .is_some_and(|identifier| param_names.contains(identifier.name.as_str()))
  {
    // Without an object shape we cannot invent keys; leave quiet.
    return;
  }
  let Expression::ObjectExpression(object) = expression else {
    return;
  };
  for property in &object.properties {
    let ObjectPropertyKind::ObjectProperty(property) = property else {
      continue;
    };
    let Some(exported) = property.key.static_name() else {
      continue;
    };
    let Some(kind) = reactive_return_kind(
      semantic,
      &property.value,
      graph,
      imported_bindings,
      param_names,
      script_offset,
    ) else {
      continue;
    };
    let exported = exported.into_owned();
    if ambiguous.contains(&exported) {
      continue;
    }
    match shape.entry(exported.clone()) {
      Entry::Vacant(entry) => {
        entry.insert(kind);
      }
      Entry::Occupied(entry) if *entry.get() == kind => {}
      Entry::Occupied(entry) => {
        entry.remove();
        ambiguous.insert(exported);
      }
    }
  }
}

fn function_param_names(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
) -> BTreeSet<String> {
  let mut names = BTreeSet::new();
  let parameters = match semantic.nodes().kind(function_id) {
    AstKind::Function(function) => function.params.items.as_slice(),
    AstKind::ArrowFunctionExpression(callback) => callback.params.items.as_slice(),
    _ => return names,
  };
  for parameter in parameters {
    let mut identifiers = Vec::new();
    collect_binding_identifiers(&parameter.pattern, &mut identifiers);
    for (name, _) in identifiers {
      names.insert(name);
    }
  }
  names
}

fn reactive_return_kind(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  graph: &ReactivityGraph,
  imported_bindings: &BTreeMap<String, (String, String)>,
  param_names: &BTreeSet<String>,
  script_offset: usize,
) -> Option<ReactiveBindingKind> {
  if let Some(reference) = expression.get_identifier_reference() {
    if param_names.contains(reference.name.as_str()) {
      // Parametric pass-through: treat as reactive object/ref surface.
      return Some(ReactiveBindingKind::Reactive);
    }
    return graph
      .bindings
      .iter()
      .find(|binding| {
        binding.name == reference.name.as_str()
          && reference_resolves_to_binding(semantic, reference, binding, script_offset)
      })
      .map(|binding| binding.kind);
  }

  let Expression::CallExpression(call) = expression else {
    return None;
  };
  let callee = resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Script)?;
  if matches!(callee.as_str(), "toRef" | "toRefs") {
    // Parametric when first argument is a function parameter.
    if call
      .arguments
      .first()
      .and_then(Argument::as_expression)
      .and_then(Expression::get_identifier_reference)
      .is_some_and(|identifier| param_names.contains(identifier.name.as_str()))
    {
      return Some(ReactiveBindingKind::ToRef);
    }
  }
  reactive_binding_kind(&callee)
}

fn collect_exports(semantic: &oxc_semantic::Semantic<'_>) -> Vec<ExportSummary> {
  let mut exports = Vec::new();
  for node in semantic.nodes() {
    match node.kind() {
      AstKind::ExportNamedDeclaration(declaration) => {
        match &declaration.declaration {
          Some(Declaration::VariableDeclaration(variable)) => {
            for declarator in &variable.declarations {
              let mut identifiers = Vec::new();
              collect_binding_identifiers(&declarator.id, &mut identifiers);
              for (local, _) in identifiers {
                exports.push(ExportSummary::Local { exported: local.clone(), local });
              }
            }
          }
          Some(Declaration::FunctionDeclaration(function)) => {
            if let Some(identifier) = &function.id {
              let local = identifier.name.to_string();
              exports.push(ExportSummary::Local { exported: local.clone(), local });
            }
          }
          _ => {}
        }
        for specifier in &declaration.specifiers {
          let local = module_export_name(&specifier.local);
          let exported = module_export_name(&specifier.exported);
          if let Some(source) = &declaration.source {
            exports.push(ExportSummary::Reexport {
              source: source.value.to_string(),
              imported: local,
              exported,
            });
          } else {
            exports.push(ExportSummary::Local { local, exported });
          }
        }
      }
      AstKind::ExportDefaultDeclaration(declaration) => match &declaration.declaration {
        ExportDefaultDeclarationKind::Identifier(identifier) => {
          exports.push(ExportSummary::Local {
            local: identifier.name.to_string(),
            exported: "default".into(),
          });
        }
        ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
          // `export default function useX() { … }` — local name is the function id.
          if let Some(identifier) = &function.id {
            exports.push(ExportSummary::Local {
              local: identifier.name.to_string(),
              exported: "default".into(),
            });
          }
        }
        _ => {}
      },
      AstKind::ExportAllDeclaration(declaration) if declaration.exported.is_none() => {
        exports.push(ExportSummary::Star { source: declaration.source.value.to_string() });
      }
      _ => {}
    }
  }
  exports
}

pub(super) fn join_errors(errors: &[impl ToString]) -> String {
  errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
}

mod link;

pub use link::{
  ModuleTraceState, TraceModulesOptions, TraceModulesReport, TraceModulesStats, trace_modules,
  trace_modules_incremental_with_options, trace_modules_with_options,
};
