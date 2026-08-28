//! Deterministic text and machine-readable reporters.
//!
//! Renders [`vue_vet_core::ScanSummary`] and explain models only — no session
//! ownership, analysis, or file mutation. See the crate README and
//! `docs/json-output.md`.

use std::collections::BTreeMap;

use serde::Serialize;
use vue_vet_core::ScanSummary;

mod binding_nav;
mod color;
mod component_nav;
mod explain;
mod github;
mod humanize;
mod json;
mod reactivity;
mod sarif;
mod text;

pub use binding_nav::{
  BindingNav, BindingNavDep, BindingNavReader, BindingNavSource, binding_nav_from_details,
};
pub use component_nav::{
  ComponentNavDigest, ComponentNavEdgeInput, ComponentNavLink, ComponentNavModule,
  component_nav_from_edges,
};
pub use explain::{
  documentation_path, explain_finding, explain_rule, find_rule_meta, finding_explain_with_tracking,
  looks_like_finding_id, render_finding_explain_json, render_finding_explain_text,
  render_rule_explain_json, render_rule_explain_text, render_scope_explain_json,
  render_scope_explain_markdown, render_scope_explain_text, render_scope_explains_json,
  render_scope_explains_markdown, render_scope_explains_text,
};
pub use humanize::{
  humanize_binding, humanize_edge, humanize_edge_parts_with_property, humanize_scope,
  humanize_source, humanize_template_read, humanize_template_surface, parse_name_offset, to_path,
};
use json::render_json;
pub use json::{render_error, report_diagnostic_id};
pub use reactivity::{
  ReactivityBindingDetail, ReactivityDigest, ReactivityEdgeDetail, ReactivityHotspot,
  ReactivityModuleDetail, ReactivityModuleStats, ReactivityScopeDetail, ReactivitySpanRef,
  ReactivityTemplateReadDetail, binding_detail, edge_detail, render_reactivity_detail,
  render_reactivity_footer, scope_detail, scope_detail_with_uncertain, scope_label_with_uncertain,
  template_read_detail, to_span_from_identity,
};
use text::render_text;
pub use text::{render_text_diagnostics, render_text_score_footer};

pub use vue_vet_core::{
  FindingExplain, RuleExplain, ScopeExplain, ScopeExplainDep, ScopeTrackReason,
};

