use std::{
  collections::{BTreeMap, BTreeSet},
  path::Path,
  sync::Arc,
};

use rayon::prelude::*;
use vue_vet_config::Config;
use vue_vet_core::{
  Diagnostic, FileId, ModuleId, ReactivityGraph, RuleEnvironment, ScanSummary, ScriptFacts,
  Severity, SfcFacts, SourceSpan, TemplateFacts, content_digest, serde_digest,
};
use vue_vet_oxc::analyze_module_source;
use vue_vet_project::{
  ContextEpochs, ProjectFile, ProjectGraph, ProjectGraphState,
  build_project_graph_incremental_with_options,
};
use vue_vet_reactivity::{ModuleSource, TraceModulesOptions};
use vue_vet_vize::{AnalyzeError, AnalyzedSfc, analyze_sfc_facts_reusing};

use crate::{
  AnalysisIssue, AnalysisStage, Recoverability, SessionError,
  diagnostics::DiagnosticFinalizer,
  discovery::{SourceInput, SourceKind, WorkspaceInputSnapshot},
  file_analysis_registry,
  invalidation::{expand_reverse_dependencies, reverse_dependency_index},
  locality::{ChangeImpact, DirtyPlan, ScanWorkCounters, change_impact_from, dirty_plan_from},
  package_index::PackageIndex,
};

