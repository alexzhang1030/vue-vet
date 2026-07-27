//! Long-lived project analysis session for CLI, LSP, and agent surfaces.
//!
//! Owns configuration loading, cached/fresh scans, unsaved buffer overlays, rule
//! and finding explain, and workspace path containment. Protocol adapters
//! (clap, LSP, MCP) stay outside.

mod diagnostics;
mod discovery;
mod explain;
mod package_index;
mod path;
mod scan;

use std::{
  collections::BTreeMap,
  path::{Path, PathBuf},
  sync::{LazyLock, Mutex, MutexGuard},
};

use thiserror::Error;
use vue_vet_cache::default_cache_dir;
use vue_vet_config::{CONFIG_FILE, Config};
use vue_vet_core::{
  Confidence, FileId, FindingExplain, RuleExplain, RuleMeta, RuleRegistry, ScanSummary, Severity,
  WorkspaceRoot,
};
use vue_vet_practice::practice_rules;
use vue_vet_project::{PROJECT_RULE_IDS, ProjectGraph};
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
  pub coverage: AnalysisCoverage,
  pub issues: Vec<AnalysisIssue>,
  /// Normalized `/`-separated paths matching JSON `project.analyzed_files`.
  pub analyzed_files: Vec<String>,
}

impl AnalysisSnapshot {
  #[must_use]
  pub const fn complete(&self) -> bool {
    self.issues.is_empty()
  }
}

/// Source coverage is distinct from non-source inputs that invalidate a graph.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AnalysisCoverage {
  pub analyzed_source_files: Vec<FileId>,
  pub invalidation_inputs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisStage {
  SfcParse,
  ScriptParse,
  ModuleTracing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Recoverability {
  File,
  Module,
  Fatal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalysisIssue {
  pub stage: AnalysisStage,
  pub file: Option<FileId>,
  pub message: String,
  pub recoverability: Recoverability,
}

/// Overlay mutations applied to a long-lived session before affected analysis.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChangeSet {
  pub files: BTreeMap<PathBuf, Option<String>>,
}

impl ChangeSet {
  #[must_use]
  pub fn upsert(path: PathBuf, source: String) -> Self {
    Self { files: BTreeMap::from([(path, Some(source))]) }
  }

  #[must_use]
  pub fn remove(path: PathBuf) -> Self {
    Self { files: BTreeMap::from([(path, None)]) }
  }
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
  root: WorkspaceRoot,
  config: Config,
  cache_dir: PathBuf,
  no_cache: bool,
  threads: Option<usize>,
  state: Mutex<SessionState>,
}

#[derive(Debug, Default)]
struct SessionState {
  overlays: BTreeMap<PathBuf, String>,
  analysis: scan::AnalysisState,
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
      root: WorkspaceRoot::new(options.root),
      config,
      cache_dir: options.cache_dir.unwrap_or_else(default_cache_dir),
      no_cache: options.no_cache,
      threads: options.threads,
      state: Mutex::new(SessionState::default()),
    })
  }

  #[must_use]
  pub const fn config(&self) -> &Config {
    &self.config
  }

  #[must_use]
  pub fn root(&self) -> &Path {
    self.root.as_path()
  }

  /// Directory boundary used for project graph and git diff (file parents collapse here).
  #[must_use]
  pub fn workspace_root(&self) -> &Path {
    scan_directory(self.root.as_path())
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
    let mut state = self.lock_state()?;
    scan::analyze(
      self.root.as_path(),
      &self.config,
      &self.cache_dir,
      self.no_cache,
      self.threads,
      &mut state.analysis,
    )
  }

  /// Always bypass the content-addressed cache (fix apply rescan).
  ///
  /// # Errors
  ///
  /// Returns analysis or I/O failures.
  pub fn analyze_fresh(&self) -> Result<AnalysisSnapshot, SessionError> {
    let mut state = self.lock_state()?;
    scan::analyze(
      self.root.as_path(),
      &self.config,
      &self.cache_dir,
      true,
      self.threads,
      &mut state.analysis,
    )
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
    let mut state = self.lock_state()?;
    state.overlays.clone_from(overlays);
    scan::analyze_with_overlays(
      self.root.as_path(),
      &self.config,
      self.threads,
      overlays,
      &mut state.analysis,
    )
  }

  /// Update unsaved source overlays without reopening configuration or the workspace.
  ///
  /// # Errors
  ///
  /// Returns when the session state lock was poisoned.
  pub fn apply_changes(&self, changes: ChangeSet) -> Result<(), SessionError> {
    let mut state = self.lock_state()?;
    for (path, source) in changes.files {
      if let Some(source) = source {
        state.overlays.insert(path, source);
      } else {
        state.overlays.remove(&path);
      }
    }
    drop(state);
    Ok(())
  }

  /// Analyze the current overlay set, reusing unchanged per-file facts.
  ///
  /// # Errors
  ///
  /// Returns analysis or I/O failures.
  pub fn analyze_affected(&self) -> Result<AnalysisSnapshot, SessionError> {
    let mut state = self.lock_state()?;
    let overlays = state.overlays.clone();
    if overlays.is_empty() {
      return scan::analyze(
        self.root.as_path(),
        &self.config,
        &self.cache_dir,
        true,
        self.threads,
        &mut state.analysis,
      );
    }
    scan::analyze_with_overlays(
      self.root.as_path(),
      &self.config,
      self.threads,
      &overlays,
      &mut state.analysis,
    )
  }

  /// Files invalidated by the most recent in-memory analysis.
  ///
  /// # Errors
  ///
  /// Returns when the session state lock was poisoned.
  pub fn affected_files(&self) -> Result<Vec<FileId>, SessionError> {
    let state = self.lock_state()?;
    Ok(state.analysis.last_affected.iter().cloned().collect())
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

  fn lock_state(&self) -> Result<MutexGuard<'_, SessionState>, SessionError> {
    self.state.lock().map_err(|_| SessionError::message("project session state lock was poisoned"))
  }
}

/// Per-file lint + practice registry shared by session scans.
static FILE_RULES: LazyLock<RuleRegistry> = LazyLock::new(|| {
  let mut rules = builtin_rules();
  rules.extend(practice_rules());
  RuleRegistry::new(rules)
});

#[must_use]
pub fn file_analysis_registry() -> &'static RuleRegistry {
  &FILE_RULES
}

/// Look up built-in, practice, or project rule metadata by exact id.
#[must_use]
pub fn resolve_rule_meta(rule_id: &str) -> Option<&'static RuleMeta> {
  let mut metas = file_analysis_registry().metadata();
  metas.extend(PROJECT_RULE_META.iter());
  metas.into_iter().find(|meta| meta.id == rule_id)
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
