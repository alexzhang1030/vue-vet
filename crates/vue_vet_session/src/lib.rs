//! Long-lived project analysis session for CLI, LSP, and agent surfaces.
//!
//! Owns configuration loading, cached/fresh scans, unsaved buffer overlays, rule
//! and finding explain, and workspace path containment. Protocol adapters
//! (clap, LSP, MCP) stay outside.
//!
//! Modules:
//! - [`session`] — [`ProjectSession`] and snapshot / error types
//! - [`discovery`] — workspace walk + input snapshot
//! - [`pipeline`] — scan stages (facts → project → rules → finalize)
//! - [`scan`] / [`explain`] / [`diagnostics`] / [`locality`] / [`progress`]

mod diagnostics;
mod discovery;
mod explain;
mod invalidation;
mod locality;
mod package_index;
mod path;
mod pipeline;
mod progress;
mod scan;
mod session;

pub use explain::Explained;
pub use locality::{AnalysisProduct, ChangeImpact, DirtyPlan, ResolutionScope, ScanWorkCounters};
pub use path::resolve_under_root;
pub use progress::{ProgressEvent, ProgressReporter};
pub use scan::{discover_workspace_boundary, scan_directory};
pub use session::{
  AnalysisCoverage, AnalysisIssue, AnalysisSnapshot, AnalysisStage, ChangeSet, ProjectSession,
  Recoverability, SessionError, SessionOptions, SessionStats, file_analysis_registry,
  resolve_rule_meta,
};