pub const JSON_SCHEMA_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportMode {
  Full,
  Baseline,
  Diff,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportFramework {
  Vue,
  Nuxt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportContext {
  pub mode: ReportMode,
  pub framework: ReportFramework,
  pub project_root: String,
  pub analyzed_files: Vec<String>,
  pub complete: bool,
  pub skipped_check_reasons: BTreeMap<String, String>,
  pub reactivity: Option<ReactivityDigest>,
  /// Structural component `uses` / `used_by` (not prop dataflow).
  pub component_nav: Option<ComponentNavDigest>,
  /// When true, text reports wrap ANSI styles. Default false for snapshots / CI.
  pub color: bool,
}

impl Default for ReportContext {
  fn default() -> Self {
    Self {
      mode: ReportMode::Full,
      framework: ReportFramework::Vue,
      project_root: ".".into(),
      analyzed_files: Vec::new(),
      complete: true,
      skipped_check_reasons: BTreeMap::new(),
      reactivity: None,
      component_nav: None,
      color: false,
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportFormat {
  Text,
  Json,
  Sarif,
  Github,
}

/// Renders a scan summary without a terminal newline.
///
/// # Errors
///
/// Returns a serialization error when JSON or SARIF output cannot be encoded.
pub fn render(
  summary: &ScanSummary,
  format: ReportFormat,
  context: &ReportContext,
) -> Result<String, serde_json::Error> {
  match format {
    ReportFormat::Text => Ok(render_text(summary, context)),
    ReportFormat::Json => render_json(summary, context),
    ReportFormat::Sarif => sarif::render(summary, context),
    ReportFormat::Github => Ok(github::render(summary, context)),
  }
}

#[cfg(test)]
mod tests {
  use serde_json::Value;
  use vue_vet_core::{Confidence, Diagnostic, FileId, Severity, SourceSpan};

  use super::*;

  fn fixture_summary() -> ScanSummary {
    ScanSummary {
      files_scanned: 1,
      diagnostics: vec![Diagnostic {
        rule_id: "vue-vet/security/no-v-html".into(),
        category: "security".into(),
        severity: Severity::Warning,
        confidence: Some(Confidence::High),
        documentation: Some("rules/security/no-v-html".into()),
        message: "`v-html` can render untrusted HTML into the page".into(),
        help: Some(
          "Prefer normal template interpolation. If raw HTML is required, sanitize it at the trust boundary."
            .into(),
        ),
        file: FileId::from("no-v-html.vue"),
        span: SourceSpan { offset: 19, length: 6, line: 2, column: 9 },
        edits: Vec::new(),
        recommendation: None,
      }],
      score: 94,
    }
  }

  fn fixture_context() -> ReportContext {
    ReportContext {
      project_root: "fixtures/reporters".into(),
      analyzed_files: vec!["no-v-html.vue".into()],
      reactivity: Some(ReactivityDigest::default()),
      component_nav: Some(ComponentNavDigest::default()),
      ..ReportContext::default()
    }
  }

  #[test]
  fn text_report_matches_the_existing_snapshot() {
    let rendered = render(&fixture_summary(), ReportFormat::Text, &fixture_context());
    assert_eq!(
      rendered.as_deref().ok().map(str::trim_end),
      Some(include_str!("../../../fixtures/reporters/no-v-html.txt").trim_end()),
      "text output must retain its stable snapshot"
    );
  }

  #[test]
  fn text_report_color_wraps_severity_and_location() {
    let context = ReportContext { color: true, ..fixture_context() };
    let rendered = render(&fixture_summary(), ReportFormat::Text, &context);
    assert!(
      rendered.as_ref().is_ok_and(|output| {
        output.contains('\u{1b}')
          && output.contains("warning")
          && output.contains("no-v-html.vue")
          && output.contains("Reactivity")
      }),
      "colored text must wrap ANSI while keeping readable labels: {rendered:?}"
    );
  }

  #[test]
  fn json_report_matches_the_version_one_snapshot() {
    let rendered = render(&fixture_summary(), ReportFormat::Json, &fixture_context());
    assert_eq!(
      rendered.as_deref().ok(),
      Some(include_str!("../../../fixtures/reporters/no-v-html.json").trim_end()),
      "JSON v1 output must retain its stable snapshot"
    );
  }

  #[test]
  fn json_report_uses_the_pre_normalized_file_id() {
    let mut summary = fixture_summary();
    if let Some(diagnostic) = summary.diagnostics.first_mut() {
      diagnostic.file = FileId::from(r"src\App.vue");
    }
    let context =
      ReportContext { analyzed_files: vec!["src/App.vue".into()], ..ReportContext::default() };
    let rendered = render(&summary, ReportFormat::Json, &context);
    let parsed =
      rendered.as_ref().ok().and_then(|output| serde_json::from_str::<Value>(output).ok());
    assert_eq!(
      parsed
        .as_ref()
        .and_then(|report| report.get("diagnostics"))
        .and_then(Value::as_array)
        .and_then(|diagnostics| diagnostics.first())
        .and_then(|diagnostic| diagnostic.get("file"))
        .and_then(Value::as_str),
      Some("src/App.vue"),
      "JSON paths must use the discovery-normalized FileId"
    );
  }

  #[test]
  fn incomplete_scan_explains_skipped_checks() {
    let context = ReportContext {
      complete: false,
      skipped_check_reasons: BTreeMap::from([(
        "module_reactivity".into(),
        "module tracing failed".into(),
      )]),
      ..fixture_context()
    };
    let rendered = render(&fixture_summary(), ReportFormat::Json, &context);
    let parsed =
      rendered.as_ref().ok().and_then(|output| serde_json::from_str::<Value>(output).ok());
    let project = parsed.as_ref().and_then(|report| report.get("project"));
    assert_eq!(
      project.and_then(|value| value.get("complete")).and_then(Value::as_bool),
      Some(false),
      "incomplete scans must be explicit"
    );
    assert_eq!(
      project
        .and_then(|value| value.get("skipped_checks"))
        .and_then(Value::as_array)
        .and_then(|checks| checks.first())
        .and_then(Value::as_str),
      Some("module_reactivity"),
      "skipped checks must name the omitted analysis"
    );
  }

  #[test]
  fn operational_error_uses_the_same_parseable_contract() {
    let context = ReportContext {
      complete: false,
      skipped_check_reasons: BTreeMap::from([("scan".into(), "parser failed".into())]),
      ..fixture_context()
    };
    let rendered = render_error("parser failed", &context);
    let parsed =
      rendered.as_ref().ok().and_then(|output| serde_json::from_str::<Value>(output).ok());
    assert_eq!(
      parsed.as_ref().and_then(|report| report.get("ok")).and_then(Value::as_bool),
      Some(false),
      "operational failures must set ok=false"
    );
    assert_eq!(
      parsed
        .as_ref()
        .and_then(|report| report.get("summary"))
        .and_then(|summary| summary.get("score")),
      Some(&Value::Null),
      "failed scans must not claim a score"
    );
    assert_eq!(
      parsed
        .as_ref()
        .and_then(|report| report.get("error"))
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str),
      Some("parser failed"),
      "structured failures must preserve the message"
    );
  }

  #[test]
  fn empty_text_report_retains_the_summary_line() {
    let rendered = render(&ScanSummary::default(), ReportFormat::Text, &ReportContext::default());
    assert_eq!(
      rendered.as_deref().ok(),
      Some("\nVue Vet score: 0/100 — 0 file(s), 0 finding(s)"),
      "empty text scans must retain the summary"
    );
  }

  #[test]
  fn text_report_appends_reactivity_digest() {
    let mut module = ReactivityModuleStats::empty("App.vue");
    module.bindings = 2;
    module.scopes = 1;
    module.edges = 1;
    module.template_reads = 1;
    let digest = ReactivityDigest::from_modules(&[module], None);
    let context = ReportContext { reactivity: Some(digest), ..ReportContext::default() };
    let rendered = render(&ScanSummary::default(), ReportFormat::Text, &context);
    let rendered = rendered.as_deref().ok().unwrap_or("");
    assert!(rendered.contains("Vue Vet score: 0/100"));
    assert!(rendered.contains("Reactivity"));
    assert!(
      rendered.contains("traced 1 module(s) · 2 bindings · 1 scopes · 1 edges · 1 template reads")
    );
    assert!(rendered.contains("App.vue"));
  }

  #[test]
  fn json_report_includes_reactivity_when_present() {
    let digest = ReactivityDigest::from_modules(&[], Some("tracing failed".into()));
    let context = ReportContext { reactivity: Some(digest), ..fixture_context() };
    let rendered = render(&fixture_summary(), ReportFormat::Json, &context);
    let parsed =
      rendered.as_ref().ok().and_then(|output| serde_json::from_str::<Value>(output).ok());
    assert_eq!(
      parsed.as_ref().and_then(|value| value.pointer("/reactivity/error")).and_then(Value::as_str),
      Some("tracing failed")
    );
  }
}
