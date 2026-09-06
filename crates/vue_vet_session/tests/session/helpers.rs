use std::path::PathBuf;

pub use std::collections::BTreeMap;
pub use vue_vet_core::{FileId, finding_id};
pub use vue_vet_session::{
  AnalysisProduct, AnalysisSnapshot, ChangeSet, ProjectSession, SessionOptions,
};

pub fn fixture(name: &str) -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

/// Copy the committed `module-seeds` Vue package stub so fixture scans resolve `vue`.
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
pub fn install_module_seeds_vue_stub(root: impl AsRef<std::path::Path>) {
  let root = root.as_ref();
  let stub = fixture("projects/module-seeds/node_modules/vue");
  let dest = root.join("node_modules/vue");
  std::fs::create_dir_all(&dest).unwrap_or_else(|error| panic!("vue stub dir: {error}"));
  for name in ["package.json", "index.js"] {
    std::fs::copy(stub.join(name), dest.join(name))
      .unwrap_or_else(|error| panic!("vue stub {name}: {error}"));
  }
  std::fs::write(root.join("package.json"), r#"{"dependencies":{"vue":"3.5.0"}}"#)
    .unwrap_or_else(|error| panic!("package.json: {error}"));
}

pub fn open_session(root: impl Into<PathBuf>) -> ProjectSession {
  open_session_threads(root, 1)
}

#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
pub fn open_session_threads(root: impl Into<PathBuf>, threads: usize) -> ProjectSession {
  match ProjectSession::open(SessionOptions {
    root: root.into(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(threads),
  }) {
    Ok(session) => session,
    Err(error) => panic!("session must open: {error}"),
  }
}

pub fn assert_analysis_parity(incremental: &AnalysisSnapshot, clean: &AnalysisSnapshot) {
  assert_eq!(incremental.summary, clean.summary, "incremental diagnostics must equal clean");
  assert_eq!(incremental.graph, clean.graph, "incremental graph must equal clean");
  assert_eq!(incremental.coverage, clean.coverage, "incremental coverage must equal clean");
  assert_eq!(incremental.issues, clean.issues, "incremental issues must equal clean");
  assert_eq!(
    incremental.analyzed_files, clean.analyzed_files,
    "incremental analyzed file identities must equal clean"
  );
}
