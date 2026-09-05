//! Mixed Vue+TS whole-project scan modes. Fixture generation stays outside
//! the measured closures. `scan_modes` benches are unchanged.

#![expect(clippy::expect_used, reason = "benchmark harness aborts on setup failure")]

use std::{
  path::PathBuf,
  sync::atomic::{AtomicUsize, Ordering},
};

use vue_vet_core::{ReactiveDependencyKind, TrackingScopeKind};
use vue_vet_project::EdgeKind;
use vue_vet_reporters::{ReportContext, ReportFormat, render};
use vue_vet_session::{ChangeSet, ProjectSession, SessionOptions};

fn temp_dir(label: &str) -> PathBuf {
  static NEXT: AtomicUsize = AtomicUsize::new(0);
  let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
  std::env::temp_dir().join(format!("vue-vet-whole-{label}-{}-{sequence}", std::process::id()))
}

fn mixed_workspace(file_count: usize) -> PathBuf {
  assert!(file_count.is_multiple_of(4), "mixed fixture uses four files per group");
  let root = temp_dir(&format!("workspace-{file_count}"));
  let _ignored = std::fs::remove_dir_all(&root);
  let groups = file_count / 4;
  std::fs::create_dir_all(root.join("src")).expect("mixed src");
  std::fs::write(
    root.join("package.json"),
    r#"{"name":"mixed-vue-performance","private":true,"dependencies":{"vue":"3.5.0"}}"#,
  )
  .expect("package.json");
  for index in 0..groups {
    let dir = root.join(format!("src/group-{index:04}"));
    std::fs::create_dir_all(&dir).expect("group dir");
    std::fs::write(dir.join("data.ts"), format!("export const initial = {index};\n"))
      .expect("data.ts");
    std::fs::write(
      dir.join("useCounter.ts"),
      "import { ref, computed, watchEffect } from 'vue';\nimport { initial } from './data';\nexport function useCounter() {\n  const count = ref(initial);\n  const doubled = computed(() => count.value * 2);\n  watchEffect(() => console.log(doubled.value));\n  return { count, doubled };\n}\n",
    )
    .expect("useCounter.ts");
    let ordinary = if index % 16 == 0 {
      "<script lang=\"ts\">\nexport const category = 'counter';\n</script>\n"
    } else {
      ""
    };
    std::fs::write(
      dir.join("Parent.vue"),
      format!(
        "{ordinary}<script setup lang=\"ts\">\nimport Child from './Child.vue';\nimport {{ useCounter }} from './useCounter';\nconst {{ count, doubled }} = useCounter();\nconst markup = '<b>counter</b>';\n</script>\n<template>\n  <section><Child :value=\"count\" :label=\"String(doubled)\" /><div v-html=\"markup\"></div></section>\n</template>\n"
      ),
    )
    .expect("Parent.vue");
    let suppression =
      if index % 8 == 0 { "// vue-vet-disable-next-line vue-vet/security/no-v-html\n" } else { "" };
    std::fs::write(
      dir.join("Child.vue"),
      format!(
        "<script setup lang=\"ts\">\nimport {{ computed, ref, watchEffect }} from 'vue';\nconst props = defineProps<{{ value: number; label: string }}>();\nconst local = ref(0);\n{suppression}const total = computed(() => props.value + local.value);\nwatchEffect(() => console.log(total.value));\n</script>\n<template><span>{{{{ label }}}}: {{{{ total }}}}</span></template>\n"
      ),
    )
    .expect("Child.vue");
  }
  root
}

fn open(root: &std::path::Path, cache: PathBuf, no_cache: bool) -> ProjectSession {
  ProjectSession::open(SessionOptions {
    root: root.to_path_buf(),
    config_path: None,
    cache_dir: Some(cache),
    no_cache,
    threads: Some(1),
  })
  .expect("open session")
}

