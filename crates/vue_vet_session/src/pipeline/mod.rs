//! Scan orchestration stages (in [`scan_parallel`] order):
//! 1. **facts** — reuse or parse each source into analyzed candidates
//! 2. **project** — structural graph + reactivity module linking
//! 3. **rules** — seed-aware file rules over final graphs
//! 4. **finalize** — [`DiagnosticFinalizer`] → [`ScanSummary`]
//!
//! Discovery / input snapshot construction happens before this module
//! (`WorkspaceInputSnapshot` from [`crate::discovery`]).

use std::{
  collections::{BTreeMap, BTreeSet},
  sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  },
};

use rayon::prelude::*;
use vue_vet_config::Config;
use vue_vet_core::{
  Diagnostic, FileId, ModuleId, ReactivityGraph, RuleEnvironment, ScanSummary, content_digest,
  serde_digest,
};
use vue_vet_plugins::default_trace_modules_options;
use vue_vet_project::{
  ContextEpochs, ProjectGraph, ProjectGraphState, build_project_graph_incremental_with_options,
};

mod analyze;

use analyze::{
  AnalyzedCandidate, PendingVueFile, analyze_candidate, issue_diagnostic, needs_file_rules,
  run_file_rules, script_source_kind, source_environment,
};

use crate::{
  AnalysisIssue, AnalysisStage, ProgressEvent, ProgressReporter, Recoverability, SessionError,
  diagnostics::{DiagnosticFinalizer, finalize_file_diagnostics},
  discovery::{SourceKind, WorkspaceInputSnapshot},
  invalidation::{expand_reverse_dependencies, reverse_dependency_index},
  locality::{ChangeImpact, DirtyPlan, ScanWorkCounters, change_impact_from, dirty_plan_from},
};

#[derive(Debug)]
struct CachedCandidate {
  source: Arc<str>,
  environment: Option<RuleEnvironment>,
  analyzed: Arc<AnalyzedCandidate>,
}

/// IR dependency key for reusing per-file rule diagnostics.
///
/// Source and environment use stable digests. Final module graphs stay as
/// `Arc` values and compare by content (`PartialEq`): serializing full graphs
/// into digests on every file was a measurable session regress.
#[derive(Clone, Debug, Eq, PartialEq)]
struct FileRuleInputKey {
  source: String,
  environment: String,
  primary_graph: Option<Arc<ReactivityGraph>>,
  ordinary_graph: Option<Arc<ReactivityGraph>>,
}

impl FileRuleInputKey {
  fn new(
    source: &str,
    environment: &RuleEnvironment,
    primary_graph: Option<Arc<ReactivityGraph>>,
    ordinary_graph: Option<Arc<ReactivityGraph>>,
  ) -> Self {
    Self {
      source: content_digest(source.as_bytes()),
      environment: serde_digest(environment),
      primary_graph,
      ordinary_graph,
    }
  }
}

#[derive(Clone, Debug)]
struct CachedFileDiagnostics {
  key: FileRuleInputKey,
  diagnostics: Arc<[Diagnostic]>,
}

/// In-memory facts and dependency state retained by a long-lived session.
#[derive(Clone, Debug, Default)]
pub struct AnalysisState {
  files: Arc<BTreeMap<FileId, CachedCandidate>>,
  file_diagnostics: Arc<BTreeMap<FileId, CachedFileDiagnostics>>,
  pub reverse_dependencies: Arc<BTreeMap<FileId, BTreeSet<FileId>>>,
  pub last_affected: BTreeSet<FileId>,
  pub last_work: ScanWorkCounters,
  pub last_plan: Arc<DirtyPlan>,
  last_context_epochs: ContextEpochs,
  /// Internal Arc partitions; [`ProjectGraphState::share`] only bumps refcounts.
  project: ProjectGraphState,
}

impl AnalysisState {
  /// Seed a mutable candidate from the previous committed state without cloning
  /// the full file/diagnostic maps (they are rebuilt from lookups into `previous`).
  #[must_use]
  pub fn prepare_from(previous: &Self) -> Self {
    Self {
      files: Arc::new(BTreeMap::new()),
      file_diagnostics: Arc::new(BTreeMap::new()),
      reverse_dependencies: Arc::new(BTreeMap::new()),
      last_affected: BTreeSet::new(),
      last_work: ScanWorkCounters::default(),
      last_plan: Arc::new(DirtyPlan::default()),
      last_context_epochs: previous.last_context_epochs,
      project: previous.project.share(),
    }
  }

