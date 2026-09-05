//! Per-file parse / rule execution used by the scan orchestrator.
use std::{path::Path, sync::Arc};

use vue_vet_core::{
  Diagnostic, FileId, ModuleId, ReactivityGraph, RuleEnvironment, ScriptFacts, Severity, SfcFacts,
  SourceSpan,
};
use vue_vet_oxc::analyze_module_source;
use vue_vet_project::ProjectFile;
use vue_vet_reactivity::ModuleSource;
use vue_vet_vize::{AnalyzeError, AnalyzedSfc, analyze_sfc_facts_reusing};

use crate::{
  AnalysisIssue, AnalysisStage, Recoverability,
  discovery::{SourceInput, SourceKind},
  package_index::PackageIndex,
  registry::file_analysis_registry,
};

#[derive(Debug)]
pub enum AnalyzedCandidate {
  Vue {
    project_file: Arc<ProjectFile>,
    pending: Arc<PendingVueFile>,
    /// Retained for SFC block-level reuse on the next edit.
    sfc: Arc<AnalyzedSfc>,
  },
  Script {
    project_file: Arc<ProjectFile>,
  },
}

pub fn analyze_candidate(
  input: &SourceInput,
  environment: Option<RuleEnvironment>,
  previous_sfc: Option<&AnalyzedSfc>,
) -> Result<AnalyzedCandidate, AnalysisIssue> {
  match &input.kind {
    SourceKind::Vue => {
      let environment = environment.unwrap_or_default();
      let analysis =
        analyze_sfc_facts_reusing(input.file_id.as_path(), &input.source, previous_sfc).map_err(
          |error| AnalysisIssue {
            stage: match &error {
              AnalyzeError::Parse(_) | AnalyzeError::Template(_) => AnalysisStage::SfcParse,
              AnalyzeError::Script(_) => AnalysisStage::ScriptParse,
            },
            file: Some(input.file_id.clone()),
            message: format!("failed to analyze {}: {error}", input.physical_path.display()),
            recoverability: Recoverability::File,
          },
        )?;
      let sfc = Arc::new(analysis);
      let facts = Arc::new(sfc.facts.clone());
      let project_file = Arc::new(ProjectFile {
        path: input.file_id.clone(),
        source_len: input.source.len(),
        facts: Arc::clone(&facts),
        module_source: sfc.module_source.clone().map(|mut module| {
          module.id = ModuleId::primary(&input.file_id);
          Arc::new(module)
        }),
        ordinary_module_source: sfc.ordinary_module_source.clone().map(|mut module| {
          module.id = ModuleId::ordinary(&input.file_id);
          Arc::new(module)
        }),
      });
      Ok(AnalyzedCandidate::Vue {
        project_file,
        pending: Arc::new(PendingVueFile {
          file_id: input.file_id.clone(),
          source: Arc::clone(&input.source),
          environment,
          facts,
          include_ordinary_graph: true,
        }),
        sfc,
      })
    }
    SourceKind::Script { language } => {
      let analysis = analyze_module_source(
        &input.source,
        &input.source,
        0,
        language,
        vue_vet_core::ScriptKind::Script,
      )
      .map_err(|error| AnalysisIssue {
        stage: AnalysisStage::ScriptParse,
        file: Some(input.file_id.clone()),
        message: format!("failed to analyze {}: {error}", input.physical_path.display()),
        recoverability: Recoverability::File,
      })?;
      Ok(AnalyzedCandidate::Script {
        project_file: Arc::new(ProjectFile {
          path: input.file_id.clone(),
          source_len: input.source.len(),
          facts: Arc::new(SfcFacts {
            template: analysis.template_facts,
            script: ScriptFacts { blocks: vec![analysis.script_facts] },
          }),
          module_source: Some(Arc::new(
            ModuleSource::standalone(
              ModuleId::primary(&input.file_id),
              Arc::clone(&input.source),
              language.clone(),
              vue_vet_core::ScriptKind::Script,
            )
            .with_module_summary(analysis.module_trace),
          )),
          ordinary_module_source: None,
        }),
      })
    }
  }
}

#[derive(Debug)]
pub struct PendingVueFile {
  pub file_id: FileId,
  pub source: Arc<str>,
  pub environment: RuleEnvironment,
  pub facts: Arc<SfcFacts>,
  /// Include the companion ordinary `<script>` module for Vue SFCs.
  pub include_ordinary_graph: bool,
}

pub fn run_file_rules(
  pending: &PendingVueFile,
  primary_graph: Option<Arc<ReactivityGraph>>,
  ordinary_graph: Option<Arc<ReactivityGraph>>,
) -> Vec<Diagnostic> {
  let mut facts = (*pending.facts).clone();
  if let Some(graph) = primary_graph {
    facts.apply_module_reactivity(graph);
  }
  if let Some(graph) = ordinary_graph {
    facts.apply_module_reactivity_for(vue_vet_core::ScriptKind::Script, graph);
  }
  file_analysis_registry().run_with_environment(
    pending.file_id.as_path(),
    &pending.source,
    &facts.template,
    &facts.script,
    pending.environment.clone(),
  )
}

