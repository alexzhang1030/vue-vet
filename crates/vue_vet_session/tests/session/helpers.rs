use std::path::PathBuf;

pub use std::collections::BTreeMap;
pub use vue_vet_core::{FileId, finding_id};
pub use vue_vet_session::{
  AnalysisProduct, AnalysisSnapshot, ChangeSet, ProjectSession, SessionOptions,
};

pub fn fixture(name: &str) -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
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
