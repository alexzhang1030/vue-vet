use std::{
  collections::BTreeMap,
  ffi::OsStr,
  fs,
  path::{Path, PathBuf},
};

use ignore::{DirEntry, WalkBuilder};
use vue_vet_cache::{CacheLookup, CachePayload, CacheStore, content_key};
use vue_vet_config::{Config, apply_suppressions};
use vue_vet_core::{
  RuleEnvironment, ScanSummary, ScriptFacts, SfcFacts, TemplateFacts, VueVersion,
};
use vue_vet_oxc::analyze_module;
use vue_vet_project::{ProjectFile, ProjectGraph, build_project_graph, resolver_config_inputs};
use vue_vet_reactivity::ModuleSource;
use vue_vet_rules::builtin_registry;
use vue_vet_vize::analyze_sfc_facts_with_environment;

use crate::{AnalysisSnapshot, SessionError};

pub fn analyze(
  root: &Path,
  config: &Config,
  cache_dir: &Path,
  no_cache: bool,
  threads: Option<usize>,
) -> Result<AnalysisSnapshot, SessionError> {
  analyze_inner(root, config, cache_dir, no_cache, threads, &BTreeMap::new())
}

/// Analyze with unsaved buffer overlays. Always bypasses the content-addressed cache
/// because disk bytes are not the analysis input for overlaid paths.
pub fn analyze_with_overlays(
  root: &Path,
  config: &Config,
  threads: Option<usize>,
  overlays: &BTreeMap<PathBuf, String>,
) -> Result<AnalysisSnapshot, SessionError> {
  // Cache keys hash disk content; overlays must never authorize a cached plan.
  analyze_inner(root, config, Path::new(""), true, threads, overlays)
}

fn analyze_inner(
  root: &Path,
  config: &Config,
  cache_dir: &Path,
  no_cache: bool,
  threads: Option<usize>,
  overlays: &BTreeMap<PathBuf, String>,
) -> Result<AnalysisSnapshot, SessionError> {
  let (summary, graph, cache_status) = if no_cache {
    let result = scan_with_threads(root, config, threads, overlays)?;
    (result.summary, result.graph, "disabled")
  } else {
    let files = cache_inputs(root)?;
    let serialized_config = serde_json::to_vec(config)
      .map_err(|error| SessionError::message(format!("failed to hash config: {error}")))?;
    let key = content_key(&files, &serialized_config);
    let store = CacheStore::new(cache_dir.to_path_buf());
    match store.load(&key) {
      CacheLookup::Hit(payload) => (payload.summary, payload.graph, "hit"),
      CacheLookup::Miss => fill_cache(&store, &key, root, config, "miss", threads, overlays)?,
      CacheLookup::RecoveredCorruption => {
        fill_cache(&store, &key, root, config, "recovered-corruption", threads, overlays)?
      }
    }
  };
  Ok(AnalysisSnapshot {
    analyzed_files: analyzed_report_files(&graph),
    summary,
    graph,
    cache_status,
  })
}

fn fill_cache(
  store: &CacheStore,
  key: &str,
  root: &Path,
  config: &Config,
  status: &'static str,
  threads: Option<usize>,
  overlays: &BTreeMap<PathBuf, String>,
) -> Result<(ScanSummary, ProjectGraph, &'static str), SessionError> {
  let result = scan_with_threads(root, config, threads, overlays)?;
  store
    .store(key, &CachePayload { summary: result.summary.clone(), graph: result.graph.clone() })
    .map_err(|error| SessionError::message(error.to_string()))?;
  Ok((result.summary, result.graph, status))
}

fn analyzed_report_files(graph: &ProjectGraph) -> Vec<String> {
  let mut analyzed_files =
    graph.invalidation_inputs.iter().map(|path| path.replace('\\', "/")).collect::<Vec<_>>();
  analyzed_files.sort();
  analyzed_files.dedup();
  analyzed_files
}

