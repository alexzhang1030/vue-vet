//! Long-lived project analysis session for CLI, LSP, and agent surfaces.
//!
//! Owns configuration loading, cached/fresh scans, unsaved buffer overlays, rule
//! and finding explain, and workspace path containment. Protocol adapters
//! (clap, LSP, MCP) stay outside.

mod diagnostics;
mod discovery;
mod explain;
mod invalidation;
mod package_index;
mod path;
mod pipeline;
mod scan;

use std::{
  collections::BTreeMap,
  path::{Path, PathBuf},
  sync::{
    LazyLock, Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
  },
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

use self::discovery::WorkspaceInputSnapshot;

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
  #[error("analysis was superseded by a newer workspace revision")]
  Cancelled,
  #[error("{0}")]
  Message(String),
}

impl SessionError {
  #[must_use]
  pub fn message(message: impl Into<String>) -> Self {
    Self::Message(message.into())
  }

  #[must_use]
  pub const fn is_cancelled(&self) -> bool {
    matches!(self, Self::Cancelled)
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
  inputs: Mutex<SessionInputs>,
  analysis: Mutex<scan::AnalysisState>,
  revision: AtomicU64,
  workspace_discoveries: AtomicU64,
  incremental_file_updates: AtomicU64,
  committed_analyses: AtomicU64,
  cancelled_analyses: AtomicU64,
}

#[derive(Debug, Default)]
struct SessionInputs {
  overlays: BTreeMap<PathBuf, String>,
  snapshot: Option<WorkspaceInputSnapshot>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SessionStats {
  pub workspace_discoveries: u64,
  pub incremental_file_updates: u64,
  pub committed_analyses: u64,
  pub cancelled_analyses: u64,
}

impl ProjectSession {
  /// Load and validate configuration for `options.root`.
  ///
  /// # Errors
  ///
  /// Returns a config I/O, parse, or rule-validation error.
  pub fn open(options: SessionOptions) -> Result<Self, SessionError> {
    let root = options.root.canonicalize().unwrap_or(options.root);
    let config = load_config(&root, options.config_path.as_deref())?;
    Ok(Self {
      root: WorkspaceRoot::new(root),
      config,
      cache_dir: options.cache_dir.unwrap_or_else(default_cache_dir),
      no_cache: options.no_cache,
      threads: options.threads,
      inputs: Mutex::new(SessionInputs::default()),
      analysis: Mutex::new(scan::AnalysisState::default()),
      revision: AtomicU64::new(1),
      workspace_discoveries: AtomicU64::new(0),
      incremental_file_updates: AtomicU64::new(0),
      committed_analyses: AtomicU64::new(0),
      cancelled_analyses: AtomicU64::new(0),
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

  #[must_use]
  pub fn stats(&self) -> SessionStats {
    SessionStats {
      workspace_discoveries: self.workspace_discoveries.load(Ordering::Relaxed),
      incremental_file_updates: self.incremental_file_updates.load(Ordering::Relaxed),
      committed_analyses: self.committed_analyses.load(Ordering::Relaxed),
      cancelled_analyses: self.cancelled_analyses.load(Ordering::Relaxed),
    }
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
    let (input, revision, has_overlays) = self.prepare_snapshot(false)?;
    self.run_analysis(&input, revision, self.no_cache || has_overlays)
  }

  /// Always bypass the content-addressed cache (fix apply rescan).
  ///
  /// # Errors
  ///
  /// Returns analysis or I/O failures.
  pub fn analyze_fresh(&self) -> Result<AnalysisSnapshot, SessionError> {
    let (input, revision, _) = self.prepare_snapshot(true)?;
    self.run_analysis(&input, revision, true)
  }

  /// Scan with unsaved buffer overlays (LSP `didChange` text).
  ///
  /// Overlay keys should identify workspace files. Analysis always bypasses the
  /// content-addressed cache. The overlay set replaces the previous set while
  /// retaining the session's discovered source snapshot; use
  /// [`Self::analyze_fresh`] when new on-disk paths may have appeared.
  ///
  /// # Errors
  ///
  /// Returns analysis or I/O failures.
  pub fn analyze_with_overlays(
    &self,
    overlays: &BTreeMap<PathBuf, String>,
  ) -> Result<AnalysisSnapshot, SessionError> {
    self.replace_overlays(overlays)?;
    let (input, revision, _) = self.prepare_snapshot(false)?;
    self.run_analysis(&input, revision, true)
  }

  /// Update unsaved source overlays without reopening configuration or the workspace.
  ///
  /// # Errors
  ///
  /// Returns when the session state lock was poisoned.
  pub fn apply_changes(&self, changes: ChangeSet) -> Result<(), SessionError> {
    if changes.files.is_empty() {
      return Ok(());
    }
    let changes = changes
      .files
      .into_iter()
      .map(|(path, source)| self.resolve_workspace_path(&path).map(|path| (path, source)))
      .collect::<Result<BTreeMap<_, _>, _>>()?;
    let mut inputs = self.lock_inputs()?;
    for (path, source) in &changes {
      if let Some(source) = source {
        inputs.overlays.insert(path.clone(), source.clone());
      } else {
        inputs.overlays.remove(path);
      }
    }
    if let Some(snapshot) = &mut inputs.snapshot {
      snapshot.apply_changes(self.root.as_path(), &self.config, &changes)?;
    }
    drop(inputs);
    self
      .incremental_file_updates
      .fetch_add(u64::try_from(changes.len()).unwrap_or(u64::MAX), Ordering::Relaxed);
    self.revision.fetch_add(1, Ordering::AcqRel);
    Ok(())
  }

  /// Analyze the current overlay set, reusing unchanged per-file facts.
  ///
  /// # Errors
  ///
  /// Returns analysis or I/O failures.
  pub fn analyze_affected(&self) -> Result<AnalysisSnapshot, SessionError> {
    let (input, revision, _) = self.prepare_snapshot(false)?;
    self.run_analysis(&input, revision, true)
  }

  /// Files invalidated by the most recent in-memory analysis.
  ///
  /// # Errors
  ///
  /// Returns when the session state lock was poisoned.
  pub fn affected_files(&self) -> Result<Vec<FileId>, SessionError> {
    let state = self.lock_analysis()?;
    Ok(state.last_affected.iter().cloned().collect())
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

  fn replace_overlays(&self, overlays: &BTreeMap<PathBuf, String>) -> Result<(), SessionError> {
    let normalized = overlays
      .iter()
      .map(|(path, source)| self.resolve_workspace_path(path).map(|path| (path, source.clone())))
      .collect::<Result<BTreeMap<_, _>, _>>()?;
    let mut inputs = self.lock_inputs()?;
    let mut changes = BTreeMap::new();
    for path in inputs.overlays.keys() {
      if !normalized.contains_key(path) {
        changes.insert(path.clone(), None);
      }
    }
    for (path, source) in &normalized {
      if inputs.overlays.get(path) != Some(source) {
        changes.insert(path.clone(), Some(source.clone()));
      }
    }
    if changes.is_empty() {
      return Ok(());
    }
    if let Some(snapshot) = &mut inputs.snapshot {
      snapshot.apply_changes(self.root.as_path(), &self.config, &changes)?;
    }
    inputs.overlays = normalized;
    drop(inputs);
    self
      .incremental_file_updates
      .fetch_add(u64::try_from(changes.len()).unwrap_or(u64::MAX), Ordering::Relaxed);
    self.revision.fetch_add(1, Ordering::AcqRel);
    Ok(())
  }

  fn prepare_snapshot(
    &self,
    rediscover: bool,
  ) -> Result<(WorkspaceInputSnapshot, u64, bool), SessionError> {
    let mut inputs = self.lock_inputs()?;
    if rediscover || inputs.snapshot.is_none() {
      let snapshot =
        WorkspaceInputSnapshot::discover(self.root.as_path(), &self.config, &inputs.overlays)?;
      inputs.snapshot = Some(snapshot);
      self.workspace_discoveries.fetch_add(1, Ordering::Relaxed);
      if rediscover {
        self.revision.fetch_add(1, Ordering::AcqRel);
      }
    }
    let revision = self.revision.load(Ordering::Acquire);
    let has_overlays = !inputs.overlays.is_empty();
    let snapshot = inputs
      .snapshot
      .clone()
      .ok_or_else(|| SessionError::message("workspace input snapshot was not initialized"))?;
    drop(inputs);
    Ok((snapshot, revision, has_overlays))
  }

  fn run_analysis(
    &self,
    input: &WorkspaceInputSnapshot,
    revision: u64,
    no_cache: bool,
  ) -> Result<AnalysisSnapshot, SessionError> {
    let mut candidate = self.lock_analysis()?.clone();
    let cancelled = || self.revision.load(Ordering::Acquire) != revision;
    let snapshot = match scan::analyze_snapshot(
      input,
      &self.config,
      &self.cache_dir,
      no_cache,
      self.threads,
      &mut candidate,
      &cancelled,
    ) {
      Ok(snapshot) => snapshot,
      Err(SessionError::Cancelled) => {
        self.cancelled_analyses.fetch_add(1, Ordering::Relaxed);
        return Err(SessionError::Cancelled);
      }
      Err(error) => return Err(error),
    };
    if cancelled() {
      self.cancelled_analyses.fetch_add(1, Ordering::Relaxed);
      return Err(SessionError::Cancelled);
    }
    // Hold the input lock while checking the revision and committing. Changes
    // also take this lock before incrementing, so no newer input can slip
    // between the final check and the state publication.
    let _inputs = self.lock_inputs()?;
    if cancelled() {
      self.cancelled_analyses.fetch_add(1, Ordering::Relaxed);
      return Err(SessionError::Cancelled);
    }
    *self.lock_analysis()? = candidate;
    self.committed_analyses.fetch_add(1, Ordering::Relaxed);
    Ok(snapshot)
  }

  fn lock_inputs(&self) -> Result<MutexGuard<'_, SessionInputs>, SessionError> {
    self.inputs.lock().map_err(|_| SessionError::message("session input lock was poisoned"))
  }

  fn lock_analysis(&self) -> Result<MutexGuard<'_, scan::AnalysisState>, SessionError> {
    self.analysis.lock().map_err(|_| SessionError::message("session analysis lock was poisoned"))
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
