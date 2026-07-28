//! Map Vue Vet diagnostics onto LSP types without changing identity semantics.

use std::path::Path;

use tower_lsp::lsp_types::{
  CodeAction, CodeActionKind, CodeActionOrCommand, Diagnostic as LspDiagnostic, DiagnosticSeverity,
  DocumentChanges, NumberOrString, OneOf, OptionalVersionedTextDocumentIdentifier, Position, Range,
  TextDocumentEdit, TextEdit as LspTextEdit, Url, WorkspaceEdit,
};
use vue_vet_core::{
  ByteRange, Diagnostic, EditApplicability, EditPlan, Severity, SourceSpan, TextEdit,
};
use vue_vet_reporters::report_diagnostic_id;

/// Convert a Vue Vet finding into an LSP diagnostic.
///
/// `data` is a JSON object with opaque finding `id` (for `--explain`) and an
/// optional `recommendation` payload for practice suggestions. `code` is the
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
    data: Some(lsp_diagnostic_data(&id, diagnostic)),
  }
}

fn lsp_diagnostic_data(id: &str, diagnostic: &Diagnostic) -> serde_json::Value {
  let mut map = serde_json::Map::new();
  map.insert("id".into(), serde_json::Value::String(id.into()));
  if let Some(recommendation) = &diagnostic.recommendation
    && let Ok(value) = serde_json::to_value(recommendation)
  {
    map.insert("recommendation".into(), value);
  }
  serde_json::Value::Object(map)
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

/// Inputs for building safe quick-fix code actions for one open document.
pub struct SafeCodeActionRequest<'a> {
  pub uri: Url,
  pub version: i32,
  pub source: &'a str,
  pub root: &'a Path,
  pub document_path: &'a Path,
  pub analyzed_files: &'a [String],
  pub range: Range,
  pub only: Option<&'a [CodeActionKind]>,
}

/// Build LSP quick fixes from explicitly safe edits on active findings.
///
/// Unsafe edits are never exposed. Edits that leave the open document, fail
/// [`EditPlan`] validation, or fall outside the requested range are skipped.
/// The client applies the returned [`WorkspaceEdit`]; the server does not write.
#[must_use]
pub fn safe_code_actions(
  diagnostics: &[Diagnostic],
  request: &SafeCodeActionRequest<'_>,
) -> Vec<CodeActionOrCommand> {
  if !allows_quickfix(request.only) {
    return Vec::new();
  }
  let normalized_document = normalize_report_path(request.document_path, request.root);
  let mut actions = Vec::new();
  for diagnostic in diagnostics {
    if !diagnostic_matches_document(diagnostic, &normalized_document) {
      continue;
    }
    let diagnostic_range = span_to_range(&diagnostic.span, Some(request.source));
    if !ranges_intersect(request.range, diagnostic_range) {
      continue;
    }
    let safe_edits = diagnostic
      .edits
      .iter()
      .filter(|edit| edit.applicability == EditApplicability::Safe)
      .filter(|edit| edit_targets_document(edit, request.root, request.document_path))
      .cloned()
      .collect::<Vec<_>>();
    if safe_edits.is_empty() {
      continue;
    }
    let Ok(plan) = EditPlan::new(safe_edits) else {
      continue;
    };
    let Some(action) = code_action_from_plan(diagnostic, &plan, request) else {
      continue;
    };
    actions.push(CodeActionOrCommand::CodeAction(action));
  }
  actions
}

#[must_use]
pub fn byte_range_to_range(range: ByteRange, source: &str) -> Range {
  let (start_line, start_column) = line_column(source, range.offset);
  let end_offset = range.end().unwrap_or(range.offset);
  let (end_line, end_column) = line_column(source, end_offset);
  Range {
    start: Position { line: u32_line(start_line), character: u32_column(start_column) },
    end: Position { line: u32_line(end_line), character: u32_column(end_column) },
  }
}

