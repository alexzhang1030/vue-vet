use std::sync::{Arc, Mutex};

use super::helpers::*;
use vue_vet_session::{ProgressEvent, ProgressReporter, SessionOptions};

const fn event_kind(event: &ProgressEvent) -> &'static str {
  match event {
    ProgressEvent::Discovering => "discovering",
    ProgressEvent::CheckingCache => "checking-cache",
    ProgressEvent::CacheHit => "cache-hit",
    ProgressEvent::SavingCache => "saving-cache",
    ProgressEvent::Parsing { .. } => "parsing",
    ProgressEvent::BuildingGraph => "building-graph",
    ProgressEvent::LoadingExternalSeeds { .. } => "resolving",
    ProgressEvent::RunningRules { .. } => "running-rules",
    ProgressEvent::FileRules { .. } => "file-rules",
    ProgressEvent::WritingReport => "writing-report",
  }
}

#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn collect(no_cache: bool, cache_dir: Option<std::path::PathBuf>) -> Vec<&'static str> {
  let kinds = Arc::new(Mutex::new(Vec::new()));
  let reporter = {
    let kinds = Arc::clone(&kinds);
    ProgressReporter::new(move |event: &ProgressEvent| {
      let Ok(mut kinds) = kinds.lock() else {
        return;
      };
      kinds.push(event_kind(event));
    })
  };
  let root = fixture("projects/basic");
  let session = match ProjectSession::open(SessionOptions {
    root,
    config_path: None,
    cache_dir,
    no_cache,
    threads: Some(1),
  }) {
    Ok(session) => session.with_progress(reporter),
    Err(error) => panic!("session must open: {error}"),
  };
  assert!(session.analyze().is_ok(), "analyze must succeed");
  kinds.lock().map_or_else(|error| error.into_inner().clone(), |kinds| kinds.clone())
}

#[test]
fn cache_progress_events_follow_lookup_and_store_boundaries() {
  let cache = fixture("projects/basic")
    .join("..")
    .join("..")
    .join("target")
    .join(format!("session-progress-cache-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&cache);
  let disabled = collect(true, None);
  assert!(
    !disabled.contains(&"checking-cache")
      && !disabled.contains(&"cache-hit")
      && !disabled.contains(&"saving-cache"),
    "no-cache scans must skip cache events: {disabled:?}"
  );
  let miss = collect(false, Some(cache.clone()));
  let check = miss.iter().position(|kind| *kind == "checking-cache");
  let save = miss.iter().position(|kind| *kind == "saving-cache");
  assert!(
    matches!((check, save), (Some(check), Some(save)) if check < save),
    "miss must check then save: {miss:?}"
  );
  assert!(!miss.contains(&"cache-hit"), "first cache fill is not a hit: {miss:?}");
  let hit = collect(false, Some(cache.clone()));
  let check = hit.iter().position(|kind| *kind == "checking-cache");
  let hit_at = hit.iter().position(|kind| *kind == "cache-hit");
  assert!(
    matches!((check, hit_at), (Some(check), Some(hit_at)) if check < hit_at),
    "warm scan must check then hit: {hit:?}"
  );
  assert!(!hit.contains(&"saving-cache"), "a hit must not save: {hit:?}");
  let _ignored = std::fs::remove_dir_all(cache);
}
