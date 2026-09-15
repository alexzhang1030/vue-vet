//! Rule, finding, and scope explain formatters for CLI / MCP / LSP.
//!
//! Domain lookup lives in `vue_vet_session`. This module formats already-built
//! [`RuleExplain`] / [`FindingExplain`] / [`ScopeExplain`] payloads and maps
//! documentation keys to the JSON/report path form.

use std::fmt::Write;

use vue_vet_core::{Confidence, FindingExplain, RuleExplain, ScopeExplain, Severity};

/// Map a documentation key to the JSON/report path form.
#[must_use]
pub fn documentation_path(documentation: &str) -> String {
  format!("docs/{documentation}.md")
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

/// Render every matching scope explain, separated by a blank line.
#[must_use]
pub fn render_scope_explains_text(explains: &[ScopeExplain]) -> String {
  let mut output = String::new();
  for (index, explain) in explains.iter().enumerate() {
    if index > 0 {
      output.push('\n');
    }
    output.push_str(&render_scope_explain_text(explain));
  }
  output
}

/// JSON form of one or more standalone scope explains (same as CLI `--explain-scope`).
///
/// A single match is an object; multiple matches are an array. Neither is wrapped
/// in the scan `schema_version` report.
///
/// # Errors
///
/// Returns a serialization error when the payload cannot be encoded.
pub fn render_scope_explains_json(explains: &[ScopeExplain]) -> Result<String, serde_json::Error> {
  match explains {
    [single] => render_scope_explain_json(single),
    _ => serde_json::to_string_pretty(explains),
  }
}

/// Markdown form of a tracking-scope explain (LSP hover / editor hosts).
#[must_use]
pub fn render_scope_explain_markdown(explain: &ScopeExplain) -> String {
  let who = match explain.binding.as_deref() {
    Some(name) if !name.is_empty() => name,
    _ => explain.callee.as_str(),
  };
  let mut output = String::new();
  output.push_str("## ");
  output.push_str(who);
  output.push_str("\n\n_");
  output.push_str(&explain.kind);
  output.push_str("_ · `");
  output.push_str(&explain.module_id);
  output.push_str("`\n\n");
  output.push_str(&explain.summary);
  if !explain.tracks.is_empty() {
    output.push_str("\n\n**Tracks**");
    for dep in &explain.tracks {
      output.push_str("\n- `");
      output.push_str(&dep.path);
      output.push_str("` — ");
      output.push_str(&dep.reason_label);
    }
  }
  if !explain.does_not_track.is_empty() {
    output.push_str("\n\n**Does not track**");
    for dep in &explain.does_not_track {
      output.push_str("\n- `");
      output.push_str(&dep.path);
      output.push_str("` — ");
      output.push_str(&dep.reason_label);
    }
  }
  if !explain.uncertain.is_empty() {
    output.push_str("\n\n**Uncertain:** ");
    let mut first = true;
    for name in &explain.uncertain {
      if !first {
        output.push_str(", ");
      }
      first = false;
      output.push('`');
      output.push_str(name);
      output.push('`');
    }
  }
  output
}

/// Markdown for every matching scope explain, separated by a horizontal rule.
#[must_use]
pub fn render_scope_explains_markdown(explains: &[ScopeExplain]) -> String {
  let mut output = String::new();
  for (index, explain) in explains.iter().enumerate() {
    if index > 0 {
      output.push_str("\n\n---\n\n");
    }
    output.push_str(&render_scope_explain_markdown(explain));
  }
  output
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
  use vue_vet_core::{Confidence, Severity, SourceSpan};

  #[test]
  fn documentation_path_matches_json_report_shape() {
    assert_eq!(documentation_path("rules/security/no-v-html"), "docs/rules/security/no-v-html.md");
    assert_eq!(documentation_path("project-graph"), "docs/project-graph.md");
  }

  #[test]
  fn rule_explain_text_renders_fields() {
    let explain = RuleExplain {
      rule_id: "vue-vet/security/no-v-html".into(),
      category: "security".into(),
      severity: Severity::Warning,
      confidence: Confidence::High,
      documentation: "docs/rules/security/no-v-html.md".into(),
      body: Some("## Bad\n".into()),
      body_path: None,
      body_error: None,
    };
    let text = render_rule_explain_text(&explain);
    assert!(text.contains("category: security"));
    assert!(text.contains("## Bad"));
  }

  #[test]
  #[expect(clippy::panic, reason = "malformed finding explain JSON must fail the unit test")]
  fn finding_explain_nests_rule_docs() {
    let rule = RuleExplain {
      rule_id: "vue-vet/security/no-v-html".into(),
      category: "security".into(),
      severity: Severity::Warning,
      confidence: Confidence::High,
      documentation: "docs/rules/security/no-v-html.md".into(),
      body: Some("## Bad\n".into()),
      body_path: None,
      body_error: None,
    };
    let explain = FindingExplain {
      id: "basic.vue::2:9::vue-vet/security/no-v-html::abc".into(),
      file: "basic.vue".into(),
      span: SourceSpan { offset: 19, length: 6, line: 2, column: 9 },
      severity: Severity::Warning,
      confidence: Some(Confidence::High),
      message: "`v-html` can render untrusted HTML into the page".into(),
      help: Some("Prefer normal template interpolation.".into()),
      recommendation: None,
      rule,
      tracking: None,
    };
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
      unknown_calls: Vec::new(),
      follow_truncated: false,
      analysis_complete: true,
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

    let single = render_scope_explains_json(std::slice::from_ref(&explain))
      .unwrap_or_else(|_| panic!("single scope explain JSON must serialize"));
    assert_eq!(single, json, "one match stays an object, not an array");
    let many = render_scope_explains_json(&[explain.clone(), explain.clone()])
      .unwrap_or_else(|_| panic!("multi scope explain JSON must serialize"));
    assert!(many.trim_start().starts_with('['), "multiple matches are an array: {many}");
    assert!(render_scope_explains_text(&[]).is_empty());

    let markdown = render_scope_explain_markdown(&explain);
    assert!(markdown.contains("## label"));
    assert!(markdown.contains("_computed_"));
    assert!(markdown.contains("`App.vue`"));
    assert!(markdown.contains("no known reactive dependency"));
    assert!(markdown.contains("**Does not track**"));
    assert!(markdown.contains("`count.value`"));
    assert!(markdown.contains("**Uncertain:** `maybeRoot`"));
    assert!(render_scope_explains_markdown(&[]).is_empty());
    let many_md = render_scope_explains_markdown(&[explain.clone(), explain]);
    assert!(many_md.contains("\n\n---\n\n"), "multiple scopes are separated: {many_md}");
  }
}