fn cache_inputs(root: &Path) -> Result<Vec<(String, Vec<u8>)>, SessionError> {
  let mut files = Vec::new();
  for entry in project_walk(root) {
    let entry = entry.map_err(|error| SessionError::message(error.to_string()))?;
    let path = entry.path();
    // Follow symlinks: package dirs named `*.js` (e.g. node_modules/pixi.js)
    // and symlink installs must not be treated as source files.
    if !path.is_file() {
      continue;
    }
    let source_file = matches!(
      path.extension().and_then(|extension| extension.to_str()),
      Some("vue" | "js" | "jsx" | "ts" | "tsx")
    );
    let package_file = path.file_name().and_then(|name| name.to_str()) == Some("package.json");
    if !source_file && !package_file {
      continue;
    }
    let content = fs::read(path).map_err(|error| {
      SessionError::message(format!("failed to read {} for cache key: {error}", path.display()))
    })?;
    files.push((logical_path(root, path).to_string_lossy().replace('\\', "/"), content));
  }
  if root.is_file()
    && let Some(package) = nearest_package_json(root, scan_directory(root))
  {
    let content = fs::read(&package).map_err(|error| {
      SessionError::message(format!("failed to read {} for cache key: {error}", package.display()))
    })?;
    files.push(("package.json".into(), content));
  }
  let boundary = scan_directory(root);
  for relative in resolver_config_inputs(boundary) {
    let path = boundary.join(&relative);
    if !path.is_file() {
      continue;
    }
    let content = fs::read(&path).map_err(|error| {
      SessionError::message(format!("failed to read {} for cache key: {error}", path.display()))
    })?;
    files.push((relative, content));
  }
  files.sort_by(|left, right| left.0.cmp(&right.0));
  files.dedup_by(|left, right| left.0 == right.0);
  Ok(files)
}

/// Directory used as the project boundary for a file or directory scan path.
#[must_use]
pub fn scan_directory(path: &Path) -> &Path {
  if path.is_dir() { path } else { path.parent().unwrap_or(path) }
}

/// Walk project files for cache keys and analysis.
///
/// Always skips `node_modules` even when `.gitignore` is absent (common when
/// scanning a nested docs/app directory). `standard_filters` still applies
/// gitignore/global ignore when present.
fn project_walk(root: &Path) -> ignore::Walk {
  WalkBuilder::new(root)
    .standard_filters(true)
    .filter_entry(|entry| !is_node_modules_entry(entry))
    .build()
}

fn is_node_modules_entry(entry: &DirEntry) -> bool {
  entry.file_name() == OsStr::new("node_modules")
}

struct ScanResult {
  summary: ScanSummary,
  graph: ProjectGraph,
}

/// Oxlint-style scan: walk/collect paths sequentially, analyze files and run
/// seed-aware rules in parallel, then sort diagnostics for determinism.
fn scan_with_threads(
  root: &Path,
  config: &Config,
  threads: Option<usize>,
  overlays: &BTreeMap<PathBuf, String>,
) -> Result<ScanResult, SessionError> {
  if !root.exists() {
    return Err(SessionError::message(format!("path does not exist: {}", root.display())));
  }

  let run = || scan_parallel(root, config, overlays);
  match threads {
    Some(threads) => {
      let pool =
        rayon::ThreadPoolBuilder::new().num_threads(threads.max(1)).build().map_err(|error| {
          SessionError::message(format!("failed to configure analysis threads: {error}"))
        })?;
      pool.install(run)
    }
    None => run(),
  }
}

