use std::{collections::BTreeMap, path::PathBuf};

use vue_vet_core::{FileId, finding_id};
use vue_vet_session::{ChangeSet, ProjectSession, SessionOptions};

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
    let id = finding_id(diagnostic);
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

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn syntax_errors_are_partial_results() {
  let root = std::env::temp_dir().join(format!("vue-vet-partial-{}", std::process::id()));
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("temp workspace: {error}"));
  std::fs::write(root.join("Good.vue"), "<template><main v-html=\"html\" /></template>")
    .unwrap_or_else(|error| panic!("good fixture: {error}"));
  std::fs::write(root.join("Broken.vue"), "<script setup>const = ;</script>")
    .unwrap_or_else(|error| panic!("broken fixture: {error}"));
  let session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(2),
  })
  .unwrap_or_else(|error| panic!("session: {error}"));
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("partial analyze: {error}"));
  assert!(!snapshot.complete());
  assert_eq!(snapshot.issues.len(), 1);
  assert!(snapshot.summary.diagnostics.iter().any(|diagnostic| {
    diagnostic.file == FileId::from("Good.vue") && diagnostic.rule_id.contains("no-v-html")
  }));
  assert!(snapshot.summary.diagnostics.iter().any(|diagnostic| {
    diagnostic.file == FileId::from("Broken.vue")
      && diagnostic.rule_id == "vue-vet/analysis/parse-error"
  }));
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn affected_analysis_reuses_facts_and_invalidates_reverse_dependencies() {
  let root = std::env::temp_dir().join(format!("vue-vet-incremental-{}", std::process::id()));
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("temp workspace: {error}"));
  let child = root.join("Child.vue");
  std::fs::write(&child, "<template><span>one</span></template>")
    .unwrap_or_else(|error| panic!("child fixture: {error}"));
  std::fs::write(
    root.join("App.vue"),
    "<script setup>import Child from './Child.vue'</script><template><Child /></template>",
  )
  .unwrap_or_else(|error| panic!("app fixture: {error}"));
  let session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(2),
  })
  .unwrap_or_else(|error| panic!("session: {error}"));
  session.analyze().unwrap_or_else(|error| panic!("initial analyze: {error}"));
  session
    .apply_changes(ChangeSet::upsert(child.clone(), "<template><span>two</span></template>".into()))
    .unwrap_or_else(|error| panic!("apply change: {error}"));
  session.analyze_affected().unwrap_or_else(|error| panic!("affected analyze: {error}"));
  let affected = session.affected_files().unwrap_or_else(|error| panic!("affected files: {error}"));
  assert!(affected.contains(&FileId::from("Child.vue")));
  assert!(affected.contains(&FileId::from("App.vue")));
  std::fs::remove_file(&child).unwrap_or_else(|error| panic!("remove child fixture: {error}"));
  session
    .apply_changes(ChangeSet::remove(child))
    .unwrap_or_else(|error| panic!("remove overlay: {error}"));
  session.analyze_affected().unwrap_or_else(|error| panic!("removed-file analyze: {error}"));
  let affected = session.affected_files().unwrap_or_else(|error| panic!("affected files: {error}"));
  assert!(affected.contains(&FileId::from("Child.vue")));
  assert!(affected.contains(&FileId::from("App.vue")));
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn duplicate_suffix_paths_keep_distinct_file_ids() {
  let root = std::env::temp_dir().join(format!("vue-vet-file-id-{}", std::process::id()));
  for directory in ["apps/admin/src", "apps/customer/src"] {
    std::fs::create_dir_all(root.join(directory))
      .unwrap_or_else(|error| panic!("temp workspace: {error}"));
    std::fs::write(
      root.join(directory).join("App.vue"),
      "<template><main v-html=\"html\" /></template>",
    )
    .unwrap_or_else(|error| panic!("fixture: {error}"));
  }
  let session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(2),
  })
  .unwrap_or_else(|error| panic!("session: {error}"));
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  let files = snapshot
    .summary
    .diagnostics
    .iter()
    .filter(|diagnostic| diagnostic.rule_id.contains("no-v-html"))
    .map(|diagnostic| diagnostic.file.clone())
    .collect::<std::collections::BTreeSet<_>>();
  assert_eq!(
    files,
    [FileId::from("apps/admin/src/App.vue"), FileId::from("apps/customer/src/App.vue"),]
      .into_iter()
      .collect::<std::collections::BTreeSet<_>>()
  );
  let _ignored = std::fs::remove_dir_all(root);
}
