use std::{
  collections::{BTreeMap, BTreeSet},
  path::Path,
};

use serde::Serialize;
use vue_vet_core::{Confidence, Diagnostic, ScanSummary, Severity, SourceSpan, diagnostic_id};

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
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportFormat {
  Text,
  Json,
}

/// Renders a scan summary without a terminal newline.
///
/// # Errors
///
/// Returns a serialization error when JSON output cannot be encoded.
pub fn render(
  summary: &ScanSummary,
  format: ReportFormat,
  context: &ReportContext,
) -> Result<String, serde_json::Error> {
  match format {
    ReportFormat::Text => Ok(render_text(summary)),
    ReportFormat::Json => render_json(summary, context),
  }
}

#[derive(Serialize)]
struct JsonReport<'a> {
  schema_version: u8,
  tool: JsonTool,
  ok: bool,
  mode: ReportMode,
  project: JsonProject,
  diagnostics: Vec<JsonDiagnostic<'a>>,
  summary: JsonSummary,
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
  file: String,
  span: &'a SourceSpan,
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

fn render_json(
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
    diagnostics.iter().map(|diagnostic| diagnostic.file.as_str()).collect::<BTreeSet<_>>().len();
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
  let file = report_path(&diagnostic.file, analyzed_files);
  JsonDiagnostic {
    id: diagnostic_id(diagnostic, &file),
    rule_id: &diagnostic.rule_id,
    category: &diagnostic.category,
    severity: diagnostic.severity,
    confidence: diagnostic.confidence,
    message: &diagnostic.message,
    help: diagnostic.help.as_deref(),
    documentation: diagnostic.documentation.as_deref().map(documentation_path),
    file,
    span: &diagnostic.span,
  }
}

fn report_path(path: &Path, analyzed_files: &[String]) -> String {
  let normalized = normalize_path(&path.to_string_lossy());
  analyzed_files
    .iter()
    .find(|candidate| {
      normalized == candidate.as_str()
        || normalized.strip_suffix(candidate.as_str()).is_some_and(|prefix| prefix.ends_with('/'))
    })
    .cloned()
    .unwrap_or(normalized)
}

fn normalize_path(path: &str) -> String {
  path.replace('\\', "/")
}

fn documentation_path(documentation: &str) -> String {
  format!("docs/{documentation}.md")
}

fn render_text(summary: &ScanSummary) -> String {
  let mut output = String::new();
  for diagnostic in &summary.diagnostics {
    output.push_str(&diagnostic.file.display().to_string());
    output.push(':');
    output.push_str(&diagnostic.span.line.to_string());
    output.push(':');
    output.push_str(&diagnostic.span.column.to_string());
    output.push_str("  ");
    output.push_str(severity_name(diagnostic.severity));
    output.push_str("  ");
    output.push_str(&diagnostic.rule_id);
    output.push_str("  ");
    output.push_str(&diagnostic.message);
    output.push('\n');
    if let Some(help) = &diagnostic.help {
      output.push_str("  help: ");
      output.push_str(help);
      output.push('\n');
    }
  }
  output.push('\n');
  output.push_str("Vue Vet score: ");
  output.push_str(&summary.score.to_string());
  output.push_str("/100 — ");
  output.push_str(&summary.files_scanned.to_string());
  output.push_str(" file(s), ");
  output.push_str(&summary.diagnostics.len().to_string());
  output.push_str(" finding(s)");
  output
}

const fn severity_name(severity: Severity) -> &'static str {
  match severity {
    Severity::Info => "info",
    Severity::Warning => "warning",
    Severity::Error => "error",
  }
}

#[cfg(test)]
mod tests {
  use std::path::PathBuf;

  use serde_json::Value;

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
        file: PathBuf::from("fixtures/reporters/no-v-html.vue"),
        span: SourceSpan { offset: 19, length: 6, line: 2, column: 9 },
      }],
      score: 97,
    }
  }

  fn fixture_context() -> ReportContext {
    ReportContext {
      project_root: "fixtures/reporters".into(),
      analyzed_files: vec!["no-v-html.vue".into()],
      ..ReportContext::default()
    }
  }

  #[test]
  fn text_report_matches_the_existing_snapshot() {
    let rendered = render(&fixture_summary(), ReportFormat::Text, &fixture_context());
    assert_eq!(
      rendered.as_deref().ok(),
      Some(include_str!("../../../fixtures/reporters/no-v-html.txt").trim_end())
    );
  }

  #[test]
  fn json_report_matches_the_version_one_snapshot() {
    let rendered = render(&fixture_summary(), ReportFormat::Json, &fixture_context());
    assert_eq!(
      rendered.as_deref().ok(),
      Some(include_str!("../../../fixtures/reporters/no-v-html.json").trim_end())
    );
  }

  #[test]
  fn json_report_normalizes_absolute_windows_paths_against_coverage() {
    let mut summary = fixture_summary();
    if let Some(diagnostic) = summary.diagnostics.first_mut() {
      diagnostic.file = PathBuf::from(r"C:\repo\src\App.vue");
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
      Some("src/App.vue")
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
      Some(false)
    );
    assert_eq!(
      project
        .and_then(|value| value.get("skipped_checks"))
        .and_then(Value::as_array)
        .and_then(|checks| checks.first())
        .and_then(Value::as_str),
      Some("module_reactivity")
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
      Some(false)
    );
    assert_eq!(
      parsed
        .as_ref()
        .and_then(|report| report.get("summary"))
        .and_then(|summary| summary.get("score")),
      Some(&Value::Null)
    );
    assert_eq!(
      parsed
        .as_ref()
        .and_then(|report| report.get("error"))
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str),
      Some("parser failed")
    );
  }

  #[test]
  fn empty_text_report_retains_the_summary_line() {
    let rendered = render(&ScanSummary::default(), ReportFormat::Text, &ReportContext::default());
    assert_eq!(rendered.as_deref().ok(), Some("\nVue Vet score: 0/100 — 0 file(s), 0 finding(s)"));
  }
}
