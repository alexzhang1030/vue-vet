//! JSON report contract (`schema_version` + operational errors).
use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use vue_vet_core::{
  ByteRange, Confidence, Diagnostic, EditApplicability, FileId, Recommendation, ScanSummary,
  Severity, SourceSpan, diagnostic_id,
};

use crate::{
  ComponentNavDigest, JSON_SCHEMA_VERSION, ReactivityDigest, ReportContext, ReportFramework,
  ReportMode, documentation_path,
};

#[derive(Serialize)]
struct JsonReport<'a> {
  schema_version: u8,
  tool: JsonTool,
  ok: bool,
  mode: ReportMode,
  project: JsonProject,
  diagnostics: Vec<JsonDiagnostic<'a>>,
  summary: JsonSummary,
  #[serde(skip_serializing_if = "Option::is_none")]
  reactivity: Option<&'a ReactivityDigest>,
  #[serde(skip_serializing_if = "Option::is_none")]
  component_nav: Option<&'a ComponentNavDigest>,
  error: Option<JsonError<'a>>,
}

#[derive(Serialize)]
struct JsonTool {
  name: &'static str,
  version: &'static str,
}

#[derive(Serialize)]
struct JsonProject {
  root: String,
  framework: ReportFramework,
  analyzed_files: Vec<String>,
  analyzed_file_count: usize,
  files_scanned: usize,
  complete: bool,
  skipped_checks: Vec<String>,
  skipped_check_reasons: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct JsonDiagnostic<'a> {
  id: String,
  rule_id: &'a str,
  category: &'a str,
  severity: Severity,
  confidence: Option<Confidence>,
  message: &'a str,
  help: Option<&'a str>,
  documentation: Option<String>,
  file: &'a str,
  span: &'a SourceSpan,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  edits: Vec<JsonTextEdit<'a>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  recommendation: Option<&'a Recommendation>,
}

#[derive(Serialize)]
struct JsonTextEdit<'a> {
  file: &'a str,
  range: &'a ByteRange,
  replacement: &'a str,
  applicability: EditApplicability,
  rule_id: &'a str,
}

#[derive(Serialize)]
struct JsonSummary {
  score: Option<u8>,
  finding_count: usize,
  affected_file_count: usize,
  by_severity: SeverityCounts,
}

#[derive(Default, Serialize)]
struct SeverityCounts {
  info: usize,
  warning: usize,
  error: usize,
}

#[derive(Serialize)]
struct JsonError<'a> {
  message: &'a str,
}

pub fn render_json(
  summary: &ScanSummary,
  context: &ReportContext,
) -> Result<String, serde_json::Error> {
  let mut analyzed_files =
    context.analyzed_files.iter().map(|path| normalize_path(path)).collect::<Vec<_>>();
  analyzed_files.sort();
  analyzed_files.dedup();

  let diagnostics = summary
    .diagnostics
    .iter()
    .map(|diagnostic| json_diagnostic(diagnostic, &analyzed_files))
    .collect::<Vec<_>>();
  let affected_file_count =
    diagnostics.iter().map(|diagnostic| diagnostic.file).collect::<BTreeSet<_>>().len();
  let mut by_severity = SeverityCounts::default();
  for diagnostic in &summary.diagnostics {
    match diagnostic.severity {
      Severity::Info => by_severity.info = by_severity.info.saturating_add(1),
      Severity::Warning => by_severity.warning = by_severity.warning.saturating_add(1),
      Severity::Error => by_severity.error = by_severity.error.saturating_add(1),
    }
  }
  let skipped_checks = context.skipped_check_reasons.keys().cloned().collect();
  let report = JsonReport {
    schema_version: JSON_SCHEMA_VERSION,
    tool: JsonTool { name: "vue-vet", version: env!("CARGO_PKG_VERSION") },
    ok: true,
    mode: context.mode,
    project: json_project(summary.files_scanned, context, analyzed_files, skipped_checks),
    diagnostics,
    summary: JsonSummary {
      score: Some(summary.score),
      finding_count: summary.diagnostics.len(),
      affected_file_count,
      by_severity,
    },
    reactivity: context.reactivity.as_ref(),
    component_nav: context.component_nav.as_ref(),
    error: None,
  };
  serde_json::to_string_pretty(&report)
}

