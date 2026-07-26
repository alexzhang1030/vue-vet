//! Rule documentation lookup for `--explain` and future LSP / agent surfaces.
//!
//! Resolves Vue Vet-owned [`RuleMeta`] documentation keys to repository-local
//! Markdown paths and optional file bodies. Callers supply the metadata table
//! (built-ins + project rules); this module does not depend on `vue_vet_rules`.

use std::{
  fs,
  path::{Path, PathBuf},
};

use serde::Serialize;
use vue_vet_core::{Confidence, RuleMeta, Severity};

/// Machine-readable `--explain` payload (early-exit; not the scan report).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuleExplain {
  pub rule_id: String,
  pub category: String,
  pub severity: Severity,
  pub confidence: Confidence,
  /// Repository-relative documentation path (`docs/rules/...md`).
  pub documentation: String,
  /// Markdown body when the file was found and readable.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub body: Option<String>,
  /// Absolute or relative path that supplied `body`, when known.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub body_path: Option<String>,
  /// Why `body` is missing when the rule is known but docs are unavailable.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub body_error: Option<String>,
}

/// Map a [`RuleMeta::documentation`] key to the JSON/report path form.
#[must_use]
pub fn documentation_path(documentation: &str) -> String {
  format!("docs/{documentation}.md")
}

/// Build an explain payload from rule metadata, loading docs from `search_roots`.
#[must_use]
pub fn explain_rule(meta: &RuleMeta, search_roots: &[PathBuf]) -> RuleExplain {
  let documentation = documentation_path(meta.documentation);
  let (body, body_path, body_error) =
    match load_documentation_body(meta.documentation, search_roots) {
      Ok((path, text)) => (Some(text), Some(path.display().to_string().replace('\\', "/")), None),
      Err(error) => (None, None, Some(error)),
    };
  RuleExplain {
    rule_id: meta.id.into(),
    category: meta.category.into(),
    severity: meta.default_severity,
    confidence: meta.confidence,
    documentation,
    body,
    body_path,
    body_error,
  }
}

/// Find `meta` by exact `rule_id` in a caller-supplied table.
#[must_use]
pub fn find_rule_meta<'a>(rule_id: &str, metas: &[&'a RuleMeta]) -> Option<&'a RuleMeta> {
  metas.iter().copied().find(|meta| meta.id == rule_id)
}

/// Render a human-readable explain report.
#[must_use]
pub fn render_rule_explain_text(explain: &RuleExplain) -> String {
  let mut output = String::new();
  output.push_str(&explain.rule_id);
  output.push('\n');
  output.push_str("category: ");
  output.push_str(&explain.category);
  output.push('\n');
  output.push_str("severity: ");
  output.push_str(severity_label(explain.severity));
  output.push('\n');
  output.push_str("confidence: ");
  output.push_str(confidence_label(explain.confidence));
  output.push('\n');
  output.push_str("documentation: ");
  output.push_str(&explain.documentation);
  output.push('\n');
  if let Some(body) = &explain.body {
    output.push('\n');
    output.push_str(body);
    if !body.ends_with('\n') {
      output.push('\n');
    }
  } else if let Some(error) = &explain.body_error {
    output.push('\n');
    output.push_str("documentation body unavailable: ");
    output.push_str(error);
    output.push('\n');
  }
  output
}

/// Serialize explain JSON (pretty).
///
/// # Errors
///
/// Returns a serialization error when the payload cannot be encoded.
pub fn render_rule_explain_json(explain: &RuleExplain) -> Result<String, serde_json::Error> {
  serde_json::to_string_pretty(explain)
}

fn load_documentation_body(
  documentation_key: &str,
  search_roots: &[PathBuf],
) -> Result<(PathBuf, String), String> {
  let relative = documentation_path(documentation_key);
  let mut tried = Vec::new();
  for root in search_roots {
    for candidate in documentation_candidates(root, &relative) {
      tried.push(candidate.display().to_string());
      match fs::read_to_string(&candidate) {
        Ok(text) => return Ok((candidate, text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
          return Err(format!("failed to read {}: {error}", candidate.display()));
        }
      }
    }
  }
  Err(format!(
    "could not find {relative} under {}; install from source or clone the Vue Vet repository to read full rule docs",
    if tried.is_empty() { "the search roots".into() } else { tried.join(", ") }
  ))
}

fn documentation_candidates(root: &Path, relative_docs_path: &str) -> Vec<PathBuf> {
  let mut candidates = Vec::new();
  // Direct: <root>/docs/...
  candidates.push(root.join(relative_docs_path));
  // Walk ancestors so scanning a nested path still finds the repo docs/.
  let mut current = root.to_path_buf();
  for _ in 0..8 {
    let parent = current.parent().map(Path::to_path_buf);
    let Some(parent) = parent else {
      break;
    };
    if parent == current {
      break;
    }
    candidates.push(parent.join(relative_docs_path));
    current = parent;
  }
  candidates
}

const fn severity_label(severity: Severity) -> &'static str {
  match severity {
    Severity::Info => "info",
    Severity::Warning => "warning",
    Severity::Error => "error",
  }
}

const fn confidence_label(confidence: Confidence) -> &'static str {
  match confidence {
    Confidence::High => "high",
    Confidence::Medium => "medium",
    Confidence::Low => "low",
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use vue_vet_core::{Confidence, Severity};

  static SAMPLE: RuleMeta = RuleMeta {
    id: "vue-vet/security/no-v-html",
    category: "security",
    default_severity: Severity::Warning,
    confidence: Confidence::High,
    documentation: "rules/security/no-v-html",
  };

  #[test]
  fn documentation_path_matches_json_report_shape() {
    assert_eq!(documentation_path("rules/security/no-v-html"), "docs/rules/security/no-v-html.md");
    assert_eq!(documentation_path("project-graph"), "docs/project-graph.md");
  }

  #[test]
  fn explain_loads_body_from_workspace_docs() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let explain = explain_rule(&SAMPLE, &[root]);
    assert_eq!(explain.rule_id, SAMPLE.id);
    assert_eq!(explain.documentation, "docs/rules/security/no-v-html.md");
    assert!(
      explain.body.as_deref().is_some_and(|body| body.contains("v-html")),
      "expected rule markdown body; error={:?}",
      explain.body_error
    );
    let text = render_rule_explain_text(&explain);
    assert!(text.contains("category: security"));
    assert!(text.contains("## Bad"));
  }

  #[test]
  fn find_rule_meta_matches_exact_id() {
    assert!(find_rule_meta(SAMPLE.id, &[&SAMPLE]).is_some());
    assert!(find_rule_meta("vue-vet/missing", &[&SAMPLE]).is_none());
  }
}
