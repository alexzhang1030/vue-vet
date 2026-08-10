//! Rule and finding documentation lookup for `--explain` and future LSP / agent surfaces.
//!
//! Resolves Vue Vet-owned [`RuleMeta`] documentation keys to repository-local
//! Markdown paths and optional file bodies. Callers supply the metadata table
//! (built-ins + project rules); this module does not depend on `vue_vet_rules`.
//! Finding explain attaches scan evidence to the same rule docs payload.

use std::{
  fmt::Write,
  fs,
  path::{Path, PathBuf},
};

use vue_vet_core::{
  Confidence, Diagnostic, FindingExplain, RuleExplain, RuleMeta, ScopeExplain, Severity,
};

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

/// Heuristic for CLI routing: finding ids always contain `::`; rule ids do not.
///
/// Consumers must still treat diagnostic ids as opaque strings and never parse
/// fields out of them; this only decides whether `--explain` should scan.
#[must_use]
pub fn looks_like_finding_id(target: &str) -> bool {
  target.contains("::")
}

/// Attach scan evidence to rule documentation for a matched finding.
#[must_use]
pub fn explain_finding(
  id: impl Into<String>,
  diagnostic: &Diagnostic,
  file: impl Into<String>,
  rule: RuleExplain,
) -> FindingExplain {
  FindingExplain {
    id: id.into(),
    file: file.into(),
    span: diagnostic.span.clone(),
    severity: diagnostic.severity,
    confidence: diagnostic.confidence,
    message: diagnostic.message.clone(),
    help: diagnostic.help.clone(),
    recommendation: diagnostic.recommendation.clone(),
    rule,
    tracking: None,
  }
}

