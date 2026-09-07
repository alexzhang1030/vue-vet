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
  pub invalid_custom_ref_interface: Vec<InvalidCustomRefInterfaceFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub inactive_scope_result: Vec<InactiveScopeResultFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub missing_torefs_key: Vec<MissingToRefsKeyFact>,
}

impl SourceContractFacts {
  #[must_use]
  pub const fn is_empty(&self) -> bool {
    self.trigger_ref_non_ref.is_empty()
      && self.torefs_non_proxy.is_empty()
      && self.primitive_reactive_target.is_empty()
      && self.watch_unwrapped_source.is_empty()
      && self.watch_replaced_object_source.is_empty()
      && self.invalid_custom_ref_interface.is_empty()
      && self.inactive_scope_result.is_empty()
      && self.missing_torefs_key.is_empty()
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

/// Demanded `customRef` factory capability.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomRefCapability {
  Get,
  Set,
}

impl CustomRefCapability {
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Get => "get",
      Self::Set => "set",
    }
  }
}

/// `customRef` factory missing/noncallable `get` or `set` plus a demanded `.value`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InvalidCustomRefInterfaceFact {
  pub interface_span: SourceSpan,
  pub demand_span: SourceSpan,
  pub factory_span: SourceSpan,
  pub missing: CustomRefCapability,
}

/// `effectScope().stop()` then `scope.run` whose result is used as an object.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[expect(clippy::struct_field_names, reason = "each field is a distinct causal span")]
pub struct InactiveScopeResultFact {
  pub consumer_span: SourceSpan,
  pub stop_span: SourceSpan,
  pub run_span: SourceSpan,
}

/// `toRefs` of a closed reactive object, then a missing key is dereferenced.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MissingToRefsKeyFact {
  pub demand_span: SourceSpan,
  pub torefs_span: SourceSpan,
  pub key: String,
}
