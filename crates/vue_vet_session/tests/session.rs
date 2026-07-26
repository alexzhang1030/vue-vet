use std::{collections::BTreeMap, path::PathBuf};

use vue_vet_reporters::report_diagnostic_id;
use vue_vet_session::{ProjectSession, SessionOptions};

fn fixture(name: &str) -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn analyze_diagnostic_ids_match_report_identity() {
  let root = fixture("rules/no-v-html/invalid/basic.vue");
  let Ok(session) = ProjectSession::open(SessionOptions {
    root,
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

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn explain_rule_loads_documentation_without_scan_diagnostics() {
  let Ok(session) = ProjectSession::open(SessionOptions {
    root: fixture("rules/no-v-html/invalid/basic.vue"),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  }) else {
    panic!("session must open");
  };
  let Ok(explain) = session.explain_rule("vue-vet/security/no-v-html") else {
    panic!("rule explain");
  };
  assert_eq!(explain.rule_id, "vue-vet/security/no-v-html");
  assert!(explain.body.as_deref().is_some_and(|body| body.contains("v-html")));
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn analyze_with_overlays_uses_unsaved_buffer_source() {
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
  let Ok(disk) = session.analyze() else {
    panic!("disk analyze must succeed");
  };
  assert!(
    disk.summary.diagnostics.iter().any(|diagnostic| diagnostic.rule_id.contains("no-v-html")),
    "disk fixture must report no-v-html"
  );

  let clean = "<template>\n  <main>{{ html }}</main>\n</template>\n";
  let mut overlays = BTreeMap::new();
  overlays.insert(root, clean.into());
  let Ok(overlay) = session.analyze_with_overlays(&overlays) else {
    panic!("overlay analyze must succeed");
  };
  assert!(
    !overlay.summary.diagnostics.iter().any(|diagnostic| diagnostic.rule_id.contains("no-v-html")),
    "unsaved buffer without v-html must clear the finding"
  );
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn resolve_workspace_path_rejects_escape() {
  let Ok(session) = ProjectSession::open(SessionOptions {
    root: fixture("rules/no-v-html/invalid"),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  }) else {
    panic!("session must open");
  };
  let Ok(inside) = session.resolve_workspace_path(std::path::Path::new("basic.vue")) else {
    panic!("inside path must resolve");
  };
  assert!(inside.ends_with("basic.vue"));
  let Err(error) = session.resolve_workspace_path(std::path::Path::new("../secret")) else {
    panic!("parent escape must fail");
  };
  assert!(error.to_string().contains("escapes"));
}
