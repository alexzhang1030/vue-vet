//! Built-in semantic lint / gate rules (excludes practice suggestions).
//!
//! Assembles [`builtin_registry`] from `rules/` modules. Shared fact walks live
//! in `vue_vet_rule_query`. Practice channel: `vue_vet_practice`. Never import
//! Vize or Oxc AST.

use vue_vet_core::{Rule, RuleRegistry};

mod overlap;
mod rules;

pub use overlap::{
  consolidate_overlapping_computed_impurity, consolidate_overlapping_watch_source_sites,
};

/// Built-in lint / gate rules (excludes practice suggestions).
#[must_use]
pub fn builtin_rules() -> Vec<&'static dyn Rule> {
  rules::builtins()
}

#[must_use]
pub fn builtin_registry() -> RuleRegistry {
  RuleRegistry::new(builtin_rules())
}

#[cfg(test)]
mod tests {
  use std::path::PathBuf;

  use vue_vet_core::Confidence;

  use super::*;

  #[test]
  fn builtins_have_stable_metadata() {
    let metadata = builtin_registry().metadata();
    assert!(
      metadata.len() >= 80,
      "builtin pack should grow with the reactivity matrix (got {})",
      metadata.len()
    );
    assert!(
      metadata.windows(2).all(|pair| matches!(pair, [first, second] if first.id < second.id)),
      "registry metadata must be sorted by stable rule ID"
    );
    assert!(
      metadata.iter().all(|meta| meta.confidence == Confidence::High),
      "the recommended preset must contain only high-confidence rules"
    );
  }

  #[test]
  fn every_builtin_rule_has_unique_metadata() {
    let metadata = builtin_registry().metadata();
    let unique_ids = metadata.iter().map(|meta| meta.id).collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
      unique_ids.len(),
      metadata.len(),
      "every rule module must register one unique rule ID"
    );
  }

  #[test]
  fn every_builtin_rule_has_documentation_file() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut missing = Vec::new();
    for meta in builtin_registry().metadata() {
      let path = root.join(format!("docs/{}.md", meta.documentation));
      if !path.is_file() {
        missing.push(format!("{} -> {}", meta.id, path.display()));
      }
    }
    assert!(missing.is_empty(), "missing rule docs:\n{}", missing.join("\n"));
  }

  const RETIRED_RULE_IDS: &[&str] = &[
    "vue-vet/correctness/no-define-emits-after-await",
    "vue-vet/correctness/no-define-model-after-await",
    "vue-vet/correctness/no-define-options-after-await",
    "vue-vet/correctness/no-define-props-after-await",
    "vue-vet/correctness/no-define-slots-after-await",
    "vue-vet/correctness/no-effect-scope-after-await",
    "vue-vet/correctness/no-get-current-instance-after-await",
    "vue-vet/correctness/no-inject-after-await",
    "vue-vet/correctness/no-next-tick-after-await",
    "vue-vet/correctness/no-on-activated-after-await",
    "vue-vet/correctness/no-on-before-mount-after-await",
    "vue-vet/correctness/no-on-before-unmount-after-await",
    "vue-vet/correctness/no-on-before-update-after-await",
    "vue-vet/correctness/no-on-deactivated-after-await",
    "vue-vet/correctness/no-on-error-captured-after-await",
    "vue-vet/correctness/no-on-mounted-after-await",
    "vue-vet/correctness/no-on-render-tracked-after-await",
    "vue-vet/correctness/no-on-render-triggered-after-await",
    "vue-vet/correctness/no-on-server-prefetch-after-await",
    "vue-vet/correctness/no-on-unmounted-after-await",
    "vue-vet/correctness/no-on-updated-after-await",
    "vue-vet/correctness/no-provide-after-await",
    "vue-vet/correctness/no-use-attrs-after-await",
    "vue-vet/correctness/no-use-css-module-after-await",
    "vue-vet/correctness/no-use-css-vars-after-await",
    "vue-vet/correctness/no-use-slots-after-await",
    "vue-vet/correctness/no-watch-after-await",
    "vue-vet/correctness/no-watch-effect-after-await",
    "vue-vet/correctness/no-watch-post-effect-after-await",
    "vue-vet/correctness/no-watch-sync-effect-after-await",
    "vue-vet/correctness/no-with-defaults-after-await",
    "vue-vet/reactivity/no-conditional-dependency-in-computed",
    "vue-vet/reactivity/no-conditional-dependency-in-effect-scope",
    "vue-vet/reactivity/no-conditional-dependency-in-render",
    "vue-vet/reactivity/no-conditional-dependency-in-watch-sources",
    "vue-vet/reactivity/no-conditional-watch-effect-dependency",
    "vue-vet/reactivity/prefer-explicit-sources-for-conditional-deps",
    "vue-vet/reactivity/no-self-trigger-in-watch-effect",
    "vue-vet/reactivity/no-self-trigger-in-watch-post-effect",
    "vue-vet/reactivity/no-self-trigger-in-watch-sync-effect",
  ];

  #[test]
  fn retired_rule_ids_are_absent_from_the_builtin_catalog() {
    assert_eq!(RETIRED_RULE_IDS.len(), 40);
    let ids = builtin_registry()
      .metadata()
      .into_iter()
      .map(|meta| meta.id)
      .collect::<std::collections::BTreeSet<_>>();
    let lingering =
      RETIRED_RULE_IDS.iter().copied().filter(|id| ids.contains(id)).collect::<Vec<_>>();
    assert!(lingering.is_empty(), "retired ids still registered: {lingering:?}");
    assert!(ids.contains("vue-vet/correctness/no-define-expose-after-await"));
    assert!(ids.contains("vue-vet/reactivity/no-computed-self-trigger"));
    assert!(ids.contains("vue-vet/reactivity/no-after-await-watch-effect-dependency"));
  }
}