fn assert_mixed_semantics(snapshot: &vue_vet_session::AnalysisSnapshot, file_count: usize) {
  let groups = file_count / 4;
  assert_eq!(snapshot.summary.files_scanned, file_count, "mixed fixture file count");
  assert!(
    snapshot.graph.module_reactivity.len() >= file_count,
    "mixed fixture must expose at least one module per file, got {}",
    snapshot.graph.module_reactivity.len()
  );
  assert!(
    snapshot
      .summary
      .diagnostics
      .iter()
      .any(|diagnostic| { diagnostic.rule_id == "vue-vet/security/no-v-html" }),
    "mixed Parent.vue v-html must produce diagnostics"
  );
  assert!(
    snapshot.graph.edges.iter().any(|edge| matches!(edge.kind, EdgeKind::ComponentUsage)),
    "mixed Parent→Child must produce ComponentUsage edges"
  );
  let prop_edges = count_graph_edges(snapshot, ReactiveDependencyKind::Prop);
  let computed_edges = count_graph_edges(snapshot, ReactiveDependencyKind::Computed);
  let effect_edges = count_graph_edges(snapshot, ReactiveDependencyKind::Effect);
  let template_edges = count_graph_edges(snapshot, ReactiveDependencyKind::Template);
  let computed_scopes = count_scopes(snapshot, TrackingScopeKind::Computed);
  let effect_scopes = count_scopes(snapshot, TrackingScopeKind::WatchEffect);
  let template_reads = snapshot
    .graph
    .module_reactivity
    .iter()
    .map(|module| module.graph.template_reads.len())
    .sum::<usize>();
  assert_eq!(prop_edges, groups, "Prop edges");
  assert_eq!(computed_edges, groups.saturating_mul(3), "computed edges");
  assert_eq!(effect_edges, groups.saturating_mul(2), "effect edges");
  assert_eq!(template_edges, groups.saturating_mul(3), "template edges");
  assert_eq!(computed_scopes, groups.saturating_mul(2), "computed scopes");
  assert_eq!(effect_scopes, groups.saturating_mul(2), "watchEffect scopes");
  assert_eq!(template_reads, groups.saturating_mul(3), "template_reads length");
}

fn count_graph_edges(
  snapshot: &vue_vet_session::AnalysisSnapshot,
  kind: ReactiveDependencyKind,
) -> usize {
  snapshot
    .graph
    .module_reactivity
    .iter()
    .flat_map(|module| module.graph.edges.iter())
    .filter(|edge| edge.kind == kind)
    .count()
}

fn count_scopes(snapshot: &vue_vet_session::AnalysisSnapshot, kind: TrackingScopeKind) -> usize {
  snapshot
    .graph
    .module_reactivity
    .iter()
    .flat_map(|module| module.graph.scopes.iter())
    .filter(|scope| scope.kind == kind)
    .count()
}

// `codspeed-divan-compat` 5.0.1 `BenchOptions` has no `threads` (missing
// `IntoThreads` under `cfg(codspeed)`). Analysis concurrency stays
// `SessionOptions { threads: Some(1) }`, same as `scan_modes`.
#[divan::bench(sample_count = 10, sample_size = 1)]
fn scan_cold_mixed_1k(bencher: divan::Bencher) {
  let root = mixed_workspace(1_000);
  let cache = temp_dir("cold-1k");
  {
    let probe = open(&root, cache.clone(), true);
    let snapshot = probe.analyze().expect("probe mixed 1k");
    assert_mixed_semantics(&snapshot, 1_000);
  }
  bencher.bench(|| {
    let session = open(&root, cache.clone(), true);
    let snapshot = session.analyze().expect("cold mixed 1k");
    divan::black_box((snapshot.summary.files_scanned, snapshot.graph.edges.len()))
  });
  let _ignored = std::fs::remove_dir_all(&cache);
  let _ignored = std::fs::remove_dir_all(&root);
}

#[divan::bench(sample_count = 10, sample_size = 1)]
fn scan_warm_mixed_1k(bencher: divan::Bencher) {
  let root = mixed_workspace(1_000);
  let cache = temp_dir("warm-1k");
  {
    let primer = open(&root, cache.clone(), false);
    let first = primer.analyze().expect("seed warm cache");
    assert_mixed_semantics(&first, 1_000);
  }
  bencher.bench(|| {
    let session = open(&root, cache.clone(), false);
    let snapshot = session.analyze().expect("warm mixed 1k");
    divan::black_box(snapshot.summary.files_scanned)
  });
  let _ignored = std::fs::remove_dir_all(&cache);
  let _ignored = std::fs::remove_dir_all(&root);
}

