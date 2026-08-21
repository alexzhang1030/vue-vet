use super::helpers::*;
use vue_vet_reporters::report_diagnostic_id;

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn analyze_diagnostic_ids_match_report_identity() {
  let session = open_session(fixture("rules/no-v-html/invalid/basic.vue"));
  let Ok(snapshot) = session.analyze() else {
    panic!("analyze must succeed");
  };
  assert!(!snapshot.summary.diagnostics.is_empty(), "fixture must emit at least one finding");
  for diagnostic in &snapshot.summary.diagnostics {
    let id = report_diagnostic_id(diagnostic, &snapshot.analyzed_files);
    assert!(id.contains("vue-vet/security/no-v-html"), "opaque id must include rule: {id}");
    let Ok(explained) = session.explain_finding(&id) else {
      panic!("finding explain must match analyze");
    };
    assert_eq!(explained.id, id);
    assert!(explained.message.contains("v-html"));
  }
}
