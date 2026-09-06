//! Long-lived project analysis session for CLI, LSP, and agent surfaces.
//!
//! Owns configuration loading, cached/fresh scans, unsaved buffer overlays, rule
//! and finding explain, and workspace path containment. Protocol adapters
//! (clap, LSP, MCP) stay outside.

mod config;
mod diagnostics;
mod discovery;
mod explain;
mod groups;
mod invalidation;
mod locality;
mod package_index;
mod path;
mod pipeline;
mod progress;
mod registry;
mod scan;
mod session;
mod types;

pub use explain::Explained;
pub use groups::{
  RULE_GROUP_TABLE, apply_selected_groups, group_of, normalize_groups, rule_inventory,
};
pub use locality::{AnalysisProduct, ChangeImpact, DirtyPlan, ResolutionScope, ScanWorkCounters};
pub use path::resolve_under_root;
pub use progress::{ProgressEvent, ProgressReporter};
pub use registry::{
  composed_rule_metadata, file_analysis_registry, known_rule_ids, resolve_rule_meta,
};
pub use scan::{discover_workspace_boundary, scan_directory};
pub use session::{ProjectSession, SessionStats};
pub use types::{
  AnalysisCoverage, AnalysisIssue, AnalysisSnapshot, AnalysisStage, ChangeSet, Recoverability,
  SessionError, SessionOptions,
};
pub use vue_vet_core::RuleGroupId;