fn scan_parallel(
  root: &Path,
  config: &Config,
  overlays: &BTreeMap<PathBuf, String>,
) -> Result<ScanResult, SessionError> {
  use rayon::prelude::*;

  let filter = config.path_filter().map_err(|error| SessionError::message(error.to_string()))?;
  let boundary = scan_directory(root);

  // Phase 0: collect candidates (sequential; ignore crate walk is not parallel-safe).
  let mut candidates = Vec::new();
  for entry in project_walk(root) {
    let entry = entry.map_err(|error| SessionError::message(error.to_string()))?;
    let path = entry.path().to_path_buf();
    // Follow symlinks so directory packages / symlink installs are skipped.
    // Overlay-only paths still require a walk entry (file must exist on disk).
    if !path.is_file() {
      continue;
    }
    let logical = logical_path(root, &path).to_path_buf();
    let extension = path.extension().and_then(|extension| extension.to_str()).map(str::to_owned);
    match extension.as_deref() {
      Some("vue") if filter.matches(&logical) => {
        candidates.push(ScanCandidate::Vue { path, logical_path: logical });
      }
      Some(language @ ("js" | "jsx" | "ts" | "tsx")) => {
        candidates.push(ScanCandidate::Script {
          path,
          logical_path: logical,
          language: language.to_owned(),
        });
      }
      _ => {}
    }
  }
  candidates.sort_by(|left, right| left.sort_key().cmp(right.sort_key()));

  // Phase 1: per-file parse/facts in parallel (oxlint Runtime model).
  let analyzed = candidates
    .par_iter()
    .map(|candidate| analyze_candidate(candidate, boundary, overlays))
    .collect::<Result<Vec<_>, _>>()?;

  let mut summary = ScanSummary::default();
  let mut project_files = Vec::new();
  let mut pending_vue = Vec::new();
  for item in analyzed {
    match item {
      AnalyzedCandidate::Vue { project_file, pending } => {
        summary.files_scanned = summary.files_scanned.saturating_add(1);
        project_files.push(project_file);
        pending_vue.push(pending);
      }
      AnalyzedCandidate::Script { project_file } => {
        project_files.push(project_file);
      }
    }
  }

  // Phase 2: project graph + module seed linking (module re-trace is itself parallel).
  let graph = build_project_graph(boundary, &project_files);
  let modules = graph
    .module_reactivity
    .iter()
    .map(|module| (module.id.clone(), module.graph.clone()))
    .collect::<BTreeMap<_, _>>();

  // Phase 3: seed-aware rules in parallel; diagnostics sorted later by finish().
  let file_diagnostics = pending_vue
    .into_par_iter()
    .map(|pending| {
      let mut facts = pending.facts;
      let module_id = pending.logical_path.to_string_lossy().replace('\\', "/");
      if let Some(graph) = modules.get(module_id.as_str()) {
        facts.apply_module_reactivity(graph.clone());
      }
      let ordinary_id = format!("{module_id}#script");
      if let Some(graph) = modules.get(ordinary_id.as_str()) {
        facts.apply_module_reactivity_for(vue_vet_core::ScriptKind::Script, graph.clone());
      }
      let diagnostics = builtin_registry().run_with_environment(
        &pending.path,
        &pending.source,
        &facts.template,
        &facts.script,
        pending.environment,
      );
      let mut diagnostics = config.apply(diagnostics);
      for diagnostic in &mut diagnostics {
        for edit in &mut diagnostic.edits {
          edit.file.clone_from(&pending.logical_path);
        }
      }
      let diagnostics = apply_suppressions(&pending.path, &pending.source, diagnostics);
      Ok::<_, SessionError>((pending.logical_path, diagnostics))
    })
    .collect::<Result<Vec<_>, _>>()?;

  for (_, diagnostics) in file_diagnostics {
    summary.diagnostics.extend(diagnostics);
  }
  let project_diagnostics = config.apply(graph.diagnostics.clone());
  summary.diagnostics.extend(project_diagnostics);
  Ok(ScanResult { summary: summary.finish(), graph })
}

enum ScanCandidate {
  Vue { path: PathBuf, logical_path: PathBuf },
  Script { path: PathBuf, logical_path: PathBuf, language: String },
}

impl ScanCandidate {
  fn sort_key(&self) -> &Path {
    match self {
      Self::Vue { logical_path, .. } | Self::Script { logical_path, .. } => logical_path,
    }
  }
}

enum AnalyzedCandidate {
  Vue { project_file: ProjectFile, pending: PendingVueFile },
  Script { project_file: ProjectFile },
}

