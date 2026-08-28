//! Oxc-powered JavaScript / TypeScript semantic facts for Vue Vet.
//!
//! One parse yields [`ScriptBlockFacts`], optional JSX [`TemplateFacts`], and a
//! [`ModuleSummary`]. Product scans auto-load
//! [`vue_vet_plugins::default_trace_config`]. Oxc arena values must not leave
//! this crate.

use std::sync::Arc;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use thiserror::Error;
use vue_vet_core::{ScriptBlockFacts, ScriptKind, TemplateFacts};
use vue_vet_plugins::default_trace_config;
use vue_vet_reactivity::{
  ModuleSummary, prepare_module_summary_with_config, trace_reactivity_with_config,
};

mod facts;
mod jsx;
mod template_expr;

pub(crate) use facts::source_span;
pub use template_expr::{
  slot_prop_alias_identifiers, template_expression_identifiers,
  template_expression_identifiers_with_shadow, v_for_alias_identifiers,
};

#[derive(Debug, Error)]
pub enum AnalyzeScriptError {
  #[error("Oxc could not parse the script: {0}")]
  Parse(String),
  #[error("Oxc could not build script semantics: {0}")]
  Semantic(String),
  #[error("unsupported script language `{0}`")]
  UnsupportedLanguage(String),
}

/// Facts produced from one Oxc parse for both file rules and module linking.
#[derive(Debug)]
pub struct ModuleAnalysis {
  pub script_facts: ScriptBlockFacts,
  /// JSX/TSX lowered into template facts (empty when the script has no JSX).
  pub template_facts: TemplateFacts,
  pub module_trace: Arc<ModuleSummary>,
}

/// Analyze one extracted Vue SFC script block and map all facts to original
/// SFC byte offsets.
///
/// # Errors
///
/// Returns a deterministic parser or semantic error for invalid scripts, and
/// rejects script languages outside JavaScript, TypeScript, JSX, and TSX.
pub fn analyze_script(
  sfc_source: &str,
  script_source: &str,
  script_offset: usize,
  language: &str,
  kind: ScriptKind,
) -> Result<ScriptBlockFacts, AnalyzeScriptError> {
  analyze_module_source(sfc_source, script_source, script_offset, language, kind)
    .map(|analysis| analysis.script_facts)
}

/// Analyze one script surface once for file facts and cross-module linking.
///
/// # Errors
///
/// Returns a deterministic parser or semantic error for invalid scripts, and
/// rejects script languages outside JavaScript, TypeScript, JSX, and TSX.
pub fn analyze_module_source(
  sfc_source: &str,
  script_source: &str,
  script_offset: usize,
  language: &str,
  kind: ScriptKind,
) -> Result<ModuleAnalysis, AnalyzeScriptError> {
  let source_type = source_type(language)?;
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, script_source, source_type).parse();
  if !parsed.diagnostics.is_empty() {
    return Err(AnalyzeScriptError::Parse(join_errors(parsed.diagnostics.as_slice())));
  }

  let built = SemanticBuilder::new()
    .with_build_nodes(true)
    .with_check_syntax_error(true)
    .build(&parsed.program);
  if !built.diagnostics.is_empty() {
    return Err(AnalyzeScriptError::Semantic(join_errors(built.diagnostics.as_slice())));
  }
  let semantic = built.semantic;
  let line_index = vue_vet_core::LineIndex::new(sfc_source);
  let (imports, imported_bindings) =
    facts::collect_import_facts(&semantic, &line_index, sfc_source, script_offset);
  let bindings = facts::collect_binding_facts(&semantic, &line_index, sfc_source, script_offset);
  let node_facts = facts::collect_node_facts(
    &semantic,
    &imported_bindings,
    &line_index,
    sfc_source,
    script_offset,
  )
  .into_source_order();
  // Plain JS/TS has no JSX nodes; skip the AST walk on the CodSpeed hot path.
  let template_facts = if matches!(language, "jsx" | "tsx") {
    jsx::collect_jsx_template_facts(&semantic, &line_index, sfc_source, script_offset)
  } else {
    TemplateFacts::default()
  };

  // Auto-load ecosystem plugins (Nuxt / vue-i18n) at the analysis boundary.
  let trace_config = default_trace_config();
  let reactivity_graph = Arc::new(trace_reactivity_with_config(
    &semantic,
    sfc_source,
    script_offset,
    kind,
    &trace_config,
  ));
  let module_trace = Arc::new(prepare_module_summary_with_config(
    &semantic,
    sfc_source,
    script_offset,
    kind,
    Arc::clone(&reactivity_graph),
    &trace_config,
  ));

  Ok(ModuleAnalysis {
    script_facts: ScriptBlockFacts {
      kind,
      language: language.into(),
      imports,
      bindings,
      calls: node_facts.calls,
      member_writes: node_facts.member_writes,
      destructures: node_facts.destructures,
      top_level_await_ends: node_facts.top_level_await_ends,
      operands: node_facts.operands,
      reactivity_graph,
    },
    template_facts,
    module_trace,
  })
}

fn source_type(language: &str) -> Result<SourceType, AnalyzeScriptError> {
  match language {
    "js" | "javascript" => Ok(SourceType::mjs()),
    "jsx" => Ok(SourceType::jsx()),
    "ts" | "typescript" => Ok(SourceType::ts()),
    "tsx" => Ok(SourceType::tsx()),
    other => Err(AnalyzeScriptError::UnsupportedLanguage(other.into())),
  }
}

fn join_errors(errors: &[impl ToString]) -> String {
  errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
}

#[cfg(test)]
mod tests;
