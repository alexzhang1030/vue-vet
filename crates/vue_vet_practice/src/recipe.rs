//! Compile-time recipe catalog entries for practice suggestions.

/// Recommended ecosystem API attached to a practice finding.
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
  pub recommend: EcosystemApi,
}
