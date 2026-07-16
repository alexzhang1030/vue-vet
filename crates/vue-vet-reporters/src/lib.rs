use vue_vet_core::{ScanSummary, Severity};

pub const JSON_SCHEMA_VERSION: u8 = 1;

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
pub fn render(summary: &ScanSummary, format: ReportFormat) -> Result<String, serde_json::Error> {
  match format {
    ReportFormat::Text => Ok(render_text(summary)),
    ReportFormat::Json => render_json(summary),
  }
}

fn render_json(summary: &ScanSummary) -> Result<String, serde_json::Error> {
  let report = serde_json::json!({
    "schema_version": JSON_SCHEMA_VERSION,
    "files_scanned": summary.files_scanned,
    "diagnostics": &summary.diagnostics,
    "score": summary.score,
  });
  serde_json::to_string_pretty(&report)
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

  use vue_vet_core::{Diagnostic, SourceSpan};

  use super::*;

  fn fixture_summary() -> ScanSummary {
    ScanSummary {
      files_scanned: 1,
      diagnostics: vec![Diagnostic {
        rule_id: "vue-vet/security/no-v-html".into(),
        category: "security".into(),
        severity: Severity::Warning,
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

  #[test]
  fn text_report_matches_the_existing_snapshot() {
    let rendered = render(&fixture_summary(), ReportFormat::Text);
    assert_eq!(
      rendered.as_deref().ok(),
      Some(include_str!("../../../fixtures/reporters/no-v-html.txt").trim_end())
    );
  }

  #[test]
  fn json_report_matches_the_existing_snapshot() {
    let rendered = render(&fixture_summary(), ReportFormat::Json);
    assert_eq!(
      rendered.as_deref().ok(),
      Some(include_str!("../../../fixtures/reporters/no-v-html.json").trim_end())
    );
  }

  #[test]
  fn empty_text_report_retains_the_summary_line() {
    let rendered = render(&ScanSummary::default(), ReportFormat::Text);
    assert_eq!(rendered.as_deref().ok(), Some("\nVue Vet score: 0/100 — 0 file(s), 0 finding(s)"));
  }
}
