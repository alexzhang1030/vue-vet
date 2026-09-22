//! Module semantic IR: phase-one summaries and cross-module linking.
//!
//! - [`model`] — `ModuleSummary` and the A6 `ExportState` data model.
//! - [`locals`] — per-module local `ExportState` classification (phase one).
//! - [`return_kind`] — composable / factory return-shape analysis over Oxc bodies.
//! - [`declared_types`] — TypeScript type surface → reactive kinds and bag shapes.
//! - [`export_lattice`] — pure lattice rules (no AST).
//! - [`resolve`] — cross-module fixed point, seed plans, caches, and phase two.

#[cfg(test)]
use std::cell::RefCell;
use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

use oxc_allocator::Allocator;
use oxc_ast::{
  AstKind,
  ast::{Declaration, ExportDefaultDeclarationKind, ImportDeclarationSpecifier},
};
use oxc_parser::Parser;
use oxc_semantic::{Semantic, SemanticBuilder};
use oxc_span::SourceType;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vue_vet_core::{ModuleId, ReactivityGraph, ScriptKind};

use super::bindings::collect_reactive_bindings;
#[cfg(test)]
use super::kinds::import_binding_collect_snapshot;
use super::kinds::{collect_binding_identifiers, collect_imported_bindings, module_export_name};
use super::{TraceSeeds, collect_inject_sites, collect_provide_sites, trace_reactivity_seeded};

mod declared_types;
mod export_lattice;
mod locals;
mod model;
mod options_callback;
mod resolve;
mod return_kind;
mod typed_callback;

pub use declared_types::{
  arrow_return_type_kind, arrow_return_type_shape, function_return_type_kind,
  function_return_type_shape,
};
pub(super) use declared_types::{composable_shape_from_ts_type, ts_type_reactive_kind};
use locals::collect_local_values;
pub use model::{
  ComposableReturn, ComposableShape, ModuleSummary, ValueBag, ValueBagEntry,
  merge_declaration_implementation_summary,
};
use model::{
  ExportState, ExportSummary, ImportSummary, ModuleExportFacts, NUXT_IMPORTS_RANGE_END,
  NUXT_IMPORTS_SPECIFIER_PREFIX,
};
pub use options_callback::{
  OptionsCallbackSlots, collect_local_options_callback_slots, seed_options_callback_params_at_calls,
};
pub use resolve::{
  ModuleTraceState, TraceModulesOptions, TraceModulesReport, TraceModulesStats, trace_modules,
  trace_modules_incremental_from_arcs, trace_modules_incremental_from_refs,
  trace_modules_incremental_with_options, trace_modules_with_options,
};
pub use return_kind::{
  build_returns_by_function, composable_return_with_index, static_member_call_path,
};
pub use typed_callback::{
  TypedCallbackParamSlots, collect_local_typed_callback_param_slots,
  seed_typed_callback_params_at_calls,
};

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
#[derive(Debug, Eq, Error, PartialEq)]
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
  let phase = analyze_module_phase_one(&module, &super::TraceConfig::empty())?;
  Ok(module.with_module_summary(phase.facts.summary))
}

/// Test-only count of the one summary-local `imported_bindings` index build.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SummaryScanWork {
  pub import_index_builds: u64,
  pub import_index_node_visits: u64,
}

#[cfg(test)]
thread_local! {
  static LAST_SUMMARY_SCAN_WORK: RefCell<SummaryScanWork> =
    const { RefCell::new(SummaryScanWork { import_index_builds: 0, import_index_node_visits: 0 }) };
}

#[cfg(test)]
pub fn last_summary_scan_work() -> SummaryScanWork {
  LAST_SUMMARY_SCAN_WORK.with(|slot| *slot.borrow())
}

#[cfg(test)]
fn store_summary_scan_work(work: SummaryScanWork) {
  LAST_SUMMARY_SCAN_WORK.with(|slot| *slot.borrow_mut() = work);
}

/// Facts a preceding trace already collected. Summary reuses them instead of
/// walking the same semantic again. Shape bindings are the nested-included
/// collection from before typed-ref and options-slot augmentation.
pub(super) struct SummaryReuse {
  pub(super) imported_bindings: BTreeMap<String, (String, String)>,
  pub(super) shape_bindings: Vec<vue_vet_core::ReactiveBindingFact>,
  pub(super) options_callback_slots: BTreeMap<String, OptionsCallbackSlots>,
  pub(super) typed_callback_param_slots: BTreeMap<String, TypedCallbackParamSlots>,
}

/// Prepare a module summary with an explicit plugin API-bag catalog.
pub fn prepare_module_summary_with_config(
  semantic: &Semantic<'_>,
  span_source: &str,
  source_offset: usize,
  kind: ScriptKind,
  local_graph: impl Into<Arc<ReactivityGraph>>,
  config: &super::TraceConfig<'_>,
) -> ModuleSummary {
  prepare_module_summary_inner(
    semantic,
    span_source,
    source_offset,
    kind,
    local_graph,
    config,
    None,
  )
}

pub(super) fn prepare_module_summary_reusing(
  semantic: &Semantic<'_>,
  span_source: &str,
  source_offset: usize,
  kind: ScriptKind,
  local_graph: impl Into<Arc<ReactivityGraph>>,
  config: &super::TraceConfig<'_>,
  reuse: SummaryReuse,
) -> ModuleSummary {
  prepare_module_summary_inner(
    semantic,
    span_source,
    source_offset,
    kind,
    local_graph,
    config,
    Some(reuse),
  )
}

