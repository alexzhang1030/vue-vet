use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
  sync::Arc,
};

use crate::{
  AnalysisCoverage, AnalysisIssue, AnalysisSnapshot, AnalysisStage, Recoverability, SessionError,
  diagnostics::DiagnosticFinalizer,
  discovery::{SourceInput, SourceKind, WorkspaceInputSnapshot},
  file_analysis_registry,
  package_index::PackageIndex,
};
use vue_vet_cache::{CacheLookup, CachePayload, CacheStore, content_key};
use vue_vet_config::Config;
use vue_vet_core::{
  Diagnostic, FileId, RuleEnvironment, ScanSummary, ScriptFacts, Severity, SfcFacts, SourceSpan,
  TemplateFacts,
};
use vue_vet_oxc::analyze_module_source;
use vue_vet_project::{ProjectFile, ProjectGraph, build_project_graph_with_options};
use vue_vet_reactivity::{ModuleSource, TraceModulesOptions};
use vue_vet_vize::{AnalyzeError, analyze_sfc_facts};

pub fn analyze(
  root: &Path,
  config: &Config,
  cache_dir: &Path,
  no_cache: bool,
  threads: Option<usize>,
  state: &mut AnalysisState,
) -> Result<AnalysisSnapshot, SessionError> {
  analyze_inner(root, config, cache_dir, no_cache, threads, &BTreeMap::new(), state)
}

/// Analyze with unsaved buffer overlays. Always bypasses the content-addressed cache
/// because disk bytes are not the analysis input for overlaid paths.
pub fn analyze_with_overlays(
  root: &Path,
  config: &Config,
  threads: Option<usize>,
  overlays: &BTreeMap<PathBuf, String>,
  state: &mut AnalysisState,
) -> Result<AnalysisSnapshot, SessionError> {
  // Cache keys hash disk content; overlays must never authorize a cached plan.
  analyze_inner(root, config, Path::new(""), true, threads, overlays, state)
}

fn analyze_inner(
  root: &Path,
  config: &Config,
  cache_dir: &Path,
  no_cache: bool,
  threads: Option<usize>,
  overlays: &BTreeMap<PathBuf, String>,
  state: &mut AnalysisState,
) -> Result<AnalysisSnapshot, SessionError> {
  let input = WorkspaceInputSnapshot::discover(root, config, overlays)?;
  let analyzed_files =
    input.analyzed_source_files.iter().map(|file| file.as_str().to_owned()).collect();
  let (summary, graph, cache_status, mut issues) = if no_cache {
    let result = scan_with_threads(&input, config, threads, state)?;
    (result.summary, result.graph, "disabled", result.issues)
  } else {
    let serialized_config = serde_json::to_vec(config)
      .map_err(|error| SessionError::message(format!("failed to hash config: {error}")))?;
    let key = content_key(&input.cache_inputs, &serialized_config);
    let store = CacheStore::new(cache_dir.to_path_buf());
    match store.load(&key) {
      CacheLookup::Hit(payload) => (payload.summary, payload.graph, "hit", Vec::new()),
      CacheLookup::Miss => fill_cache(&store, &key, &input, config, "miss", threads, state)?,
      CacheLookup::RecoveredCorruption => {
        fill_cache(&store, &key, &input, config, "recovered-corruption", threads, state)?
      }
    }
  };
  let coverage = AnalysisCoverage {
    analyzed_source_files: input.analyzed_source_files.clone(),
    invalidation_inputs: graph.invalidation_inputs.clone(),
  };
  if let Some(message) = &graph.reactivity_error {
    issues.push(AnalysisIssue {
      stage: AnalysisStage::ModuleTracing,
      file: None,
      message: message.clone(),
      recoverability: Recoverability::Module,
    });
  }
  Ok(AnalysisSnapshot { summary, graph, cache_status, coverage, issues, analyzed_files })
}

fn fill_cache(
  store: &CacheStore,
  key: &str,
  input: &WorkspaceInputSnapshot,
  config: &Config,
  status: &'static str,
  threads: Option<usize>,
  state: &mut AnalysisState,
) -> Result<(ScanSummary, ProjectGraph, &'static str, Vec<AnalysisIssue>), SessionError> {
  let result = scan_with_threads(input, config, threads, state)?;
  if result.issues.is_empty() {
    store
      .store(key, &CachePayload { summary: result.summary.clone(), graph: result.graph.clone() })
      .map_err(|error| SessionError::message(error.to_string()))?;
  }
  Ok((result.summary, result.graph, status, result.issues))
}

/// Directory used as the project boundary for a file or directory scan path.
#[must_use]
pub fn scan_directory(path: &Path) -> &Path {
  if path.is_dir() { path } else { path.parent().unwrap_or(path) }
}