/// Whether file rules should run on a script after module graphs are applied.
///
/// Vue SFCs always participate (queued before this check). JSX/TSX always
/// participate. Plain JS/TS participate when local or seeded facts exist
/// (scopes, bindings, member writes, operands, calls). Cross-file seeds are
/// visible only on the applied primary graph. Call-only practice rules
/// (`prefer-to-value`, lifecycle listeners) need `block.calls`.
#[must_use]
pub fn needs_file_rules(
  language: &str,
  facts: &SfcFacts,
  primary_graph: Option<&ReactivityGraph>,
) -> bool {
  if !matches!(language, "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts") {
    return false;
  }
  if matches!(language, "jsx" | "tsx") {
    return true;
  }
  script_has_rule_facts(facts, primary_graph)
}

fn script_has_rule_facts(facts: &SfcFacts, primary_graph: Option<&ReactivityGraph>) -> bool {
  if !facts.template.elements.is_empty() || !facts.template.expressions.is_empty() {
    return true;
  }
  if primary_graph.is_some_and(graph_has_rule_facts) {
    return true;
  }
  facts.script.blocks.iter().any(|block| {
    graph_has_rule_facts(&block.reactivity_graph)
      || !block.member_writes.is_empty()
      || !block.operands.is_empty()
      || !block.destructures.is_empty()
      || !block.calls.is_empty()
  })
}

const fn graph_has_rule_facts(graph: &ReactivityGraph) -> bool {
  !graph.scopes.is_empty()
    || !graph.bindings.is_empty()
    || !graph.effects.is_empty()
    || !graph.edges.is_empty()
    || !graph.template_reads.is_empty()
}

pub fn source_environment(
  input: &SourceInput,
  boundary: &Path,
  package_index: &PackageIndex,
) -> Option<RuleEnvironment> {
  crate::locality::file_rule_source_kind(&input.kind)
    .then(|| package_index.environment_for(input.physical_path.as_path(), boundary))
}

pub fn issue_diagnostic(issue: &AnalysisIssue) -> Option<Diagnostic> {
  let file = issue.file.clone()?;
  let (rule_id, help) = match issue.stage {
    AnalysisStage::ModuleTracing => (
      "vue-vet/analysis/module-tracing",
      "Fix the module or its resolved import edge; other healthy module links were retained.",
    ),
    AnalysisStage::SfcParse | AnalysisStage::ScriptParse => (
      "vue-vet/analysis/parse-error",
      "Fix the syntax error; analysis continued for the rest of the workspace.",
    ),
  };
  Some(Diagnostic {
    rule_id: rule_id.into(),
    category: "analysis".into(),
    severity: Severity::Error,
    confidence: None,
    documentation: None,
    message: issue.message.clone(),
    help: Some(help.into()),
    file,
    span: SourceSpan { offset: 0, length: 0, line: 1, column: 1 },
    edits: Vec::new(),
    recommendation: None,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use vue_vet_core::{
    ScriptBlockFacts, ScriptCallFact, ScriptFacts, ScriptKind, SourceSpan, TemplateFacts,
  };

  fn span() -> SourceSpan {
    SourceSpan { offset: 0, length: 1, line: 1, column: 1 }
  }

  fn block() -> ScriptBlockFacts {
    ScriptBlockFacts {
      kind: ScriptKind::Script,
      language: "ts".into(),
      imports: Vec::new(),
      bindings: Vec::new(),
      calls: Vec::new(),
      member_writes: Vec::new(),
      destructures: Vec::new(),
      top_level_await_ends: Vec::new(),
      operands: Vec::new(),
      reactivity_graph: std::sync::Arc::new(ReactivityGraph::default()),
    }
  }

  #[test]
  fn call_only_script_needs_file_rules() {
    let mut block = block();
    block.calls.push(ScriptCallFact {
      callee: "unref".into(),
      assigned_to: None,
      resolved_import: Some(("vue".into(), "unref".into())),
      argument_identifiers: vec!["x".into()],
      span: span(),
    });
    let facts =
      SfcFacts { template: TemplateFacts::default(), script: ScriptFacts { blocks: vec![block] } };
    assert!(needs_file_rules("ts", &facts, None));
  }

  #[test]
  fn empty_script_skips_file_rules() {
    let facts = SfcFacts {
      template: TemplateFacts::default(),
      script: ScriptFacts { blocks: vec![block()] },
    };
    assert!(!needs_file_rules("ts", &facts, None));
    assert!(needs_file_rules("tsx", &facts, None));
    assert!(needs_file_rules("jsx", &facts, None));
  }

  #[test]
  fn seeded_graph_bindings_need_file_rules() {
    let facts = SfcFacts {
      template: TemplateFacts::default(),
      script: ScriptFacts { blocks: vec![block()] },
    };
    let mut graph = ReactivityGraph::default();
    graph.bindings.push(vue_vet_core::ReactiveBindingFact {
      name: "state".into(),
      kind: vue_vet_core::ReactiveBindingKind::Readonly,
      initialized_with_null: false,
      alias_of: None,
      span: span(),
    });
    assert!(needs_file_rules("ts", &facts, Some(&graph)));
    assert!(!needs_file_rules("ts", &facts, None));
  }
}
