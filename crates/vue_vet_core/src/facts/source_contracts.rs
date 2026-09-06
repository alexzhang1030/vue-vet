//! Neutral Vue API source-contract facts (issue #224).
//!
//! Produced by `vue_vet_oxc`; consumed by `vue_vet_rules`. No parser AST types.
//!
//! This slice adds `toref_ignored_key` and `effect_scope_callback`.

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
  pub toref_ignored_key: Vec<ToRefIgnoredKeyFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub effect_scope_callback: Vec<SourceContractSiteFact>,
}

impl SourceContractFacts {
  #[must_use]
  pub const fn is_empty(&self) -> bool {
    self.trigger_ref_non_ref.is_empty()
      && self.torefs_non_proxy.is_empty()
      && self.primitive_reactive_target.is_empty()
      && self.watch_unwrapped_source.is_empty()
      && self.watch_replaced_object_source.is_empty()
      && self.toref_ignored_key.is_empty()
      && self.effect_scope_callback.is_empty()
  }
}

/// One call/argument site with a proven contract failure.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceContractSiteFact {
  pub span: SourceSpan,
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub api: String,
}

/// Proven `toRef(source, key)` overload that ignores a static key.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToRefIgnoredKeyFact {
  pub span: SourceSpan,
  pub reason: ToRefIgnoredKeyReason,
}

/// Runtime `toRef` branch that discards the key argument.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToRefIgnoredKeyReason {
  Ref,
  Function,
  Primitive,
}

/// `watch(objectMember)` plus a later same-identity property replacement.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WatchReplacedObjectSourceFact {
  pub source_span: SourceSpan,
  pub replacement_span: SourceSpan,
  pub object: String,
  pub property: String,
}