fn prepare_module_summary_inner(
  semantic: &Semantic<'_>,
  span_source: &str,
  source_offset: usize,
  kind: ScriptKind,
  local_graph: impl Into<Arc<ReactivityGraph>>,
  config: &super::TraceConfig<'_>,
  reuse: Option<SummaryReuse>,
) -> ModuleSummary {
  let local_graph = local_graph.into();
  let imports = collect_imports(semantic);
  let exports = collect_exports(semantic);
  // One summary-local canonical import index; helpers and return analysis borrow it.
  // A trace that already built the index passes it in and this counter stays at zero.
  #[cfg(test)]
  let (builds_before, visits_before) = import_binding_collect_snapshot();
  let SummaryReuse {
    imported_bindings,
    shape_bindings,
    options_callback_slots,
    typed_callback_param_slots,
  } = reuse.unwrap_or_else(|| {
    let imported_bindings = collect_imported_bindings(semantic);
    let shape_bindings = collect_reactive_bindings(
      semantic,
      &imported_bindings,
      span_source,
      source_offset,
      kind,
      true,
      config.named_api_bags,
    )
    .bindings;
    SummaryReuse {
      imported_bindings,
      shape_bindings,
      options_callback_slots: collect_local_options_callback_slots(semantic),
      typed_callback_param_slots: collect_local_typed_callback_param_slots(semantic),
    }
  });
  let shape_graph = ReactivityGraph { bindings: shape_bindings, ..ReactivityGraph::default() };
  let locals = collect_local_values(
    semantic,
    &local_graph,
    &shape_graph,
    source_offset,
    span_source,
    &imported_bindings,
  );
  let provides = collect_provide_sites(
    semantic,
    &imported_bindings,
    &local_graph.bindings,
    &local_graph.composable_instances,
    &BTreeMap::new(),
    kind,
  );
  let injects = collect_inject_sites(semantic, &imported_bindings, &local_graph.bindings, kind);
  let called_locals = collect_called_locals(semantic);
  #[cfg(test)]
  {
    let (builds_after, visits_after) = import_binding_collect_snapshot();
    store_summary_scan_work(SummaryScanWork {
      import_index_builds: builds_after.saturating_sub(builds_before),
      import_index_node_visits: visits_after.saturating_sub(visits_before),
    });
  }
  ModuleSummary {
    imports,
    exports,
    locals,
    options_callback_slots,
    typed_callback_param_slots,
    provides,
    injects,
    called_locals,
    local_graph,
  }
}

struct ModulePhaseOne {
  facts: ModuleExportFacts,
  local_graph: Arc<ReactivityGraph>,
}

fn analyze_module_phase_one_cached(
  module: &ModuleSource,
  cached: Option<(&ModuleSource, &Arc<ModuleSummary>)>,
  config: &super::TraceConfig<'_>,
) -> Result<ModulePhaseOne, TraceModulesError> {
  if let Some(summary) = &module.module_summary {
    return Ok(phase_one_from_summary(module, summary));
  }
  if let Some((source, summary)) = cached
    && source == module
  {
    return Ok(phase_one_from_summary(module, summary));
  }
  analyze_module_phase_one(module, config)
}

fn analyze_module_phase_one(
  module: &ModuleSource,
  config: &super::TraceConfig<'_>,
) -> Result<ModulePhaseOne, TraceModulesError> {
  if let Some(summary) = &module.module_summary {
    return Ok(phase_one_from_summary(module, summary));
  }

  with_module_semantic(module, |semantic| {
    let empty = TraceSeeds::default();
    let local_graph = Arc::new(trace_reactivity_seeded(
      semantic,
      module.span_origin(),
      module.source_offset,
      module.kind,
      &empty,
      config,
    ));
    let summary = Arc::new(prepare_module_summary_with_config(
      semantic,
      module.span_origin(),
      module.source_offset,
      module.kind,
      Arc::clone(&local_graph),
      config,
    ));
    Ok(phase_one_from_summary(module, &summary))
  })
}

fn with_module_semantic<T>(
  module: &ModuleSource,
  body: impl FnOnce(&Semantic<'_>) -> Result<T, TraceModulesError>,
) -> Result<T, TraceModulesError> {
  let allocator = Allocator::default();
  let source_type = source_type(module)?;
  let parsed = Parser::new(&allocator, module.source.as_ref(), source_type).parse();
  if !parsed.diagnostics.is_empty() {
    return Err(TraceModulesError::Parse {
      module: module.id.clone(),
      message: join_errors(parsed.diagnostics.as_slice()),
    });
  }
  let built = SemanticBuilder::new()
    .with_build_nodes(true)
    .with_check_syntax_error(true)
    .build(&parsed.program);
  if !built.diagnostics.is_empty() {
    return Err(TraceModulesError::Semantic {
      module: module.id.clone(),
      message: join_errors(built.diagnostics.as_slice()),
    });
  }
  body(&built.semantic)
}

fn phase_one_from_summary(module: &ModuleSource, summary: &Arc<ModuleSummary>) -> ModulePhaseOne {
  ModulePhaseOne {
    facts: ModuleExportFacts { id: module.id.clone(), summary: Arc::clone(summary) },
    local_graph: Arc::clone(&summary.local_graph),
  }
}

fn source_type(module: &ModuleSource) -> Result<SourceType, TraceModulesError> {
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

/// Identifier callees in this file. Same `get_identifier_reference` gate as
/// seed materialize — a conservative superset (every call, not only declarator
/// inits) so a skip never drops a seed that materialize would apply.
fn collect_called_locals(semantic: &Semantic<'_>) -> BTreeSet<String> {
  let mut names = BTreeSet::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = call.callee.get_identifier_reference() else {
      continue;
    };
    names.insert(callee.name.to_string());
  }
  names
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

fn join_errors(errors: &[impl ToString]) -> String {
  errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
}
