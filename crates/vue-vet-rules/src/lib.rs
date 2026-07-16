use vue_vet_core::RuleRegistry;

mod rules;

#[must_use]
pub fn builtin_registry() -> RuleRegistry {
  RuleRegistry::new(rules::builtins())
}

#[cfg(test)]
mod tests {
  use vue_vet_core::Confidence;

  use super::*;

  #[test]
  fn builtins_have_stable_metadata() {
    let metadata = builtin_registry().metadata();
    assert_eq!(metadata.len(), 28, "the recommended preset contains twenty-eight rules");
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
    let unique_ids =
      metadata.iter().map(|meta| meta.id).collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique_ids.len(), metadata.len(), "every rule module must register one unique rule ID");
  }
}
