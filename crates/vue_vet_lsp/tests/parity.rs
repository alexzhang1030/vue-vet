use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
};

use tower_lsp::lsp_types::{
  CodeActionKind, CodeActionOrCommand, DocumentChanges, OneOf, Position, Range, Url,
};
use vue_vet_lsp::{
  SafeCodeActionRequest, is_current_generation, safe_code_actions, to_lsp_diagnostic,
};
use vue_vet_reporters::report_diagnostic_id;
use vue_vet_session::{ProjectSession, SessionOptions};

fn fixture(name: &str) -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

#[test]
#[expect(clippy::panic, reason = "parity setup failures must fail the integration test")]
fn lsp_diagnostics_carry_cli_finding_ids() {
  let root = fixture("rules/no-v-html/invalid/basic.vue");
  let Ok(session) = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  }) else {
    panic!("session must open");
  };
  let Ok(snapshot) = session.analyze() else {
    panic!("analyze must succeed");
  };
  let Ok(source) = std::fs::read_to_string(&root) else {
    panic!("fixture source must read");
  };
  assert!(!snapshot.summary.diagnostics.is_empty(), "fixture must emit findings");
  for diagnostic in &snapshot.summary.diagnostics {
    let expected = report_diagnostic_id(diagnostic, &snapshot.analyzed_files);
    let lsp = to_lsp_diagnostic(diagnostic, &snapshot.analyzed_files, Some(&source));
    assert_eq!(lsp.data, Some(serde_json::Value::String(expected)));
    assert_eq!(lsp.source.as_deref(), Some("vue-vet"));
  }
}

#[test]
#[expect(clippy::panic, reason = "parity setup failures must fail the integration test")]
fn overlay_snapshot_finding_ids_match_cli_identity() {
  let root = fixture("rules/no-v-html/invalid/basic.vue");
  let Ok(session) = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  }) else {
    panic!("session must open");
  };
  let dirty = "<template>\n  <div v-html=\"x\"></div>\n</template>\n";
  let mut overlays = BTreeMap::new();
  overlays.insert(root, dirty.into());
  let Ok(snapshot) = session.analyze_with_overlays(&overlays) else {
    panic!("overlay analyze must succeed");
  };
  assert!(!snapshot.summary.diagnostics.is_empty(), "overlay must emit findings");
  for diagnostic in &snapshot.summary.diagnostics {
    let expected = report_diagnostic_id(diagnostic, &snapshot.analyzed_files);
    let lsp = to_lsp_diagnostic(diagnostic, &snapshot.analyzed_files, Some(dirty));
    assert_eq!(lsp.data, Some(serde_json::Value::String(expected)));
  }
  assert!(is_current_generation(Some(1), 1));
  assert!(!is_current_generation(Some(2), 1));
}

#[test]
#[expect(clippy::expect_used, reason = "parity setup failures must fail the integration test")]
#[expect(clippy::indexing_slicing, reason = "parity test indexes known action shape")]
#[expect(clippy::panic, reason = "parity setup failures must fail the integration test")]
fn safe_code_actions_match_autofocus_producer() {
  let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("vue-vet-lsp-safe-code-action");
  fs::create_dir_all(&dir).expect("temp dir");
  let path = dir.join("App.vue");
  let source = "<template>\n  <input autofocus>\n</template>\n";
  fs::write(&path, source).expect("write fixture");

  let Ok(session) = ProjectSession::open(SessionOptions {
    root: path.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  }) else {
    panic!("session must open");
  };
  let Ok(snapshot) = session.analyze() else {
    panic!("analyze must succeed");
  };
  let diagnostic = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| diagnostic.rule_id.contains("no-autofocus"))
    .expect("autofocus finding");
  assert!(
    diagnostic
      .edits
      .iter()
      .any(|edit| { matches!(edit.applicability, vue_vet_core::EditApplicability::Safe) }),
    "autofocus boolean removal must be safe"
  );

  let uri = path_to_file_url(&path);
  let actions = safe_code_actions(
    &snapshot.summary.diagnostics,
    &SafeCodeActionRequest {
      uri,
      version: 1,
      source,
      root: path.parent().unwrap_or_else(|| Path::new(".")),
      document_path: &path,
      analyzed_files: &snapshot.analyzed_files,
      range: Range {
        start: Position { line: 0, character: 0 },
        end: Position { line: 10, character: 0 },
      },
      only: Some(&[CodeActionKind::QUICKFIX]),
    },
  );
  assert_eq!(actions.len(), 1);
  let CodeActionOrCommand::CodeAction(action) = &actions[0] else {
    panic!("expected code action");
  };
  assert_eq!(
    action.data,
    Some(serde_json::Value::String(report_diagnostic_id(diagnostic, &snapshot.analyzed_files)))
  );
  let Some(DocumentChanges::Edits(edits)) =
    action.edit.as_ref().and_then(|edit| edit.document_changes.as_ref())
  else {
    panic!("expected versioned edits");
  };
  let OneOf::Left(text_edit) = &edits[0].edits[0] else {
    panic!("expected plain text edit");
  };
  assert_eq!(text_edit.new_text, "");
}

#[expect(clippy::expect_used, reason = "test helper builds a file URL")]
fn path_to_file_url(path: &Path) -> Url {
  Url::from_file_path(path).unwrap_or_else(|()| Url::parse("file:///App.vue").expect("fallback"))
}
