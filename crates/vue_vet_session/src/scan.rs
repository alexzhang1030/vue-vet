use std::{collections::BTreeSet, path::Path, sync::Arc};

use vue_vet_cache::{CacheLookup, CachePayload, CacheStore, content_key};
use vue_vet_config::Config;
use vue_vet_core::{FileId, ScanSummary};
use vue_vet_project::ProjectGraph;

use crate::{
  AnalysisCoverage, AnalysisIssue, AnalysisSnapshot, SessionError,
  discovery::WorkspaceInputSnapshot, pipeline::scan_with_threads,
};

pub use crate::pipeline::AnalysisState;

#[expect(
  clippy::too_many_arguments,
  reason = "analyze_snapshot forwards the explicit analysis lifecycle inputs"
)]
pub fn analyze_snapshot(
  input: &WorkspaceInputSnapshot,
  config: &Config,
  cache_dir: &Path,
  no_cache: bool,
  pool: impl FnOnce() -> Result<Option<Arc<rayon::ThreadPool>>, SessionError>,
  previous: &AnalysisState,
  state: &mut AnalysisState,
  cancelled: &(dyn Fn() -> bool + Sync),
  dirty_files: &BTreeSet<FileId>,
  force_full_parse: bool,
) -> Result<AnalysisSnapshot, SessionError> {
  if cancelled() {
    return Err(SessionError::Cancelled);
  }
  let analyzed_files: Arc<[String]> = input
    .analyzed_source_files
    .iter()
    .map(|file| file.as_str().to_owned())
    .collect::<Vec<_>>()
    .into();
  let (summary, graph, cache_status, issues, work) = if no_cache {
    let result = scan_with_threads(
      input,
      config,
      pool()?,
      previous,
      state,
      cancelled,
      dirty_files,
      force_full_parse,
    )?;
    (result.summary, result.graph, "disabled", result.issues, result.work)
  } else {
    let serialized_config = serde_json::to_vec(config)
      .map_err(|error| SessionError::message(format!("failed to hash config: {error}")))?;
    let key = content_key(&input.cache_inputs, &serialized_config);
    let store = CacheStore::new(cache_dir.to_path_buf());
    match store.load(&key) {
      // Preserve committed incremental state on hit — do not clear file/module IR.
      // Never hydrate eagerly here: warm disk hits must stay cache-load cheap
      // (CodSpeed `scan_warm_*`, CLI re-scan). Empty IR is fine — the next
      // real dirty analyze uses `force_full_parse` via `!has_file_facts()` and
      // seeds facts then; subsequent edits stay incremental.
      CacheLookup::Hit(payload) => {
        *state = AnalysisState::share_from(previous);
        (payload.summary, payload.graph, "hit", Vec::new(), state.last_work)
      }
      CacheLookup::Miss => fill_cache(
        &store,
        &key,
        input,
        config,
        "miss",
        pool()?,
        previous,
        state,
        cancelled,
        dirty_files,
        force_full_parse,
      )?,
      CacheLookup::RecoveredCorruption => fill_cache(
        &store,
        &key,
        input,
        config,
        "recovered-corruption",
        pool()?,
        previous,
        state,
        cancelled,
        dirty_files,
        force_full_parse,
      )?,
    }
  };
  let coverage = Arc::new(AnalysisCoverage {
    analyzed_source_files: input.analyzed_source_files.clone(),
    invalidation_inputs: graph.invalidation_inputs.clone(),
  });
  Ok(AnalysisSnapshot {
    summary: Arc::new(summary),
    graph: Arc::new(graph),
    cache_status,
    coverage,
    issues: issues.into(),
    analyzed_files,
    work,
  })
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
  pool: Option<Arc<rayon::ThreadPool>>,
  previous: &AnalysisState,
  state: &mut AnalysisState,
  cancelled: &(dyn Fn() -> bool + Sync),
  dirty_files: &BTreeSet<FileId>,
  force_full_parse: bool,
) -> Result<
  (ScanSummary, ProjectGraph, &'static str, Vec<AnalysisIssue>, crate::ScanWorkCounters),
  SessionError,
> {
  let result = scan_with_threads(
    input,
    config,
    pool,
    previous,
    state,
    cancelled,
    dirty_files,
    force_full_parse,
  )?;
  if result.issues.is_empty() {
    store
      .store(key, &CachePayload { summary: result.summary.clone(), graph: result.graph.clone() })
      .map_err(|error| SessionError::message(error.to_string()))?;
  }
  Ok((result.summary, result.graph, status, result.issues, result.work))
}

/// Directory used as the project boundary for a file or directory scan path.
#[must_use]
pub fn scan_directory(path: &Path) -> &Path {
  if path.is_dir() { path } else { path.parent().unwrap_or(path) }
}
