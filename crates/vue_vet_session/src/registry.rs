//! Built-in + practice + project rule metadata used by session scans.
use std::sync::LazyLock;

use vue_vet_core::{Confidence, MIGRATION_CATEGORY, RuleGroupId, RuleMeta, RuleRegistry, Severity};
use vue_vet_practice::practice_rules;
use vue_vet_project::{PROJECT_RULE_IDS, VAPOR_MIGRATION_RULE_IDS};
use vue_vet_rules::builtin_rules;

/// Project-graph rules live outside `builtin_registry` but share the same docs key.
static PROJECT_RULE_META: [RuleMeta; 2] = [
  RuleMeta {
    id: PROJECT_RULE_IDS[0],
    category: "project",
    default_severity: Severity::Error,
    confidence: Confidence::High,
    documentation: "project-graph",
    group: Some(RuleGroupId::Project),
  },
  RuleMeta {
    id: PROJECT_RULE_IDS[1],
    category: "project",
    default_severity: Severity::Warning,
    confidence: Confidence::Medium,
    documentation: "project-graph",
    group: Some(RuleGroupId::Project),
  },
];

static VAPOR_MIGRATION_RULE_META: [RuleMeta; 5] = [
  RuleMeta {
    id: VAPOR_MIGRATION_RULE_IDS[0],
    category: MIGRATION_CATEGORY,
    default_severity: Severity::Info,
    confidence: Confidence::High,
    documentation: "rules/migration/vapor-assessment",
    group: Some(RuleGroupId::VaporMigration),
  },
  RuleMeta {
    id: VAPOR_MIGRATION_RULE_IDS[1],
    category: MIGRATION_CATEGORY,
    default_severity: Severity::Info,
    confidence: Confidence::High,
    documentation: "rules/migration/vapor-interop-required",
    group: Some(RuleGroupId::VaporMigration),
  },
  RuleMeta {
    id: VAPOR_MIGRATION_RULE_IDS[2],
    category: MIGRATION_CATEGORY,
    default_severity: Severity::Info,
    confidence: Confidence::High,
    documentation: "rules/migration/vapor-memo-contract-dropped",
    group: Some(RuleGroupId::VaporMigration),
  },
  RuleMeta {
    id: VAPOR_MIGRATION_RULE_IDS[3],
    category: MIGRATION_CATEGORY,
    default_severity: Severity::Info,
    confidence: Confidence::High,
    documentation: "rules/migration/vapor-runtime-envelope",
    group: Some(RuleGroupId::VaporMigration),
  },
  RuleMeta {
    id: VAPOR_MIGRATION_RULE_IDS[4],
    category: MIGRATION_CATEGORY,
    default_severity: Severity::Info,
    confidence: Confidence::High,
    documentation: "rules/migration/vapor-sfc-compile-contract",
    group: Some(RuleGroupId::VaporMigration),
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
  composed_rule_metadata().into_iter().find(|meta| meta.id == rule_id)
}

pub fn known_rule_ids() -> impl Iterator<Item = &'static str> {
  composed_rule_metadata().into_iter().map(|meta| meta.id)
}

/// Built-in, practice, and project metadata sorted by stable ID.
///
/// Does not drop duplicate registrations: inventory and tests must see them.
#[must_use]
pub fn composed_rule_metadata() -> Vec<&'static RuleMeta> {
  let mut metas = file_analysis_registry().metadata();
  metas.extend(PROJECT_RULE_META.iter());
  metas.extend(VAPOR_MIGRATION_RULE_META.iter());
  metas.sort_by_key(|meta| meta.id);
  metas
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn composed_runtime_registry_excludes_retired_ids_and_keeps_project_rules() {
    let ids = known_rule_ids().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
      ids.len(),
      file_analysis_registry().metadata().len()
        + PROJECT_RULE_IDS.len()
        + VAPOR_MIGRATION_RULE_IDS.len()
    );
    assert!(ids.contains(PROJECT_RULE_IDS[0]));
    assert!(ids.contains(PROJECT_RULE_IDS[1]));
    assert!(ids.contains(VAPOR_MIGRATION_RULE_IDS[0]));
    assert!(!ids.contains("vue-vet/correctness/no-on-mounted-after-await"));
    assert!(!ids.contains("vue-vet/reactivity/no-conditional-watch-effect-dependency"));
    assert!(!ids.contains("vue-vet/reactivity/no-self-trigger-in-watch-effect"));
  }
}
