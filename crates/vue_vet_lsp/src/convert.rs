//! Map Vue Vet diagnostics onto LSP types without changing identity semantics.

use tower_lsp::lsp_types::{
  CodeAction, CodeActionKind, CodeActionOrCommand, Diagnostic as LspDiagnostic, DiagnosticSeverity,
  DocumentChanges, Hover, HoverContents, MarkupContent, MarkupKind, NumberOrString, OneOf,
  OptionalVersionedTextDocumentIdentifier, Position, Range, TextDocumentEdit,
  TextEdit as LspTextEdit, Url, WorkspaceEdit,
};
use vue_vet_core::{
  ByteRange, Diagnostic, EditApplicability, EditPlan, FileId, LineIndex, ScopeExplain, Severity,
  SourceSpan,
};
use vue_vet_reporters::{render_scope_explains_markdown, report_diagnostic_id};

/// Convert a Vue Vet finding into an LSP diagnostic.
///
/// `data` is a JSON object with opaque finding `id` (for `--explain`) and an
/// optional `recommendation` payload for practice suggestions. `code` is the
/// stable rule id. Positions are 0-based LSP UTF-16 columns derived from source
/// byte offsets via [`LineIndex`].
#[must_use]
pub fn to_lsp_diagnostic(
  diagnostic: &Diagnostic,
  analyzed_files: &[String],
  source: Option<&str>,
) -> LspDiagnostic {
  to_lsp_diagnostic_with_index(diagnostic, analyzed_files, source, None)
}

