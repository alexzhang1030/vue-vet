//! Built-in analysis enrichment passes (compile-time, Rust-only).
//!
//! These enrich Vue Vet IR before diagnostic [`vue_vet_core::Rule`] passes run.
//! They are **not** user plugins, and **not** Oxc/SWC AST Traverse transforms.
//!
//! Each step is a named `struct` with an inherent `::run(...)`. There is no
//! dynamic plugin ABI and no empty metadata-only trait. Execution is explicit
//! call sites in the project graph builder; [`ENRICHMENT_STEPS`] is the
//! documentation / test checklist for those steps.
//!
//! Stages (enrichment only — Trace / Rules live outside this module):
//! `ConventionsLoad` → `StructuralLink` → `ExternalSummaryLoad`
//! (per-module `SummaryMerge` at load completion).

mod external_summary;
mod nuxt_imports;
mod provisional_factory;
mod types;

pub use external_summary::ExternalSummaryLoadPass;
pub use nuxt_imports::NuxtImportsSeedPass;
pub use provisional_factory::{EXTERNAL_COMPANION_MAX_BYTES, ProvisionalFactoryMergePass};
pub use types::ExternalReactivityRoot;

/// Enrichment stages owned by `vue_vet_project::passes` (+ conventions load).
///
/// `SeedPlan` / `Trace` / `RuleRegistry` are product-pipeline phases outside this
/// module; they are not listed here so readers do not expect a pass runner.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EnrichmentStage {
  ConventionsLoad,
  StructuralLink,
  ExternalSummaryLoad,
  SummaryMerge,
}

impl EnrichmentStage {
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::ConventionsLoad => "conventions_load",
      Self::StructuralLink => "structural_link",
      Self::ExternalSummaryLoad => "external_summary_load",
      Self::SummaryMerge => "summary_merge",
    }
  }
}

/// Static checklist entry for an enrichment step (docs + tests, not a plugin ABI).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnrichmentStepMeta {
  pub name: &'static str,
  pub stage: EnrichmentStage,
  pub summary: &'static str,
}

/// Ordered enrichment steps. Call sites must stay aligned with this table.
pub const ENRICHMENT_STEPS: &[EnrichmentStepMeta] = &[
  EnrichmentStepMeta {
    name: "conventions_load",
    stage: EnrichmentStage::ConventionsLoad,
    summary: "Load .nuxt imports/components maps into ProjectContext (conventions.rs)",
  },
  EnrichmentStepMeta {
    name: NuxtImportsSeedPass::NAME,
    stage: NuxtImportsSeedPass::STAGE,
    summary: "Bare Nuxt auto-import calls → #nuxt-imports: seeds (StructuralLink)",
  },
  EnrichmentStepMeta {
    name: ExternalSummaryLoadPass::NAME,
    stage: ExternalSummaryLoadPass::STAGE,
    summary: "Load external package .d.ts / bodies for reactivity seed follow",
  },
  EnrichmentStepMeta {
    name: ProvisionalFactoryMergePass::NAME,
    stage: ProvisionalFactoryMergePass::STAGE,
    summary: "Merge provisional .d.ts Factory halves with size-capped companion bodies",
  },
];

#[cfg(test)]
mod tests {
  use super::{
    ENRICHMENT_STEPS, EnrichmentStage, EnrichmentStepMeta, ExternalSummaryLoadPass,
    NuxtImportsSeedPass, ProvisionalFactoryMergePass,
  };

  #[test]
  fn enrichment_steps_match_pass_constants() {
    let expected = [
      ("conventions_load", EnrichmentStage::ConventionsLoad),
      (NuxtImportsSeedPass::NAME, NuxtImportsSeedPass::STAGE),
      (ExternalSummaryLoadPass::NAME, ExternalSummaryLoadPass::STAGE),
      (ProvisionalFactoryMergePass::NAME, ProvisionalFactoryMergePass::STAGE),
    ];
    assert_eq!(ENRICHMENT_STEPS.len(), expected.len());
    for (step, (name, stage)) in ENRICHMENT_STEPS.iter().zip(expected) {
      assert_step(step, name, stage);
    }
    assert_eq!(EnrichmentStage::SummaryMerge.as_str(), "summary_merge");
  }

  fn assert_step(step: &EnrichmentStepMeta, name: &str, stage: EnrichmentStage) {
    assert_eq!(step.name, name);
    assert_eq!(step.stage, stage);
  }
}
