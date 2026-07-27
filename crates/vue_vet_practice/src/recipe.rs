//! Compile-time recipe catalog entries for practice suggestions.

use vue_vet_core::Confidence;

/// Recommended ecosystem / official API attached to a practice finding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EcosystemApi {
  pub package: &'static str,
  pub export: &'static str,
  pub docs_url: &'static str,
  pub import_example: &'static str,
}

/// Declarative recipe metadata. Matching logic lives in the thin [`vue_vet_core::Rule`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PracticeRecipe {
  pub rule_id: &'static str,
  pub documentation: &'static str,
  pub confidence: Confidence,
  /// Minimum Vue version `(major, minor)` when the recommendation applies.
  pub min_vue: Option<(u64, u64)>,
  pub recommend: EcosystemApi,
}

impl PracticeRecipe {
  #[must_use]
  pub const fn meets_vue(self, major: u64, minor: u64) -> bool {
    match self.min_vue {
      None => true,
      Some((need_major, need_minor)) => {
        major > need_major || (major == need_major && minor >= need_minor)
      }
    }
  }
}
