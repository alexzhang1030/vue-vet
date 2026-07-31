//! Stable project-graph DTOs (public contract).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use vue_vet_core::{Diagnostic, FileId, ModuleId, SfcFacts, SourceSpan};
use vue_vet_reactivity::{ModuleReactivity, ModuleSource};

/// Bump when Nuxt convention / resolver seed semantics change (cache invalidation).
pub const CONVENTIONS_VERSION: u32 = 9;

pub const PROJECT_RULE_IDS: [&str; 2] =
  ["vue-vet/project/unresolved-import", "vue-vet/project/unused-component"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectFile {
  pub path: FileId,
  pub source_len: usize,
  pub facts: Arc<SfcFacts>,
  pub module_source: Option<ModuleSource>,
  /// Ordinary `<script>` companion when dual-script SFCs also have setup
  /// (`id` ends with `#script`).
  pub ordinary_module_source: Option<ModuleSource>,
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

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectGraph {
  pub conventions_version: u32,
  pub nodes: Vec<GraphNode>,
  pub edges: Vec<GraphEdge>,
  pub diagnostics: Vec<Diagnostic>,
  pub invalidation_inputs: Vec<String>,
  pub module_reactivity: Vec<ModuleReactivity>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub reactivity_issues: Vec<ReactivityIssue>,
  /// Compatibility summary for reporters that have not adopted structured issues.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reactivity_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReactivityIssue {
  pub module: Option<ModuleId>,
  pub message: String,
}
