use std::{collections::BTreeMap, path::Path};

use serde::Serialize;
use vue_vet_core::{Confidence, Diagnostic, ScanSummary, Severity, diagnostic_id};

use crate::ReportContext;

const SARIF_SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";
const SARIF_VERSION: &str = "2.1.0";
const REPOSITORY_URL: &str = "https://github.com/alexzhang1030/vue-vet";

#[derive(Serialize)]
struct SarifLog {
  #[serde(rename = "$schema")]
  schema: &'static str,
  version: &'static str,
  runs: Vec<SarifRun>,
}

#[derive(Serialize)]
struct SarifRun {
  tool: SarifTool,
  results: Vec<SarifResult>,
}

#[derive(Serialize)]
struct SarifTool {
  driver: SarifDriver,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifDriver {
  name: &'static str,
  semantic_version: &'static str,
  information_uri: &'static str,
  rules: Vec<SarifRule>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRule {
  id: String,
  short_description: SarifMessage,
  #[serde(skip_serializing_if = "Option::is_none")]
  help_uri: Option<String>,
  properties: SarifRuleProperties,
}

#[derive(Serialize)]
struct SarifRuleProperties {
  category: String,
  severity: &'static str,
  #[serde(skip_serializing_if = "Option::is_none")]
  confidence: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifResult {
  rule_id: String,
  level: &'static str,
  message: SarifMessage,
  locations: Vec<SarifLocation>,
  partial_fingerprints: BTreeMap<&'static str, String>,
  properties: SarifResultProperties,
}

#[derive(Serialize)]
struct SarifMessage {
  text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifLocation {
  physical_location: SarifPhysicalLocation,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifPhysicalLocation {
  artifact_location: SarifArtifactLocation,
  region: SarifRegion,
}

#[derive(Serialize)]
struct SarifArtifactLocation {
  uri: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRegion {
  start_line: usize,
  start_column: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifResultProperties {
  category: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  confidence: Option<&'static str>,
  #[serde(skip_serializing_if = "Option::is_none")]
  help: Option<String>,
  byte_offset: usize,
  byte_length: usize,
}

pub fn render(
  summary: &ScanSummary,
  context: &ReportContext,
) -> Result<String, serde_json::Error> {
  let analyzed_files = analyzed_files(context);
  let mut rules = BTreeMap::new();
  let results = summary
    .diagnostics
    .iter()
    .map(|diagnostic| {
      rules.entry(diagnostic.rule_id.clone()).or_insert_with(|| sarif_rule(diagnostic));
      sarif_result(diagnostic, &analyzed_files)
    })
    .collect();
  let log = SarifLog {
    schema: SARIF_SCHEMA,
    version: SARIF_VERSION,
    runs: vec![SarifRun {
      tool: SarifTool {
        driver: SarifDriver {
          name: "vue-vet",
          semantic_version: env!("CARGO_PKG_VERSION"),
          information_uri: REPOSITORY_URL,
          rules: rules.into_values().collect(),
        },
      },
      results,
    }],
  };
  serde_json::to_string_pretty(&log)
}

fn sarif_rule(diagnostic: &Diagnostic) -> SarifRule {
  SarifRule {
    id: diagnostic.rule_id.clone(),
    short_description: SarifMessage { text: format!("{} diagnostic", diagnostic.category) },
    help_uri: diagnostic.documentation.as_deref().map(documentation_url),
    properties: SarifRuleProperties {
      category: diagnostic.category.clone(),
      severity: severity_name(diagnostic.severity),
      confidence: diagnostic.confidence.map(confidence_name),
    },
  }
}

fn sarif_result(diagnostic: &Diagnostic, analyzed_files: &[String]) -> SarifResult {
  let file = report_path(&diagnostic.file, analyzed_files);
  SarifResult {
    rule_id: diagnostic.rule_id.clone(),
    level: sarif_level(diagnostic.severity),
    message: SarifMessage { text: diagnostic.message.clone() },
    locations: vec![SarifLocation {
      physical_location: SarifPhysicalLocation {
        artifact_location: SarifArtifactLocation { uri: file.clone() },
        region: SarifRegion {
          start_line: diagnostic.span.line,
          start_column: diagnostic.span.column,
        },
      },
    }],
    partial_fingerprints: BTreeMap::from([(
      "vueVetDiagnosticId/v1",
      diagnostic_id(diagnostic, &file),
    )]),
    properties: SarifResultProperties {
      category: diagnostic.category.clone(),
      confidence: diagnostic.confidence.map(confidence_name),
      help: diagnostic.help.clone(),
      byte_offset: diagnostic.span.offset,
      byte_length: diagnostic.span.length,
    },
  }
}

fn analyzed_files(context: &ReportContext) -> Vec<String> {
  let mut files =
    context.analyzed_files.iter().map(|path| normalize_path(path)).collect::<Vec<_>>();
  files.sort();
  files.dedup();
  files
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

fn documentation_url(documentation: &str) -> String {
  format!("{REPOSITORY_URL}/blob/main/docs/{documentation}.md")
}

const fn severity_name(severity: Severity) -> &'static str {
  match severity {
    Severity::Info => "info",
    Severity::Warning => "warning",
    Severity::Error => "error",
  }
}

const fn sarif_level(severity: Severity) -> &'static str {
  match severity {
    Severity::Info => "note",
    Severity::Warning => "warning",
    Severity::Error => "error",
  }
}

const fn confidence_name(confidence: Confidence) -> &'static str {
  match confidence {
    Confidence::High => "high",
    Confidence::Medium => "medium",
    Confidence::Low => "low",
  }
}

#[cfg(test)]
mod tests {
  use std::path::PathBuf;

  use serde_json::Value;
  use vue_vet_core::{SourceSpan, diagnostic_id};

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
        help: Some("Prefer normal template interpolation.".into()),
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
  fn sarif_report_matches_snapshot() {
    let rendered = render(&fixture_summary(), &fixture_context());
    assert_eq!(
      rendered.as_deref().ok(),
      Some(include_str!("../../../fixtures/reporters/no-v-html.sarif").trim_end()),
      "SARIF output must remain deterministic"
    );
  }

  #[test]
  fn sarif_normalizes_windows_paths_and_keeps_stable_fingerprint() {
    let mut summary = fixture_summary();
    if let Some(diagnostic) = summary.diagnostics.first_mut() {
      diagnostic.file = PathBuf::from(r"C:\repo\src\App.vue");
    }
    let context =
      ReportContext { analyzed_files: vec!["src/App.vue".into()], ..ReportContext::default() };
    let rendered = render(&summary, &context);
    let parsed =
      rendered.as_ref().ok().and_then(|output| serde_json::from_str::<Value>(output).ok());
    let result = parsed
      .as_ref()
      .and_then(|log| log.get("runs"))
      .and_then(Value::as_array)
      .and_then(|runs| runs.first())
      .and_then(|run| run.get("results"))
      .and_then(Value::as_array)
      .and_then(|results| results.first());
    assert_eq!(
      result
        .and_then(|value| value.get("locations"))
        .and_then(Value::as_array)
        .and_then(|locations| locations.first())
        .and_then(|location| location.get("physicalLocation"))
        .and_then(|location| location.get("artifactLocation"))
        .and_then(|location| location.get("uri"))
        .and_then(Value::as_str),
      Some("src/App.vue"),
      "SARIF paths must be repository-relative and slash-normalized"
    );
    let expected =
      summary.diagnostics.first().map(|diagnostic| diagnostic_id(diagnostic, "src/App.vue"));
    assert_eq!(
      result
        .and_then(|value| value.get("partialFingerprints"))
        .and_then(|fingerprints| fingerprints.get("vueVetDiagnosticId/v1"))
        .and_then(Value::as_str),
      expected.as_deref(),
      "SARIF must reuse the canonical Vue Vet finding identity"
    );
  }
}
