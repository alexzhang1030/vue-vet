//! Long-lived project analysis session for CLI, LSP, and agent surfaces.
//!
//! Owns configuration loading, cached/fresh scans, unsaved buffer overlays, rule
//! and finding explain, and workspace path containment. Protocol adapters
//! (clap, LSP, MCP) stay outside.
//!
//! Modules:
//! - [`session`] — [`ProjectSession`] handle
//! - [`types`] — snapshot / options / error types
//! - [`registry`] — file-rule + project metadata
//! - [`discovery`] — workspace walk + input snapshot
//! - [`pipeline`] — scan stages (facts → project → rules → finalize)
//! - [`scan`] / [`explain`] / [`diagnostics`] / [`locality`] / [`progress`]

mod config;
mod diagnostics;
mod discovery;
mod explain;
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
pub use locality::{AnalysisProduct, ChangeImpact, DirtyPlan, ResolutionScope, ScanWorkCounters};
pub use path::resolve_under_root;
pub use progress::{ProgressEvent, ProgressReporter};
pub use registry::{file_analysis_registry, resolve_rule_meta};
pub use scan::{discover_workspace_boundary, scan_directory};
pub use session::{ProjectSession, SessionStats};
pub use types::{
  AnalysisCoverage, AnalysisIssue, AnalysisSnapshot, AnalysisStage, ChangeSet, Recoverability,
  SessionError, SessionOptions,
};
