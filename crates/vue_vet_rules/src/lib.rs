//! Built-in semantic lint / gate rules (excludes practice suggestions).
//!
//! Each rule is a self-contained module under `rules/` with stable metadata.
//! Shared fact walks live in [`vue_vet_rule_query`]. See the crate README and
//! `docs/rules/README.md`.

use vue_vet_core::{Rule, RuleRegistry};

mod rules;

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
}
