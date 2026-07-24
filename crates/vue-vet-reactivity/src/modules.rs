use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use oxc_allocator::Allocator;
use oxc_ast::{
  AstKind,
  ast::{
    BindingPattern, Declaration, ExportDefaultDeclarationKind, Expression,
    ImportDeclarationSpecifier, ObjectPropertyKind,
  },
};
use oxc_parser::Parser;
use oxc_semantic::{NodeId, SemanticBuilder};
use oxc_span::{SourceType, Span};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vue_vet_core::{ReactiveBindingFact, ReactiveBindingKind, ReactivityGraph, ScriptKind};

use super::{
  TraceSeeds, collect_binding_identifiers, collect_imported_bindings, module_export_name,
  reactive_binding_kind, reference_resolves_to_binding, resolved_vue_callee, source_span,
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

#[derive(Clone, Debug)]
struct ModuleSummary {
  module: ModuleSource,
  local_graph: ReactivityGraph,
  imports: Vec<ImportSummary>,
  exports: Vec<ExportSummary>,
  locals: BTreeMap<String, ExportState>,
  destructured_calls: Vec<DestructuredCallBinding>,
  instance_calls: Vec<InstanceCallBinding>,
}

/// Traces local and linked reactivity across a resolved module graph.
///
/// # Errors
///
/// Returns an error when a module cannot be parsed or analyzed, module identifiers
/// are duplicated, or a supplied resolved link is unknown or ambiguous.
pub fn trace_modules(
  modules: &[ModuleSource],
  links: &[ModuleLink],
) -> Result<Vec<ModuleReactivity>, TraceModulesError> {
  let mut summaries = BTreeMap::new();
  for module in modules {
    if summaries.contains_key(&module.id) {
      return Err(TraceModulesError::DuplicateModule(module.id.clone()));
    }
    summaries.insert(module.id.clone(), analyze_module(module, &TraceSeeds::default())?);
  }
  let resolved_links = resolved_links(&summaries, links)?;
  let exports = resolve_exports(&summaries, &resolved_links);

  let mut traced = Vec::with_capacity(summaries.len());
  for (id, summary) in &summaries {
    let seeds = imported_bindings(summary, &exports, &resolved_links);
    let analysis = analyze_module(&summary.module, &seeds)?;
    traced.push(ModuleReactivity { id: id.clone(), graph: analysis.local_graph });
  }
  Ok(traced)
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

fn analyze_module(
  module: &ModuleSource,
  seeds: &TraceSeeds,
) -> Result<ModuleSummary, TraceModulesError> {
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, &module.source, source_type(module)?).parse();
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
  let local_graph = trace_reactivity_seeded(
    &semantic,
    module.span_origin(),
    module.source_offset,
    module.kind,
    seeds,
  );
  let imports = collect_imports(&semantic);
  let exports = collect_exports(&semantic);
  let locals = collect_local_values(&semantic, &local_graph);
  let destructured_calls = collect_destructured_calls(&semantic, &imports);
  let instance_calls = collect_instance_calls(&semantic, &imports);
  Ok(ModuleSummary {
    module: module.clone(),
    local_graph,
    imports,
    exports,
    locals,
    destructured_calls,
    instance_calls,
  })
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
  graph: &ReactivityGraph,
) -> BTreeMap<String, ExportState> {
  let mut locals = graph
    .bindings
    .iter()
    .map(|binding| (binding.name.clone(), ExportState::Known(binding.kind)))
    .collect::<BTreeMap<_, _>>();

  for node in semantic.nodes() {
    let AstKind::Function(function) = node.kind() else {
      continue;
    };
    let Some(identifier) = &function.id else {
      continue;
    };
    let shape = composable_return_shape(semantic, function.node_id.get(), graph);
    if !shape.is_empty() {
      locals.insert(identifier.name.to_string(), ExportState::Composable(shape));
    }
  }
  locals
}

fn composable_return_shape(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  graph: &ReactivityGraph,
) -> BTreeMap<String, ReactiveBindingKind> {
  let imported_bindings = collect_imported_bindings(semantic);
  let param_names = function_param_names(semantic, function_id);
  let mut shape = BTreeMap::new();
  let mut ambiguous = BTreeSet::new();
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
    // `return toRefs(param)` — every static key is ToRef when the argument is a parameter.
    if let Some(Expression::CallExpression(call)) = &statement.argument
      && resolved_vue_callee(&call.callee, &imported_bindings, ScriptKind::Script)
        .is_some_and(|callee| callee == "toRefs")
      && call
        .arguments
        .first()
        .and_then(Argument::as_expression)
        .and_then(Expression::get_identifier_reference)
        .is_some_and(|identifier| param_names.contains(identifier.name.as_str()))
    {
      // Without an object shape we cannot invent keys; leave quiet.
      continue;
    }
    let Some(Expression::ObjectExpression(object)) = &statement.argument else {
      continue;
    };
    for property in &object.properties {
      let ObjectPropertyKind::ObjectProperty(property) = property else {
        continue;
      };
      let Some(exported) = property.key.static_name() else {
        continue;
      };
      let Some(kind) =
        reactive_return_kind(semantic, &property.value, graph, &imported_bindings, &param_names)
      else {
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
  shape
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
          && reference_resolves_to_binding(semantic, reference, binding, 0)
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
      AstKind::ExportDefaultDeclaration(declaration) => {
        if let ExportDefaultDeclarationKind::Identifier(identifier) = &declaration.declaration {
          exports.push(ExportSummary::Local {
            local: identifier.name.to_string(),
            exported: "default".into(),
          });
        }
      }
      AstKind::ExportAllDeclaration(declaration) if declaration.exported.is_none() => {
        exports.push(ExportSummary::Star { source: declaration.source.value.to_string() });
      }
      _ => {}
    }
  }
  exports
}

fn resolved_links(
  summaries: &BTreeMap<String, ModuleSummary>,
  links: &[ModuleLink],
) -> Result<BTreeMap<(String, String), String>, TraceModulesError> {
  let mut resolved = BTreeMap::new();
  for link in links {
    if !summaries.contains_key(&link.from) || !summaries.contains_key(&link.to) {
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
  summaries: &BTreeMap<String, ModuleSummary>,
  links: &BTreeMap<(String, String), String>,
) -> BTreeMap<String, BTreeMap<String, ExportState>> {
  let mut resolved =
    summaries.keys().map(|id| (id.clone(), BTreeMap::new())).collect::<BTreeMap<_, _>>();

  for (id, summary) in summaries {
    for export in &summary.exports {
      let ExportSummary::Local { local, exported } = export else {
        continue;
      };
      if let Some(state) = summary.locals.get(local) {
        insert_export(&mut resolved, id, exported, state.clone());
      }
    }
  }

  loop {
    let snapshot = resolved.clone();
    let mut changed = false;
    for (id, summary) in summaries {
      for export in &summary.exports {
        match export {
          ExportSummary::Local { .. } => {}
          ExportSummary::Reexport { source, imported, exported } => {
            let Some(target) = links.get(&(id.clone(), source.clone())) else {
              continue;
            };
            let Some(state) = snapshot.get(target).and_then(|exports| exports.get(imported)) else {
              continue;
            };
            changed |= insert_export(&mut resolved, id, exported, state.clone());
          }
          ExportSummary::Star { source } => {
            let Some(target) = links.get(&(id.clone(), source.clone())) else {
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

fn imported_bindings(
  summary: &ModuleSummary,
  exports: &BTreeMap<String, BTreeMap<String, ExportState>>,
  links: &BTreeMap<(String, String), String>,
) -> TraceSeeds {
  let mut seeds = TraceSeeds::default();
  for import in &summary.imports {
    if import.imported == "*" {
      continue;
    }
    let Some(target) = links.get(&(summary.module.id.clone(), import.source.clone())) else {
      continue;
    };
    let Some(state) = exports.get(target).and_then(|exports| exports.get(&import.imported)) else {
      continue;
    };
    // Seed spans must use the same origin/offset as module re-trace so
    // `reference_resolves_to_binding` matches SFC-absolute symbol offsets.
    let span_source = summary.module.span_origin();
    let span_base = summary.module.source_offset;
    match state {
      ExportState::Known(kind) => seeds.bindings.push(ReactiveBindingFact {
        name: import.local.clone(),
        kind: *kind,
        initialized_with_null: false,
        span: source_span(span_source, span_base, import.span),
      }),
      ExportState::Composable(shape) => {
        for call in
          summary.destructured_calls.iter().filter(|call| call.imported_local == import.local)
        {
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
        for call in summary.instance_calls.iter().filter(|call| call.imported_local == import.local)
        {
          // Only record the instance bag for `bag.field.value` resolution.
          // Do **not** inject shape fields as top-level bindings — that invents
          // edges for bare `field.value` when the consumer never destructured.
          seeds.composable_instances.insert(call.local.clone(), shape.clone());
        }
      }
      ExportState::Ambiguous => {}
    }
  }
  seeds
}

fn join_errors(errors: &[impl ToString]) -> String {
  errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
}
