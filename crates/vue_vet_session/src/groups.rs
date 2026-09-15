//! Product groups come from composed `RuleMeta.group` (not a parallel table).

use std::collections::BTreeMap;

use vue_vet_config::{AssessmentMode, Config, RuleLevel};
use vue_vet_core::{
  RULE_INVENTORY_KIND, RULE_INVENTORY_SCHEMA_VERSION, RuleGroupDescriptor, RuleGroupId,
  RuleInventory, RuleInventoryCounts, RuleInventoryRow, RuleMeta,
};

use crate::registry::{composed_rule_metadata, resolve_rule_meta};

/// Lookup the canonical group for a stable rule ID.
#[must_use]
pub fn group_of(rule_id: &str) -> Option<RuleGroupId> {
  resolve_rule_meta(rule_id).and_then(|meta| meta.group)
}

/// Sort and deduplicate a group union so identical selections serialize the same.
#[must_use]
pub fn normalize_groups(groups: &[RuleGroupId]) -> Vec<RuleGroupId> {
  let mut ordered = groups.to_vec();
  ordered.sort();
  ordered.dedup();
  ordered
}

/// Disable known IDs that are not in the selected groups. Leaves selected entries intact.
pub fn apply_selected_groups(config: &mut Config, groups: &[RuleGroupId]) {
  let selected = normalize_groups(groups);
  if selected.is_empty() {
    return;
  }
  for meta in composed_rule_metadata() {
    match meta.group {
      Some(group) if selected.contains(&group) => {}
      Some(_) | None => {
        config.rules.insert(meta.id.to_string(), RuleLevel::Off);
      }
    }
  }
}

/// Default-off for vapor-migration IDs unless `assessment = "vapor"` or the group is selected.
pub fn apply_vapor_migration_defaults(config: &mut Config, groups: &[RuleGroupId]) {
  let enabled =
    config.assessment == AssessmentMode::Vapor || groups.contains(&RuleGroupId::VaporMigration);
  if enabled {
    return;
  }
  for meta in composed_rule_metadata() {
    if meta.group == Some(RuleGroupId::VaporMigration) {
      config.rules.entry(meta.id.to_string()).or_insert(RuleLevel::Off);
    }
  }
}

/// Composed registry inventory, optionally narrowed to a group union.
#[must_use]
pub fn rule_inventory(filter: &[RuleGroupId]) -> RuleInventory {
  let selected = normalize_groups(filter);
  let metas = composed_rule_metadata();
  let rules = metas
    .into_iter()
    .filter_map(|meta| {
      let group = meta.group;
      if !selected.is_empty() && group.is_none_or(|group| !selected.contains(&group)) {
        return None;
      }
      Some(inventory_row(meta, group))
    })
    .collect::<Vec<_>>();
  let mut by_group = BTreeMap::new();
  let mut mapped = 0;
  for row in &rules {
    if let Some(group) = row.group {
      mapped += 1;
      *by_group.entry(group).or_insert(0) += 1;
    }
  }
  let total = rules.len();
  RuleInventory {
    schema_version: RULE_INVENTORY_SCHEMA_VERSION,
    kind: RULE_INVENTORY_KIND,
    groups: RuleGroupId::ALL
      .into_iter()
      .map(|id| RuleGroupDescriptor { id, title: id.title() })
      .collect(),
    counts: RuleInventoryCounts { total, mapped, unmapped: total.saturating_sub(mapped), by_group },
    rules,
  }
}

fn inventory_row(meta: &RuleMeta, group: Option<RuleGroupId>) -> RuleInventoryRow {
  RuleInventoryRow {
    id: meta.id.into(),
    category: meta.category.into(),
    group,
    group_title: group.map(RuleGroupId::title).map(str::to_string),
    severity: meta.default_severity,
  }
}

#[cfg(test)]
mod tests {
  use std::path::PathBuf;

  use super::*;

