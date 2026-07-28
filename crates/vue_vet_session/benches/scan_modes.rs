//! Cold / warm / overlay / diff-filter scan benchmarks on the quality corpus.

#![expect(clippy::expect_used, reason = "benchmark harness aborts on setup failure")]

use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
  sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  },
};

use vue_vet_cache::{ChangedLines, filter_diff};
use vue_vet_session::{ChangeSet, ProjectSession, SessionOptions};

fn nuxt_graph() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/projects/nuxt-graph")
}

fn temp_cache(label: &str) -> PathBuf {
  static NEXT: AtomicUsize = AtomicUsize::new(0);
  let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
  std::env::temp_dir().join(format!("vue-vet-bench-{label}-{}-{sequence}", std::process::id()))
}

fn synthetic_workspace(count: usize) -> PathBuf {
  let root = temp_cache("synthetic-workspace");
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(root.join("src")).expect("synthetic src directory");
  for index in 0..count {
    let source = if index == 0 {
      "export const value0 = 0;\n".to_owned()
    } else {
      format!(
        "import {{ value{} }} from './module-{}'; export const value{index} = value{};\n",
        index - 1,
        index - 1,
        index - 1
      )
    };
    std::fs::write(root.join(format!("src/module-{index}.ts")), source).expect("synthetic module");
  }
  root
}

fn independent_workspace(count: usize) -> PathBuf {
  let root = temp_cache("independent-workspace");
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(root.join("src")).expect("independent src directory");
  for index in 0..count {
    std::fs::write(
      root.join(format!("src/module-{index}.ts")),
      format!("export const value{index} = {index};\n"),
    )
    .expect("independent module");
  }
  root
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
fn scan_incremental_edits_nuxt_graph(bencher: divan::Bencher) {
  let root = nuxt_graph();
  let index = root.join("pages/index.vue");
  let source = std::fs::read_to_string(&index).expect("index.vue");
  let cache = temp_cache("incremental");
  let session = open(&root, cache.clone(), true);
  let _initial = session.analyze().expect("initial analyze");
  let edit = AtomicUsize::new(0);
  bencher.bench(|| {
    let sequence = edit.fetch_add(1, Ordering::Relaxed);
    session
      .apply_changes(ChangeSet::upsert(
        index.clone(),
        format!("{source}\n<!-- edit {sequence} -->"),
      ))
      .expect("apply incremental overlay");
    let snapshot = session.analyze_affected().expect("incremental analyze");
    divan::black_box(snapshot.summary.diagnostics.len())
  });
  let _ignored = std::fs::remove_dir_all(&cache);
}

#[divan::bench]
fn scan_noop_analyze_affected(bencher: divan::Bencher) {
  let root = nuxt_graph();
  let cache = temp_cache("noop");
  let session = open(&root, cache.clone(), true);
  let _initial = session.analyze().expect("initial analyze");
  bencher.bench(|| {
    let snapshot = session.analyze_affected().expect("noop analyze_affected");
    divan::black_box(snapshot.summary.diagnostics.len())
  });
  let _ignored = std::fs::remove_dir_all(&cache);
}

#[divan::bench(sample_count = 10, sample_size = 1)]
fn scan_independent_leaf_edit_1k_modules(bencher: divan::Bencher) {
  let root = independent_workspace(1_000);
  let module = root.join("src/module-999.ts");
  let cache = temp_cache("independent-1k");
  let session = open(&root, cache.clone(), true);
  let _initial = session.analyze().expect("initial independent analysis");
  let sources = ["export const value999 = 1;\n", "export const value999 = 2;\n"];
  let edit = AtomicUsize::new(0);
  bencher.bench(|| {
    let sequence = edit.fetch_add(1, Ordering::Relaxed);
    let source =
      sources.get(sequence % sources.len()).copied().unwrap_or("export const value999 = 1;\n");
    session
      .apply_changes(ChangeSet::upsert(module.clone(), source.into()))
      .expect("apply independent leaf overlay");
    let snapshot = session.analyze_affected().expect("independent leaf analysis");
    divan::black_box((snapshot.graph.nodes.len(), session.stats()))
  });
  let _ignored = std::fs::remove_dir_all(&root);
  let _ignored = std::fs::remove_dir_all(&cache);
}

#[divan::bench(sample_count = 20, sample_size = 1)]
fn scan_incremental_root_edit_1k_modules(bencher: divan::Bencher) {
  let root = synthetic_workspace(1_000);
  let module = root.join("src/module-0.ts");
  let cache = temp_cache("incremental-1k");
  let session = open(&root, cache.clone(), true);
  let _initial = session.analyze().expect("initial synthetic analysis");
  let sources = ["export const value0 = 1;\n", "export const value0 = 2;\n"];
  let edit = AtomicUsize::new(0);
  bencher.bench(|| {
    let sequence = edit.fetch_add(1, Ordering::Relaxed);
    let source =
      sources.get(sequence % sources.len()).copied().unwrap_or("export const value0 = 1;\n");
    session
      .apply_changes(ChangeSet::upsert(module.clone(), source.into()))
      .expect("apply synthetic incremental overlay");
    let snapshot = session.analyze_affected().expect("large incremental analysis");
    divan::black_box((snapshot.graph.edges.len(), session.stats()))
  });
  let _ignored = std::fs::remove_dir_all(&root);
  let _ignored = std::fs::remove_dir_all(&cache);
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
      let filtered = filter_diff(Arc::unwrap_or_clone(summary), &changed);
      let _ignored = std::fs::remove_dir_all(&cache);
      divan::black_box(filtered.diagnostics.len())
    });
}
