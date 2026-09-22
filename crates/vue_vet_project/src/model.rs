//! Stable project-graph DTOs (public contract).

use std::{collections::BTreeMap, sync::Arc};

use serde::{Deserialize, Serialize};
use vue_vet_core::{Diagnostic, FileId, ModuleId, SfcFacts, SourceSpan};
use vue_vet_reactivity::{ModuleReactivity, ModuleSource};

/// Bump when Nuxt/Vite seed-map or external-summary follow semantics change.
///
/// Invalidates content-addressed project cache. v18 contract: type-only
/// declaration resolution (type-vs-runtime split specifiers keep runtime
/// Import edges and reactive result bindings), grouped unresolved imports,
/// Nuxt Content nearest-owner naming with literal `srcDir`, retained-snapshot
/// layer enablement, unknown computed keys unprove `srcDir`/`extends` until a
/// later literal restores them, and exported Nuxt config follows only
/// immutable local `const` initializers.
/// v14: bare `export * from 'pkg'` follow and widened bare auto-import /
/// `ForwardReturn` seed resolution.
pub const CONVENTIONS_VERSION: u32 = 18;
/// Version of the stable graph DTO consumed by project rules and session
/// invalidation. Bump when node/edge identity or provenance semantics change.
pub const PROJECT_GRAPH_SCHEMA_VERSION: u32 = 1;

const fn default_project_graph_schema_version() -> u32 {
  // Unversioned payloads used the original v1 shape.
  1
}

pub const PROJECT_RULE_IDS: [&str; 2] =
  ["vue-vet/project/unresolved-import", "vue-vet/project/unused-component"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectFile {
  pub path: FileId,
  pub source_len: usize,
  pub facts: Arc<SfcFacts>,
  pub module_source: Option<Arc<ModuleSource>>,
  /// Ordinary `<script>` companion when dual-script SFCs also have setup
  /// (`id` ends with `#script`).
  pub ordinary_module_source: Option<Arc<ModuleSource>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
  VueFile,
  Module,
  Component,
  Composable,
  Page,
  Layout,
  Plugin,
  Middleware,
  Store,
  External,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
  Import,
  ExternalImport,
  ComponentUsage,
  AutoComponent,
  AutoComposable,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct GraphNode {
  pub id: String,
  pub kind: NodeKind,
  pub path: String,
  pub name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct GraphEdge {
  pub id: String,
  pub from: String,
  pub to: String,
  pub kind: EdgeKind,
  pub specifier: String,
  pub evidence: SourceSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectGraph {
  /// Version of the serialized graph DTO contract.
  #[serde(default = "default_project_graph_schema_version")]
  pub schema_version: u32,
  pub conventions_version: u32,
  pub nodes: Vec<GraphNode>,
  pub edges: Vec<GraphEdge>,
  pub diagnostics: Vec<Diagnostic>,
  pub invalidation_inputs: Vec<String>,
  pub module_reactivity: Arc<[Arc<ModuleReactivity>]>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub reactivity_issues: Vec<ReactivityIssue>,
  /// Compatibility summary for reporters that have not adopted structured issues.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reactivity_error: Option<String>,
  /// Joined model-default demand findings keyed by parent file path.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub model_demand: BTreeMap<String, vue_vet_core::ModelDemandFileFacts>,
}

impl Default for ProjectGraph {
  fn default() -> Self {
    Self {
      schema_version: PROJECT_GRAPH_SCHEMA_VERSION,
      conventions_version: CONVENTIONS_VERSION,
      nodes: Vec::new(),
      edges: Vec::new(),
      diagnostics: Vec::new(),
      invalidation_inputs: Vec::new(),
      module_reactivity: Arc::from([]),
      reactivity_issues: Vec::new(),
      reactivity_error: None,
      model_demand: BTreeMap::new(),
    }
  }
}

impl ProjectGraph {
  /// Stable graph contract version for machine consumers and cache policy.
  #[must_use]
  pub const fn schema_version(&self) -> u32 {
    self.schema_version
  }

  /// Whether the graph carries the current stable contract.
  #[must_use]
  pub const fn has_current_schema(&self) -> bool {
    self.schema_version() == PROJECT_GRAPH_SCHEMA_VERSION
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReactivityIssue {
  pub module: Option<ModuleId>,
  pub message: String,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn graph_schema_is_explicit_and_stable() {
    let graph = ProjectGraph::default();
    assert_eq!(graph.schema_version(), PROJECT_GRAPH_SCHEMA_VERSION);
    assert!(graph.has_current_schema());
  }

  #[test]
  fn legacy_graph_payload_defaults_to_current_schema() {
    let graph = serde_json::from_str::<ProjectGraph>(
      r#"{"conventions_version":18,"nodes":[],"edges":[],"diagnostics":[],"invalidation_inputs":[],"module_reactivity":[]}"#,
    );
    assert_eq!(graph.as_ref().map(|graph| graph.schema_version).ok(), Some(1));
  }
}
