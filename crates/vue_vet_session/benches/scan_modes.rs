//! Cold / warm / overlay / diff-filter scan benchmarks on the quality corpus.

#![expect(clippy::expect_used, reason = "benchmark harness aborts on setup failure")]

use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
  sync::atomic::{AtomicUsize, Ordering},
};

use vue_vet_cache::{ChangedLines, filter_diff};
use vue_vet_session::{ProjectSession, SessionOptions};

fn nuxt_graph() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/projects/nuxt-graph")
}

fn temp_cache(label: &str) -> PathBuf {
  static NEXT: AtomicUsize = AtomicUsize::new(0);
  let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
  std::env::temp_dir().join(format!("vue-vet-bench-{label}-{}-{sequence}", std::process::id()))
}

fn open(root: &Path, cache_dir: PathBuf, no_cache: bool) -> ProjectSession {
  ProjectSession::open(SessionOptions {
    root: root.to_path_buf(),
    config_path: None,
    cache_dir: Some(cache_dir),
    no_cache,
    threads: Some(1),
  })
  .expect("session opens for quality corpus")
}

fn main() {
  divan::main();
}

#[divan::bench]
fn scan_cold_nuxt_graph(bencher: divan::Bencher) {
  let root = nuxt_graph();
  bencher
    .with_inputs(|| {
      let cache = temp_cache("cold");
      let _ignored = std::fs::remove_dir_all(&cache);
      cache
    })
    .bench_values(|cache| {
      let session = open(&root, cache.clone(), false);
      let snapshot = session.analyze().expect("cold analyze");
      let _ignored = std::fs::remove_dir_all(&cache);
      divan::black_box(snapshot.summary.diagnostics.len())
    });
}

#[divan::bench]
fn scan_warm_nuxt_graph(bencher: divan::Bencher) {
  let root = nuxt_graph();
  bencher
    .with_inputs(|| {
      let cache = temp_cache("warm");
      let _ignored = std::fs::remove_dir_all(&cache);
      let session = open(&root, cache.clone(), false);
      let _primed = session.analyze().expect("prime cache");
      cache
    })
    .bench_values(|cache| {
      let session = open(&root, cache.clone(), false);
      let snapshot = session.analyze().expect("warm analyze");
      let _ignored = std::fs::remove_dir_all(&cache);
      divan::black_box(snapshot.summary.diagnostics.len())
    });
}

#[divan::bench]
fn scan_overlay_nuxt_graph(bencher: divan::Bencher) {
  let root = nuxt_graph();
  let index = root.join("pages/index.vue");
  let source = std::fs::read_to_string(&index).expect("index.vue");
  bencher.bench(|| {
    let cache = temp_cache("overlay");
    let _ignored = std::fs::remove_dir_all(&cache);
    let session = open(&root, cache.clone(), true);
    let mut overlays = BTreeMap::new();
    overlays.insert(index.clone(), source.clone());
    let snapshot = session.analyze_with_overlays(&overlays).expect("overlay analyze");
    let _ignored = std::fs::remove_dir_all(&cache);
    divan::black_box(snapshot.summary.diagnostics.len())
  });
}

#[divan::bench]
fn scan_diff_filter_nuxt_graph(bencher: divan::Bencher) {
  let root = nuxt_graph();
  bencher
    .with_inputs(|| {
      let cache = temp_cache("diff");
      let _ignored = std::fs::remove_dir_all(&cache);
      let session = open(&root, cache.clone(), true);
      let snapshot = session.analyze().expect("analyze for diff");
      (cache, snapshot.summary)
    })
    .bench_values(|(cache, summary)| {
      let mut changed = ChangedLines::default();
      changed.files.insert("pages/index.vue".into(), BTreeSet::from([1]));
      let filtered = filter_diff(summary, &changed);
      let _ignored = std::fs::remove_dir_all(&cache);
      divan::black_box(filtered.diagnostics.len())
    });
}
