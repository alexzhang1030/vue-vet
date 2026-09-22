//! Structured reasons for a content-addressed cache miss.

use serde::{Deserialize, Serialize};

/// Why a cache lookup could not reuse persisted work.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum CacheRejection {
  /// No entry exists for the current content key.
  Absent,
  /// An entry exists but its envelope cannot be decoded.
  Undecodable,
  /// The stored graph uses a different DTO contract.
  GraphSchema { found: u32, expected: u32 },
}

impl CacheRejection {
  #[must_use]
  pub const fn id(self) -> &'static str {
    match self {
      Self::Absent => "absent",
      Self::Undecodable => "undecodable",
      Self::GraphSchema { .. } => "graph-schema",
    }
  }
}
