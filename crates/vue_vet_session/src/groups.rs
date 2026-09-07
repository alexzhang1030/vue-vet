//! Product mapping from composed rule IDs to canonical groups.
//!
//! The table lives here so `vue_vet_core` does not hardcode built-in IDs.

use std::collections::BTreeMap;

use vue_vet_config::{Config, RuleLevel};
use vue_vet_core::{
  RULE_INVENTORY_KIND, RULE_INVENTORY_SCHEMA_VERSION, RuleGroupDescriptor, RuleGroupId,
  RuleInventory, RuleInventoryCounts, RuleInventoryRow, RuleMeta,
};

use crate::registry::{composed_rule_metadata, known_rule_ids};

/// Sorted `(id, group)` pairs. Binary-searchable; each ID appears at most once.
pub static RULE_GROUP_TABLE: &[(&str, RuleGroupId)] = &[
  ("vue-vet/correctness/no-mutating-props", RuleGroupId::SourceContracts),
  ("vue-vet/project/unresolved-import", RuleGroupId::Project),
  ("vue-vet/project/unused-component", RuleGroupId::Project),
  ("vue-vet/reactivity/no-after-await-dependency-in-computed", RuleGroupId::Tracking),
  ("vue-vet/reactivity/no-after-await-dependency-in-effect-scope", RuleGroupId::Tracking),
  ("vue-vet/reactivity/no-after-await-dependency-in-watch-sources", RuleGroupId::Tracking),
  ("vue-vet/reactivity/no-after-await-watch-effect-dependency", RuleGroupId::Tracking),
  ("vue-vet/reactivity/no-computed-as-operand", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-computed-self-trigger", RuleGroupId::Derivation),
  ("vue-vet/reactivity/no-computed-without-dependency", RuleGroupId::Tracking),
  ("vue-vet/reactivity/no-deep-watch-on-reactive-root", RuleGroupId::Derivation),
  ("vue-vet/reactivity/no-deferred-callback-reactive-read-in-effect", RuleGroupId::Tracking),
  ("vue-vet/reactivity/no-effect-write-without-read", RuleGroupId::Tracking),
  ("vue-vet/reactivity/no-empty-watch-sources", RuleGroupId::Tracking),
  ("vue-vet/reactivity/no-extracted-reactive-collection-method", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-model-ref-as-operand", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-multiple-effects-same-target", RuleGroupId::Derivation),
  ("vue-vet/reactivity/no-nonreactive-props-destructure", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-on-scope-dispose-reactive-read", RuleGroupId::Lifetime),
  ("vue-vet/reactivity/no-outside-tracking-dependency-in-computed", RuleGroupId::Tracking),
  ("vue-vet/reactivity/no-outside-tracking-dependency-in-effect-scope", RuleGroupId::Tracking),
  ("vue-vet/reactivity/no-outside-tracking-dependency-in-watch-sources", RuleGroupId::Tracking),
  ("vue-vet/reactivity/no-primitive-reactive-target", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-props-snapshot-in-ref", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-reactive-destructure", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-reactive-read-during-pause-tracking", RuleGroupId::Tracking),
  ("vue-vet/reactivity/no-readonly-mutation", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-ref-as-operand", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-route-destructure", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-router-destructure", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-shallow-reactive-destructure", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-side-effects-in-computed", RuleGroupId::Derivation),
  ("vue-vet/reactivity/no-stale-prop-flow", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-torefs-on-non-proxy", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-trigger-ref-on-non-ref", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-unused-computed-binding", RuleGroupId::Derivation),
  ("vue-vet/reactivity/no-v-model-nonreactive-source", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-watch-callback-as-tracking-scope", RuleGroupId::Tracking),
  ("vue-vet/reactivity/no-watch-replaced-object-source", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/no-watch-unwrapped-source", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/prefer-computed", RuleGroupId::Derivation),
  ("vue-vet/reactivity/prefer-store-to-refs", RuleGroupId::SourceContracts),
  ("vue-vet/reactivity/prefer-watch-over-effect-for-single-source", RuleGroupId::Derivation),
];

/// Lookup the canonical group for a stable rule ID.
#[must_use]
pub fn group_of(rule_id: &str) -> Option<RuleGroupId> {
  RULE_GROUP_TABLE
    .binary_search_by_key(&rule_id, |entry| entry.0)
    .ok()
    .and_then(|index| RULE_GROUP_TABLE.get(index).map(|entry| entry.1))
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
  for id in known_rule_ids() {
    match group_of(id) {
      Some(group) if selected.contains(&group) => {}
      Some(_) | None => {
        config.rules.insert(id.to_string(), RuleLevel::Off);
      }
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
      let group = group_of(meta.id);
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
  use super::*;

  #[test]
  fn group_table_is_sorted_unique_and_live() {
    assert!(!RULE_GROUP_TABLE.is_empty(), "group table must not be empty");
    let known = raw_composed_ids();
    assert_unique_ids(&known);
    let mut previous = "";
    let mut seen = std::collections::BTreeSet::new();
    for (id, _) in RULE_GROUP_TABLE {
      assert!(*id > previous, "group table must be strictly sorted: {id} after {previous}");
      assert!(seen.insert(*id), "group table must be disjoint: duplicate {id}");
      assert!(known.contains(id), "mapped id must exist in composed registry: {id}");
      previous = id;
    }
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
    assert!(inventory.rules.iter().any(|row| row.id == "vue-vet/project/unresolved-import"));
    assert!(inventory.rules.iter().any(|row| row.id == "vue-vet/project/unused-component"));
    let tracking = rule_inventory(&[RuleGroupId::Tracking, RuleGroupId::Tracking]);
    assert!(tracking.rules.iter().all(|row| row.group == Some(RuleGroupId::Tracking)));
    assert_eq!(tracking.counts.total, tracking.rules.len());
  }

  fn raw_composed_ids() -> Vec<&'static str> {
    let mut ids: Vec<_> =
      crate::file_analysis_registry().metadata().into_iter().map(|meta| meta.id).collect();
    ids.extend(vue_vet_project::PROJECT_RULE_IDS);
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