  #[test]
  fn every_registry_id_has_a_documentation_file() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut missing = Vec::new();
    for meta in composed_rule_metadata() {
      let path = root.join(format!("docs/{}.md", meta.documentation));
      if !path.is_file() {
        missing.push(format!("{} -> {}", meta.id, path.display()));
      }
    }
    assert!(missing.is_empty(), "missing rule docs:\n{}", missing.join("\n"));
  }

  #[test]
  #[expect(clippy::expect_used, reason = "unit test asserts serialization")]
  fn identical_group_unions_produce_identical_effective_config() {
    let mut first = Config::default();
    let mut second = Config::default();
    apply_selected_groups(
      &mut first,
      &[RuleGroupId::Tracking, RuleGroupId::Project, RuleGroupId::Tracking],
    );
    apply_selected_groups(&mut second, &[RuleGroupId::Project, RuleGroupId::Tracking]);
    assert_eq!(first, second);
    assert_eq!(
      serde_json::to_vec(&first).expect("serialize first"),
      serde_json::to_vec(&second).expect("serialize second")
    );
  }

  #[test]
  fn group_filter_preserves_selected_overrides_and_offs_the_rest() {
    let mut config = Config::default();
    config.rules.insert("vue-vet/reactivity/no-empty-watch-sources".into(), RuleLevel::Error);
    config.rules.insert("vue-vet/security/no-v-html".into(), RuleLevel::Error);
    apply_selected_groups(&mut config, &[RuleGroupId::Tracking]);
    assert_eq!(
      config.rules.get("vue-vet/reactivity/no-empty-watch-sources").copied(),
      Some(RuleLevel::Error)
    );
    assert_eq!(config.rules.get("vue-vet/security/no-v-html").copied(), Some(RuleLevel::Off));
  }

  #[test]
  fn inventory_is_sorted_unique_and_includes_project_ids() {
    let inventory = rule_inventory(&[]);
    let raw = raw_composed_ids();
    assert_unique_ids(&raw);
    let composed: Vec<_> = composed_rule_metadata().into_iter().map(|meta| meta.id).collect();
    assert_eq!(composed, raw, "composed metadata must be the raw file registry plus project IDs");
    let inventory_ids: Vec<_> = inventory.rules.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(inventory_ids, raw);
    assert_eq!(inventory.counts.total, raw.len());
    assert!(
      composed_rule_metadata().len() >= 150,
      "composed registry wipe guard (got {})",
      composed_rule_metadata().len()
    );
    assert!(inventory.rules.iter().any(|row| row.id == "vue-vet/project/unresolved-import"));
    assert!(inventory.rules.iter().any(|row| row.id == "vue-vet/project/unused-component"));
    let tracking = rule_inventory(&[RuleGroupId::Tracking, RuleGroupId::Tracking]);
    assert!(tracking.rules.iter().all(|row| row.group == Some(RuleGroupId::Tracking)));
    assert_eq!(tracking.counts.total, tracking.rules.len());
    assert_eq!(tracking.counts.total, registry_ids_in(RuleGroupId::Tracking).len());
  }

  #[test]
  fn inventory_group_counts_match_composed_registry() {
    let inventory = rule_inventory(&[]);
    assert_eq!(inventory.counts.total, composed_rule_metadata().len());
    for group in RuleGroupId::ALL {
      let filtered = rule_inventory(&[group]);
      let expected = registry_ids_in(group);
      let printed: Vec<_> = filtered.rules.iter().map(|row| row.id.as_str()).collect();
      assert_eq!(printed, expected, "{group:?} inventory must match registry metadata");
      assert_eq!(filtered.counts.total, expected.len());
    }
    let lifetime = registry_ids_in(RuleGroupId::Lifetime);
    let tracking = registry_ids_in(RuleGroupId::Tracking);
    for id in lifetime {
      assert!(!tracking.contains(&id), "lifetime id {id} must not also be tracking");
    }
    assert_eq!(
      group_of("vue-vet/reactivity/no-pre-flush-template-ref-demand"),
      Some(RuleGroupId::Derivation)
    );
    assert_eq!(
      group_of("vue-vet/reactivity/no-v-memo-blocked-ref-demand"),
      Some(RuleGroupId::Tracking)
    );
    assert_eq!(
      group_of("vue-vet/practice/prefer-attached-effect-scope"),
      Some(RuleGroupId::Lifetime)
    );
  }

  fn registry_ids_in(group: RuleGroupId) -> Vec<&'static str> {
    composed_rule_metadata()
      .into_iter()
      .filter(|meta| meta.group == Some(group))
      .map(|meta| meta.id)
      .collect()
  }

  fn raw_composed_ids() -> Vec<&'static str> {
    let mut ids: Vec<_> =
      crate::file_analysis_registry().metadata().into_iter().map(|meta| meta.id).collect();
    ids.extend(vue_vet_project::PROJECT_RULE_IDS);
    ids.extend(vue_vet_project::VAPOR_MIGRATION_RULE_IDS);
    ids.sort_unstable();
    ids
  }

  fn assert_unique_ids(ids: &[&str]) {
    let mut seen = std::collections::BTreeSet::new();
    for id in ids {
      assert!(seen.insert(*id), "duplicate rule registration `{id}` must be visible in inventory");
    }
  }
}