fn analyze_candidate(
  candidate: &ScanCandidate,
  boundary: &Path,
  overlays: &BTreeMap<PathBuf, String>,
) -> Result<AnalyzedCandidate, SessionError> {
  match candidate {
    ScanCandidate::Vue { path, logical_path } => {
      let source = read_source(path, overlays)?;
      let environment = RuleEnvironment { vue_version: vue_version_for(path, boundary) };
      let analysis = analyze_sfc_facts_with_environment(path, &source).map_err(|error| {
        SessionError::message(format!("failed to analyze {}: {error}", path.display()))
      })?;
      let module_id = logical_path.to_string_lossy().replace('\\', "/");
      let project_file = ProjectFile {
        path: logical_path.clone(),
        source_len: source.len(),
        facts: analysis.facts.clone(),
        module_source: analysis.module_source.map(|mut module| {
          module.id.clone_from(&module_id);
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
          path: path.clone(),
          logical_path: logical_path.clone(),
          source,
          environment,
          facts: analysis.facts,
        },
      })
    }
    ScanCandidate::Script { path, logical_path, language } => {
      let source = read_source(path, overlays)?;
      let block = analyze_module(&source, language).map_err(|error| {
        SessionError::message(format!("failed to analyze {}: {error}", path.display()))
      })?;
      Ok(AnalyzedCandidate::Script {
        project_file: ProjectFile {
          path: logical_path.clone(),
          source_len: source.len(),
          facts: SfcFacts {
            template: TemplateFacts::default(),
            script: ScriptFacts { blocks: vec![block] },
          },
          module_source: Some(ModuleSource::standalone(
            logical_path.to_string_lossy().replace('\\', "/"),
            source,
            language.clone(),
            vue_vet_core::ScriptKind::Script,
          )),
          ordinary_module_source: None,
        },
      })
    }
  }
}

struct PendingVueFile {
  path: PathBuf,
  logical_path: PathBuf,
  source: String,
  environment: RuleEnvironment,
  facts: SfcFacts,
}

fn vue_version_for(path: &Path, boundary: &Path) -> Option<VueVersion> {
  let package = nearest_package_json(path, boundary)?;
  let source = fs::read_to_string(package).ok()?;
  let package: serde_json::Value = serde_json::from_str(&source).ok()?;
  ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"]
    .iter()
    .filter_map(|section| package.get(section))
    .filter_map(|section| section.get("vue"))
    .filter_map(serde_json::Value::as_str)
    .find_map(VueVersion::parse_requirement)
}

fn nearest_package_json(path: &Path, boundary: &Path) -> Option<PathBuf> {
  let mut directory = path.parent()?;
  loop {
    if !directory.starts_with(boundary) {
      return None;
    }
    let candidate = directory.join("package.json");
    if candidate.is_file() {
      return Some(candidate);
    }
    if directory == boundary {
      return None;
    }
    directory = directory.parent()?;
  }
}

fn read_source(path: &Path, overlays: &BTreeMap<PathBuf, String>) -> Result<String, SessionError> {
  if let Some(source) = overlay_source(path, overlays) {
    return Ok(source.to_owned());
  }
  fs::read_to_string(path)
    .map_err(|error| SessionError::message(format!("failed to read {}: {error}", path.display())))
}

fn overlay_source<'a>(path: &Path, overlays: &'a BTreeMap<PathBuf, String>) -> Option<&'a str> {
  if let Some(source) = overlays.get(path) {
    return Some(source.as_str());
  }
  // Clients and `ignore` walks may disagree on slash style only.
  let needle = path.to_string_lossy().replace('\\', "/");
  overlays.iter().find_map(|(overlay_path, source)| {
    (overlay_path.to_string_lossy().replace('\\', "/") == needle).then_some(source.as_str())
  })
}

fn logical_path<'a>(root: &'a Path, path: &'a Path) -> &'a Path {
  if root.is_file() {
    path.file_name().map_or(path, |name| Path::new(name))
  } else {
    path.strip_prefix(root).unwrap_or(path)
  }
}
