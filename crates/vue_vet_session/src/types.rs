//! Session-facing snapshot, options, and error types.
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use thiserror::Error;
use vue_vet_core::{
  CacheRejection, EvidenceGap, EvidenceGapCode, EvidenceSummary, FileId, RuleGroupId, ScanSummary,
};
use vue_vet_project::ProjectGraph;

use crate::locality::ScanWorkCounters;

/// Options for opening a [`ProjectSession`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionOptions {
  /// File or directory to analyze (same semantics as the CLI path argument).
  pub root: PathBuf,
  /// Explicit `vue-vet.toml`, or discover next to the scan directory.
  pub config_path: Option<PathBuf>,
  /// Override content-addressed cache directory.
  pub cache_dir: Option<PathBuf>,
  /// Skip the content-addressed cache (also used by fix modes).
  pub no_cache: bool,
  /// Analysis worker threads; `None` uses Rayon defaults.
  pub threads: Option<usize>,
  /// Canonical groups that narrow the effective rule set. Empty means no filter.
  pub selected_groups: Vec<RuleGroupId>,
}

/// Deterministic analysis result shared across surfaces.
///
/// Every heavy field is `Arc`. `Clone` / noop / product publish only bumps
/// refcounts — never deep-copies diagnostics, graphs, coverage, or path lists.
#[derive(Clone, Debug)]
pub struct AnalysisSnapshot {
  pub summary: Arc<ScanSummary>,
  pub graph: Arc<ProjectGraph>,
  pub cache_status: &'static str,
  /// Structured cache miss reason for machine consumers.
  pub cache_rejection: Option<CacheRejection>,
  pub coverage: Arc<AnalysisCoverage>,
  pub issues: Arc<[AnalysisIssue]>,
  /// Typed coverage contract shared by CLI, LSP, MCP, and reporters.
  pub evidence: EvidenceSummary,
  /// Normalized `/`-separated paths matching JSON `project.analyzed_files`.
  pub analyzed_files: Arc<[String]>,
  /// Real work performed by the scan that produced this snapshot.
  pub work: ScanWorkCounters,
}

impl AnalysisSnapshot {
  #[must_use]
  pub const fn complete(&self) -> bool {
    self.evidence.is_complete()
  }
}