  /// Share committed maps after a cache hit (no deep clone of file IR).
  #[must_use]
  pub fn share_from(previous: &Self) -> Self {
    Self {
      files: Arc::clone(&previous.files),
      file_diagnostics: Arc::clone(&previous.file_diagnostics),
      reverse_dependencies: Arc::clone(&previous.reverse_dependencies),
      last_affected: previous.last_affected.clone(),
      last_work: previous.last_work,
      last_plan: Arc::clone(&previous.last_plan),
      last_context_epochs: previous.last_context_epochs,
      project: previous.project.share(),
    }
  }

  /// Whether any per-file facts have been committed yet.
  #[must_use]
  pub fn has_file_facts(&self) -> bool {
    !self.files.is_empty()
  }

  /// Work counters from the last committed (or in-progress) scan.
  #[must_use]
  pub const fn last_work(&self) -> ScanWorkCounters {
    self.last_work
  }
}

pub struct ScanResult {
  pub summary: ScanSummary,
  pub graph: ProjectGraph,
  pub issues: Vec<AnalysisIssue>,
  pub work: ScanWorkCounters,
}

#[expect(
  clippy::too_many_arguments,
  reason = "scan forwards pool, prior state, cancellation, and dirty-set schedule"
)]
pub fn scan_with_threads(
  input: &WorkspaceInputSnapshot,
  config: &Config,
  pool: Option<Arc<rayon::ThreadPool>>,
  previous: &AnalysisState,
  state: &mut AnalysisState,
  cancelled: &(dyn Fn() -> bool + Sync),
  dirty_files: &BTreeSet<FileId>,
  force_full_parse: bool,
  progress: Option<&ProgressReporter>,
) -> Result<ScanResult, SessionError> {
  let mut run = || {
    scan_parallel(
      input,
      config,
      rayon::current_num_threads(),
      previous,
      state,
      cancelled,
      dirty_files,
      force_full_parse,
      progress,
    )
  };
  match pool {
    Some(pool) => pool.install(run),
    None => run(),
  }
}

