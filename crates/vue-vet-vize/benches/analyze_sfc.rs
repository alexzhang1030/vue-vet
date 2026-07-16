use std::path::Path;

use vue_vet_vize::{AnalyzeError, analyze_sfc};

const VALID_SFC: &str = include_str!("../../../fixtures/rules/recommended/valid.vue");
const INVALID_SFC: &str = include_str!("../../../fixtures/rules/recommended/invalid.vue");

fn main() {
  divan::main();
}

#[divan::bench]
fn analyze_recommended_valid() -> Result<usize, AnalyzeError> {
  analyze_sfc(Path::new("RecommendedValid.vue"), divan::black_box(VALID_SFC))
    .map(|diagnostics| diagnostics.len())
}

#[divan::bench]
fn analyze_recommended_invalid() -> Result<usize, AnalyzeError> {
  analyze_sfc(Path::new("RecommendedInvalid.vue"), divan::black_box(INVALID_SFC))
    .map(|diagnostics| diagnostics.len())
}