/// Source coverage is distinct from non-source inputs that invalidate a graph.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AnalysisCoverage {
  pub analyzed_source_files: Vec<FileId>,
  pub invalidation_inputs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisStage {
  SfcParse,
  ScriptParse,
  ModuleTracing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Recoverability {
  File,
  Module,
  Fatal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalysisIssue {
  pub stage: AnalysisStage,
  pub file: Option<FileId>,
  pub message: String,
  pub recoverability: Recoverability,
}

/// Overlay mutations applied to a long-lived session before affected analysis.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChangeSet {
  pub files: BTreeMap<PathBuf, Option<String>>,
}

impl ChangeSet {
  #[must_use]
  pub fn upsert(path: PathBuf, source: String) -> Self {
    Self { files: BTreeMap::from([(path, Some(source))]) }
  }

  #[must_use]
  pub fn remove(path: PathBuf) -> Self {
    Self { files: BTreeMap::from([(path, None)]) }
  }
}

/// Errors from session open, analyze, explain, or path resolution.
#[derive(Debug, Error)]
pub enum SessionError {
  #[error("analysis was superseded by a newer workspace revision")]
  Cancelled,
  #[error("{0}")]
  Message(String),
}

/// Convert recoverable pipeline issues and graph tracing issues into the
/// stable evidence contract. Counts are merged and sorted by `EvidenceSummary`.
#[must_use]
pub fn evidence_summary(graph: &ProjectGraph, issues: &[AnalysisIssue]) -> EvidenceSummary {
  let mut gaps = Vec::new();
  let mut unavailable = false;
  let mut module_issues: usize = 0;
  for issue in issues {
    match issue.stage {
      AnalysisStage::SfcParse | AnalysisStage::ScriptParse => {
        gaps.push(EvidenceGap { code: EvidenceGapCode::Parse, count: 1 });
      }
      AnalysisStage::ModuleTracing => module_issues += 1,
    }
    unavailable |= matches!(issue.recoverability, Recoverability::Fatal);
  }
  // The pipeline projects graph issues into AnalysisIssue. Count that family
  // once; the graph also supplies coverage when loading a persisted result.
  module_issues = module_issues
    .max(graph.reactivity_issues.len())
    .max(usize::from(graph.reactivity_error.is_some()));
  if module_issues > 0 {
    gaps.push(EvidenceGap { code: EvidenceGapCode::ModuleReactivity, count: module_issues });
  }
  if unavailable {
    EvidenceSummary::unavailable(gaps)
  } else if gaps.is_empty() {
    EvidenceSummary::complete()
  } else {
    EvidenceSummary::partial(gaps)
  }
}

impl SessionError {
  #[must_use]
  pub fn message(message: impl Into<String>) -> Self {
    Self::Message(message.into())
  }

  #[must_use]
  pub const fn is_cancelled(&self) -> bool {
    matches!(self, Self::Cancelled)
  }
}

impl From<String> for SessionError {
  fn from(message: String) -> Self {
    Self::Message(message)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use vue_vet_core::EvidenceStatus;

  #[test]
  fn recoverable_issue_produces_partial_evidence() {
    let graph = ProjectGraph::default();
    let issues = [AnalysisIssue {
      stage: AnalysisStage::ScriptParse,
      file: Some(FileId::from("App.vue")),
      message: "script parse failed".into(),
      recoverability: Recoverability::File,
    }];
    let evidence = evidence_summary(&graph, &issues);
    assert_eq!(evidence.status, EvidenceStatus::Partial);
    assert_eq!(evidence.gaps.first().map(|gap| gap.code), Some(EvidenceGapCode::Parse));
  }

  #[test]
  fn fatal_issue_produces_unavailable_evidence() {
    let graph = ProjectGraph::default();
    let issues = [AnalysisIssue {
      stage: AnalysisStage::SfcParse,
      file: None,
      message: "workspace unavailable".into(),
      recoverability: Recoverability::Fatal,
    }];
    let evidence = evidence_summary(&graph, &issues);
    assert_eq!(evidence.status, EvidenceStatus::Unavailable);
  }

  #[test]
  fn graph_reactivity_issue_is_partial_evidence() {
    let graph = ProjectGraph {
      reactivity_issues: vec![vue_vet_project::ReactivityIssue {
        module: None,
        message: "module link failed".into(),
      }],
      ..ProjectGraph::default()
    };
    let evidence = evidence_summary(&graph, &[]);
    assert_eq!(evidence.status, EvidenceStatus::Partial);
    assert_eq!(evidence.gaps.first().map(|gap| gap.code), Some(EvidenceGapCode::ModuleReactivity));
  }

  #[test]
  fn graph_and_session_projections_count_the_same_issue_once() {
    let graph = ProjectGraph {
      reactivity_issues: vec![vue_vet_project::ReactivityIssue {
        module: Some("App.vue".into()),
        message: "module link failed".into(),
      }],
      reactivity_error: Some("module link failed".into()),
      ..ProjectGraph::default()
    };
    let issue = AnalysisIssue {
      stage: AnalysisStage::ModuleTracing,
      file: Some(FileId::from("App.vue")),
      message: "module link failed".into(),
      recoverability: Recoverability::Module,
    };
    assert_eq!(evidence_summary(&graph, &[issue]), evidence_summary(&graph, &[]));
    assert_eq!(
      evidence_summary(&graph, &[]).gaps,
      [EvidenceGap { code: EvidenceGapCode::ModuleReactivity, count: 1 }]
    );
  }
}
