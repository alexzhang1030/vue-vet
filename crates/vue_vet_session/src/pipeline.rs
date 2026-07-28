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
use vue_vet_vize::{AnalyzeError, analyze_sfc_facts};

use crate::{
  AnalysisIssue, AnalysisStage, Recoverability, SessionError,
  diagnostics::DiagnosticFinalizer,
  discovery::{SourceInput, SourceKind, WorkspaceInputSnapshot},
  file_analysis_registry,
  invalidation::{expand_reverse_dependencies, reverse_dependency_index},
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
  last_context_epochs: ContextEpochs,
  /// Shared until a mutation needs copy-on-write (`Arc::make_mut`).
  project: Arc<ProjectGraphState>,
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
      last_context_epochs: previous.last_context_epochs,
      project: Arc::clone(&previous.project),
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
      last_context_epochs: previous.last_context_epochs,
      project: Arc::clone(&previous.project),
    }
  }

  /// Whether any per-file facts have been committed yet.
  #[must_use]
  pub fn has_file_facts(&self) -> bool {
    !self.files.is_empty()
  }
}

pub struct ScanResult {
  pub summary: ScanSummary,
  pub graph: ProjectGraph,
  pub issues: Vec<AnalysisIssue>,
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
  invalidate_all_sources: bool,
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
      invalidate_all_sources,
    )
  };
  match pool {
    Some(pool) => pool.install(run),
    None => run(),
  }
}