fn code_action_from_plan(
  diagnostic: &Diagnostic,
  plan: &EditPlan,
  request: &SafeCodeActionRequest<'_>,
) -> Option<CodeAction> {
  let lsp_edits = plan
    .edits()
    .iter()
    .map(|edit| LspTextEdit {
      range: byte_range_to_range(edit.range, request.source),
      new_text: edit.replacement.clone(),
    })
    .collect::<Vec<_>>();
  if lsp_edits.is_empty() {
    return None;
  }
  let finding_id = report_diagnostic_id(diagnostic, request.analyzed_files);
  let lsp_diagnostic = to_lsp_diagnostic(diagnostic, request.analyzed_files, Some(request.source));
  Some(CodeAction {
    title: format!("Safe fix: {}", diagnostic.rule_id),
    kind: Some(CodeActionKind::QUICKFIX),
    diagnostics: Some(vec![lsp_diagnostic]),
    edit: Some(WorkspaceEdit {
      changes: None,
      document_changes: Some(DocumentChanges::Edits(vec![TextDocumentEdit {
        text_document: OptionalVersionedTextDocumentIdentifier {
          uri: request.uri.clone(),
          version: Some(request.version),
        },
        edits: lsp_edits.into_iter().map(OneOf::Left).collect(),
      }])),
      change_annotations: None,
    }),
    command: None,
    is_preferred: Some(true),
    disabled: None,
    data: Some(serde_json::Value::String(finding_id)),
  })
}

fn allows_quickfix(only: Option<&[CodeActionKind]>) -> bool {
  let Some(kinds) = only else {
    return true;
  };
  kinds.is_empty()
    || kinds.iter().any(|kind| {
      kind == &CodeActionKind::EMPTY
        || kind == &CodeActionKind::QUICKFIX
        || kind.as_str().starts_with("quickfix")
    })
}

fn diagnostic_matches_document(diagnostic: &Diagnostic, normalized_document: &str) -> bool {
  diagnostic.file.as_str() == normalized_document
}

fn edit_targets_document(edit: &TextEdit, root: &Path, document_path: &Path) -> bool {
  let edit_path =
    if edit.file.is_absolute() { edit.file.to_path_buf() } else { root.join(edit.file.as_path()) };
  paths_equal_lossy(&edit_path, document_path)
    || paths_equal_lossy(edit.file.as_path(), document_path)
    || normalize_report_path(document_path, root) == edit.file.as_str()
}

fn paths_equal_lossy(left: &Path, right: &Path) -> bool {
  left.to_string_lossy().replace('\\', "/") == right.to_string_lossy().replace('\\', "/")
}

