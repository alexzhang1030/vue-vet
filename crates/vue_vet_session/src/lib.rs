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
    Arc, LazyLock, Mutex, MutexGuard,
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

#[cfg(test)]
use std::sync::Barrier;

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
  core: Mutex<SessionCore>,
  workspace_discoveries: AtomicU64,
  incremental_file_updates: AtomicU64,
  committed_analyses: AtomicU64,
  cancelled_analyses: AtomicU64,
  #[cfg(test)]
  test_hooks: SessionTestHooks,
}

#[derive(Debug, Default)]
struct SessionInputs {
  overlays: BTreeMap<PathBuf, String>,
  snapshot: Option<Arc<WorkspaceInputSnapshot>>,
}

#[derive(Debug)]
struct SessionCore {
  revision: u64,
  inputs: SessionInputs,
  committed: Arc<scan::AnalysisState>,
}

struct PreparedAnalysis {
  revision: u64,
  input: Arc<WorkspaceInputSnapshot>,
  committed: Arc<scan::AnalysisState>,
  has_overlays: bool,
}

#[cfg(test)]
#[derive(Clone, Debug)]
struct PausePoint {
  entered: Arc<Barrier>,
  resume: Arc<Barrier>,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct SessionTestHooks {
  after_input_mutation: Mutex<Option<PausePoint>>,
  before_commit: Mutex<Option<PausePoint>>,
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
      core: Mutex::new(SessionCore {
        revision: 1,
        inputs: SessionInputs::default(),
        committed: Arc::new(scan::AnalysisState::default()),
      }),
      workspace_discoveries: AtomicU64::new(0),
      incremental_file_updates: AtomicU64::new(0),
      committed_analyses: AtomicU64::new(0),
      cancelled_analyses: AtomicU64::new(0),
      #[cfg(test)]
      test_hooks: SessionTestHooks::default(),
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
    let prepared = self.prepare_analysis(false)?;
    let no_cache = self.no_cache || prepared.has_overlays;
    self.run_analysis(&prepared, no_cache)
  }

  /// Always bypass the content-addressed cache (fix apply rescan).
  ///
  /// # Errors
  ///
  /// Returns analysis or I/O failures.
  pub fn analyze_fresh(&self) -> Result<AnalysisSnapshot, SessionError> {
    let prepared = self.prepare_analysis(true)?;
    self.run_analysis(&prepared, true)
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
    let prepared = self.prepare_analysis(false)?;
    self.run_analysis(&prepared, true)
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
    let mut core = self.lock_core()?;
    for (path, source) in &changes {
      if let Some(source) = source {
        core.inputs.overlays.insert(path.clone(), source.clone());
      } else {
        core.inputs.overlays.remove(path);
      }
    }
    if let Some(snapshot) = &mut core.inputs.snapshot {
      Arc::make_mut(snapshot).apply_changes(self.root.as_path(), &self.config, &changes)?;
    }
    #[cfg(test)]
    Self::pause_at(&self.test_hooks.after_input_mutation);
    core.revision = core.revision.wrapping_add(1);
    drop(core);
    self
      .incremental_file_updates
      .fetch_add(u64::try_from(changes.len()).unwrap_or(u64::MAX), Ordering::Relaxed);
    Ok(())
  }

  /// Analyze the current overlay set, reusing unchanged per-file facts.
  ///
  /// # Errors
  ///
  /// Returns analysis or I/O failures.
  pub fn analyze_affected(&self) -> Result<AnalysisSnapshot, SessionError> {
    let prepared = self.prepare_analysis(false)?;
    self.run_analysis(&prepared, true)
  }

  /// Files invalidated by the most recent in-memory analysis.
  ///
  /// # Errors
  ///
  /// Returns when the session state lock was poisoned.
  pub fn affected_files(&self) -> Result<Vec<FileId>, SessionError> {
    let core = self.lock_core()?;
    Ok(core.committed.last_affected.iter().cloned().collect())
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
    let mut core = self.lock_core()?;
    let mut changes = BTreeMap::new();
    for path in core.inputs.overlays.keys() {
      if !normalized.contains_key(path) {
        changes.insert(path.clone(), None);
      }
    }
    for (path, source) in &normalized {
      if core.inputs.overlays.get(path) != Some(source) {
        changes.insert(path.clone(), Some(source.clone()));
      }
    }
    if changes.is_empty() {
      return Ok(());
    }
    if let Some(snapshot) = &mut core.inputs.snapshot {
      Arc::make_mut(snapshot).apply_changes(self.root.as_path(), &self.config, &changes)?;
    }
    core.inputs.overlays = normalized;
    core.revision = core.revision.wrapping_add(1);
    drop(core);
    self
      .incremental_file_updates
      .fetch_add(u64::try_from(changes.len()).unwrap_or(u64::MAX), Ordering::Relaxed);
    Ok(())
  }

  fn prepare_analysis(&self, rediscover: bool) -> Result<PreparedAnalysis, SessionError> {
    let mut core = self.lock_core()?;
    if rediscover || core.inputs.snapshot.is_none() {
      let snapshot =
        WorkspaceInputSnapshot::discover(self.root.as_path(), &self.config, &core.inputs.overlays)?;
      core.inputs.snapshot = Some(Arc::new(snapshot));
      self.workspace_discoveries.fetch_add(1, Ordering::Relaxed);
      if rediscover {
        core.revision = core.revision.wrapping_add(1);
      }
    }
    let input = core
      .inputs
      .snapshot
      .as_ref()
      .map(Arc::clone)
      .ok_or_else(|| SessionError::message("workspace input snapshot was not initialized"))?;
    Ok(PreparedAnalysis {
      revision: core.revision,
      input,
      committed: Arc::clone(&core.committed),
      has_overlays: !core.inputs.overlays.is_empty(),
    })
  }