#[divan::bench(sample_count = 10, sample_size = 1)]
fn scan_script_edit_mixed_1k(bencher: divan::Bencher) {
  let root = mixed_workspace(1_000);
  let parent = root.join("src/group-0000/Parent.vue");
  let original = std::fs::read_to_string(&parent).expect("parent source");
  let cache = temp_dir("script-1k");
  let session = open(&root, cache.clone(), true);
  let initial = session.analyze().expect("initial mixed 1k");
  assert_mixed_semantics(&initial, 1_000);
  let sources =
    [original.replace("const markup", "const added = 1;\nconst markup"), original.clone()];
  let edit = AtomicUsize::new(0);
  bencher.bench(|| {
    let sequence = edit.fetch_add(1, Ordering::Relaxed);
    let source = sources.get(sequence % sources.len()).cloned().unwrap_or_else(|| original.clone());
    session.apply_changes(ChangeSet::upsert(parent.clone(), source)).expect("script overlay");
    let snapshot = session.analyze_affected().expect("script edit");
    divan::black_box(snapshot.work.files_parsed)
  });
  let _ignored = std::fs::remove_dir_all(&cache);
  let _ignored = std::fs::remove_dir_all(&root);
}

#[divan::bench(sample_count = 10, sample_size = 1)]
fn scan_dependency_edit_mixed_1k(bencher: divan::Bencher) {
  let root = mixed_workspace(1_000);
  let dep = root.join("src/group-0000/useCounter.ts");
  let original = std::fs::read_to_string(&dep).expect("dep source");
  let cache = temp_dir("dep-1k");
  let session = open(&root, cache.clone(), true);
  let initial = session.analyze().expect("initial mixed 1k");
  assert_mixed_semantics(&initial, 1_000);
  let sources = [original.replace("* 2", "* 3"), original.clone()];
  let edit = AtomicUsize::new(0);
  bencher.bench(|| {
    let sequence = edit.fetch_add(1, Ordering::Relaxed);
    let source = sources.get(sequence % sources.len()).cloned().unwrap_or_else(|| original.clone());
    session.apply_changes(ChangeSet::upsert(dep.clone(), source)).expect("dep overlay");
    let snapshot = session.analyze_affected().expect("dep edit");
    divan::black_box(snapshot.work.files_parsed)
  });
  let _ignored = std::fs::remove_dir_all(&cache);
  let _ignored = std::fs::remove_dir_all(&root);
}

#[divan::bench(sample_count = 10, sample_size = 1)]
fn json_render_mixed_1k(bencher: divan::Bencher) {
  let root = mixed_workspace(1_000);
  let cache = temp_dir("json-1k");
  let session = open(&root, cache.clone(), true);
  let snapshot = session.analyze().expect("json mixed 1k");
  assert_mixed_semantics(&snapshot, 1_000);
  let context =
    ReportContext { analyzed_files: snapshot.analyzed_files.to_vec(), ..ReportContext::default() };
  bencher.bench(|| {
    let output = render(&snapshot.summary, ReportFormat::Json, &context).expect("json render");
    divan::black_box(output.len())
  });
  let _ignored = std::fs::remove_dir_all(&cache);
  let _ignored = std::fs::remove_dir_all(&root);
}

#[divan::bench(sample_count = 10, sample_size = 1)]
fn scan_template_edit_mixed_5k(bencher: divan::Bencher) {
  let root = mixed_workspace(5_000);
  let parent = root.join("src/group-0000/Parent.vue");
  let original = std::fs::read_to_string(&parent).expect("parent source");
  let cache = temp_dir("template-5k");
  let session = open(&root, cache.clone(), true);
  let initial = session.analyze().expect("initial mixed 5k");
  assert_mixed_semantics(&initial, 5_000);
  let sources = [original.replace("<section>", "<section class=\"edited\">"), original.clone()];
  let edit = AtomicUsize::new(0);
  bencher.bench(|| {
    let sequence = edit.fetch_add(1, Ordering::Relaxed);
    let source = sources.get(sequence % sources.len()).cloned().unwrap_or_else(|| original.clone());
    session.apply_changes(ChangeSet::upsert(parent.clone(), source)).expect("template overlay");
    let snapshot = session.analyze_affected().expect("template edit");
    divan::black_box(snapshot.work.files_parsed)
  });
  let _ignored = std::fs::remove_dir_all(&cache);
  let _ignored = std::fs::remove_dir_all(&root);
}

fn main() {
  divan::main();
}
