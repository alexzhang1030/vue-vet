use std::path::PathBuf;

use vue_vet_lsp::to_lsp_diagnostic;
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