#[expect(
  clippy::too_many_arguments,
  reason = "parallel scan keeps dirty plan + prior state explicit"
)]
#[expect(
  clippy::too_many_lines,
  reason = "scan orchestration stays in one function so dirty plan, counters, and cancellation order stay reviewable"
)]
fn scan_parallel(
  input: &WorkspaceInputSnapshot,
  config: &Config,
  max_workers: usize,
  previous: &AnalysisState,
  state: &mut AnalysisState,
  cancelled: &(dyn Fn() -> bool + Sync),
  dirty_files: &BTreeSet<FileId>,
  force_full_parse: bool,
  progress: Option<&ProgressReporter>,
) -> Result<ScanResult, SessionError> {
  // --- Stage: facts (dirty plan + parse / reuse) ---
  let previously_analyzed = previous.files.keys().cloned().collect::<BTreeSet<_>>();
  let impact = change_impact_from(
    dirty_files,
    force_full_parse,
    &previous.last_context_epochs,
    &input.project_context.epochs,
    &input.sources,
    &previously_analyzed,
  );

  let mut reuse = Vec::new();
  let mut need_parse = Vec::new();
  let mut env_refreshed = BTreeSet::new();
  let mut files_reused = 0_u64;
  for source in &input.sources {
    let environment = source_environment(source, &input.boundary, &input.package_index);
    let must_parse = impact.parse.contains(&source.file_id);
    if let Some(cached) = previous.files.get(&source.file_id)
      && cached.source.as_ref() == source.source.as_ref()
      && !must_parse
    {
      if cached.environment != environment {
        env_refreshed.insert(source.file_id.clone());
      }
      files_reused += 1;
      reuse.push((source, Arc::clone(&cached.analyzed), environment));
      continue;
    }
    need_parse.push((source, environment));
  }

  let files_parsed = u64::try_from(need_parse.len()).unwrap_or(u64::MAX);
  if let Some(progress) = progress {
    progress.emit(&ProgressEvent::Parsing {
      pending: need_parse.len(),
      reused: usize::try_from(files_reused).unwrap_or(usize::MAX),
    });
  }
  let parsed = need_parse
    .par_iter()
    .map(|(source, environment)| {
      let previous_sfc =
        previous.files.get(&source.file_id).and_then(|cached| match cached.analyzed.as_ref() {
          AnalyzedCandidate::Vue { sfc, .. } => Some(sfc.as_ref()),
          AnalyzedCandidate::Script { .. } => None,
        });
      analyze_candidate(source, environment.clone(), previous_sfc)
        .map(|analyzed| (*source, Arc::new(analyzed), environment.clone()))
    })
    .collect::<Vec<_>>();
  if cancelled() {
    return Err(SessionError::Cancelled);
  }

  let discovered = input.sources.iter().map(|source| &source.file_id).collect::<BTreeSet<_>>();
  let mut next_files = BTreeMap::new();
  let mut issues = Vec::new();
  let mut parse_files = BTreeSet::new();
  state.last_affected =
    previous.files.keys().filter(|file| !discovered.contains(file)).cloned().collect();

  for (source, item, environment) in reuse {
    if env_refreshed.contains(&source.file_id) || impact.environment.contains(&source.file_id) {
      state.last_affected.insert(source.file_id.clone());
    }
    next_files.insert(
      source.file_id.clone(),
      CachedCandidate {
        source: Arc::clone(&source.source),
        environment,
        analyzed: Arc::clone(&item),
      },
    );
  }
  for outcome in parsed {
    match outcome {
      Ok((source, item, environment)) => {
        parse_files.insert(source.file_id.clone());
        state.last_affected.insert(source.file_id.clone());
        next_files.insert(
          source.file_id.clone(),
          CachedCandidate {
            source: Arc::clone(&source.source),
            environment,
            analyzed: Arc::clone(&item),
          },
        );
      }
      Err(error) => {
        if let Some(file) = &error.file {
          parse_files.insert(file.clone());
          state.last_affected.insert(file.clone());
        }
        issues.push(error);
      }
    }
  }
  apply_context_consumers(&mut state.last_affected, input, &impact);
  state.last_context_epochs = input.project_context.epochs;
  expand_reverse_dependencies(&mut state.last_affected, &previous.reverse_dependencies);
  state.files = Arc::new(next_files);

  let files_scanned = input.sources.len();
  let mut project_files = Vec::new();
  let mut pending_vue = Vec::new();
  // Prefer `state.files` so environment refreshes (no re-parse) reach rules.
  // Script eligibility waits until module graphs (cross-file seeds) are applied.
  for cached in state.files.values() {
    match cached.analyzed.as_ref() {
      AnalyzedCandidate::Vue { project_file, pending, .. } => {
        project_files.push(Arc::clone(project_file));
        let environment = cached.environment.clone().unwrap_or_default();
        if pending.environment == environment {
          pending_vue.push(Arc::clone(pending));
        } else {
          pending_vue.push(Arc::new(PendingVueFile {
            file_id: pending.file_id.clone(),
            source: Arc::clone(&pending.source),
            environment,
            facts: Arc::clone(&pending.facts),
          }));
        }
      }
      AnalyzedCandidate::Script { project_file } => {
        project_files.push(Arc::clone(project_file));
      }
    }
  }

  // --- Stage: project (graph + module reactivity) ---
  if let Some(progress) = progress {
    progress.emit(&ProgressEvent::BuildingGraph);
  }
  let on_external = progress.map(|reporter| {
    move |roots: usize| {
      reporter.emit(&ProgressEvent::LoadingExternalSeeds { roots });
    }
  });
  let on_external_ref = on_external.as_ref().map(|callback| callback as &dyn Fn(usize));
  // Auto-load ecosystem plugins; only override worker/pool settings.
  let mut trace_options = default_trace_modules_options();
  trace_options.max_workers = max_workers;
  trace_options.reuse_current_pool = true;
  let graph = build_project_graph_incremental_with_options(
    &input.boundary,
    project_files.iter().map(AsRef::as_ref),
    &trace_options,
    &input.project_context,
    &mut state.project,
    on_external_ref,
  );
  if cancelled() {
    return Err(SessionError::Cancelled);
  }
  let project_stats = state.project.last_stats();
  let graph_cow_clones = u64::try_from(project_stats.partition_cow_clones).unwrap_or(u64::MAX);
  issues.extend(graph.reactivity_issues.iter().map(|issue| AnalysisIssue {
    stage: AnalysisStage::ModuleTracing,
    file: issue.module.as_ref().map(ModuleId::file_id),
    message: issue.message.clone(),
    recoverability: Recoverability::Module,
  }));
  let reverse_dependencies = reverse_dependency_index(&graph);
  expand_reverse_dependencies(&mut state.last_affected, &reverse_dependencies);
  state.reverse_dependencies = Arc::new(reverse_dependencies);
  state.last_plan = Arc::new(dirty_plan_from(
    &impact,
    parse_files,
    &state.last_affected,
    &input.sources,
    state.project.last_export_closure.clone(),
  ));
  let plan = Arc::clone(&state.last_plan);
  let modules = graph
    .module_reactivity
    .iter()
    .map(|module| (module.id.clone(), Arc::clone(&module.graph)))
    .collect::<BTreeMap<_, _>>();

  for cached in state.files.values() {
    let AnalyzedCandidate::Script { project_file } = cached.analyzed.as_ref() else {
      continue;
    };
    let primary = modules.get(&ModuleId::primary(&project_file.path)).map(AsRef::as_ref);
    let ordinary = modules.get(&ModuleId::ordinary(&project_file.path)).map(AsRef::as_ref);
    let kind = script_source_kind(project_file.path.as_path());
    if !needs_file_rules(&kind, &project_file.facts, primary, ordinary) {
      continue;
    }
    pending_vue.push(Arc::new(PendingVueFile {
      file_id: project_file.path.clone(),
      source: Arc::clone(&cached.source),
      environment: cached.environment.clone().unwrap_or_default(),
      facts: Arc::clone(&project_file.facts),
    }));
  }

  // --- Stage: rules (seed-aware file diagnostics) ---
  let rules_total = pending_vue.len();
  if let Some(progress) = progress {
    progress.emit(&ProgressEvent::RunningRules { files: rules_total });
  }
  let rules_done = AtomicUsize::new(0);
  let file_diagnostics = pending_vue
    .into_par_iter()
    .map(|pending| {
      let file_id = pending.file_id.clone();
      let module_id = ModuleId::primary(&file_id);
      let ordinary_id = ModuleId::ordinary(&file_id);
      let primary_graph = modules.get(&module_id).map(Arc::clone);
      let ordinary_graph = modules.get(&ordinary_id).map(Arc::clone);
      let key = FileRuleInputKey::new(
        &pending.source,
        &pending.environment,
        primary_graph.as_ref().map(Arc::clone),
        ordinary_graph.as_ref().map(Arc::clone),
      );
      // Outside DirtyPlan.rule_files: keep the previous finalized diagnostics.
      // Inside the plan: FileRuleInputKey still allows reuse when IR is unchanged.
      let (cached, reran) = if !plan.rule_files.contains(&file_id) {
        previous.file_diagnostics.get(&file_id).map_or_else(
          || {
            let diagnostics = run_file_rules(&pending, primary_graph, ordinary_graph);
            (CachedFileDiagnostics { key, diagnostics: diagnostics.into() }, true)
          },
          |cached| {
            (
              CachedFileDiagnostics {
                key: cached.key.clone(),
                diagnostics: Arc::clone(&cached.diagnostics),
              },
              false,
            )
          },
        )
      } else if let Some(cached) = previous.file_diagnostics.get(&file_id)
        && cached.key == key
      {
        (CachedFileDiagnostics { key, diagnostics: Arc::clone(&cached.diagnostics) }, false)
      } else {
        let diagnostics = run_file_rules(&pending, primary_graph, ordinary_graph);
        (CachedFileDiagnostics { key, diagnostics: diagnostics.into() }, true)
      };
      if let Some(progress) = progress {
        let done = rules_done.fetch_add(1, Ordering::Relaxed).saturating_add(1);
        let streamed = finalize_file_diagnostics(
          config,
          &file_id,
          pending.source.as_ref(),
          cached.diagnostics.iter().cloned().collect(),
        );
        progress.emit(&ProgressEvent::FileRules {
          path: file_id.as_str().to_owned(),
          done,
          total: rules_total,
          diagnostics: streamed,
        });
      }
      (file_id, cached, reran)
    })
    .collect::<Vec<_>>();
  if cancelled() {
    return Err(SessionError::Cancelled);
  }

  let rules_rerun = u64::try_from(file_diagnostics.iter().filter(|(_, _, reran)| *reran).count())
    .unwrap_or(u64::MAX);
  state.file_diagnostics = Arc::new(
    file_diagnostics.iter().map(|(file, cached, _)| (file.clone(), cached.clone())).collect(),
  );
  // --- Stage: finalize ---
  let mut raw_diagnostics = file_diagnostics
    .into_iter()
    .flat_map(|(_, cached, _)| cached.diagnostics.iter().cloned().collect::<Vec<_>>())
    .collect::<Vec<_>>();
  raw_diagnostics.extend(graph.diagnostics.clone());
  raw_diagnostics.extend(issues.iter().filter_map(issue_diagnostic));
  let diagnostics_finalized = u64::try_from(raw_diagnostics.len()).unwrap_or(u64::MAX);
  let sources =
    input.sources.iter().map(|source| (source.file_id.clone(), Arc::clone(&source.source)));
  let summary = DiagnosticFinalizer::new(config, sources).finalize(files_scanned, raw_diagnostics);

  // Phase one visits the dirty subset. Cached live modules merge only on a linking miss.
  let module_summaries_visited =
    u64::try_from(project_stats.module_summaries_visited).unwrap_or(u64::MAX);
  let cached_modules_merged =
    u64::try_from(project_stats.cached_modules_merged).unwrap_or(u64::MAX);
  let seed_plans_recomputed =
    u64::try_from(project_stats.seed_plans_recomputed).unwrap_or(u64::MAX);

  let work = ScanWorkCounters {
    files_visited: u64::try_from(input.sources.len()).unwrap_or(u64::MAX),
    files_parsed,
    files_reused,
    structural_partitions_rebuilt: u64::try_from(project_stats.structural_files_rebuilt)
      .unwrap_or(u64::MAX),
    module_summaries_visited,
    cached_modules_merged,
    seed_plans_recomputed,
    export_resolve_ran: project_stats.export_resolve_ran,
    seeded_reparses: u64::try_from(project_stats.seeded_module_reparses).unwrap_or(u64::MAX),
    graph_cow_clones,
    rules_rerun,
    diagnostics_finalized,
  };
  state.last_work = work;
  Ok(ScanResult { summary, graph, issues, work })
}

