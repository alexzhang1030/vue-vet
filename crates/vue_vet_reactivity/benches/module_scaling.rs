#![expect(clippy::expect_used, reason = "benchmark harness aborts on setup failure")]

use vue_vet_core::ScriptKind;
use vue_vet_reactivity::{ModuleSource, TraceModulesOptions, trace_modules_with_options};

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
    TraceModulesOptions { max_workers: 8, reuse_current_pool: true },
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
