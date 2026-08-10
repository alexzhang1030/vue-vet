//! [`ProjectSession`] and session-facing types.
//!
//! Orchestration stages live in [`crate::pipeline`]: discovery input → per-file
//! facts → project graph / reactivity → seed-aware rules → finalize.

use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
  sync::{
    Arc, LazyLock, Mutex, MutexGuard, OnceLock,
    atomic::{AtomicU64, Ordering},
  },
};

use rayon::ThreadPool;

use thiserror::Error;
use vue_vet_cache::default_cache_dir;
use vue_vet_config::{CONFIG_FILE, Config};
use vue_vet_core::{
  Confidence, FileId, FindingExplain, RuleExplain, RuleMeta, RuleRegistry, ScanSummary,
  ScopeExplain, Severity, WorkspaceRoot,
};
use vue_vet_practice::practice_rules;
use vue_vet_project::{PROJECT_RULE_IDS, ProjectGraph};
use vue_vet_rules::builtin_rules;

use crate::{
  discovery::{WorkspaceInputSnapshot, file_id_for_physical},
  explain::Explained,
  locality::{AnalysisProduct, DirtyPlan, ScanWorkCounters},
  path::resolve_under_root,
  progress::{ProgressEvent, ProgressReporter},
  scan::discover_workspace_boundary,
};

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
///
/// Every heavy field is `Arc`. `Clone` / noop / product publish only bumps
/// refcounts — never deep-copies diagnostics, graphs, coverage, or path lists.
#[derive(Clone, Debug)]
pub struct AnalysisSnapshot {
  pub summary: Arc<ScanSummary>,
  pub graph: Arc<ProjectGraph>,
  pub cache_status: &'static str,
  pub coverage: Arc<AnalysisCoverage>,
  pub issues: Arc<[AnalysisIssue]>,
  /// Normalized `/`-separated paths matching JSON `project.analyzed_files`.
  pub analyzed_files: Arc<[String]>,
  /// Real work performed by the scan that produced this snapshot.
  pub work: ScanWorkCounters,
}

impl AnalysisSnapshot {
  #[must_use]
  pub fn complete(&self) -> bool {
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
  /// Package / workspace boundary (file scans walk up to nearest `package.json`).
  boundary: PathBuf,
  config: Config,
  cache_dir: PathBuf,
  no_cache: bool,
  /// Analysis worker threads; `None` uses Rayon defaults.
  threads: Option<usize>,
  /// Built on first real scan — cache hits never pay pool construction.
  pool: OnceLock<Arc<ThreadPool>>,
  core: Mutex<SessionCore>,
  progress: Option<ProgressReporter>,
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

/// Dirty files accumulated since the last committed analysis.
#[derive(Debug, Default)]
struct PendingChanges {
  files: BTreeSet<FileId>,
  /// Cold start / rediscover: every source must enter `analyze_candidate`.
  force_full_parse: bool,
}

impl PendingChanges {
  fn clear(&mut self) {
    self.files.clear();
    self.force_full_parse = false;
  }

  fn merge_files(&mut self, files: impl IntoIterator<Item = FileId>) {
    self.files.extend(files);
  }
}

#[derive(Debug)]
struct SessionCore {
  revision: u64,
  /// Revision of the last successfully committed analysis.
  committed_revision: u64,
  inputs: SessionInputs,
  committed: Arc<crate::scan::AnalysisState>,
  pending: PendingChanges,
  last_snapshot: Option<Arc<AnalysisSnapshot>>,
}

struct PreparedAnalysis {
  revision: u64,
  input: Arc<WorkspaceInputSnapshot>,
  committed: Arc<crate::scan::AnalysisState>,
  has_overlays: bool,
  dirty_files: BTreeSet<FileId>,
  force_full_parse: bool,
}

#[cfg(test)]
#[derive(Debug)]
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
    let boundary = discover_workspace_boundary(&root);
    let config = load_config(&root, options.config_path.as_deref())?;
    Ok(Self {
      root: WorkspaceRoot::new(root),
      boundary,
      config,
      cache_dir: options.cache_dir.unwrap_or_else(default_cache_dir),
      no_cache: options.no_cache,
      threads: options.threads,
      pool: OnceLock::new(),
      core: Mutex::new(SessionCore {
        revision: 1,
        committed_revision: 0,
        inputs: SessionInputs::default(),
        committed: Arc::new(crate::scan::AnalysisState::default()),
        pending: PendingChanges { force_full_parse: true, ..PendingChanges::default() },
        last_snapshot: None,
      }),
      progress: None,
      workspace_discoveries: AtomicU64::new(0),
      incremental_file_updates: AtomicU64::new(0),
      committed_analyses: AtomicU64::new(0),
      cancelled_analyses: AtomicU64::new(0),
      #[cfg(test)]
      test_hooks: SessionTestHooks::default(),
    })
  }

