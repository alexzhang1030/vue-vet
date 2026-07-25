use std::path::Path;

use vue_vet_core::{Diagnostic, ScanSummary, Severity};

use crate::ReportContext;

pub fn render(summary: &ScanSummary, context: &ReportContext) -> String {
  let analyzed_files = analyzed_files(context);
  summary
    .diagnostics
    .iter()
    .map(|diagnostic| annotation(diagnostic, &analyzed_files))
    .collect::<Vec<_>>()
    .join("\n")
}

fn annotation(diagnostic: &Diagnostic, analyzed_files: &[String]) -> String {
  let file = escape_property(&report_path(&diagnostic.file, analyzed_files));
  let title = escape_property(&diagnostic.rule_id);
  let message = diagnostic.help.as_ref().map_or_else(
    || diagnostic.message.clone(),
    |help| format!("{}\nhelp: {help}", diagnostic.message),
  );
  format!(
    "::{} file={file},line={},col={},title={title}::{}",
    annotation_level(diagnostic.severity),
    diagnostic.span.line,
    diagnostic.span.column,
    escape_data(&message)
  )
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

fn escape_data(value: &str) -> String {
  value.replace('%', "%25").replace('\r', "%0D").replace('\n', "%0A")
}

fn escape_property(value: &str) -> String {
  escape_data(value).replace(':', "%3A").replace(',', "%2C")
}

const fn annotation_level(severity: Severity) -> &'static str {
  match severity {
    Severity::Info => "notice",
    Severity::Warning => "warning",
    Severity::Error => "error",
  }
}

#[cfg(test)]
mod tests {
  use std::path::PathBuf;

  use vue_vet_core::{Confidence, SourceSpan};

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
        edits: Vec::new(),
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
  fn github_report_matches_snapshot() {
    let rendered = render(&fixture_summary(), &fixture_context());
    assert_eq!(
      rendered,
      include_str!("../../../fixtures/reporters/no-v-html.github").trim_end(),
      "GitHub workflow commands must remain deterministic"
    );
  }

  #[test]
  fn github_annotations_escape_command_data_and_properties() {
    let summary = ScanSummary {
      files_scanned: 1,
      diagnostics: vec![Diagnostic {
        rule_id: "vue-vet/test:rule,one".into(),
        category: "test".into(),
        severity: Severity::Error,
        confidence: None,
        documentation: None,
        message: "first%line\r\nsecond".into(),
        help: None,
        file: PathBuf::from(r"C:\repo\src\Odd:Name,One.vue"),
        span: SourceSpan { offset: 0, length: 1, line: 3, column: 7 },
        edits: Vec::new(),
      }],
      score: 0,
    };
    let context = ReportContext {
      analyzed_files: vec!["src/Odd:Name,One.vue".into()],
      ..ReportContext::default()
    };
    assert_eq!(
      render(&summary, &context),
      "::error file=src/Odd%3AName%2COne.vue,line=3,col=7,title=vue-vet/test%3Arule%2Cone::first%25line%0D%0Asecond",
      "workflow command delimiters and data control characters must be escaped"
    );
  }
}
