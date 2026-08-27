#![expect(clippy::expect_used, reason = "benchmark harness aborts on setup failure")]

use std::sync::atomic::{AtomicUsize, Ordering};

use vue_vet_core::ScriptKind;
use vue_vet_reactivity::{
  ModuleLink, ModuleSource, ModuleTraceState, TraceModulesOptions,
  trace_modules_incremental_from_refs, trace_modules_incremental_with_options,
  trace_modules_with_options,
};

fn main() {
  divan::main();
}

fn synthetic_modules(count: usize) -> Vec<ModuleSource> {
  (0..count)
    .map(|index| {
      ModuleSource::standalone(
        format!("src/module-{index}.ts"),
        format!("import {{ ref }} from 'vue'; export const value{index} = ref({index});"),
        "ts",
        ScriptKind::Script,
      )
    })
    .collect()
}

fn worker_pool() -> rayon::ThreadPool {
  rayon::ThreadPoolBuilder::new().num_threads(8).build().expect("benchmark worker pool")
}

fn trace_synthetic(modules: &[ModuleSource]) -> usize {
  // Pool is installed by the caller; reuse it so samples measure tracing, not
  // ThreadPoolBuilder overhead from honoring max_workers on each call.
  trace_modules_with_options(
    divan::black_box(modules),
    &[],
    TraceModulesOptions { max_workers: 8, reuse_current_pool: true, ..Default::default() },
  )
  .map_or(0, |traced| traced.len())
}

#[divan::bench(sample_count = 5, sample_size = 1)]
fn trace_1k_modules(bencher: divan::Bencher) {
  let modules = synthetic_modules(1_000);
  let pool = worker_pool();
  bencher.bench_local(|| pool.install(|| trace_synthetic(&modules)));
}

#[divan::bench(sample_count = 5, sample_size = 1)]
fn trace_5k_modules(bencher: divan::Bencher) {
  let modules = synthetic_modules(5_000);
  let pool = worker_pool();
  bencher.bench_local(|| pool.install(|| trace_synthetic(&modules)));
}

fn reexport_chain(count: usize) -> (Vec<ModuleSource>, Vec<ModuleLink>) {
  let modules = (0..count)
    .map(|index| {
      let source = if index + 1 == count {
        "import { ref } from 'vue'; export const value = ref(1);".to_owned()
      } else {
        format!("export {{ value }} from './module-{}.ts';", index + 1)
      };
      ModuleSource::standalone(format!("src/module-{index}.ts"), source, "ts", ScriptKind::Script)
    })
    .collect::<Vec<_>>();
  let links = (0..count.saturating_sub(1))
    .map(|index| ModuleLink {
      from: format!("src/module-{index}.ts").into(),
      specifier: format!("./module-{}.ts", index + 1),
      to: format!("src/module-{}.ts", index + 1).into(),
    })
    .collect();
  (modules, links)
}

/// Warm `ModuleTraceState` + one independent leaf body edit.
///
/// One-shot `trace_*` benches force `persist_linking_cache = false` and do not
/// measure this path. Keep those names as no-regression; this name is the
/// locality win signal.
#[divan::bench(sample_count = 5, sample_size = 1)]
fn trace_warm_leaf_edit_1k_modules(bencher: divan::Bencher) {
  let pool = worker_pool();
  let mut modules = synthetic_modules(1_000);
  let options = TraceModulesOptions {
    max_workers: 8,
    reuse_current_pool: true,
    persist_linking_cache: true,
    retain_cached_modules: true,
    ..Default::default()
  };
  let mut state = ModuleTraceState::default();
  pool.install(|| {
    let warmed = trace_modules_incremental_with_options(&modules, &[], &options, &mut state);
    assert!(warmed.issues.is_empty(), "warm leaf-edit setup must trace: {:?}", warmed.issues);
  });

  let leaf_id = modules.last().expect("1k synthetic modules").id.clone();
  drop(modules);
  let sources = [
    "import { ref } from 'vue'; export const value999 = ref(1);",
    "import { ref } from 'vue'; export const value999 = ref(2);",
  ];
  let edit = AtomicUsize::new(0);
  bencher.bench_local(|| {
    pool.install(|| {
      let sequence = edit.fetch_add(1, Ordering::Relaxed);
      let next = sources
        .get(sequence % sources.len())
        .copied()
        .unwrap_or("import { ref } from 'vue'; export const value999 = ref(1);");
      let leaf = ModuleSource::standalone(leaf_id.clone(), next, "ts", ScriptKind::Script);
      let report =
        trace_modules_incremental_from_refs(divan::black_box(&[&leaf]), &[], &options, &mut state);
      divan::black_box((
        report.modules.len(),
        report.stats.reused_graphs,
        report.stats.seeded_reparses,
        report.stats.export_resolve_ran,
      ))
    })
  });
}

#[divan::bench(sample_count = 5, sample_size = 1)]
fn trace_1k_reexport_chain(bencher: divan::Bencher) {
  let (modules, links) = reexport_chain(1_000);
  let pool = worker_pool();
  bencher.bench_local(|| {
    pool.install(|| {
      trace_modules_with_options(
        divan::black_box(&modules),
        divan::black_box(&links),
        TraceModulesOptions { max_workers: 8, reuse_current_pool: true, ..Default::default() },
      )
      .map_or(0, |traced| traced.len())
    })
  });
}