struct ScanResult {
  summary: ScanSummary,
  graph: ProjectGraph,
  issues: Vec<AnalysisIssue>,
}

#[derive(Clone, Debug)]
struct CachedCandidate {
  source: Arc<str>,
  environment: Option<RuleEnvironment>,
  analyzed: AnalyzedCandidate,
}

/// In-memory facts and dependency state retained by a long-lived session.
#[derive(Debug, Default)]
pub struct AnalysisState {
  files: BTreeMap<FileId, CachedCandidate>,
  file_diagnostics: BTreeMap<FileId, Vec<Diagnostic>>,
  pub reverse_dependencies: BTreeMap<FileId, BTreeSet<FileId>>,
  pub last_affected: BTreeSet<FileId>,
}

/// Oxlint-style scan: walk/collect paths sequentially, analyze files and run
/// seed-aware rules in parallel, then sort diagnostics for determinism.
fn scan_with_threads(
  input: &WorkspaceInputSnapshot,
  config: &Config,
  threads: Option<usize>,
  state: &mut AnalysisState,
) -> Result<ScanResult, SessionError> {
  let mut run = || scan_parallel(input, config, rayon::current_num_threads(), state);
  let result = match threads {
    Some(threads) => {
      let pool =
        rayon::ThreadPoolBuilder::new().num_threads(threads.max(1)).build().map_err(|error| {
          SessionError::message(format!("failed to configure analysis threads: {error}"))
        })?;
      pool.install(run)
    }
    None => run(),
  };
  Ok(result)
}

fn scan_parallel(
  input: &WorkspaceInputSnapshot,
  config: &Config,
  max_workers: usize,
  state: &mut AnalysisState,
) -> ScanResult {
  use rayon::prelude::*;

  // Phase 1: per-file parse/facts in parallel (oxlint Runtime model).
  let outcomes = input
    .sources
    .par_iter()
    .map(|source| {
      let environment = source_environment(source, &input.boundary, &input.package_index);
      if let Some(cached) = state.files.get(&source.file_id)
        && cached.source.as_ref() == source.source.as_ref()
        && cached.environment == environment
      {
        return Ok((cached.analyzed.clone(), false, environment));
      }
      analyze_candidate(source, environment.clone()).map(|analyzed| (analyzed, true, environment))
    })
    .collect::<Vec<_>>();
  let discovered = input.sources.iter().map(|source| &source.file_id).collect::<BTreeSet<_>>();
  let mut analyzed = Vec::new();
  let mut next_files = BTreeMap::new();
  let mut issues = Vec::new();
  state.last_affected =
    state.files.keys().filter(|file| !discovered.contains(file)).cloned().collect();
  for (source, outcome) in input.sources.iter().zip(outcomes) {
    match outcome {
      Ok((item, changed, environment)) => {
        if changed {
          state.last_affected.insert(source.file_id.clone());
        }
        next_files.insert(
          source.file_id.clone(),
          CachedCandidate {
            source: Arc::clone(&source.source),
            environment,
            analyzed: item.clone(),
          },
        );
        analyzed.push(item);
      }
      Err(error) => {
        state.last_affected.insert(source.file_id.clone());
        issues.push(error);
      }
    }
  }
  expand_reverse_dependencies(&mut state.last_affected, &state.reverse_dependencies);
  state.files = next_files;

  let files_scanned =
    input.sources.iter().filter(|source| matches!(&source.kind, SourceKind::Vue)).count();
  let mut project_files = Vec::new();
  let mut pending_vue = Vec::new();
  for item in analyzed {
    match item {
      AnalyzedCandidate::Vue { project_file, pending } => {
        project_files.push(project_file);
        pending_vue.push(pending);
      }
      AnalyzedCandidate::Script { project_file } => {
        project_files.push(project_file);
      }
    }
  }

  // Phase 2: project graph + module seed linking (module re-trace is itself parallel).
  let graph = build_project_graph_with_options(
    &input.boundary,
    &project_files,
    TraceModulesOptions { max_workers },
  );
  let reverse_dependencies = reverse_dependency_index(&graph);
  expand_reverse_dependencies(&mut state.last_affected, &reverse_dependencies);
  state.reverse_dependencies = reverse_dependencies;
  let modules = graph
    .module_reactivity
    .iter()
    .map(|module| (module.id.clone(), module.graph.clone()))
    .collect::<BTreeMap<_, _>>();

  // Phase 3: seed-aware rules in parallel; diagnostics sorted later by finish().
  let file_diagnostics = pending_vue
    .into_par_iter()
    .map(|pending| {
      if !state.last_affected.contains(&pending.file_id)
        && let Some(diagnostics) = state.file_diagnostics.get(&pending.file_id)
      {
        return (pending.file_id, diagnostics.clone());
      }
      let mut facts = pending.facts;
      let module_id = pending.file_id.as_str();
      if let Some(graph) = modules.get(module_id) {
        facts.apply_module_reactivity(graph.clone());
      }
      let ordinary_id = format!("{module_id}#script");
      if let Some(graph) = modules.get(ordinary_id.as_str()) {
        facts.apply_module_reactivity_for(vue_vet_core::ScriptKind::Script, graph.clone());
      }
      let diagnostics = file_analysis_registry().run_with_environment(
        pending.file_id.as_path(),
        &pending.source,
        &facts.template,
        &facts.script,
        pending.environment,
      );
      (pending.file_id, diagnostics)
    })
    .collect::<Vec<_>>();

  state.file_diagnostics = file_diagnostics
    .iter()
    .map(|(file, diagnostics)| (file.clone(), diagnostics.clone()))
    .collect();
  let mut raw_diagnostics =
    file_diagnostics.into_iter().flat_map(|(_, diagnostics)| diagnostics).collect::<Vec<_>>();
  raw_diagnostics.extend(graph.diagnostics.clone());
  raw_diagnostics.extend(issues.iter().filter_map(issue_diagnostic));
  let sources = input
    .sources
    .iter()
    .filter(|source| matches!(&source.kind, SourceKind::Vue))
    .map(|source| (source.file_id.clone(), Arc::clone(&source.source)));
  let summary = DiagnosticFinalizer::new(config, sources).finalize(files_scanned, raw_diagnostics);
  ScanResult { summary, graph, issues }
}