#[expect(
  clippy::too_many_arguments,
  reason = "parallel scan keeps dirty-set schedule explicit beside prior state"
)]
fn scan_parallel(
  input: &WorkspaceInputSnapshot,
  config: &Config,
  max_workers: usize,
  previous: &AnalysisState,
  state: &mut AnalysisState,
  cancelled: &(dyn Fn() -> bool + Sync),
  dirty_files: &BTreeSet<FileId>,
  invalidate_all_sources: bool,
) -> Result<ScanResult, SessionError> {
  let context_invalidate_all =
    epochs_invalidate_all_sources(&previous.last_context_epochs, &input.project_context.epochs)
      || invalidate_all_sources;
  let nuxt_vue_only = !context_invalidate_all
    && previous.last_context_epochs.nuxt_declarations
      != input.project_context.epochs.nuxt_declarations;

  let mut reuse = Vec::new();
  let mut need_parse = Vec::new();
  for source in &input.sources {
    let environment = source_environment(source, &input.boundary, &input.package_index);
    let force = context_invalidate_all
      || dirty_files.contains(&source.file_id)
      || (nuxt_vue_only && matches!(source.kind, SourceKind::Vue));
    if !force
      && let Some(cached) = previous.files.get(&source.file_id)
      && cached.source.as_ref() == source.source.as_ref()
      && cached.environment == environment
    {
      reuse.push((source, Arc::clone(&cached.analyzed), environment));
      continue;
    }
    need_parse.push((source, environment));
  }

  let parsed = need_parse
    .par_iter()
    .map(|(source, environment)| {
      analyze_candidate(source, environment.clone())
        .map(|analyzed| (*source, Arc::new(analyzed), environment.clone()))
    })
    .collect::<Vec<_>>();
  if cancelled() {
    return Err(SessionError::Cancelled);
  }

  let discovered = input.sources.iter().map(|source| &source.file_id).collect::<BTreeSet<_>>();
  let mut analyzed = Vec::new();
  let mut next_files = BTreeMap::new();
  let mut issues = Vec::new();
  state.last_affected =
    previous.files.keys().filter(|file| !discovered.contains(file)).cloned().collect();

  for (source, item, environment) in reuse {
    next_files.insert(
      source.file_id.clone(),
      CachedCandidate {
        source: Arc::clone(&source.source),
        environment,
        analyzed: Arc::clone(&item),
      },
    );
    analyzed.push(item);
  }
  for outcome in parsed {
    match outcome {
      Ok((source, item, environment)) => {
        state.last_affected.insert(source.file_id.clone());
        next_files.insert(
          source.file_id.clone(),
          CachedCandidate {
            source: Arc::clone(&source.source),
            environment,
            analyzed: Arc::clone(&item),
          },
        );
        analyzed.push(item);
      }
      Err(error) => {
        if let Some(file) = &error.file {
          state.last_affected.insert(file.clone());
        }
        issues.push(error);
      }
    }
  }
  apply_context_invalidation(
    &mut state.last_affected,
    input,
    &previous.last_context_epochs,
    &input.project_context.epochs,
  );
  state.last_context_epochs = input.project_context.epochs;
  expand_reverse_dependencies(&mut state.last_affected, &previous.reverse_dependencies);
  state.files = Arc::new(next_files);

  let files_scanned =
    input.sources.iter().filter(|source| matches!(&source.kind, SourceKind::Vue)).count();
  let mut project_files = Vec::new();
  let mut pending_vue = Vec::new();
  for item in &analyzed {
    match item.as_ref() {
      AnalyzedCandidate::Vue { project_file, pending } => {
        project_files.push(Arc::clone(project_file));
        pending_vue.push(Arc::clone(pending));
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
    Arc::make_mut(&mut state.project),
  );
  if cancelled() {
    return Err(SessionError::Cancelled);
  }
  issues.extend(graph.reactivity_issues.iter().map(|issue| AnalysisIssue {
    stage: AnalysisStage::ModuleTracing,
    file: issue.module.as_ref().map(ModuleId::file_id),
    message: issue.message.clone(),
    recoverability: Recoverability::Module,
  }));
  let reverse_dependencies = reverse_dependency_index(&graph);
  expand_reverse_dependencies(&mut state.last_affected, &reverse_dependencies);
  state.reverse_dependencies = Arc::new(reverse_dependencies);
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
        primary_graph.clone(),
        ordinary_graph.clone(),
      );
      if !state.last_affected.contains(&file_id)
        && let Some(cached) = previous.file_diagnostics.get(&file_id)
        && cached.key == key
      {
        return (
          file_id,
          CachedFileDiagnostics { key, diagnostics: Arc::clone(&cached.diagnostics) },
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
      (file_id, CachedFileDiagnostics { key, diagnostics: diagnostics.into() })
    })
    .collect::<Vec<_>>();
  if cancelled() {
    return Err(SessionError::Cancelled);
  }

  state.file_diagnostics = Arc::new(
    file_diagnostics.iter().map(|(file, cached)| (file.clone(), cached.clone())).collect(),
  );
  let mut raw_diagnostics = file_diagnostics
    .into_iter()
    .flat_map(|(_, cached)| cached.diagnostics.iter().cloned().collect::<Vec<_>>())
    .collect::<Vec<_>>();
  raw_diagnostics.extend(graph.diagnostics.clone());
  raw_diagnostics.extend(issues.iter().filter_map(issue_diagnostic));
  let sources = input
    .sources
    .iter()
    .filter(|source| matches!(&source.kind, SourceKind::Vue))
    .map(|source| (source.file_id.clone(), Arc::clone(&source.source)));
  let summary = DiagnosticFinalizer::new(config, sources).finalize(files_scanned, raw_diagnostics);
  Ok(ScanResult { summary, graph, issues })
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

const fn epochs_invalidate_all_sources(previous: &ContextEpochs, current: &ContextEpochs) -> bool {
  previous.package_manifest != current.package_manifest
    || previous.lockfile != current.lockfile
    || previous.tsconfig != current.tsconfig
    || previous.source_membership != current.source_membership
}

fn apply_context_invalidation(
  last_affected: &mut BTreeSet<FileId>,
  input: &WorkspaceInputSnapshot,
  previous: &ContextEpochs,
  current: &ContextEpochs,
) {
  let invalidate_all = epochs_invalidate_all_sources(previous, current);
  if invalidate_all {
    // package.json participates in module resolution (imports/exports/main), not
    // only RuleEnvironment capabilities — force all source consumers.
    last_affected.extend(input.sources.iter().map(|source| source.file_id.clone()));
    return;
  }
  if previous.nuxt_declarations != current.nuxt_declarations {
    last_affected.extend(
      input
        .sources
        .iter()
        .filter(|source| matches!(source.kind, SourceKind::Vue))
        .map(|source| source.file_id.clone()),
    );
  }
}

#[derive(Clone, Debug)]
enum AnalyzedCandidate {
  Vue { project_file: Arc<ProjectFile>, pending: Arc<PendingVueFile> },
  Script { project_file: Arc<ProjectFile> },
}

fn analyze_candidate(
  input: &SourceInput,
  environment: Option<RuleEnvironment>,
) -> Result<AnalyzedCandidate, AnalysisIssue> {
  match &input.kind {
    SourceKind::Vue => {
      let environment = environment.unwrap_or_default();
      let analysis =
        analyze_sfc_facts(input.file_id.as_path(), &input.source).map_err(|error| {
          AnalysisIssue {
            stage: match &error {
              AnalyzeError::Parse(_) | AnalyzeError::Template(_) => AnalysisStage::SfcParse,
              AnalyzeError::Script(_) => AnalysisStage::ScriptParse,
            },
            file: Some(input.file_id.clone()),
            message: format!("failed to analyze {}: {error}", input.physical_path.display()),
            recoverability: Recoverability::File,
          }
        })?;
      let facts = Arc::new(analysis.facts);
      let project_file = Arc::new(ProjectFile {
        path: input.file_id.clone(),
        source_len: input.source.len(),
        facts: Arc::clone(&facts),
        module_source: analysis.module_source.map(|mut module| {
          module.id = ModuleId::primary(&input.file_id);
          module
        }),
        ordinary_module_source: analysis.ordinary_module_source.map(|mut module| {
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
