use std::path::PathBuf;

pub use std::collections::BTreeMap;
pub use vue_vet_core::{FileId, finding_id};
pub use vue_vet_session::{
  AnalysisProduct, AnalysisSnapshot, ChangeSet, ProjectSession, SessionOptions,
};

pub fn fixture(name: &str) -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
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