/// Renders an operational failure through the same JSON wire contract.
///
/// # Errors
///
/// Returns a serialization error when JSON output cannot be encoded.
pub fn render_error(message: &str, context: &ReportContext) -> Result<String, serde_json::Error> {
  let mut analyzed_files =
    context.analyzed_files.iter().map(|path| normalize_path(path)).collect::<Vec<_>>();
  analyzed_files.sort();
  analyzed_files.dedup();
  let skipped_checks = context.skipped_check_reasons.keys().cloned().collect();
  let report = JsonReport {
    schema_version: JSON_SCHEMA_VERSION,
    tool: JsonTool { name: "vue-vet", version: env!("CARGO_PKG_VERSION") },
    ok: false,
    mode: context.mode,
    project: json_project(0, context, analyzed_files, skipped_checks),
    diagnostics: Vec::new(),
    summary: JsonSummary {
      score: None,
      finding_count: 0,
      affected_file_count: 0,
      by_severity: SeverityCounts::default(),
    },
    reactivity: context.reactivity.as_ref(),
    component_nav: context.component_nav.as_ref(),
    error: Some(JsonError { message }),
  };
  serde_json::to_string_pretty(&report)
}

fn json_project(
  files_scanned: usize,
  context: &ReportContext,
  analyzed_files: Vec<String>,
  skipped_checks: Vec<String>,
) -> JsonProject {
  JsonProject {
    root: normalize_path(&context.project_root),
    framework: context.framework,
    analyzed_file_count: analyzed_files.len(),
    analyzed_files,
    files_scanned,
    complete: context.complete,
    skipped_checks,
    skipped_check_reasons: context.skipped_check_reasons.clone(),
  }
}

fn json_diagnostic<'a>(
  diagnostic: &'a Diagnostic,
  analyzed_files: &[String],
) -> JsonDiagnostic<'a> {
  // FileId paths are normalized and shared by the report and diagnostic id.
  let file = report_path(&diagnostic.file, analyzed_files);
  JsonDiagnostic {
    id: diagnostic_id(diagnostic, file),
    rule_id: &diagnostic.rule_id,
    category: &diagnostic.category,
    severity: diagnostic.severity,
    confidence: diagnostic.confidence,
    message: &diagnostic.message,
    help: diagnostic.help.as_deref(),
    documentation: diagnostic.documentation.as_deref().map(documentation_path),
    file,
    span: &diagnostic.span,
    edits: diagnostic
      .edits
      .iter()
      .map(|edit| JsonTextEdit {
        file: report_path(&edit.file, analyzed_files),
        range: &edit.range,
        replacement: &edit.replacement,
        applicability: edit.applicability,
        rule_id: &edit.rule_id,
      })
      .collect(),
    recommendation: diagnostic.recommendation.as_ref(),
  }
}

/// Opaque diagnostic identity matching JSON report `diagnostics[].id`.
///
/// `analyzed_files` should use `/` separators (same normalization as the JSON
/// report). Consumers treat the result as opaque; CLI `--explain` matches it
/// exactly after a scan of the same path.
#[must_use]
pub fn report_diagnostic_id(diagnostic: &Diagnostic, analyzed_files: &[String]) -> String {
  diagnostic_id(diagnostic, report_path(&diagnostic.file, analyzed_files))
}

fn report_path<'a>(path: &'a FileId, _analyzed_files: &[String]) -> &'a str {
  path.as_str()
}

fn normalize_path(path: &str) -> String {
  path.replace('\\', "/")
}
