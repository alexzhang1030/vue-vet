//! Built-in analysis enrichment passes (compile-time, Rust-only).
//!
//! These enrich Vue Vet IR before diagnostic [`vue_vet_core::Rule`] passes run.
//! They are **not** user plugins, and **not** Oxc/SWC AST Traverse transforms.
//!
//! Pipeline phases (see architecture PCR `Analysis enrichment passes`):
//! `ConventionsLoad` → `StructuralLink` → `ExternalSummaryLoad` →
//! `SummaryMerge` → `SeedPlan` / `Trace` → `RuleRegistry`.

mod nuxt_imports;
mod provisional_factory;

pub use nuxt_imports::run_nuxt_imports_seed_pass;
pub use provisional_factory::{
  EXTERNAL_COMPANION_MAX_BYTES, apply_provisional_factory_merge, companion_implementation_path,
};

/// Named enrichment phase for docs, tests, and future registry metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EnrichmentPhase {
  ConventionsLoad,
  StructuralLink,
  ExternalSummaryLoad,
  SummaryMerge,
  SeedPlan,
  Trace,
  RuleRegistry,
}

impl EnrichmentPhase {
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::ConventionsLoad => "conventions_load",
      Self::StructuralLink => "structural_link",
      Self::ExternalSummaryLoad => "external_summary_load",
      Self::SummaryMerge => "summary_merge",
      Self::SeedPlan => "seed_plan",
      Self::Trace => "trace",
      Self::RuleRegistry => "rule_registry",
    }
  }
}

/// Compile-time pass identity (name + phase). Not a dynamic plugin ABI.
pub trait EnrichmentPass {
  const NAME: &'static str;
  const PHASE: EnrichmentPhase;
}

/// [`NuxtImportsSeedPass`] — bare `.nuxt` auto-import → `#nuxt-imports:` seeds.
pub struct NuxtImportsSeedPass;

impl EnrichmentPass for NuxtImportsSeedPass {
  const NAME: &'static str = "nuxt_imports_seed";
  const PHASE: EnrichmentPhase = EnrichmentPhase::StructuralLink;
}

/// [`ProvisionalFactoryMergePass`] — provisional `.d.ts` + companion body → Factory.
pub struct ProvisionalFactoryMergePass;

impl EnrichmentPass for ProvisionalFactoryMergePass {
  const NAME: &'static str = "provisional_factory_merge";
  const PHASE: EnrichmentPhase = EnrichmentPhase::SummaryMerge;
}

#[cfg(test)]
mod tests {
  use super::{EnrichmentPass, EnrichmentPhase, NuxtImportsSeedPass, ProvisionalFactoryMergePass};

  #[test]
  fn enrichment_pass_phases_are_documented() {
    assert_eq!(NuxtImportsSeedPass::PHASE, EnrichmentPhase::StructuralLink);
    assert_eq!(ProvisionalFactoryMergePass::PHASE, EnrichmentPhase::SummaryMerge);
    assert_eq!(EnrichmentPhase::StructuralLink.as_str(), "structural_link");
    assert_eq!(EnrichmentPhase::SummaryMerge.as_str(), "summary_merge");
  }
}
