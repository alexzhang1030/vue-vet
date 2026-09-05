use std::path::PathBuf;

use vue_vet_session::{ChangeSet, ProjectSession, SessionOptions};

#[expect(clippy::panic, reason = "session fixture failures must fail the integration test")]
fn open(root: PathBuf, cache_dir: Option<PathBuf>) -> ProjectSession {
  let no_cache = cache_dir.is_none();
  match ProjectSession::open(SessionOptions {
    root,
    config_path: None,
    cache_dir,
    no_cache,
    threads: Some(1),
  }) {
    Ok(session) => session,
    Err(error) => panic!("session must open: {error}"),
  }
}

#[test]
#[expect(clippy::panic, reason = "session fixture failures must fail the integration test")]
fn overlay_and_single_file_discovery_stay_equivalent() {
  let root = std::env::temp_dir().join(format!("vue-vet-io-overlay-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  let nested = root.join("src/pages/deep");
  std::fs::create_dir_all(&nested).unwrap_or_else(|error| panic!("dirs: {error}"));
  std::fs::write(root.join("package.json"), r#"{"name":"app"}"#)
    .unwrap_or_else(|error| panic!("package: {error}"));
  std::fs::write(root.join("Existing.vue"), "<template><main /></template>")
    .unwrap_or_else(|error| panic!("existing: {error}"));
  let file = nested.join("index.tsx");
  std::fs::write(&file, "export const ok = 1\n").unwrap_or_else(|error| panic!("file: {error}"));

  let session = open(root.clone(), None);
  session
    .apply_changes(ChangeSet::upsert(
      root.join("NewComponent.vue"),
      "<template><main v-html=\"html\" /></template>".into(),
    ))
    .unwrap_or_else(|error| panic!("overlay: {error}"));
  let overlay = session.analyze().unwrap_or_else(|error| panic!("overlay analyze: {error}"));
  assert!(
    overlay.summary.diagnostics.iter().any(|diagnostic| {
      diagnostic.file.as_str() == "NewComponent.vue" && diagnostic.rule_id.contains("no-v-html")
    }),
    "overlay-only unsaved files must be analyzed"
  );

  let file_session = open(file, None);
  let snapshot = file_session.analyze().unwrap_or_else(|error| panic!("file analyze: {error}"));
  assert!(
    snapshot.analyzed_files.iter().any(|path| path == "src/pages/deep/index.tsx"),
    "single-file scan must keep package-relative ids: {:?}",
    snapshot.analyzed_files
  );
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session fixture failures must fail the integration test")]
fn cold_and_warm_cache_results_are_equivalent() {
  let root = std::env::temp_dir().join(format!("vue-vet-io-cache-{}", std::process::id()));
  let cache_dir = root.join(".vue-vet-cache");
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  std::fs::write(root.join("App.vue"), "<template><main v-html=\"html\" /></template>")
    .unwrap_or_else(|error| panic!("vue: {error}"));

  let cold = open(root.clone(), Some(cache_dir.clone()));
  let cold_snap = cold.analyze().unwrap_or_else(|error| panic!("cold: {error}"));
  assert_eq!(cold_snap.cache_status, "miss", "first scan must miss");

  let warm = open(root.clone(), Some(cache_dir));
  let warm_snap = warm.analyze().unwrap_or_else(|error| panic!("warm: {error}"));
  assert_eq!(warm_snap.cache_status, "hit", "second scan must hit");
  assert_eq!(warm_snap.summary, cold_snap.summary, "warm diagnostics must equal cold");
  assert_eq!(warm_snap.graph, cold_snap.graph, "warm graph must equal cold");
  let _ignored = std::fs::remove_dir_all(root);
}
