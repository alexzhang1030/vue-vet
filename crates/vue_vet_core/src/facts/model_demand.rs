//! Component model-default and shared-default demand facts.
//!
//! Produced by Oxc (per-file producers) and the project join (findings).
//! No parser AST types.

use serde::{Deserialize, Serialize};

use crate::diagnostics::SourceSpan;

/// How a `defineModel` default is produced.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelDefaultOrigin {
  LiteralPrimitive,
  SharedObjectFactory,
  /// Object/array literal default. Vue reuses that identity across instances.
  SharedObjectLiteral,
  FreshObjectFactory,
  Unknown,
}

/// Proven primitive kind for a model default or ref initializer.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelPrimitiveKind {
  Number,
  String,
  Boolean,
}

impl ModelPrimitiveKind {
  /// Whether this kind supplies an unguarded native callable member.
  #[must_use]
  pub fn supplies(self, member: &str) -> bool {
    match self {
      Self::Number => matches!(member, "toFixed" | "toExponential" | "toPrecision"),
      Self::String => matches!(
        member,
        "toUpperCase"
          | "toLowerCase"
          | "slice"
          | "charAt"
          | "substring"
          | "concat"
          | "includes"
          | "startsWith"
          | "endsWith"
          | "indexOf"
      ),
      Self::Boolean => false,
    }
  }
}

/// Current ordinary-ref initializer kind (not a Vue API result).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefInitKind {
  Undefined,
  Null,
  Number,
  String,
  Boolean,
  Object,
  Unknown,
}

impl RefInitKind {
  #[must_use]
  pub const fn as_primitive(self) -> Option<ModelPrimitiveKind> {
    match self {
      Self::Number => Some(ModelPrimitiveKind::Number),
      Self::String => Some(ModelPrimitiveKind::String),
      Self::Boolean => Some(ModelPrimitiveKind::Boolean),
      Self::Undefined | Self::Null | Self::Object | Self::Unknown => None,
    }
  }

  #[must_use]
  pub fn supplies(self, member: &str) -> bool {
    self.as_primitive().is_some_and(|kind| kind.supplies(member))
  }

  #[must_use]
  pub const fn is_undefined(self) -> bool {
    matches!(self, Self::Undefined)
  }
}

/// Child `defineModel` default producer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelDefaultFact {
  pub span: SourceSpan,
  pub model_name: String,
  pub origin: ModelDefaultOrigin,
  /// Identifier bound to the `defineModel` result (`const model = defineModel(...)`).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub binding: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub primitive: Option<ModelPrimitiveKind>,
  /// Normalized primitive literal text (`1`, `a`, `true`) for value identity.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub primitive_text: Option<String>,
  /// Local identifier returned by a shared factory (`() => shared`).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub shared_binding: Option<String>,
  /// Own primitive paths on a shared object/array literal default.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub own_paths: Vec<(String, ModelPrimitiveKind)>,
  #[serde(default, skip_serializing_if = "is_false")]
  pub has_get_set: bool,
  #[serde(default, skip_serializing_if = "is_false")]
  pub escaped: bool,
}

/// Module-owned object used as a shared default identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SharedObjectBindingFact {
  pub span: SourceSpan,
  pub binding: String,
  /// Proven primitive kinds of own data properties (`n` → number).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub own_paths: Vec<(String, ModelPrimitiveKind)>,
  #[serde(default, skip_serializing_if = "is_false")]
  pub escaped: bool,
}

/// Ordinary `ref` / `shallowRef` initializer in setup.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OrdinaryRefInitFact {
  pub span: SourceSpan,
  pub binding: String,
  pub kind: RefInitKind,
  #[serde(default, skip_serializing_if = "is_false")]
  pub escaped: bool,
}

/// Unguarded native member demand inside a proven `onMounted` callback.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MountedMemberDemandFact {
  pub span: SourceSpan,
  pub receiver: String,
  pub member: String,
  #[serde(default, skip_serializing_if = "is_false")]
  pub optional: bool,
  #[serde(default, skip_serializing_if = "is_false")]
  pub guarded: bool,
}

/// `defineExpose({ name })` keys in setup.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DefineExposeFact {
  pub span: SourceSpan,
  pub names: Vec<String>,
}

/// Native member demand through a template-ref instance chain.
///
/// `right.value.model.n.toFixed()` → instance `right`, path `["model","n"]`,
/// member `toFixed`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InstanceMemberDemandFact {
  pub span: SourceSpan,
  pub instance: String,
  pub path: Vec<String>,
  pub member: String,
  #[serde(default, skip_serializing_if = "is_false")]
  pub optional: bool,
  #[serde(default, skip_serializing_if = "is_false")]
  pub guarded: bool,
}

/// Write through a template-ref instance chain that changes a primitive path.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InstancePathWriteFact {
  pub span: SourceSpan,
  pub instance: String,
  pub path: Vec<String>,
  pub rhs_kind: RefInitKind,
}

/// Child model assignment that would emit (`model.value = changed`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelValueWriteFact {
  pub span: SourceSpan,
  pub binding: String,
  pub rhs_kind: RefInitKind,
  /// Normalized primitive literal text when the RHS is a proven literal.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub rhs_text: Option<String>,
  /// True when the write assigns the same proven default *value* of this binding.
  #[serde(default, skip_serializing_if = "is_false")]
  pub unchanged_default: bool,
}

/// Joined finding: parent demand before the child default initializes it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnsyncedModelParentDemandFact {
  pub demand_span: SourceSpan,
  pub parent_version_span: SourceSpan,
  pub v_model_span: SourceSpan,
  pub child_default_span: SourceSpan,
  /// Normalized workspace-relative child path.
  pub child_file: String,
  pub model_name: String,
  pub parent_binding: String,
  pub demanded_member: String,
}

/// Joined finding: sibling instance demand after a shared-default write.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SharedDefaultCrossInstanceDemandFact {
  pub demand_span: SourceSpan,
  pub default_span: SourceSpan,
  pub instance_a_span: SourceSpan,
  pub instance_b_span: SourceSpan,
  pub write_span: SourceSpan,
  /// Normalized workspace-relative child path.
  pub child_file: String,
  pub path: String,
  pub demanded_member: String,
}

/// Per-file joined model-demand findings (project → file rules).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelDemandFileFacts {
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub unsynced_parent_demands: Vec<UnsyncedModelParentDemandFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub shared_cross_instance_demands: Vec<SharedDefaultCrossInstanceDemandFact>,
}

impl ModelDemandFileFacts {
  #[must_use]
  pub const fn is_empty(&self) -> bool {
    self.unsynced_parent_demands.is_empty() && self.shared_cross_instance_demands.is_empty()
  }
}

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde skip_serializing_if takes &T")]
const fn is_false(value: &bool) -> bool {
  !*value
}
