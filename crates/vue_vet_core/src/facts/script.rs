use serde::{Deserialize, Serialize};

use crate::diagnostics::SourceSpan;

use super::{ReactivityGraph, TemplateFacts};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptKind {
  Script,
  Setup,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptImportFact {
  pub source: String,
  pub imported: String,
  pub local: String,
  pub span: SourceSpan,
}

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde skip_serializing_if takes &T")]
const fn is_false(value: &bool) -> bool {
  !*value
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptBindingFact {
  pub name: String,
  pub reads: usize,
  pub writes: usize,
  pub span: SourceSpan,
  /// Module export (`export const`, `export function`, `export { name }`, default).
  /// Absence-of-use rules must not treat exported API as dead locals.
  #[serde(default, skip_serializing_if = "is_false")]
  pub exported: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptCallFact {
  pub callee: String,
  pub assigned_to: Option<String>,
  pub resolved_import: Option<(String, String)>,
  /// Identifier argument names in source order (non-identifier args omitted).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub argument_identifiers: Vec<String>,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptMemberWriteFact {
  pub object: String,
  pub property: Option<String>,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptDestructureFact {
  pub source_call: String,
  pub span: SourceSpan,
}

/// Identifier used as an operand where a ref object's object-identity is almost
/// certainly a mistake (arithmetic / comparison / unary), not a `.value` read.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptOperandFact {
  pub name: String,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptBlockFacts {
  pub kind: ScriptKind,
  pub language: String,
  pub imports: Vec<ScriptImportFact>,
  pub bindings: Vec<ScriptBindingFact>,
  pub calls: Vec<ScriptCallFact>,
  pub member_writes: Vec<ScriptMemberWriteFact>,
  pub destructures: Vec<ScriptDestructureFact>,
  /// SFC-absolute end offsets of top-level `await` expressions in this block.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub top_level_await_ends: Vec<usize>,
  /// Identifiers used as binary/unary/logical operands (for ref-as-operand rules).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub operands: Vec<ScriptOperandFact>,
  pub reactivity_graph: std::sync::Arc<ReactivityGraph>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptFacts {
  pub blocks: Vec<ScriptBlockFacts>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SfcFacts {
  pub template: TemplateFacts,
  pub script: ScriptFacts,
}

impl SfcFacts {
  /// Replace the preferred script block's reactivity graph with a project-linked
  /// module graph (usually after cross-file seed linking). Prefers `script setup`.
  pub fn apply_module_reactivity(&mut self, graph: std::sync::Arc<ReactivityGraph>) {
    self.apply_module_reactivity_for(ScriptKind::Setup, graph);
  }

  /// Apply a project-linked graph onto the script block of the given kind.
  /// Falls back to the first block when the preferred kind is absent.
  pub fn apply_module_reactivity_for(
    &mut self,
    kind: ScriptKind,
    graph: std::sync::Arc<ReactivityGraph>,
  ) {
    if let Some(block) = self.script.blocks.iter_mut().find(|block| block.kind == kind) {
      block.reactivity_graph = graph;
      return;
    }
    if kind == ScriptKind::Setup
      && let Some(block) = self.script.blocks.first_mut()
    {
      block.reactivity_graph = graph;
    }
  }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VueVersion {
  pub major: u64,
  pub minor: u64,
  pub patch: u64,
}

impl VueVersion {
  #[must_use]
  pub fn parse_requirement(value: &str) -> Option<Self> {
    let version = value
      .split(|character: char| !character.is_ascii_digit() && character != '.')
      .find(|part| !part.is_empty())?;
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    Some(Self { major, minor, patch })
  }

  #[must_use]
  pub const fn is_at_least(self, major: u64, minor: u64) -> bool {
    self.major > major || (self.major == major && self.minor >= minor)
  }
}

/// Per-file analysis capabilities derived from the nearest `package.json`.
///
/// Stable Vue Vet-owned surface: rules see version/package names only, never
/// package-manager state or raw manifests.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuleEnvironment {
  pub vue_version: Option<VueVersion>,
  /// Sorted unique dependency names from nearest package.json dependency fields.
  pub packages: Vec<String>,
}

impl RuleEnvironment {
  #[must_use]
  pub fn has_package(&self, name: &str) -> bool {
    self.packages.iter().any(|package| package == name)
  }
}