/// Attach static tracking explain to a finding payload.
#[must_use]
pub fn finding_explain_with_tracking(
  mut explain: FindingExplain,
  tracking: ScopeExplain,
) -> FindingExplain {
  explain.tracking = Some(tracking);
  explain
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

/// Render a tracking-scope explain report (standalone or nested under a finding).
#[must_use]
pub fn render_scope_explain_text(explain: &ScopeExplain) -> String {
  let mut output = String::new();
  output.push_str("tracking scope\n");
  output.push_str("module: ");
  output.push_str(&explain.module_id);
  output.push('\n');
  output.push_str("kind: ");
  output.push_str(&explain.kind);
  output.push('\n');
  output.push_str("callee: ");
  output.push_str(&explain.callee);
  output.push('\n');
  if let Some(binding) = &explain.binding {
    output.push_str("binding: ");
    output.push_str(binding);
    output.push('\n');
  }
  output.push_str("span: ");
  if write!(
    output,
    "{}:{} (offset {}, length {})",
    explain.span.line, explain.span.column, explain.span.offset, explain.span.length
  )
  .is_err()
  {
    // Writing into String cannot fail.
  }
  output.push('\n');
  output.push_str("summary: ");
  output.push_str(&explain.summary);
  output.push('\n');
  if !explain.tracks.is_empty() {
    output.push_str("\ntracks:\n");
    for dep in &explain.tracks {
      output.push_str("  - ");
      output.push_str(&dep.path);
      output.push_str(" — ");
      output.push_str(&dep.reason_label);
      if !dep.guards.is_empty() {
        output.push_str(" (guards: ");
        output.push_str(&dep.guards.join(", "));
        output.push(')');
      }
      output.push('\n');
    }
  }
  if !explain.does_not_track.is_empty() {
    output.push_str("\ndoes not track:\n");
    for dep in &explain.does_not_track {
      output.push_str("  - ");
      output.push_str(&dep.path);
      output.push_str(" — ");
      output.push_str(&dep.reason_label);
      output.push('\n');
    }
  }
  if !explain.uncertain.is_empty() {
    output.push_str("\nuncertain accesses (maybe): ");
    output.push_str(&explain.uncertain.join(", "));
    output.push('\n');
  }
  output
}

/// JSON form of a standalone scope explain (not wrapped in scan schema).
///
/// # Errors
///
/// Returns a serialization error when the payload cannot be encoded.
pub fn render_scope_explain_json(explain: &ScopeExplain) -> Result<String, serde_json::Error> {
  serde_json::to_string_pretty(explain)
}

/// Render a human-readable finding explain report (rule docs + optional tracking).
#[must_use]
pub fn render_finding_explain_text(explain: &FindingExplain) -> String {
  let mut output = String::new();
  output.push_str("finding: ");
  output.push_str(&explain.id);
  output.push('\n');
  output.push_str("file: ");
  output.push_str(&explain.file);
  output.push('\n');
  output.push_str("span: ");
  output.push_str(&explain.span.line.to_string());
  output.push(':');
  output.push_str(&explain.span.column.to_string());
  output.push_str(" (offset ");
  output.push_str(&explain.span.offset.to_string());
  output.push_str(", length ");
  output.push_str(&explain.span.length.to_string());
  output.push_str(")\n");
  output.push_str("severity: ");
  output.push_str(severity_label(explain.severity));
  output.push('\n');
  if let Some(confidence) = explain.confidence {
    output.push_str("confidence: ");
    output.push_str(confidence_label(confidence));
    output.push('\n');
  }
  output.push_str("message: ");
  output.push_str(&explain.message);
  output.push('\n');
  if let Some(help) = &explain.help {
    output.push_str("help: ");
    output.push_str(help);
    output.push('\n');
  }
  if let Some(recommendation) = &explain.recommendation {
    output.push_str("recommendation: ");
    output.push_str(&recommendation.package);
    output.push(' ');
    output.push_str(&recommendation.export);
    output.push('\n');
    output.push_str("docs: ");
    output.push_str(&recommendation.docs_url);
    output.push('\n');
    output.push_str("import: ");
    output.push_str(&recommendation.import_example);
    output.push('\n');
  }
  output.push('\n');
  output.push_str(&render_rule_explain_text(&explain.rule));
  if let Some(tracking) = &explain.tracking {
    output.push('\n');
    output.push_str(&render_scope_explain_text(tracking));
  }
  output
}

/// Serialize finding explain JSON (pretty).
///
/// # Errors
///
/// Returns a serialization error when the payload cannot be encoded.
pub fn render_finding_explain_json(explain: &FindingExplain) -> Result<String, serde_json::Error> {
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

  #[test]
  fn finding_id_heuristic_requires_double_colon() {
    assert!(looks_like_finding_id("basic.vue::2:9::vue-vet/security/no-v-html::deadbeef"));
    assert!(!looks_like_finding_id("vue-vet/security/no-v-html"));
  }

  #[test]
  #[expect(clippy::panic, reason = "malformed finding explain JSON must fail the unit test")]
  fn finding_explain_nests_rule_docs() {
    use vue_vet_core::{Diagnostic, SourceSpan};

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let rule = explain_rule(&SAMPLE, &[root]);
    let diagnostic = Diagnostic {
      rule_id: SAMPLE.id.into(),
      category: SAMPLE.category.into(),
      severity: Severity::Warning,
      confidence: Some(Confidence::High),
      documentation: Some(SAMPLE.documentation.into()),
      message: "`v-html` can render untrusted HTML into the page".into(),
      help: Some("Prefer normal template interpolation.".into()),
      file: PathBuf::from("basic.vue").into(),
      span: SourceSpan { offset: 19, length: 6, line: 2, column: 9 },
      edits: Vec::new(),
      recommendation: None,
    };
    let explain = explain_finding(
      "basic.vue::2:9::vue-vet/security/no-v-html::abc",
      &diagnostic,
      "basic.vue",
      rule,
    );
    let text = render_finding_explain_text(&explain);
    assert!(text.contains("finding: basic.vue::"));
    assert!(text.contains("message: `v-html`"));
    assert!(text.contains("## Bad"));
    let Ok(json) = render_finding_explain_json(&explain) else {
      panic!("finding explain JSON must serialize");
    };
    assert!(json.contains("\"id\""));
    assert!(json.contains("\"rule\""));
  }

  #[test]
  #[expect(clippy::panic, reason = "malformed scope explain JSON must fail the unit test")]
  fn scope_explain_text_and_json_render() {
    use vue_vet_core::{ScopeExplain, ScopeExplainDep, ScopeTrackReason, SourceSpan};

    let explain = ScopeExplain {
      module_id: "App.vue".into(),
      kind: "computed".into(),
      callee: "computed".into(),
      binding: Some("label".into()),
      span: SourceSpan { offset: 10, length: 20, line: 2, column: 1 },
      summary:
        "`label` has no known reactive dependency — Vue will not re-run it when state changes"
          .into(),
      tracks: Vec::new(),
      does_not_track: vec![ScopeExplainDep {
        binding: "count".into(),
        property: Some("value".into()),
        path: "count.value".into(),
        reason: ScopeTrackReason::OutsideTracking,
        reason_label: "not tracked (outside active tracking: then/nextTick/callback)".into(),
        span: SourceSpan { offset: 12, length: 5, line: 3, column: 3 },
        guards: Vec::new(),
      }],
      uncertain: vec!["maybeRoot".into()],
    };
    let text = render_scope_explain_text(&explain);
    assert!(text.contains("tracking scope"));
    assert!(text.contains("summary: `label` has no known reactive dependency"));
    assert!(text.contains("does not track:"));
    assert!(text.contains("count.value"));
    assert!(text.contains("uncertain accesses (maybe): maybeRoot"));
    let Ok(json) = render_scope_explain_json(&explain) else {
      panic!("scope explain JSON must serialize");
    };
    assert!(json.contains("\"module_id\""));
    assert!(json.contains("\"does_not_track\""));
  }
}
