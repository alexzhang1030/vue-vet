//! Evidence completeness shared by analysis, reporters, and editor surfaces.
//!
//! Findings and evidence are separate products: an empty finding collection
//! can accompany complete analysis, partial analysis, or an unavailable
//! analysis stage. Consumers use this contract to make that distinction
//! without parsing human messages.

use serde::{Deserialize, Serialize};

/// Coverage level for the facts that support a scan result.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
  /// Every requested analysis stage produced its contracted evidence.
  #[default]
  Complete,
  /// The scan produced usable facts with one or more bounded gaps.
  Partial,
  /// A requested analysis stage could not produce usable evidence.
  Unavailable,
}

/// Stable reason code for a bounded evidence gap.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceGapCode {
  Parse,
  Resolution,
  Linking,
  ModuleReactivity,
  UnsupportedSyntax,
  Capacity,
  Cancelled,
  Analysis,
}

/// Counted evidence gap suitable for machine consumers.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct EvidenceGap {
  pub code: EvidenceGapCode,
  pub count: usize,
}

/// Evidence status and its deterministic gap list.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceSummary {
  pub status: EvidenceStatus,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub gaps: Vec<EvidenceGap>,
}

impl EvidenceSummary {
  #[must_use]
  pub fn complete() -> Self {
    Self::default()
  }

  #[must_use]
  pub fn unavailable(gaps: impl IntoIterator<Item = EvidenceGap>) -> Self {
    Self::with_status(EvidenceStatus::Unavailable, gaps)
  }

  #[must_use]
  pub fn partial(gaps: impl IntoIterator<Item = EvidenceGap>) -> Self {
    Self::with_status(EvidenceStatus::Partial, gaps)
  }

  #[must_use]
  pub fn with_status(status: EvidenceStatus, gaps: impl IntoIterator<Item = EvidenceGap>) -> Self {
    let mut gaps = gaps.into_iter().collect::<Vec<_>>();
    gaps.sort_unstable();
    let mut merged: Vec<EvidenceGap> = Vec::with_capacity(gaps.len());
    for gap in gaps {
      if let Some(previous) = merged.last_mut()
        && previous.code == gap.code
      {
        previous.count = previous.count.saturating_add(gap.count);
      } else {
        merged.push(gap);
      }
    }
    Self { status, gaps: merged }
  }

  #[must_use]
  pub const fn is_complete(&self) -> bool {
    matches!(self.status, EvidenceStatus::Complete)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn gaps_are_sorted_and_merged_by_code() {
    let summary = EvidenceSummary::partial([
      EvidenceGap { code: EvidenceGapCode::ModuleReactivity, count: 1 },
      EvidenceGap { code: EvidenceGapCode::Parse, count: 2 },
      EvidenceGap { code: EvidenceGapCode::ModuleReactivity, count: 3 },
    ]);
    assert_eq!(summary.status, EvidenceStatus::Partial);
    assert_eq!(summary.gaps.len(), 2);
    assert_eq!(summary.gaps.first().map(|gap| gap.code), Some(EvidenceGapCode::Parse));
    assert_eq!(summary.gaps.get(1).map(|gap| gap.count), Some(4));
  }

  #[test]
  fn complete_has_no_gap_payload() {
    let summary = EvidenceSummary::complete();
    assert!(summary.is_complete());
    assert!(summary.gaps.is_empty());
  }
}