#[derive(Clone, Debug)]
enum AnalyzedCandidate {
  Vue { project_file: ProjectFile, pending: PendingVueFile },
  Script { project_file: ProjectFile },
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
      let module_id = input.file_id.as_str();
      let project_file = ProjectFile {
        path: input.file_id.clone(),
        source_len: input.source.len(),
        facts: analysis.facts.clone(),
        module_source: analysis.module_source.map(|mut module| {
          module_id.clone_into(&mut module.id);
          module
        }),
        ordinary_module_source: analysis.ordinary_module_source.map(|mut module| {
          module.id = format!("{module_id}#script");
          module
        }),
      };
      Ok(AnalyzedCandidate::Vue {
        project_file,
        pending: PendingVueFile {
          file_id: input.file_id.clone(),
          source: Arc::clone(&input.source),
          environment,
          facts: analysis.facts,
        },
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
        project_file: ProjectFile {
          path: input.file_id.clone(),
          source_len: input.source.len(),
          facts: SfcFacts {
            template: TemplateFacts::default(),
            script: ScriptFacts { blocks: vec![analysis.script_facts] },
          },
          module_source: Some(
            ModuleSource::standalone(
              input.file_id.as_str(),
              input.source.to_string(),
              language.clone(),
              vue_vet_core::ScriptKind::Script,
            )
            .with_prepared_trace(analysis.module_trace),
          ),
          ordinary_module_source: None,
        },
      })
    }
  }
}

#[derive(Clone, Debug)]
struct PendingVueFile {
  file_id: FileId,
  source: Arc<str>,
  environment: RuleEnvironment,
  facts: SfcFacts,
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
  Some(Diagnostic {
    rule_id: "vue-vet/analysis/parse-error".into(),
    category: "analysis".into(),
    severity: Severity::Error,
    confidence: None,
    documentation: None,
    message: issue.message.clone(),
    help: Some("Fix the syntax error; analysis continued for the rest of the workspace.".into()),
    file,
    span: SourceSpan { offset: 0, length: 0, line: 1, column: 1 },
    edits: Vec::new(),
    recommendation: None,
  })
}

fn reverse_dependency_index(graph: &ProjectGraph) -> BTreeMap<FileId, BTreeSet<FileId>> {
  let mut reverse = BTreeMap::<FileId, BTreeSet<FileId>>::new();
  for edge in &graph.edges {
    let Some(from) = edge.from.strip_prefix("file:") else {
      continue;
    };
    let Some(to) = edge.to.strip_prefix("file:") else {
      continue;
    };
    reverse.entry(FileId::from(to)).or_default().insert(FileId::from(from));
  }
  reverse
}

fn expand_reverse_dependencies(
  affected: &mut BTreeSet<FileId>,
  reverse: &BTreeMap<FileId, BTreeSet<FileId>>,
) {
  let mut pending = affected.iter().cloned().collect::<Vec<_>>();
  while let Some(file) = pending.pop() {
    let Some(dependents) = reverse.get(&file) else {
      continue;
    };
    for dependent in dependents {
      if affected.insert(dependent.clone()) {
        pending.push(dependent.clone());
      }
    }
  }
}
