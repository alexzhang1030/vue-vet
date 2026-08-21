//! Built-in + practice + project rule metadata used by session scans.
use std::sync::LazyLock;

use vue_vet_core::{Confidence, RuleMeta, RuleRegistry, Severity};
use vue_vet_practice::practice_rules;
use vue_vet_project::PROJECT_RULE_IDS;
use vue_vet_rules::builtin_rules;

/// Project-graph rules live outside `builtin_registry` but share the same docs key.
static PROJECT_RULE_META: [RuleMeta; 2] = [
  RuleMeta {
    id: PROJECT_RULE_IDS[0],
    category: "project",
    default_severity: Severity::Error,
    confidence: Confidence::High,
    documentation: "project-graph",
  },
  RuleMeta {
    id: PROJECT_RULE_IDS[1],
    category: "project",
    default_severity: Severity::Warning,
    confidence: Confidence::Medium,
    documentation: "project-graph",
  },
];

/// Per-file lint + practice registry shared by session scans.
static FILE_RULES: LazyLock<RuleRegistry> = LazyLock::new(|| {
  let mut rules = builtin_rules();
  rules.extend(practice_rules());
  RuleRegistry::new(rules)
});

#[must_use]
pub fn file_analysis_registry() -> &'static RuleRegistry {
  &FILE_RULES
}

/// Look up built-in, practice, or project rule metadata by exact id.
#[must_use]
pub fn resolve_rule_meta(rule_id: &str) -> Option<&'static RuleMeta> {
  let mut metas = file_analysis_registry().metadata();
  metas.extend(PROJECT_RULE_META.iter());
  metas.into_iter().find(|meta| meta.id == rule_id)
}

pub fn known_rule_ids() -> impl Iterator<Item = &'static str> {
  file_analysis_registry().metadata().into_iter().map(|meta| meta.id).chain(PROJECT_RULE_IDS)
}
