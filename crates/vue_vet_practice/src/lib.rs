//! Ecosystem practice suggestions: recipe catalog + thin [`Rule`] implementations.
//!
//! Practice findings use [`vue_vet_core::PRACTICE_CATEGORY`], attach a
//! [`vue_vet_core::Recommendation`], and stay out of score / default CI exit.
//! Matching consumes existing Vue Vet facts only — no parallel pattern engine.

use vue_vet_core::RuleRegistry;

mod recipe;
mod rules;
mod util;

pub use recipe::{EcosystemApi, PracticeRecipe};

/// Practice suggestion rules (recipe-backed).
#[must_use]
pub fn practice_rules() -> Vec<&'static dyn vue_vet_core::Rule> {
  rules::all()
}

/// Registry containing only practice rules.
#[must_use]
pub fn practice_registry() -> RuleRegistry {
  RuleRegistry::new(practice_rules())
}

#[cfg(test)]
mod tests {
  use vue_vet_core::{PRACTICE_CATEGORY, Severity};

  use super::*;

  #[test]
  fn practice_rules_have_stable_metadata() {
    let metadata = practice_registry().metadata();
    assert_eq!(
      metadata.len(),
      6,
      "practice ships VueUse recipes, prefer-to-value, and prefer-use-template-ref"
    );
    assert!(
      metadata.windows(2).all(|pair| matches!(pair, [first, second] if first.id < second.id)),
      "practice metadata must be sorted by stable rule ID"
    );
    assert!(
      metadata.iter().all(|meta| {
        meta.category == PRACTICE_CATEGORY && meta.default_severity == Severity::Info
      }),
      "practice rules must stay on the suggestion channel"
    );
    assert!(
      metadata.iter().any(|meta| meta.id == "vue-vet/reactivity/prefer-use-template-ref"),
      "prefer-use-template-ref keeps its stable historical id"
    );
  }
}