  /// Attach a stage / per-file stream reporter (CLI `--progress`, text stream).
  #[must_use]
  pub fn with_progress(mut self, progress: ProgressReporter) -> Self {
    self.progress = Some(progress);
    self
  }

  fn emit_progress(&self, event: &ProgressEvent) {
    if let Some(progress) = &self.progress {
      progress.emit(event);
    }
  }

  /// Lazy session Rayon pool. Cache-hit analyzes never call this.
  fn analysis_pool(&self) -> Result<Option<Arc<ThreadPool>>, SessionError> {
    let Some(threads) = self.threads else {
      return Ok(None);
    };
    if let Some(pool) = self.pool.get() {
      return Ok(Some(Arc::clone(pool)));
    }
    let pool =
      Arc::new(rayon::ThreadPoolBuilder::new().num_threads(threads.max(1)).build().map_err(
        |error| SessionError::message(format!("failed to configure analysis threads: {error}")),
      )?);
    match self.pool.set(Arc::clone(&pool)) {
      Ok(()) => Ok(Some(pool)),
      Err(_) => Ok(self.pool.get().map(Arc::clone).or(Some(pool))),
    }
  }

  #[must_use]
  pub const fn config(&self) -> &Config {
    &self.config
  }

  #[must_use]
  pub fn root(&self) -> &Path {
    self.root.as_path()
  }

