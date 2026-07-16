//! Benchmarks for the Vue single-file component analysis pipeline.
//!
//! `analyze_sfc` drives the full local-doctor pass: Vize SFC parsing, Oxc
//! script analysis, template fact extraction, and the built-in rule registry.
//! These benchmarks exercise that entry point over representative fixtures so
//! the continuous-benchmarking suite can track the cost of the hot analysis path.

use std::path::Path;

use divan::{Bencher, black_box};
use vue_vet_vize::analyze_sfc;

fn main() {
  divan::main();
}

/// Source of a representative Vue SFC fixture, keyed by a benchmark-friendly name.
fn fixture_source(name: &str) -> (&'static str, &'static str) {
  match name {
    "minimal_script_setup" => (
      "fixtures/projects/vue-3.5/App.vue",
      include_str!("../../../fixtures/projects/vue-3.5/App.vue"),
    ),
    "basic_app" => {
      ("fixtures/projects/basic/App.vue", include_str!("../../../fixtures/projects/basic/App.vue"))
    }
    _ => (
      "fixtures/rules/recommended/invalid.vue",
      include_str!("../../../fixtures/rules/recommended/invalid.vue"),
    ),
  }
}

const FIXTURE_NAMES: &[&str] = &["minimal_script_setup", "basic_app", "recommended_rules_invalid"];

#[divan::bench(args = FIXTURE_NAMES)]
fn analyze(bencher: Bencher, name: &str) {
  let (path, source) = fixture_source(name);
  bencher.bench(|| black_box(analyze_sfc(Path::new(black_box(path)), black_box(source)).is_ok()));
}
