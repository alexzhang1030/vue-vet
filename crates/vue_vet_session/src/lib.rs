//! Long-lived project analysis session for CLI, LSP, and agent surfaces.
//!
//! Owns configuration loading, cached/fresh scans, unsaved buffer overlays, rule
//! and finding explain, and workspace path containment. Protocol adapters
//! (clap, LSP, MCP) stay outside.

mod explain;
mod path;
mod scan;

use std::{
  collections::BTreeMap,
  path::{Path, PathBuf},
};

use thiserror::Error;
use vue_vet_cache::default_cache_dir;
use vue_vet_config::{CONFIG_FILE, Config};
use vue_vet_core::{Confidence, RuleMeta, RuleRegistry, ScanSummary, Severity};
use vue_vet_practice::practice_rules;
use vue_vet_project::{PROJECT_RULE_IDS, ProjectGraph};
use vue_vet_reporters::{FindingExplain, RuleExplain, find_rule_meta};
use vue_vet_rules::builtin_rules;

pub use explain::Explained;
pub use path::resolve_under_root;
pub use scan::scan_directory;

/// Options for opening a [`ProjectSession`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionOptions {
  /// File or directory to analyze (same semantics as the CLI path argument).
  pub root: PathBuf,
  /// Explicit `vue-vet.toml`, or discover next to the scan directory.
  pub config_path: Option<PathBuf>,
  /// Override content-addressed cache directory.
  pub cache_dir: Option<PathBuf>,
  /// Skip the content-addressed cache (also used by fix modes).
  pub no_cache: bool,
  /// Analysis worker threads; `None` uses Rayon defaults.
  pub threads: Option<usize>,
}

/// Deterministic analysis result shared across surfaces.
#[derive(Clone, Debug)]
pub struct AnalysisSnapshot {
  pub summary: ScanSummary,
  pub graph: ProjectGraph,
  pub cache_status: &'static str,
  /// Normalized `/`-separated paths matching JSON `project.analyzed_files`.
  pub analyzed_files: Vec<String>,
}

/// Errors from session open, analyze, explain, or path resolution.
#[derive(Debug, Error)]
pub enum SessionError {
  #[error("{0}")]
  Message(String),
}

impl SessionError {
  #[must_use]
  pub fn message(message: impl Into<String>) -> Self {
    Self::Message(message.into())
  }
}

impl From<String> for SessionError {
  fn from(message: String) -> Self {
    Self::Message(message)
  }
}

/// Project-graph rules live outside `builtin_registry` but share the same docs key.
pub static PROJECT_RULE_META: [RuleMeta; 2] = [
  RuleMeta {
    id: PROJECT_RULE_IDS[0],
    category: "project",
    default_severity: Severity::Error,
    confidence: Confidence::High,
    documentation: "project-graph",
  },
  RuleMeta {
    id: PROJECT_RULE_IDS[1],
    category: "project",
    default_severity: Severity::Warning,
    confidence: Confidence::Medium,
    documentation: "project-graph",
  },
];

/// Long-lived analysis handle for one workspace root and effective config.
#[derive(Debug)]
pub struct ProjectSession {
  root: PathBuf,
  config: Config,
  cache_dir: PathBuf,
  no_cache: bool,
  threads: Option<usize>,
}

impl ProjectSession {
  /// Load and validate configuration for `options.root`.
  ///
  /// # Errors
  ///
  /// Returns a config I/O, parse, or rule-validation error.
  pub fn open(options: SessionOptions) -> Result<Self, SessionError> {
    let config = load_config(&options.root, options.config_path.as_deref())?;
    Ok(Self {
      root: options.root,
      config,
      cache_dir: options.cache_dir.unwrap_or_else(default_cache_dir),
      no_cache: options.no_cache,
      threads: options.threads,
    })
  }

  #[must_use]
  pub const fn config(&self) -> &Config {
    &self.config
  }

  #[must_use]
  pub fn root(&self) -> &Path {
    &self.root
  }

  /// Directory boundary used for project graph and git diff (file parents collapse here).
  #[must_use]
  pub fn workspace_root(&self) -> &Path {
    scan_directory(&self.root)
  }

