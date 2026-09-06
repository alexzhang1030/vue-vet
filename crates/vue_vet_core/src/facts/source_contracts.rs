//! Neutral Vue API source-contract facts (issue #224).
//!
//! Produced by `vue_vet_oxc`; consumed by `vue_vet_rules`. No parser AST types.

use serde::{Deserialize, Serialize};

use crate::diagnostics::SourceSpan;

/// Proven Vue API contract sites collected from Oxc semantics.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceContractFacts {
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub trigger_ref_non_ref: Vec<SourceContractSiteFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub torefs_non_proxy: Vec<SourceContractSiteFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub primitive_reactive_target: Vec<SourceContractSiteFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub watch_unwrapped_source: Vec<SourceContractSiteFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub watch_replaced_object_source: Vec<WatchReplacedObjectSourceFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub watch_callback_contracts: Vec<WatchCallbackContractFact>,
}

impl SourceContractFacts {
  #[must_use]
  pub const fn is_empty(&self) -> bool {
    self.trigger_ref_non_ref.is_empty()
      && self.torefs_non_proxy.is_empty()
      && self.primitive_reactive_target.is_empty()
      && self.watch_unwrapped_source.is_empty()
      && self.watch_replaced_object_source.is_empty()
      && self.watch_callback_contracts.is_empty()
  }
}

/// One call/argument site with a proven contract failure.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceContractSiteFact {
  pub span: SourceSpan,
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub api: String,
}

/// `watch(objectMember)` plus a later same-identity property replacement.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WatchReplacedObjectSourceFact {
  pub source_span: SourceSpan,
  pub replacement_span: SourceSpan,
  pub object: String,
  pub property: String,
}

/// Proven watch-callback contract failure (once/immediate discard or root identity).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WatchCallbackContractFact {
  pub watch_span: SourceSpan,
  pub source_span: SourceSpan,
  pub guard_span: SourceSpan,
  pub reason: WatchCallbackContractReason,
}

/// Why a watch callback is proven useless for later value-dependent work.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[repr(u8)]
#[serde(rename_all = "kebab-case")]
pub enum WatchCallbackContractReason {
  OnceImmediateUndefinedGuard,
  ReactiveRootIdentityGuard,
}
