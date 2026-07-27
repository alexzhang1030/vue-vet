use std::path::Path;

use vue_vet_cache::{CacheLookup, CachePayload, CacheStore, content_key};
use vue_vet_config::Config;
use vue_vet_core::ScanSummary;
use vue_vet_project::ProjectGraph;

use crate::{
  AnalysisCoverage, AnalysisIssue, AnalysisSnapshot, SessionError,
  discovery::WorkspaceInputSnapshot, pipeline::scan_with_threads,
};

pub use crate::pipeline::AnalysisState;

pub fn analyze_snapshot(
  input: &WorkspaceInputSnapshot,
  config: &Config,
  cache_dir: &Path,
  no_cache: bool,
  threads: Option<usize>,
  state: &mut AnalysisState,
  cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<AnalysisSnapshot, SessionError> {
  if cancelled() {
    return Err(SessionError::Cancelled);
  }
  let analyzed_files =
    input.analyzed_source_files.iter().map(|file| file.as_str().to_owned()).collect();
  let (summary, graph, cache_status, issues) = if no_cache {
    let result = scan_with_threads(input, config, threads, state, cancelled)?;
    (result.summary, result.graph, "disabled", result.issues)
  } else {
    let serialized_config = serde_json::to_vec(config)
      .map_err(|error| SessionError::message(format!("failed to hash config: {error}")))?;
    let key = content_key(&input.cache_inputs, &serialized_config);
    let store = CacheStore::new(cache_dir.to_path_buf());
    match store.load(&key) {
      CacheLookup::Hit(payload) => (payload.summary, payload.graph, "hit", Vec::new()),
      CacheLookup::Miss => {
        fill_cache(&store, &key, input, config, "miss", threads, state, cancelled)?
      }
      CacheLookup::RecoveredCorruption => {
        fill_cache(&store, &key, input, config, "recovered-corruption", threads, state, cancelled)?
      }
    }
  };
  let coverage = AnalysisCoverage {
    analyzed_source_files: input.analyzed_source_files.clone(),
    invalidation_inputs: graph.invalidation_inputs.clone(),
  };
  Ok(AnalysisSnapshot { summary, graph, cache_status, coverage, issues, analyzed_files })
}

#[expect(
  clippy::too_many_arguments,
  reason = "cache fill forwards the explicit analysis lifecycle inputs"
)]
fn fill_cache(
  store: &CacheStore,
  key: &str,
  input: &WorkspaceInputSnapshot,
  config: &Config,
  status: &'static str,
  threads: Option<usize>,
  state: &mut AnalysisState,
  cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<(ScanSummary, ProjectGraph, &'static str, Vec<AnalysisIssue>), SessionError> {
  let result = scan_with_threads(input, config, threads, state, cancelled)?;
  if result.issues.is_empty() {
    store
      .store(key, &CachePayload { summary: result.summary.clone(), graph: result.graph.clone() })
      .map_err(|error| SessionError::message(error.to_string()))?;
  }
  Ok((result.summary, result.graph, status, result.issues))
}

/// Directory used as the project boundary for a file or directory scan path.
#[must_use]
pub fn scan_directory(path: &Path) -> &Path {
  if path.is_dir() { path } else { path.parent().unwrap_or(path) }
}