#[derive(Clone, Debug)]
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
) -> Result<ScanResult, SessionError> {
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

  let files_scanned =
    input.sources.iter().filter(|source| matches!(&source.kind, SourceKind::Vue)).count();
  let mut project_files = Vec::new();
  let mut pending_vue = Vec::new();
  // Prefer `state.files` so environment refreshes (no re-parse) reach rules.
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

  let graph = build_project_graph_incremental_with_options(
    &input.boundary,
    project_files.iter().map(AsRef::as_ref),
    TraceModulesOptions { max_workers, reuse_current_pool: true },
    &input.project_context,
    &mut state.project,
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
  state.last_plan =
    Arc::new(dirty_plan_from(&impact, parse_files, &state.last_affected, &input.sources));
  let plan = Arc::clone(&state.last_plan);
  let modules = graph
    .module_reactivity
    .iter()
    .map(|module| (module.id.clone(), Arc::clone(&module.graph)))
    .collect::<BTreeMap<_, _>>();

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
      if !plan.rule_files.contains(&file_id) {
        if let Some(cached) = previous.file_diagnostics.get(&file_id) {
          return (
            file_id,
            CachedFileDiagnostics {
              key: cached.key.clone(),
              diagnostics: Arc::clone(&cached.diagnostics),
            },
            false,
          );
        }
      } else if let Some(cached) = previous.file_diagnostics.get(&file_id)
        && cached.key == key
      {
        return (
          file_id,
          CachedFileDiagnostics { key, diagnostics: Arc::clone(&cached.diagnostics) },
          false,
        );
      }
      let mut facts = (*pending.facts).clone();
      if let Some(graph) = primary_graph {
        facts.apply_module_reactivity(graph);
      }
      if let Some(graph) = ordinary_graph {
        facts.apply_module_reactivity_for(vue_vet_core::ScriptKind::Script, graph);
      }
      let diagnostics = file_analysis_registry().run_with_environment(
        file_id.as_path(),
        &pending.source,
        &facts.template,
        &facts.script,
        pending.environment.clone(),
      );
      (file_id, CachedFileDiagnostics { key, diagnostics: diagnostics.into() }, true)
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
  let mut raw_diagnostics = file_diagnostics
    .into_iter()
    .flat_map(|(_, cached, _)| cached.diagnostics.iter().cloned().collect::<Vec<_>>())
    .collect::<Vec<_>>();
  raw_diagnostics.extend(graph.diagnostics.clone());
  raw_diagnostics.extend(issues.iter().filter_map(issue_diagnostic));
  let diagnostics_finalized = u64::try_from(raw_diagnostics.len()).unwrap_or(u64::MAX);
  let sources = input
    .sources
    .iter()
    .filter(|source| matches!(&source.kind, SourceKind::Vue))
    .map(|source| (source.file_id.clone(), Arc::clone(&source.source)));
  let summary = DiagnosticFinalizer::new(config, sources).finalize(files_scanned, raw_diagnostics);

  // Phase-one still walks every module summary (cheap when already attached).
  // Seed-plan / export-resolve counters come from ModuleTraceState linking cache.
  let module_summaries_visited = u64::try_from(graph.module_reactivity.len()).unwrap_or(u64::MAX);
  let seed_plans_recomputed =
    u64::try_from(project_stats.seed_plans_recomputed).unwrap_or(u64::MAX);

  let work = ScanWorkCounters {
    files_visited: u64::try_from(input.sources.len()).unwrap_or(u64::MAX),
    files_parsed,
    files_reused,
    structural_partitions_rebuilt: u64::try_from(project_stats.structural_files_rebuilt)
      .unwrap_or(u64::MAX),
    module_summaries_visited,
    seed_plans_recomputed,
    graph_cow_clones,
    rules_rerun,
    diagnostics_finalized,
  };
  state.last_work = work;
  Ok(ScanResult { summary, graph, issues, work })
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

#[derive(Clone, Debug)]
enum AnalyzedCandidate {
  Vue {
    project_file: Arc<ProjectFile>,
    pending: Arc<PendingVueFile>,
    /// Retained for SFC block-level reuse on the next edit.
    sfc: Arc<AnalyzedSfc>,
  },
  Script {
    project_file: Arc<ProjectFile>,
  },
}

fn analyze_candidate(
  input: &SourceInput,
  environment: Option<RuleEnvironment>,
  previous_sfc: Option<&AnalyzedSfc>,
) -> Result<AnalyzedCandidate, AnalysisIssue> {
  match &input.kind {
    SourceKind::Vue => {
      let environment = environment.unwrap_or_default();
      let analysis =
        analyze_sfc_facts_reusing(input.file_id.as_path(), &input.source, previous_sfc).map_err(
          |error| AnalysisIssue {
            stage: match &error {
              AnalyzeError::Parse(_) | AnalyzeError::Template(_) => AnalysisStage::SfcParse,
              AnalyzeError::Script(_) => AnalysisStage::ScriptParse,
            },
            file: Some(input.file_id.clone()),
            message: format!("failed to analyze {}: {error}", input.physical_path.display()),
            recoverability: Recoverability::File,
          },
        )?;
      let sfc = Arc::new(analysis);
      let facts = Arc::new(sfc.facts.clone());
      let project_file = Arc::new(ProjectFile {
        path: input.file_id.clone(),
        source_len: input.source.len(),
        facts: Arc::clone(&facts),
        module_source: sfc.module_source.clone().map(|mut module| {
          module.id = ModuleId::primary(&input.file_id);
          module
        }),
        ordinary_module_source: sfc.ordinary_module_source.clone().map(|mut module| {
          module.id = ModuleId::ordinary(&input.file_id);
          module
        }),
      });
      Ok(AnalyzedCandidate::Vue {
        project_file,
        pending: Arc::new(PendingVueFile {
          file_id: input.file_id.clone(),
          source: Arc::clone(&input.source),
          environment,
          facts,
        }),
        sfc,
      })
    }
    SourceKind::Script { language } => {
      let analysis = analyze_module_source(
        &input.source,
        &input.source,
        0,
        language,
        vue_vet_core::ScriptKind::Script,
      )
      .map_err(|error| AnalysisIssue {
        stage: AnalysisStage::ScriptParse,
        file: Some(input.file_id.clone()),
        message: format!("failed to analyze {}: {error}", input.physical_path.display()),
        recoverability: Recoverability::File,
      })?;
      Ok(AnalyzedCandidate::Script {
        project_file: Arc::new(ProjectFile {
          path: input.file_id.clone(),
          source_len: input.source.len(),
          facts: Arc::new(SfcFacts {
            template: TemplateFacts::default(),
            script: ScriptFacts { blocks: vec![analysis.script_facts] },
          }),
          module_source: Some(
            ModuleSource::standalone(
              ModuleId::primary(&input.file_id),
              Arc::clone(&input.source),
              language.clone(),
              vue_vet_core::ScriptKind::Script,
            )
            .with_module_summary(analysis.module_trace),
          ),
          ordinary_module_source: None,
        }),
      })
    }
  }
}

#[derive(Clone, Debug)]
struct PendingVueFile {
  file_id: FileId,
  source: Arc<str>,
  environment: RuleEnvironment,
  facts: Arc<SfcFacts>,
}

fn source_environment(
  input: &SourceInput,
  boundary: &Path,
  package_index: &PackageIndex,
) -> Option<RuleEnvironment> {
  matches!(&input.kind, SourceKind::Vue)
    .then(|| package_index.environment_for(input.physical_path.as_path(), boundary))
}

fn issue_diagnostic(issue: &AnalysisIssue) -> Option<Diagnostic> {
  let file = issue.file.clone()?;
  let (rule_id, help) = match issue.stage {
    AnalysisStage::ModuleTracing => (
      "vue-vet/analysis/module-tracing",
      "Fix the module or its resolved import edge; other healthy module links were retained.",
    ),
    AnalysisStage::SfcParse | AnalysisStage::ScriptParse => (
      "vue-vet/analysis/parse-error",
      "Fix the syntax error; analysis continued for the rest of the workspace.",
    ),
  };
  Some(Diagnostic {
    rule_id: rule_id.into(),
    category: "analysis".into(),
    severity: Severity::Error,
    confidence: None,
    documentation: None,
    message: issue.message.clone(),
    help: Some(help.into()),
    file,
    span: SourceSpan { offset: 0, length: 0, line: 1, column: 1 },
    edits: Vec::new(),
    recommendation: None,
  })
}
