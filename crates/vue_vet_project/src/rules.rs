//! Project-level diagnostics emitted while building the graph.

use std::{
  collections::{BTreeMap, HashSet},
  path::Path,
};

use vue_vet_core::{Confidence, Diagnostic, Severity, SourceSpan};

use crate::model::{EdgeKind, GraphEdge, GraphNode, NodeKind, PROJECT_RULE_IDS, ProjectFile};
use crate::resolve::normalized_path;

#[must_use]
pub fn unresolved_diagnostic(file: &Path, specifier: &str, span: SourceSpan) -> Diagnostic {
  Diagnostic {
    rule_id: PROJECT_RULE_IDS[0].into(),
    category: "project".into(),
    severity: Severity::Error,
    confidence: Some(Confidence::High),
    documentation: Some("project-graph".into()),
    message: format!("cannot resolve project import `{specifier}`"),
    help: Some(
      "Check that the import resolves under Node/Vite rules: a relative path, tsconfig paths / @ or ~ aliases, or an installed package."
        .into(),
    ),
    file: file.into(),
    span,
    edits: Vec::new(),
    recommendation: None,
  }
}

#[must_use]
pub fn unused_component_diagnostics(
  files: &[&ProjectFile],
  nodes: &[GraphNode],
  edges: &[GraphEdge],
) -> Vec<Diagnostic> {
  let referenced = edges
    .iter()
    .filter(|edge| {
      matches!(edge.kind, EdgeKind::Import | EdgeKind::ComponentUsage | EdgeKind::AutoComponent)
    })
    .map(|edge| edge.to.as_str())
    .collect::<HashSet<_>>();
  let file_by_path = files
    .iter()
    .map(|file| (normalized_path(file.path.as_path()), *file))
    .collect::<BTreeMap<_, _>>();
  nodes
    .iter()
    .filter(|node| node.kind == NodeKind::Component)
    .filter(|node| !referenced.contains(node.id.as_str()))
    .filter_map(|node| {
      let file = file_by_path.get(&node.path)?;
      Some(Diagnostic {
        rule_id: PROJECT_RULE_IDS[1].into(),
        category: "project".into(),
        severity: Severity::Warning,
        confidence: Some(Confidence::Medium),
        documentation: Some("project-graph".into()),
        message: format!("component `{}` is never referenced", node.name),
        help: Some("Remove it or reference it from a template or script import.".into()),
        file: file.path.clone(),
        span: SourceSpan { offset: 0, length: file.source_len.min(1), line: 1, column: 1 },
        edits: Vec::new(),
        recommendation: None,
      })
    })
    .collect()
}