/// Like [`to_lsp_diagnostic`], reusing a precomputed [`LineIndex`] when available.
#[must_use]
pub fn to_lsp_diagnostic_with_index(
  diagnostic: &Diagnostic,
  analyzed_files: &[String],
  source: Option<&str>,
  line_index: Option<&LineIndex>,
) -> LspDiagnostic {
  let id = report_diagnostic_id(diagnostic, analyzed_files);
  LspDiagnostic {
    range: span_to_range_with_index(&diagnostic.span, source, line_index),
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
  span_to_range_with_index(span, source, None)
}

#[must_use]
pub fn span_to_range_with_index(
  span: &SourceSpan,
  source: Option<&str>,
  line_index: Option<&LineIndex>,
) -> Range {
  source.map_or_else(
    || {
      // Without source text, fall back to Vue Vet's 1-based line/byte-column
      // fields (ASCII-compatible only).
      let start = Position { line: u32_line(span.line), character: u32_column(span.column) };
      Range {
        start,
        end: Position {
          line: start.line,
          character: start.character.saturating_add(u32::try_from(span.length).unwrap_or(u32::MAX)),
        },
      }
    },
    |source| {
      line_index.map_or_else(
        || {
          let index = LineIndex::new(source);
          range_from_line_index(span, source, &index)
        },
        |index| range_from_line_index(span, source, index),
      )
    },
  )
}

fn range_from_line_index(span: &SourceSpan, source: &str, index: &LineIndex) -> Range {
  let (start_line, start_character) = index.byte_to_utf16(source, span.offset);
  let end_offset = span.offset.saturating_add(span.length);
  let (end_line, end_character) = index.byte_to_utf16(source, end_offset);
  Range {
    start: Position { line: start_line, character: start_character },
    end: Position { line: end_line, character: end_character },
  }
}

/// Inputs for building safe quick-fix code actions for one open document.
pub struct SafeCodeActionRequest<'a> {
  pub uri: Url,
  pub version: i32,
  pub source: &'a str,
  pub document_file_id: &'a FileId,
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
  let mut actions = Vec::new();
  for diagnostic in diagnostics {
    if diagnostic.file != *request.document_file_id {
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
      .filter(|edit| edit.file == *request.document_file_id)
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

/// Convert a 0-based LSP UTF-16 position into a UTF-8 byte offset.
#[must_use]
pub fn position_to_byte(source: &str, line_index: &LineIndex, position: Position) -> Option<usize> {
  line_index.utf16_to_byte(source, position.line, position.character)
}

/// `--explain-scope` query for a caret in `file_id` (start-exact, else covering).
#[must_use]
pub fn explain_scope_query(file_id: &FileId, offset: usize) -> String {
  format!("{}:@{offset}", file_id.as_str())
}

/// Hover payload from session `ScopeExplain` facts (same markdown as reporters).
#[must_use]
pub fn hover_from_scope_explains(
  explains: &[ScopeExplain],
  source: &str,
  line_index: Option<&LineIndex>,
) -> Option<Hover> {
  if explains.is_empty() {
    return None;
  }
  let range = explains
    .first()
    .map(|explain| span_to_range_with_index(&explain.span, Some(source), line_index));
  Some(Hover {
    contents: HoverContents::Markup(MarkupContent {
      kind: MarkupKind::Markdown,
      value: render_scope_explains_markdown(explains),
    }),
    range,
  })
}

#[must_use]
pub fn byte_range_to_range(range: ByteRange, source: &str) -> Range {
  let index = LineIndex::new(source);
  let (start_line, start_character) = index.byte_to_utf16(source, range.offset);
  let end_offset = range.end().unwrap_or(range.offset);
  let (end_line, end_character) = index.byte_to_utf16(source, end_offset);
  Range {
    start: Position { line: start_line, character: start_character },
    end: Position { line: end_line, character: end_character },
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

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::PathBuf;

  use vue_vet_core::{Confidence, SourceSpan, TextEdit};

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
  #[expect(clippy::panic, reason = "unit test asserts unicode fixture layout")]
  fn unicode_prefix_uses_utf16_columns() {
    let source = "<template>\n  <div>中文😀</div>\n  <main v-html=\"html\" />\n</template>\n";
    let Some(offset) = source.find("v-html") else {
      panic!("fixture must contain v-html");
    };
    let diagnostic = Diagnostic {
      rule_id: "vue-vet/security/no-v-html".into(),
      category: "security".into(),
      severity: Severity::Warning,
      confidence: Some(Confidence::High),
      documentation: None,
      message: "v-html".into(),
      help: None,
      file: PathBuf::from("App.vue").into(),
      span: SourceSpan { offset, length: 6, line: 3, column: offset },
      edits: vec![TextEdit {
        file: PathBuf::from("App.vue").into(),
        range: ByteRange { offset, length: 6 },
        replacement: "text-content".into(),
        applicability: EditApplicability::Safe,
        rule_id: "vue-vet/security/no-v-html".into(),
      }],
      recommendation: None,
    };
    let range = span_to_range(&diagnostic.span, Some(source));
    assert_eq!(range.start.line, 2);
    assert_eq!(range.start.character, 8);
    let analyzed = vec!["App.vue".into()];
    let file_id = FileId::from("App.vue");
    let actions = safe_code_actions(
      std::slice::from_ref(&diagnostic),
      &SafeCodeActionRequest {
        uri: Url::parse("file:///project/App.vue").expect("url"),
        version: 1,
        source,
        document_file_id: &file_id,
        analyzed_files: &analyzed,
        range,
        only: None,
      },
    );
    assert_eq!(actions.len(), 1);
  }

  #[test]
  #[expect(clippy::expect_used, reason = "unit test asserts Url::parse succeeds")]
  #[expect(clippy::indexing_slicing, reason = "unit test indexes known action shape")]
  #[expect(clippy::panic, reason = "unit test asserts code-action shape")]
  fn safe_code_actions_expose_only_safe_edits() {
    let source = "<template>\n  <input autofocus>\n</template>\n";
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
    let file_id = FileId::from("App.vue");
    let actions = safe_code_actions(
      std::slice::from_ref(&diagnostic),
      &SafeCodeActionRequest {
        uri: Url::parse("file:///project/App.vue").expect("url"),
        version: 3,
        source,
        document_file_id: &file_id,
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
    let file_id = FileId::from("App.vue");
    let only = [CodeActionKind::REFACTOR];
    let actions = safe_code_actions(
      &[diagnostic],
      &SafeCodeActionRequest {
        uri: Url::parse("file:///project/App.vue").expect("url"),
        version: 1,
        source,
        document_file_id: &file_id,
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

  #[test]
  #[expect(clippy::panic, reason = "unit test asserts unicode fixture layout")]
  fn position_to_byte_round_trips_utf16_prefix() {
    let source = "<template>\n  <div>中文😀</div>\n  <main v-html=\"html\" />\n</template>\n";
    let index = LineIndex::new(source);
    let Some(offset) = source.find("v-html") else {
      panic!("fixture must contain v-html");
    };
    let (line, character) = index.byte_to_utf16(source, offset);
    assert_eq!(position_to_byte(source, &index, Position { line, character }), Some(offset));
  }

  #[test]
  #[expect(clippy::panic, reason = "unit test asserts hover markdown shape")]
  fn hover_from_scope_explains_uses_markdown_and_span_range() {
    let source = "const label = computed(() => 'static')\n";
    let explain = vue_vet_core::ScopeExplain {
      module_id: "App.vue".into(),
      kind: "computed".into(),
      callee: "computed".into(),
      binding: Some("label".into()),
      span: SourceSpan { offset: 14, length: 24, line: 1, column: 15 },
      summary:
        "`label` has no known reactive dependency — Vue will not re-run it when state changes"
          .into(),
      tracks: Vec::new(),
      does_not_track: Vec::new(),
      uncertain: Vec::new(),
    };
    let hover = hover_from_scope_explains(std::slice::from_ref(&explain), source, None);
    let Some(Hover { contents: HoverContents::Markup(markup), range: Some(range) }) = hover else {
      panic!("expected markdown hover with range");
    };
    assert_eq!(markup.kind, MarkupKind::Markdown);
    assert!(markup.value.contains("## label"));
    assert!(markup.value.contains("no known reactive dependency"));
    assert_eq!(range.start.line, 0);
    assert_eq!(range.start.character, 14);
    assert!(hover_from_scope_explains(&[], source, None).is_none());
    assert_eq!(explain_scope_query(&FileId::from("App.vue"), 20), "App.vue:@20");
  }
}
