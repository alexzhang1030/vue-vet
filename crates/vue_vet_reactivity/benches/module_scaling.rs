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

fn trace_synthetic(count: usize) -> usize {
  let modules = synthetic_modules(count);
  trace_modules_with_options(
    divan::black_box(&modules),
    &[],
    TraceModulesOptions { max_workers: 8 },
  )
  .map_or(0, |traced| traced.len())
}

#[divan::bench(sample_count = 1, sample_size = 1)]
fn trace_1k_modules() -> usize {
  trace_synthetic(1_000)
}

#[divan::bench(sample_count = 1, sample_size = 1)]
fn trace_5k_modules() -> usize {
  trace_synthetic(5_000)
}
