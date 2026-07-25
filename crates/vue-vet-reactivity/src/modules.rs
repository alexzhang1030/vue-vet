use std::{
  collections::{BTreeMap, BTreeSet, btree_map::Entry},
  sync::mpsc,
  thread,
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
use vue_vet_core::{ReactiveBindingFact, ReactiveBindingKind, ReactivityGraph, ScriptKind};

use super::{
  TraceSeeds, collect_binding_identifiers, collect_imported_bindings, collect_inject_sites,
  collect_provide_sites, module_export_name, provide_kind_index, reactive_binding_kind,
  reference_resolves_to_binding, resolve_inject_kind, resolved_vue_callee, source_span,
  trace_reactivity_seeded,
};
use oxc_ast::ast::Argument;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleSource {
  pub id: String,
  /// Text parsed by Oxc (extracted `<script>` body for SFCs).
  pub source: String,
  pub language: String,
  pub kind: ScriptKind,
  /// Byte offset of [`Self::source`] within [`Self::span_source`].
  #[serde(default)]
  pub source_offset: usize,
  /// Full original file used for absolute line/column (SFC source). When empty,
  /// spans are computed against [`Self::source`] (standalone modules).
  #[serde(default)]
  pub span_source: String,
}

impl ModuleSource {
  /// Standalone JS/TS module (offset 0, spans against `source`).
  #[must_use]
  pub fn standalone(
    id: impl Into<String>,
    source: impl Into<String>,
    language: impl Into<String>,
    kind: ScriptKind,
  ) -> Self {
    Self {
      id: id.into(),
      source: source.into(),
      language: language.into(),
      kind,
      source_offset: 0,
      span_source: String::new(),
    }
  }

  /// Extracted SFC script block with absolute span mapping into the original file.
  #[must_use]
  pub fn sfc_script(
    id: impl Into<String>,
    script_source: impl Into<String>,
    language: impl Into<String>,
    kind: ScriptKind,
    source_offset: usize,
    sfc_source: impl Into<String>,
  ) -> Self {
    Self {
      id: id.into(),
      source: script_source.into(),
      language: language.into(),
      kind,
      source_offset,
      span_source: sfc_source.into(),
    }
  }

  const fn span_origin(&self) -> &str {
    if self.span_source.is_empty() { self.source.as_str() } else { self.span_source.as_str() }
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleLink {
  pub from: String,
  pub specifier: String,
  pub to: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleReactivity {
  pub id: String,
  pub graph: ReactivityGraph,
}

#[derive(Debug, Error)]
pub enum TraceModulesError {
  #[error("duplicate reactivity module id `{0}`")]
  DuplicateModule(String),
  #[error("module `{module}` uses unsupported language `{language}`")]
  UnsupportedLanguage { module: String, language: String },
  #[error("could not parse reactivity module `{module}`: {message}")]
  Parse { module: String, message: String },
  #[error("could not build semantics for reactivity module `{module}`: {message}")]
  Semantic { module: String, message: String },
  #[error("reactivity module link {from} -> {to} references an unknown module")]
  UnknownLink { from: String, to: String },
  #[error("reactivity module `{from}` resolves `{specifier}` to multiple targets")]
  AmbiguousLink { from: String, specifier: String },
  #[error("reactivity module worker disconnected before completing the seed barrier")]
  WorkerDisconnected,
}

#[derive(Clone, Debug)]
struct ImportSummary {
  local: String,
  imported: String,
  source: String,
  span: Span,
}

#[derive(Clone, Debug)]
enum ExportSummary {
  Local { local: String, exported: String },
  Reexport { source: String, imported: String, exported: String },
  Star { source: String },
}

#[derive(Clone, Debug)]
struct DestructuredCallBinding {
  imported_local: String,
  property: String,
  local: String,
  span: Span,
}

/// `const bag = useFoo()` — whole-object composable call used via member access.
#[derive(Clone, Debug)]
struct InstanceCallBinding {
  imported_local: String,
  local: String,
  span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ExportState {
  Known(ReactiveBindingKind),
  Composable(BTreeMap<String, ReactiveBindingKind>),
  Ambiguous,
}

/// Export-resolution payload only — no source body, no reactivity graph.
/// Moved (not cloned) from workers to the coordinator across the seed barrier.
#[derive(Debug)]
struct ModuleExportFacts {
  id: String,
  imports: Vec<ImportSummary>,
  exports: Vec<ExportSummary>,
  locals: BTreeMap<String, ExportState>,
  /// `provide(key, value)` sites with optional known value shapes.
  provides: Vec<super::ProvideSite>,
  /// `const local = inject(key)` sites for unique-key seed resolution.
  injects: Vec<super::InjectSite>,
}

/// Per-import resolution for one consumer module (`import.local` → export state).
/// Spans are applied on the worker that still holds the parse.
type ImportSeedPlan = BTreeMap<String, ExportState>;

/// Cross-module seeds delivered after the barrier (imports + unique inject keys).
#[derive(Debug, Default)]
struct ModuleSeedPlan {
  imports: ImportSeedPlan,
  /// inject local → reactive kind (from unique provide or static default).
  injects: BTreeMap<String, ReactiveBindingKind>,
}

impl ModuleSeedPlan {
  fn is_empty(&self) -> bool {
    self.imports.is_empty() && self.injects.is_empty()
  }
}

/// Traces local and linked reactivity across a resolved module graph.
///
/// Each module is **parsed once**. Workers keep the Oxc allocator/semantic on
/// their stack across the seed barrier (no second parse, no `unsafe` session).
/// Phase 1 moves lightweight export facts to the coordinator; phase 2 re-traces
/// on the same semantic when cross-file seeds exist. Module source is borrowed
/// via `thread::scope` — never cloned.
///
/// # Errors
///
/// Returns an error when a module cannot be parsed or analyzed, module identifiers
/// are duplicated, or a supplied resolved link is unknown or ambiguous.
pub fn trace_modules(
  modules: &[ModuleSource],
  links: &[ModuleLink],
) -> Result<Vec<ModuleReactivity>, TraceModulesError> {
  // Duplicate check is sequential and deterministic.
  let mut seen = BTreeSet::new();
  for module in modules {
    if !seen.insert(module.id.as_str()) {
      return Err(TraceModulesError::DuplicateModule(module.id.clone()));
    }
  }

  if modules.is_empty() {
    return Ok(Vec::new());
  }

  // Sticky workers: parse stays on the worker stack (Semantic is not Send).
  // One channel triple per module avoids cloning mpsc::Sender.
  thread::scope(|scope| {
    let mut seed_txs = Vec::with_capacity(modules.len());
    let mut facts_rxs = Vec::with_capacity(modules.len());
    let mut result_rxs = Vec::with_capacity(modules.len());

    for module in modules {
      let (facts_tx, facts_rx) = mpsc::channel::<Result<ModuleExportFacts, TraceModulesError>>();
      let (seed_tx, seed_rx) = mpsc::channel::<ModuleSeedPlan>();
      let (result_tx, result_rx) = mpsc::channel::<Result<ModuleReactivity, TraceModulesError>>();
      facts_rxs.push(facts_rx);
      seed_txs.push(seed_tx);
      result_rxs.push(result_rx);

      scope.spawn(move || {
        let outcome = worker_trace_module(module, &facts_tx, &seed_rx);
        drop(result_tx.send(outcome));
      });
    }

    // Phase 1: receive moved export facts (index-aligned with `modules`).
    let mut facts_by_id = BTreeMap::new();
    for facts_rx in facts_rxs {
      let facts = facts_rx.recv().map_err(|_| TraceModulesError::WorkerDisconnected)??;
      facts_by_id.insert(facts.id.clone(), facts);
    }
    if facts_by_id.len() != modules.len() {
      return Err(TraceModulesError::WorkerDisconnected);
    }

    let resolved_links = resolved_links(&facts_by_id, links)?;
    let link_index = link_index(&resolved_links);
    let exports = resolve_exports(&facts_by_id, &link_index);
    let provide_index = global_provide_index(&facts_by_id);

    // Phase 2: deliver seed plans (import shapes + unique inject keys).
    for (module, seed_tx) in modules.iter().zip(seed_txs) {
      let Some(facts) = facts_by_id.get(module.id.as_str()) else {
        return Err(TraceModulesError::WorkerDisconnected);
      };
      let plan = ModuleSeedPlan {
        imports: seed_plan_for(facts, &exports, &link_index),
        injects: inject_seed_plan(facts, &provide_index),
      };
      if seed_tx.send(plan).is_err() {
        return Err(TraceModulesError::WorkerDisconnected);
      }
    }

    let mut traced = Vec::with_capacity(modules.len());
    for result_rx in result_rxs {
      traced.push(result_rx.recv().map_err(|_| TraceModulesError::WorkerDisconnected)??);
    }
    traced.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(traced)
  })
}

/// Worker body: one parse, hand off export facts, re-trace with seeds if needed.
fn worker_trace_module(
  module: &ModuleSource,
  facts_tx: &mpsc::Sender<Result<ModuleExportFacts, TraceModulesError>>,
  seed_rx: &mpsc::Receiver<ModuleSeedPlan>,
) -> Result<ModuleReactivity, TraceModulesError> {
  let allocator = Allocator::default();
  let source_type = source_type(module)?;
  let parsed = Parser::new(&allocator, module.source.as_str(), source_type).parse();
  if !parsed.errors.is_empty() {
    let error =
      TraceModulesError::Parse { module: module.id.clone(), message: join_errors(&parsed.errors) };
    drop(facts_tx.send(Err(error_from(&error))));
    return Err(error);
  }
  let built = SemanticBuilder::new().with_check_syntax_error(true).build(&parsed.program);
  if !built.errors.is_empty() {
    let error = TraceModulesError::Semantic {
      module: module.id.clone(),
      message: join_errors(&built.errors),
    };
    drop(facts_tx.send(Err(error_from(&error))));
    return Err(error);
  }
  let semantic = built.semantic;

  let empty = TraceSeeds::default();
  let local_graph = trace_reactivity_seeded(
    &semantic,
    module.span_origin(),
    module.source_offset,
    module.kind,
    &empty,
  );
  let imports = collect_imports(&semantic);
  let export_decls = collect_exports(&semantic);
  let shape_graph = ReactivityGraph {
    bindings: super::collect_reactive_bindings(
      &semantic,
      &super::collect_imported_bindings(&semantic),
      module.span_origin(),
      module.source_offset,
      module.kind,
      true,
    ),
    ..ReactivityGraph::default()
  };
  let locals = collect_local_values(&semantic, &local_graph, &shape_graph, module.source_offset);
  let imported_bindings = super::collect_imported_bindings(&semantic);
  let provides =
    collect_provide_sites(&semantic, &imported_bindings, &local_graph.bindings, module.kind);
  let injects =
    collect_inject_sites(&semantic, &imported_bindings, &local_graph.bindings, module.kind);

  // Move export facts out; keep `local_graph` for the empty-seed fast path.
  // Destructure/instance call sites are re-read on the worker when materializing seeds.
  let facts = ModuleExportFacts {
    id: module.id.clone(),
    imports,
    exports: export_decls,
    locals,
    provides,
    injects,
  };
  if facts_tx.send(Ok(facts)).is_err() {
    return Ok(ModuleReactivity { id: module.id.clone(), graph: local_graph });
  }

  let Ok(plan) = seed_rx.recv() else {
    return Ok(ModuleReactivity { id: module.id.clone(), graph: local_graph });
  };

  let graph = if plan.is_empty() {
    local_graph
  } else {
    drop(local_graph);
    let seeds = materialize_seeds(module, &semantic, &plan);
    trace_reactivity_seeded(
      &semantic,
      module.span_origin(),
      module.source_offset,
      module.kind,
      &seeds,
    )
  };
  Ok(ModuleReactivity { id: module.id.clone(), graph })
}

fn source_type(module: &ModuleSource) -> Result<SourceType, TraceModulesError> {
  match module.language.as_str() {
    "js" | "javascript" => Ok(SourceType::mjs()),
    "jsx" => Ok(SourceType::jsx()),
    "ts" | "typescript" => Ok(SourceType::ts()),
    "tsx" => Ok(SourceType::tsx()),
    language => Err(TraceModulesError::UnsupportedLanguage {
      module: module.id.clone(),
      language: language.into(),
    }),
  }
}

/// Clone only the error strings needed to unblock the coordinator channel.
fn error_from(error: &TraceModulesError) -> TraceModulesError {
  match error {
    TraceModulesError::DuplicateModule(id) => TraceModulesError::DuplicateModule(id.clone()),
    TraceModulesError::UnsupportedLanguage { module, language } => {
      TraceModulesError::UnsupportedLanguage { module: module.clone(), language: language.clone() }
    }
    TraceModulesError::Parse { module, message } => {
      TraceModulesError::Parse { module: module.clone(), message: message.clone() }
    }
    TraceModulesError::Semantic { module, message } => {
      TraceModulesError::Semantic { module: module.clone(), message: message.clone() }
    }
    TraceModulesError::UnknownLink { from, to } => {
      TraceModulesError::UnknownLink { from: from.clone(), to: to.clone() }
    }
    TraceModulesError::AmbiguousLink { from, specifier } => {
      TraceModulesError::AmbiguousLink { from: from.clone(), specifier: specifier.clone() }
    }
    TraceModulesError::WorkerDisconnected => TraceModulesError::WorkerDisconnected,
  }
}

fn collect_imports(semantic: &oxc_semantic::Semantic<'_>) -> Vec<ImportSummary> {
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

  // `function useX() { return { field } }` (incl. `export default function useX`)
  for node in semantic.nodes() {
    let AstKind::Function(function) = node.kind() else {
      continue;
    };
    let Some(identifier) = &function.id else {
      continue;
    };
    let shape =
      composable_return_shape(semantic, function.node_id.get(), shape_graph, script_offset);
    if !shape.is_empty() {
      locals.insert(identifier.name.to_string(), ExportState::Composable(shape));
    }
  }

  // `const useX = () => ({ … })` / `export const useX = function () { … }`
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
    let function_id = match init {
      Expression::ArrowFunctionExpression(arrow) => arrow.node_id.get(),
      Expression::FunctionExpression(function) => function.node_id.get(),
      _ => continue,
    };
    let shape = composable_return_shape(semantic, function_id, shape_graph, script_offset);
    if shape.is_empty() {
      continue;
    }
    locals.insert(identifier.name.to_string(), ExportState::Composable(shape));
  }
  locals
}

/// Object shape returned by a composable function / arrow (under-approx).
///
/// `script_offset` must match the offset used when materializing `graph.bindings`
/// spans (0 for standalone modules, Vize `loc.start` for SFC script bodies).
pub fn composable_return_shape(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  graph: &ReactivityGraph,
  script_offset: usize,
) -> BTreeMap<String, ReactiveBindingKind> {
  let imported_bindings = collect_imported_bindings(semantic);
  let param_names = function_param_names(semantic, function_id);
  let mut shape = BTreeMap::new();
  let mut ambiguous = BTreeSet::new();

  // `() => ({ field: ref(0) })` expression body — no ReturnStatement node.
  if let AstKind::ArrowFunctionExpression(arrow) = semantic.nodes().kind(function_id)
    && arrow.expression
    && let Some(statement) = arrow.body.statements.first()
    && let oxc_ast::ast::Statement::ExpressionStatement(expression) = statement
  {
    merge_return_object_into_shape(
      semantic,
      &expression.expression,
      graph,
      &imported_bindings,
      &param_names,
      script_offset,
      &mut shape,
      &mut ambiguous,
    );
  }

  for (return_id, node) in semantic.nodes().iter_enumerated() {
    let AstKind::ReturnStatement(statement) = node.kind() else {
      continue;
    };
    let owner = semantic.nodes().ancestor_ids(return_id).find(|ancestor_id| {
      matches!(
        semantic.nodes().kind(*ancestor_id),
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
      )
    });
    if owner != Some(function_id) {
      continue;
    }
    let Some(argument) = &statement.argument else {
      continue;
    };
    merge_return_object_into_shape(
      semantic,
      argument,
      graph,
      &imported_bindings,
      &param_names,
      script_offset,
      &mut shape,
      &mut ambiguous,
    );
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
    && resolved_vue_callee(&call.callee, imported_bindings, ScriptKind::Script)
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
  let callee = resolved_vue_callee(&call.callee, imported_bindings, ScriptKind::Script)?;
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

fn resolve_imported_callee<'a>(
  semantic: &oxc_semantic::Semantic<'_>,
  callee: &oxc_ast::ast::IdentifierReference<'_>,
  imports: &'a [ImportSummary],
) -> Option<&'a ImportSummary> {
  imports.iter().find(|import| {
    if import.local != callee.name.as_str() {
      return false;
    }
    let Some(reference_id) = callee.reference_id.get() else {
      return false;
    };
    semantic
      .scoping()
      .get_reference(reference_id)
      .symbol_id()
      .is_some_and(|symbol_id| semantic.scoping().symbol_span(symbol_id) == import.span)
  })
}

fn collect_destructured_calls(
  semantic: &oxc_semantic::Semantic<'_>,
  imports: &[ImportSummary],
) -> Vec<DestructuredCallBinding> {
  let mut calls = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = call.callee.get_identifier_reference() else {
      continue;
    };
    let Some(import) = resolve_imported_callee(semantic, callee, imports) else {
      continue;
    };
    let AstKind::VariableDeclarator(declarator) = semantic.nodes().parent_kind(call.node_id.get())
    else {
      continue;
    };
    let BindingPattern::ObjectPattern(pattern) = &declarator.id else {
      continue;
    };
    for property in &pattern.properties {
      let Some(exported) = property.key.static_name() else {
        continue;
      };
      let mut identifiers = Vec::new();
      collect_binding_identifiers(&property.value, &mut identifiers);
      for (local, span) in identifiers {
        calls.push(DestructuredCallBinding {
          imported_local: import.local.clone(),
          property: exported.to_string(),
          local,
          span,
        });
      }
    }
  }
  calls.sort_by_key(|call| call.span.start);
  calls
}

fn collect_instance_calls(
  semantic: &oxc_semantic::Semantic<'_>,
  imports: &[ImportSummary],
) -> Vec<InstanceCallBinding> {
  let mut calls = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = call.callee.get_identifier_reference() else {
      continue;
    };
    let Some(import) = resolve_imported_callee(semantic, callee, imports) else {
      continue;
    };
    let AstKind::VariableDeclarator(declarator) = semantic.nodes().parent_kind(call.node_id.get())
    else {
      continue;
    };
    let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
      continue;
    };
    calls.push(InstanceCallBinding {
      imported_local: import.local.clone(),
      local: identifier.name.to_string(),
      span: identifier.span,
    });
  }
  calls.sort_by_key(|call| call.span.start);
  calls
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

fn resolved_links(
  facts: &BTreeMap<String, ModuleExportFacts>,
  links: &[ModuleLink],
) -> Result<BTreeMap<(String, String), String>, TraceModulesError> {
  let mut resolved = BTreeMap::new();
  for link in links {
    if !facts.contains_key(&link.from) || !facts.contains_key(&link.to) {
      return Err(TraceModulesError::UnknownLink { from: link.from.clone(), to: link.to.clone() });
    }
    let key = (link.from.clone(), link.specifier.clone());
    match resolved.entry(key) {
      Entry::Vacant(entry) => {
        entry.insert(link.to.clone());
      }
      Entry::Occupied(entry) if entry.get() == &link.to => {}
      Entry::Occupied(_) => {
        return Err(TraceModulesError::AmbiguousLink {
          from: link.from.clone(),
          specifier: link.specifier.clone(),
        });
      }
    }
  }
  Ok(resolved)
}

fn resolve_exports(
  facts: &BTreeMap<String, ModuleExportFacts>,
  links: &BTreeMap<(&str, &str), &str>,
) -> BTreeMap<String, BTreeMap<String, ExportState>> {
  let mut resolved =
    facts.keys().map(|id| (id.clone(), BTreeMap::new())).collect::<BTreeMap<_, _>>();

  for (id, module_facts) in facts {
    for export in &module_facts.exports {
      let ExportSummary::Local { local, exported } = export else {
        continue;
      };
      if let Some(state) = module_facts.locals.get(local) {
        insert_export(&mut resolved, id, exported, state.clone());
      }
    }
  }

  loop {
    let snapshot = resolved.clone();
    let mut changed = false;
    for (id, module_facts) in facts {
      for export in &module_facts.exports {
        match export {
          ExportSummary::Local { .. } => {}
          ExportSummary::Reexport { source, imported, exported } => {
            let Some(target) = links.get(&(id.as_str(), source.as_str())).copied() else {
              continue;
            };
            let Some(state) = snapshot.get(target).and_then(|exports| exports.get(imported)) else {
              continue;
            };
            changed |= insert_export(&mut resolved, id, exported, state.clone());
          }
          ExportSummary::Star { source } => {
            let Some(target) = links.get(&(id.as_str(), source.as_str())).copied() else {
              continue;
            };
            let Some(target_exports) = snapshot.get(target) else {
              continue;
            };
            for (exported, state) in target_exports {
              if exported != "default" {
                changed |= insert_export(&mut resolved, id, exported, state.clone());
              }
            }
          }
        }
      }
    }
    if !changed {
      break;
    }
  }

  resolved
}

/// Borrowed index over owned resolved links — avoids re-allocating key pairs on lookup.
fn link_index(links: &BTreeMap<(String, String), String>) -> BTreeMap<(&str, &str), &str> {
  links
    .iter()
    .map(|((from, specifier), to)| ((from.as_str(), specifier.as_str()), to.as_str()))
    .collect()
}

fn insert_export(
  resolved: &mut BTreeMap<String, BTreeMap<String, ExportState>>,
  module: &str,
  exported: &str,
  state: ExportState,
) -> bool {
  let Some(module_exports) = resolved.get_mut(module) else {
    return false;
  };
  match module_exports.entry(exported.into()) {
    Entry::Vacant(entry) => {
      entry.insert(state);
      true
    }
    Entry::Occupied(mut entry)
      if entry.get() != &state && entry.get() != &ExportState::Ambiguous =>
    {
      entry.insert(ExportState::Ambiguous);
      true
    }
    Entry::Occupied(_) => false,
  }
}

/// Coordinator-side: which of this module's import locals resolve to reactive exports.
fn seed_plan_for(
  facts: &ModuleExportFacts,
  exports: &BTreeMap<String, BTreeMap<String, ExportState>>,
  links: &BTreeMap<(&str, &str), &str>,
) -> ImportSeedPlan {
  let mut plan = ImportSeedPlan::new();
  for import in &facts.imports {
    if import.imported == "*" {
      continue;
    }
    let Some(target) = links.get(&(facts.id.as_str(), import.source.as_str())).copied() else {
      continue;
    };
    let Some(state) =
      exports.get(target).and_then(|module_exports| module_exports.get(&import.imported))
    else {
      continue;
    };
    // Only the resolved export state crosses the barrier (not source text / graphs).
    plan.insert(import.local.clone(), state.clone());
  }
  plan
}

/// Project-wide provide index (no App Tree): key → kinds from every known site.
fn global_provide_index(
  facts: &BTreeMap<String, ModuleExportFacts>,
) -> BTreeMap<super::InjectionKey, Vec<ReactiveBindingKind>> {
  let mut all = Vec::new();
  for module in facts.values() {
    all.extend(module.provides.iter().cloned());
  }
  provide_kind_index(&all)
}

/// Unique inject seeds for one consumer (multi-provide keys stay quiet).
fn inject_seed_plan(
  facts: &ModuleExportFacts,
  provide_index: &BTreeMap<super::InjectionKey, Vec<ReactiveBindingKind>>,
) -> BTreeMap<String, ReactiveBindingKind> {
  let mut plan = BTreeMap::new();
  for inject in &facts.injects {
    let Some(kind) = resolve_inject_kind(provide_index, inject) else {
      continue;
    };
    plan.insert(inject.local.clone(), kind);
  }
  plan
}

/// Worker-side: attach SFC-absolute spans from the live parse (no second parse).
fn materialize_seeds(
  module: &ModuleSource,
  semantic: &Semantic<'_>,
  plan: &ModuleSeedPlan,
) -> TraceSeeds {
  if plan.is_empty() {
    return TraceSeeds::default();
  }
  let imports = collect_imports(semantic);
  let destructured_calls = collect_destructured_calls(semantic, &imports);
  let instance_calls = collect_instance_calls(semantic, &imports);
  let span_source = module.span_origin();
  let span_base = module.source_offset;
  let mut seeds = TraceSeeds::default();
  for import in &imports {
    let Some(state) = plan.imports.get(&import.local) else {
      continue;
    };
    match state {
      ExportState::Known(kind) => seeds.bindings.push(ReactiveBindingFact {
        name: import.local.clone(),
        kind: *kind,
        initialized_with_null: false,
        span: source_span(span_source, span_base, import.span),
      }),
      ExportState::Composable(shape) => {
        for call in destructured_calls.iter().filter(|call| call.imported_local == import.local) {
          let Some(kind) = shape.get(&call.property) else {
            continue;
          };
          seeds.bindings.push(ReactiveBindingFact {
            name: call.local.clone(),
            kind: *kind,
            initialized_with_null: false,
            span: source_span(span_source, span_base, call.span),
          });
        }
        for call in instance_calls.iter().filter(|call| call.imported_local == import.local) {
          // Only record the instance bag for `bag.field.value` resolution.
          seeds.composable_instances.insert(call.local.clone(), shape.clone());
        }
      }
      ExportState::Ambiguous => {}
    }
  }
  // Inject locals: re-read sites for exact spans, kinds from the coordinator plan.
  if !plan.injects.is_empty() {
    let imported_bindings = super::collect_imported_bindings(semantic);
    let injects = collect_inject_sites(semantic, &imported_bindings, &[], module.kind);
    for inject in injects {
      let Some(kind) = plan.injects.get(&inject.local).copied() else {
        continue;
      };
      if seeds.bindings.iter().any(|binding| binding.name == inject.local) {
        continue;
      }
      seeds.bindings.push(ReactiveBindingFact {
        name: inject.local,
        kind,
        initialized_with_null: false,
        span: source_span(span_source, span_base, inject.span),
      });
    }
  }
  seeds
}

fn join_errors(errors: &[impl ToString]) -> String {
  errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
}
