//! Serializable rule-group identity. Product catalogs of rule IDs live in session.

use serde::{Deserialize, Serialize};

/// Canonical product groups for inventory and scan selection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleGroupId {
  Tracking,
  SourceContracts,
  Lifetime,
  Derivation,
  Project,
}

impl RuleGroupId {
  /// Stable CLI / JSON slug.
  #[must_use]
  pub const fn slug(self) -> &'static str {
    match self {
      Self::Tracking => "tracking",
      Self::SourceContracts => "source-contracts",
      Self::Lifetime => "lifetime",
      Self::Derivation => "derivation",
      Self::Project => "project",
    }
  }

  /// Human title for inventory rows.
  #[must_use]
  pub const fn title(self) -> &'static str {
    match self {
      Self::Tracking => "Tracking",
      Self::SourceContracts => "Source contracts",
      Self::Lifetime => "Lifetime",
      Self::Derivation => "Derivation",
      Self::Project => "Project",
    }
  }

  /// Every canonical group, in display order.
  pub const ALL: [Self; 5] =
    [Self::Tracking, Self::SourceContracts, Self::Lifetime, Self::Derivation, Self::Project];

  /// Parse a CLI slug. Unknown slugs return `None`.
  #[must_use]
  pub fn parse_slug(slug: &str) -> Option<Self> {
    Self::ALL.iter().copied().find(|group| group.slug() == slug)
  }
}

/// One composed-registry row for `--list-rules`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuleInventoryRow {
  pub id: String,
  pub category: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub group: Option<RuleGroupId>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub group_title: Option<String>,
  pub severity: crate::Severity,
}

/// Versioned inventory document. Independent of project configuration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuleInventory {
  pub schema_version: u8,
  pub kind: &'static str,
  pub groups: Vec<RuleGroupDescriptor>,
  pub counts: RuleInventoryCounts,
  pub rules: Vec<RuleInventoryRow>,
}

/// Canonical group identity for inventory JSON.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuleGroupDescriptor {
  pub id: RuleGroupId,
  pub title: &'static str,
}

/// Counts taken from the composed inventory (after optional group filter).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuleInventoryCounts {
  pub total: usize,
  pub mapped: usize,
  pub unmapped: usize,
  pub by_group: std::collections::BTreeMap<RuleGroupId, usize>,
}

pub const RULE_INVENTORY_KIND: &str = "rule_inventory";
pub const RULE_INVENTORY_SCHEMA_VERSION: u8 = 1;