fn normalize_report_path(path: &Path, root: &Path) -> String {
  path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

const fn ranges_intersect(left: Range, right: Range) -> bool {
  position_le(left.start, right.end) && position_le(right.start, left.end)
}

const fn position_le(left: Position, right: Position) -> bool {
  left.line < right.line || (left.line == right.line && left.character <= right.character)
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
  use std::path::PathBuf;
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
      file: PathBuf::from("basic.vue").into(),
      span: SourceSpan { offset: 19, length: 6, line: 2, column: 9 },
      edits: Vec::new(),
      recommendation: None,
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
    assert_eq!(
      lsp.data.as_ref().and_then(|value| value.get("id")).and_then(serde_json::Value::as_str),
      Some(id.as_str())
    );
  }

  #[test]
  #[expect(clippy::expect_used, reason = "unit test asserts Url::parse succeeds")]
  #[expect(clippy::indexing_slicing, reason = "unit test indexes known action shape")]
  #[expect(clippy::panic, reason = "unit test asserts code-action shape")]
  fn safe_code_actions_expose_only_safe_edits() {
    let source = "<template>\n  <input autofocus>\n</template>\n";
    let root = PathBuf::from("/project");
    let document = root.join("App.vue");
    let diagnostic = Diagnostic {
      rule_id: "vue-vet/accessibility/no-autofocus".into(),
      category: "accessibility".into(),
      severity: Severity::Warning,
      confidence: Some(Confidence::High),
      documentation: Some("rules/accessibility/no-autofocus".into()),
      message: "autofocus can disorient keyboard and screen-reader users".into(),
      help: None,
      file: PathBuf::from("App.vue").into(),
      span: SourceSpan { offset: 22, length: 9, line: 2, column: 10 },
      edits: vec![
        TextEdit {
          file: PathBuf::from("App.vue").into(),
          range: ByteRange { offset: 21, length: 10 },
          replacement: String::new(),
          applicability: EditApplicability::Safe,
          rule_id: "vue-vet/accessibility/no-autofocus".into(),
        },
        TextEdit {
          file: PathBuf::from("App.vue").into(),
          range: ByteRange { offset: 0, length: 0 },
          replacement: "// unsafe".into(),
          applicability: EditApplicability::Unsafe,
          rule_id: "vue-vet/accessibility/no-autofocus".into(),
        },
      ],
      recommendation: None,
    };
    let analyzed = vec!["App.vue".into()];
    let actions = safe_code_actions(
      std::slice::from_ref(&diagnostic),
      &SafeCodeActionRequest {
        uri: Url::parse("file:///project/App.vue").expect("url"),
        version: 3,
        source,
        root: &root,
        document_path: &document,
        analyzed_files: &analyzed,
        range: span_to_range(&diagnostic.span, Some(source)),
        only: None,
      },
    );
    assert_eq!(actions.len(), 1);
    let CodeActionOrCommand::CodeAction(action) = &actions[0] else {
      panic!("expected code action");
    };
    assert_eq!(action.kind.as_ref(), Some(&CodeActionKind::QUICKFIX));
    assert_eq!(action.is_preferred, Some(true));
    assert_eq!(
      action.data.as_ref().and_then(|value| value.as_str()),
      Some(report_diagnostic_id(&diagnostic, &analyzed).as_str())
    );
    let Some(WorkspaceEdit { document_changes: Some(DocumentChanges::Edits(edits)), .. }) =
      &action.edit
    else {
      panic!("expected versioned document edit");
    };
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].text_document.version, Some(3));
    assert_eq!(edits[0].edits.len(), 1);
    let OneOf::Left(edit) = &edits[0].edits[0] else {
      panic!("expected plain text edit");
    };
    assert_eq!(edit.new_text, "");
  }

  #[test]
  #[expect(clippy::expect_used, reason = "unit test asserts Url::parse succeeds")]
  fn safe_code_actions_respect_only_filter() {
    let source = "<template>\n  <input autofocus>\n</template>\n";
    let root = PathBuf::from("/project");
    let document = root.join("App.vue");
    let diagnostic = Diagnostic {
      rule_id: "vue-vet/accessibility/no-autofocus".into(),
      category: "accessibility".into(),
      severity: Severity::Warning,
      confidence: Some(Confidence::High),
      documentation: None,
      message: "autofocus".into(),
      help: None,
      file: PathBuf::from("App.vue").into(),
      span: SourceSpan { offset: 22, length: 9, line: 2, column: 10 },
      edits: vec![TextEdit {
        file: PathBuf::from("App.vue").into(),
        range: ByteRange { offset: 21, length: 10 },
        replacement: String::new(),
        applicability: EditApplicability::Safe,
        rule_id: "vue-vet/accessibility/no-autofocus".into(),
      }],
      recommendation: None,
    };
    let analyzed = vec!["App.vue".into()];
    let only = [CodeActionKind::REFACTOR];
    let actions = safe_code_actions(
      &[diagnostic],
      &SafeCodeActionRequest {
        uri: Url::parse("file:///project/App.vue").expect("url"),
        version: 1,
        source,
        root: &root,
        document_path: &document,
        analyzed_files: &analyzed,
        range: Range {
          start: Position { line: 0, character: 0 },
          end: Position { line: 10, character: 0 },
        },
        only: Some(only.as_slice()),
      },
    );
    assert!(actions.is_empty());
  }
}