  /// Resolve `path` inside the workspace; reject traversal outside the root.
  ///
  /// # Errors
  ///
  /// Returns [`SessionError`] when the path escapes the session root.
  pub fn resolve_workspace_path(&self, path: &Path) -> Result<PathBuf, SessionError> {
    resolve_under_root(self.workspace_root(), path)
  }

  /// Scan with the session cache policy.
  ///
  /// # Errors
  ///
  /// Returns analysis, cache, or I/O failures.
  pub fn analyze(&self) -> Result<AnalysisSnapshot, SessionError> {
    scan::analyze(&self.root, &self.config, &self.cache_dir, self.no_cache, self.threads)
  }

  /// Always bypass the content-addressed cache (fix apply rescan).
  ///
  /// # Errors
  ///
  /// Returns analysis or I/O failures.
  pub fn analyze_fresh(&self) -> Result<AnalysisSnapshot, SessionError> {
    scan::analyze(&self.root, &self.config, &self.cache_dir, true, self.threads)
  }

  /// Scan with unsaved buffer overlays (LSP `didChange` text).
  ///
  /// Overlay keys should be absolute paths matching the project walk. Analysis
  /// always bypasses the content-addressed cache. An empty map is equivalent to
  /// [`Self::analyze_fresh`].
  ///
  /// # Errors
  ///
  /// Returns analysis or I/O failures.
  pub fn analyze_with_overlays(
    &self,
    overlays: &BTreeMap<PathBuf, String>,
  ) -> Result<AnalysisSnapshot, SessionError> {
    if overlays.is_empty() {
      return self.analyze_fresh();
    }
    scan::analyze_with_overlays(&self.root, &self.config, self.threads, overlays)
  }

  /// Explain a rule id or opaque finding id.
  ///
  /// # Errors
  ///
  /// Returns unknown target, missing finding, or scan failures.
  pub fn explain(&self, target: &str) -> Result<Explained, SessionError> {
    explain::explain(self, target)
  }

  /// Explain a known rule without scanning.
  ///
  /// # Errors
  ///
  /// Returns when the rule id is unknown.
  pub fn explain_rule(&self, rule_id: &str) -> Result<RuleExplain, SessionError> {
    explain::explain_rule(self, rule_id)
  }

  /// Scan and explain an opaque diagnostic finding id.
  ///
  /// # Errors
  ///
  /// Returns when the finding is missing or its rule is unknown.
  pub fn explain_finding(&self, finding_id: &str) -> Result<FindingExplain, SessionError> {
    explain::explain_finding(self, finding_id)
  }
}

/// Per-file lint + practice registry shared by session scans.
#[must_use]
pub fn file_analysis_registry() -> RuleRegistry {
  let mut rules = builtin_rules();
  rules.extend(practice_rules());
  RuleRegistry::new(rules)
}

/// Look up built-in, practice, or project rule metadata by exact id.
#[must_use]
pub fn resolve_rule_meta(rule_id: &str) -> Option<&'static RuleMeta> {
  let mut metas = file_analysis_registry().metadata();
  metas.extend(PROJECT_RULE_META.iter());
  find_rule_meta(rule_id, &metas)
}

fn known_rule_ids() -> impl Iterator<Item = &'static str> {
  file_analysis_registry().metadata().into_iter().map(|meta| meta.id).chain(PROJECT_RULE_IDS)
}

fn load_config(root: &Path, explicit: Option<&Path>) -> Result<Config, SessionError> {
  let discovered = explicit.map_or_else(
    || {
      let directory = if root.is_dir() { root } else { root.parent().unwrap_or(root) };
      let candidate = directory.join(CONFIG_FILE);
      candidate.exists().then_some(candidate)
    },
    |explicit| Some(explicit.to_path_buf()),
  );
  let config = if let Some(path) = discovered {
    let source = std::fs::read_to_string(&path).map_err(|error| {
      SessionError::message(format!("failed to read {}: {error}", path.display()))
    })?;
    Config::parse(&source)
      .map_err(|error| SessionError::message(format!("{}: {error}", path.display())))?
  } else {
    Config::default()
  };
  config
    .validate_rules(known_rule_ids())
    .map_err(|error| SessionError::message(error.to_string()))?;
  Ok(config)
}