/// Mark diagnostic / rule consumers when context changes without re-parsing.
fn apply_context_consumers(
  last_affected: &mut BTreeSet<FileId>,
  input: &WorkspaceInputSnapshot,
  impact: &ChangeImpact,
) {
  use crate::locality::ResolutionScope;
  if impact.resolution != ResolutionScope::None || impact.membership {
    // Resolution / membership still invalidates all graph consumers until
    // partitioned linking lands; it must not re-parse unchanged bytes.
    last_affected.extend(input.sources.iter().map(|source| source.file_id.clone()));
    return;
  }
  if impact.component_index {
    last_affected.extend(
      input
        .sources
        .iter()
        .filter(|source| matches!(source.kind, SourceKind::Vue))
        .map(|source| source.file_id.clone()),
    );
  }
  last_affected.extend(impact.environment.iter().cloned());
}

#[cfg(test)]
mod file_rule_key_tests {
  use super::*;

  #[test]
  fn file_rule_input_key_is_stable_for_identical_ir_inputs() {
    let graph = Arc::new(ReactivityGraph::default());
    let environment = RuleEnvironment::default();
    let left = FileRuleInputKey::new("source", &environment, Some(Arc::clone(&graph)), None);
    let right = FileRuleInputKey::new("source", &environment, Some(graph), None);
    assert_eq!(left, right);
  }

  #[test]
  fn file_rule_input_key_changes_when_final_graph_changes() {
    let environment = RuleEnvironment::default();
    let empty = Arc::new(ReactivityGraph::default());
    let changed =
      Arc::new(ReactivityGraph { module_id: "App.vue".into(), ..ReactivityGraph::default() });
    let left = FileRuleInputKey::new("source", &environment, Some(empty), None);
    let right = FileRuleInputKey::new("source", &environment, Some(changed), None);
    assert_ne!(left, right);
  }

  #[test]
  fn file_rule_input_key_changes_when_source_changes() {
    let environment = RuleEnvironment::default();
    let graph = Arc::new(ReactivityGraph::default());
    let left = FileRuleInputKey::new("a", &environment, Some(Arc::clone(&graph)), None);
    let right = FileRuleInputKey::new("b", &environment, Some(graph), None);
    assert_ne!(left, right);
  }
}