  fn run_analysis(
    &self,
    prepared: &PreparedAnalysis,
    no_cache: bool,
  ) -> Result<AnalysisSnapshot, SessionError> {
    let mut candidate = (*prepared.committed).clone();
    let cancelled = || !self.is_current_revision(prepared.revision);
    let snapshot = match scan::analyze_snapshot(
      &prepared.input,
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
    #[cfg(test)]
    Self::pause_at(&self.test_hooks.before_commit);
    let mut core = self.lock_core()?;
    if core.revision != prepared.revision {
      self.cancelled_analyses.fetch_add(1, Ordering::Relaxed);
      return Err(SessionError::Cancelled);
    }
    core.committed = Arc::new(candidate);
    drop(core);
    self.committed_analyses.fetch_add(1, Ordering::Relaxed);
    Ok(snapshot)
  }

  fn is_current_revision(&self, revision: u64) -> bool {
    self.core.lock().is_ok_and(|core| core.revision == revision)
  }

  fn lock_core(&self) -> Result<MutexGuard<'_, SessionCore>, SessionError> {
    self.core.lock().map_err(|_| SessionError::message("session state lock was poisoned"))
  }

  #[cfg(test)]
  fn install_pause(
    target: &Mutex<Option<PausePoint>>,
    pause: PausePoint,
  ) -> Result<(), SessionError> {
    let mut target =
      target.lock().map_err(|_| SessionError::message("session test hook lock was poisoned"))?;
    *target = Some(pause);
    drop(target);
    Ok(())
  }

  #[cfg(test)]
  fn pause_at(target: &Mutex<Option<PausePoint>>) {
    let pause = target.lock().ok().and_then(|mut target| target.take());
    if let Some(pause) = pause {
      let _entered = pause.entered.wait();
      let _resumed = pause.resume.wait();
    }
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

#[cfg(test)]
mod tests {
  use std::thread;

  use super::*;

  #[test]
  #[expect(clippy::panic, reason = "session concurrency failures must fail the unit test")]
  fn concurrent_input_update_cannot_commit_stale_analysis() {
    let root = std::env::temp_dir().join(format!("vue-vet-session-race-{}", std::process::id()));
    std::fs::create_dir_all(&root)
      .unwrap_or_else(|error| panic!("failed to create test workspace: {error}"));
    let component = root.join("App.vue");
    std::fs::write(&component, "<template><main v-html=\"html\" /></template>")
      .unwrap_or_else(|error| panic!("failed to write test component: {error}"));
    let session = Arc::new(
      ProjectSession::open(SessionOptions {
        root: root.clone(),
        config_path: None,
        cache_dir: None,
        no_cache: true,
        threads: Some(1),
      })
      .unwrap_or_else(|error| panic!("failed to open session: {error}")),
    );
    session.analyze().unwrap_or_else(|error| panic!("initial analysis failed: {error}"));
    assert_eq!(session.stats().committed_analyses, 1);

    let analysis_entered = Arc::new(Barrier::new(2));
    let analysis_resume = Arc::new(Barrier::new(2));
    ProjectSession::install_pause(
      &session.test_hooks.before_commit,
      PausePoint { entered: Arc::clone(&analysis_entered), resume: Arc::clone(&analysis_resume) },
    )
    .unwrap_or_else(|error| panic!("failed to install analysis pause: {error}"));
    let analysis_session = Arc::clone(&session);
    let analysis = thread::spawn(move || analysis_session.analyze_affected());
    let _analysis_ready = analysis_entered.wait();

    let update_entered = Arc::new(Barrier::new(2));
    let update_resume = Arc::new(Barrier::new(2));
    ProjectSession::install_pause(
      &session.test_hooks.after_input_mutation,
      PausePoint { entered: Arc::clone(&update_entered), resume: Arc::clone(&update_resume) },
    )
    .unwrap_or_else(|error| panic!("failed to install update pause: {error}"));
    let update_session = Arc::clone(&session);
    let update = thread::spawn(move || {
      update_session.apply_changes(ChangeSet::upsert(
        component,
        "<template><main>{{ html }}</main></template>".into(),
      ))
    });
    let _input_was_mutated = update_entered.wait();

    let _analysis_may_commit = analysis_resume.wait();
    let _revision_may_advance = update_resume.wait();
    update
      .join()
      .unwrap_or_else(|_| panic!("input update thread panicked"))
      .unwrap_or_else(|error| panic!("input update failed: {error}"));
    let stale_result = analysis.join().unwrap_or_else(|_| panic!("analysis thread panicked"));
    assert!(
      stale_result.as_ref().is_err_and(SessionError::is_cancelled),
      "analysis based on the old revision must not commit after inputs changed"
    );
    assert_eq!(session.stats().committed_analyses, 1);
    assert_eq!(session.stats().cancelled_analyses, 1);

    let current =
      session.analyze_affected().unwrap_or_else(|error| panic!("current analysis failed: {error}"));
    assert!(
      !current
        .summary
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.rule_id == "vue-vet/security/no-v-html"),
      "the committed result must reflect the updated input"
    );
    let _ignored = std::fs::remove_dir_all(root);
  }
}
