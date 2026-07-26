//! Map Vue Vet diagnostics onto LSP types without changing identity semantics.

use tower_lsp::lsp_types::{
  Diagnostic as LspDiagnostic, DiagnosticSeverity, NumberOrString, Position, Range,
};
use vue_vet_core::{Diagnostic, Severity, SourceSpan};
use vue_vet_reporters::report_diagnostic_id;

/// Convert a Vue Vet finding into an LSP diagnostic.
///
/// The opaque finding id is stored in `data` as a JSON string so clients and
/// agents can call `--explain <id>` without re-deriving identity. `code` is the
/// stable rule id. Positions use the same 1-based line/byte-column convention as
/// Vue Vet user-facing spans, converted to 0-based LSP positions (UTF-8 / byte
/// columns for this thin slice; ASCII fixtures stay UTF-16 compatible).
#[must_use]
pub fn to_lsp_diagnostic(
  diagnostic: &Diagnostic,
  analyzed_files: &[String],
  source: Option<&str>,
) -> LspDiagnostic {
  let id = report_diagnostic_id(diagnostic, analyzed_files);
  LspDiagnostic {
    range: span_to_range(&diagnostic.span, source),
    severity: Some(severity_to_lsp(diagnostic.severity)),
    code: Some(NumberOrString::String(diagnostic.rule_id.clone())),
    code_description: None,
    source: Some("vue-vet".into()),
    message: diagnostic.message.clone(),
    related_information: None,
    tags: None,
    data: Some(serde_json::Value::String(id)),
  }
}

#[must_use]
pub fn span_to_range(span: &SourceSpan, source: Option<&str>) -> Range {
  let start = Position { line: u32_line(span.line), character: u32_column(span.column) };
  let end = source.map_or_else(
    || Position {
      line: start.line,
      character: start.character.saturating_add(u32::try_from(span.length).unwrap_or(u32::MAX)),
    },
    |source| {
      let end_offset = span.offset.saturating_add(span.length);
      let (line, column) = line_column(source, end_offset);
      Position { line: u32_line(line), character: u32_column(column) }
    },
  );
  Range { start, end }
}

const fn severity_to_lsp(severity: Severity) -> DiagnosticSeverity {
  match severity {
    Severity::Error => DiagnosticSeverity::ERROR,
    Severity::Warning => DiagnosticSeverity::WARNING,
    Severity::Info => DiagnosticSeverity::INFORMATION,
  }
}

fn u32_line(line: usize) -> u32 {
  u32::try_from(line.saturating_sub(1)).unwrap_or(u32::MAX)
}

fn u32_column(column: usize) -> u32 {
  u32::try_from(column.saturating_sub(1)).unwrap_or(u32::MAX)
}

/// 1-based line and byte column for a UTF-8 byte offset (matches Vize adapter).
fn line_column(source: &str, offset: usize) -> (usize, usize) {
  let bytes = source.as_bytes();
  let prefix = bytes.get(..offset.min(bytes.len())).unwrap_or(bytes);
  let line =
    prefix.iter().fold(1_usize, |line, byte| line.saturating_add(usize::from(*byte == b'\n')));
  let column = prefix
    .iter()
    .rposition(|byte| *byte == b'\n')
    .map_or_else(|| prefix.len().saturating_add(1), |newline| prefix.len().saturating_sub(newline));
  (line, column)
}

#[cfg(test)]
mod tests {
  use super::*;
  use vue_vet_core::{Confidence, SourceSpan};

  #[test]
  fn maps_rule_id_message_and_opaque_finding_id() {
    let diagnostic = Diagnostic {
      rule_id: "vue-vet/security/no-v-html".into(),
      category: "security".into(),
      severity: Severity::Warning,
      confidence: Some(Confidence::High),
      documentation: Some("rules/security/no-v-html".into()),
      message: "`v-html` can render untrusted HTML into the page".into(),
      help: None,
      file: std::path::PathBuf::from("basic.vue"),
      span: SourceSpan { offset: 19, length: 6, line: 2, column: 9 },
      edits: Vec::new(),
    };
    let analyzed = vec!["basic.vue".into()];
    let lsp = to_lsp_diagnostic(
      &diagnostic,
      &analyzed,
      Some("<template>\n  <div v-html=\"x\" />\n</template>\n"),
    );
    assert_eq!(lsp.source.as_deref(), Some("vue-vet"));
    assert_eq!(lsp.code, Some(NumberOrString::String("vue-vet/security/no-v-html".into())));
    assert_eq!(lsp.severity, Some(DiagnosticSeverity::WARNING));
    assert_eq!(lsp.range.start.line, 1);
    assert_eq!(lsp.range.start.character, 8);
    assert_eq!(lsp.range.end.character, 14);
    let id = report_diagnostic_id(&diagnostic, &analyzed);
    assert_eq!(lsp.data, Some(serde_json::Value::String(id)));
  }
}