  /// Directory boundary used for project graph and git diff.
  ///
  /// File scans use the nearest ancestor `package.json` directory when present
  /// so Vite/Nuxt auto-import maps resolve; otherwise the file's parent.
  #[must_use]
  pub fn workspace_root(&self) -> &Path {
    self.boundary.as_path()
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

  /// Stable workspace-relative [`FileId`] for a physical path under this session.
  ///
  /// # Errors
  ///
  /// Returns [`SessionError`] when the path escapes the session root.
  pub fn file_id_for_path(&self, path: &Path) -> Result<FileId, SessionError> {
    let resolved = self.resolve_workspace_path(path)?;
    Ok(file_id_for_physical(self.root.as_path(), self.boundary.as_path(), &resolved))
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
  /// Mutations are transactional: overlays, the retained snapshot, and the
  /// revision advance together, or not at all when any step fails.
  ///
  /// # Errors
  ///
  /// Returns when the session state lock was poisoned or a change cannot be applied.
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
    let mut next_inputs = SessionInputs {
      overlays: core.inputs.overlays.clone(),
      snapshot: core.inputs.snapshot.as_ref().map(Arc::clone),
    };
    apply_overlay_map(&mut next_inputs.overlays, &changes);
    if let Some(snapshot) = &mut next_inputs.snapshot {
      // `make_mut` clones once when `core.inputs` still shares the Arc.
      let affected = Arc::make_mut(snapshot).apply_changes_in_place(
        self.root.as_path(),
        &self.config,
        &changes,
      )?;
      // Context epochs are compared at scan time via ChangeImpact; do not force
      // a full re-parse of unchanged source bytes.
      core.pending.merge_files(affected);
    } else {
      core.pending.force_full_parse = true;
    }
    core.inputs = next_inputs;
    core.revision = core.revision.wrapping_add(1);
    #[cfg(test)]
    Self::pause_at(&self.test_hooks.after_input_mutation);
    drop(core);
    self
      .incremental_file_updates
      .fetch_add(u64::try_from(changes.len()).unwrap_or(u64::MAX), Ordering::Relaxed);
    Ok(())
  }

  /// Analyze the current overlay set, reusing unchanged per-file facts.
  ///
  /// When the workspace revision matches the last committed analysis, returns
  /// the cached snapshot in O(1) without re-entering the pipeline.
  ///
  /// # Errors
  ///
  /// Returns analysis or I/O failures.
  pub fn analyze_affected(&self) -> Result<AnalysisSnapshot, SessionError> {
    self.analyze_affected_product(AnalysisProduct::FullReport)
  }

  /// Like [`Self::analyze_affected`], but controls which graph DTO fields are published.
  ///
  /// # Errors
  ///
  /// Returns analysis or I/O failures.
  pub fn analyze_affected_product(
    &self,
    product: AnalysisProduct,
  ) -> Result<AnalysisSnapshot, SessionError> {
    if let Some(snapshot) = self.noop_snapshot()? {
      return Ok(publish_product(&snapshot, product));
    }
    let prepared = self.prepare_analysis(false)?;
    let snapshot = self.run_analysis_product(&prepared, true, product)?;
    Ok(snapshot)
  }

  /// Return the last committed snapshot when no inputs changed since commit.
  fn noop_snapshot(&self) -> Result<Option<Arc<AnalysisSnapshot>>, SessionError> {
    let core = self.lock_core()?;
    let snapshot = if core.revision == core.committed_revision
      && core.pending.files.is_empty()
      && !core.pending.force_full_parse
    {
      core.last_snapshot.as_ref().map(Arc::clone)
    } else {
      None
    };
    drop(core);
    Ok(snapshot)
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

  /// Real work counters from the most recent committed analysis.
  ///
  /// # Errors
  ///
  /// Returns when the session state lock was poisoned.
  pub fn last_work_counters(&self) -> Result<ScanWorkCounters, SessionError> {
    let core = self.lock_core()?;
    Ok(core.committed.last_work())
  }

  /// Dirty-plan partitions from the most recent committed analysis.
  ///
  /// # Errors
  ///
  /// Returns when the session state lock was poisoned.
  pub fn last_dirty_plan(&self) -> Result<Arc<DirtyPlan>, SessionError> {
    let core = self.lock_core()?;
    Ok(Arc::clone(&core.committed.last_plan))
  }

  /// Finalized diagnostics for one file from the last committed snapshot.
  ///
  /// # Errors
  ///
  /// Returns when the session state lock was poisoned.
  pub fn diagnostics_for(
    &self,
    file_id: &FileId,
  ) -> Result<Vec<vue_vet_core::Diagnostic>, SessionError> {
    let summary = {
      let core = self.lock_core()?;
      core.last_snapshot.as_ref().map(|snapshot| Arc::clone(&snapshot.summary))
    };
    let Some(summary) = summary else {
      return Ok(Vec::new());
    };
    Ok(
      summary
        .diagnostics
        .iter()
        .filter(|diagnostic| &diagnostic.file == file_id)
        .cloned()
        .collect(),
    )
  }

  /// Explain a rule id or opaque finding id.
  ///
  /// # Errors
  ///
  /// Returns unknown target, missing finding, or scan failures.
  pub fn explain(&self, target: &str) -> Result<Explained, SessionError> {
    crate::explain::explain(self, target)
  }

  /// Explain a known rule without scanning.
  ///
  /// # Errors
  ///
  /// Returns when the rule id is unknown.
  pub fn explain_rule(&self, rule_id: &str) -> Result<RuleExplain, SessionError> {
    crate::explain::explain_rule(self, rule_id)
  }

  /// Scan and explain an opaque diagnostic finding id.
  ///
  /// # Errors
  ///
  /// Returns when the finding is missing or its rule is unknown.
  pub fn explain_finding(&self, finding_id: &str) -> Result<FindingExplain, SessionError> {
    crate::explain::explain_finding(self, finding_id)
  }

  /// Scan and explain tracking scopes matching a human query (“would Vue re-run?”).
  ///
  /// # Errors
  ///
  /// Returns when the query is empty, no scope matches, or analysis fails.
  pub fn explain_scope(
    &self,
    query: &str,
  ) -> Result<(Vec<ScopeExplain>, &'static str), SessionError> {
    crate::explain::explain_scope(self, query)
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
    let mut next_inputs = SessionInputs {
      overlays: normalized,
      snapshot: core.inputs.snapshot.as_ref().map(Arc::clone),
    };
    if let Some(snapshot) = &mut next_inputs.snapshot {
      let affected = Arc::make_mut(snapshot).apply_changes_in_place(
        self.root.as_path(),
        &self.config,
        &changes,
      )?;
      core.pending.merge_files(affected);
    } else {
      core.pending.force_full_parse = true;
    }
    core.inputs = next_inputs;
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
      self.emit_progress(&ProgressEvent::Discovering);
      let snapshot =
        WorkspaceInputSnapshot::discover(self.root.as_path(), &self.config, &core.inputs.overlays)?;
      core.inputs.snapshot = Some(Arc::new(snapshot));
      core.pending.force_full_parse = true;
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
    let force_full_parse = core.pending.force_full_parse || !core.committed.has_file_facts();
    Ok(PreparedAnalysis {
      revision: core.revision,
      input,
      committed: Arc::clone(&core.committed),
      has_overlays: !core.inputs.overlays.is_empty(),
      dirty_files: core.pending.files.clone(),
      force_full_parse,
    })
  }

  fn run_analysis(
    &self,
    prepared: &PreparedAnalysis,
    no_cache: bool,
  ) -> Result<AnalysisSnapshot, SessionError> {
    self.run_analysis_product(prepared, no_cache, AnalysisProduct::FullReport)
  }

  fn run_analysis_product(
    &self,
    prepared: &PreparedAnalysis,
    no_cache: bool,
    product: AnalysisProduct,
  ) -> Result<AnalysisSnapshot, SessionError> {
    let mut candidate = crate::scan::AnalysisState::prepare_from(&prepared.committed);
    let cancelled = || !self.is_current_revision(prepared.revision);
    let snapshot = match crate::scan::analyze_snapshot(
      &prepared.input,
      &self.config,
      &self.cache_dir,
      no_cache,
      || self.analysis_pool(),
      &prepared.committed,
      &mut candidate,
      &cancelled,
      &prepared.dirty_files,
      prepared.force_full_parse,
      self.progress.as_ref(),
    ) {
      Ok(snapshot) => snapshot,
      Err(SessionError::Cancelled) => {
        // Keep pending dirty set — cancellation must not drop work.
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
    // Commit the full IR snapshot; trim only the caller-facing copy.
    let shared = Arc::new(snapshot);
    let mut core = self.lock_core()?;
    if core.revision != prepared.revision {
      self.cancelled_analyses.fetch_add(1, Ordering::Relaxed);
      return Err(SessionError::Cancelled);
    }
    core.committed = Arc::new(candidate);
    core.committed_revision = prepared.revision;
    core.last_snapshot = Some(Arc::clone(&shared));
    core.pending.clear();
    drop(core);
    self.committed_analyses.fetch_add(1, Ordering::Relaxed);
    Ok(publish_product(&shared, product))
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

fn publish_product(snapshot: &AnalysisSnapshot, product: AnalysisProduct) -> AnalysisSnapshot {
  let graph = match product {
    AnalysisProduct::FullReport => Arc::clone(&snapshot.graph),
    // Trimmed DTOs are newly allocated shells; the committed snapshot keeps the
    // full graph Arc untouched (no make_mut / clear of shared state).
    AnalysisProduct::DiagnosticsAndNavigation => {
      let full = snapshot.graph.as_ref();
      Arc::new(ProjectGraph {
        conventions_version: full.conventions_version,
        nodes: full.nodes.clone(),
        edges: full.edges.clone(),
        diagnostics: Vec::new(),
        invalidation_inputs: full.invalidation_inputs.clone(),
        module_reactivity: Vec::new(),
        reactivity_issues: full.reactivity_issues.clone(),
        reactivity_error: full.reactivity_error.clone(),
      })
    }
    AnalysisProduct::DiagnosticsOnly => Arc::new(ProjectGraph {
      conventions_version: snapshot.graph.conventions_version,
      nodes: Vec::new(),
      edges: Vec::new(),
      diagnostics: Vec::new(),
      invalidation_inputs: Vec::new(),
      module_reactivity: Vec::new(),
      reactivity_issues: Vec::new(),
      reactivity_error: None,
    }),
  };
  AnalysisSnapshot {
    summary: Arc::clone(&snapshot.summary),
    graph,
    cache_status: snapshot.cache_status,
    coverage: Arc::clone(&snapshot.coverage),
    issues: Arc::clone(&snapshot.issues),
    analyzed_files: Arc::clone(&snapshot.analyzed_files),
    work: snapshot.work,
  }
}

fn apply_overlay_map(
  overlays: &mut BTreeMap<PathBuf, String>,
  changes: &BTreeMap<PathBuf, Option<String>>,
) {
  for (path, source) in changes {
    if let Some(source) = source {
      overlays.insert(path.clone(), source.clone());
    } else {
      overlays.remove(path);
    }
  }
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

    // Queue dirty work so analyze_affected enters the pipeline instead of the
    // revision-matched no-op path.
    session
      .apply_changes(ChangeSet::upsert(
        component.clone(),
        "<template><main v-html=\"html\" /></template>".into(),
      ))
      .unwrap_or_else(|error| panic!("failed to queue dirty analysis: {error}"));

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
