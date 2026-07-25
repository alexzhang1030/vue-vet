use std::{
  collections::BTreeMap,
  ffi::OsStr,
  fs,
  path::{Path, PathBuf},
  process::ExitCode,
};

use clap::{Args, Parser, ValueEnum};
use ignore::{DirEntry, WalkBuilder};
use vue_vet_cache::{
  Baseline, CacheLookup, CachePayload, CacheStore, content_key, default_cache_dir, filter_diff,
  read_git_diff,
};
use vue_vet_config::{CONFIG_FILE, Config, apply_suppressions};
use vue_vet_core::{
  ReactiveBindingKind, RuleEnvironment, ScanSummary, ScriptFacts, SfcFacts, TemplateFacts,
  TrackingScopeKind, VueVersion,
};
use vue_vet_oxc::analyze_module;
use vue_vet_project::{
  PROJECT_RULE_IDS, ProjectFile, ProjectGraph, build_project_graph, resolver_config_inputs,
};
use vue_vet_reactivity::{ModuleReactivity, ModuleSource};
use vue_vet_reporters::{
  ReactivityDigest, ReactivityModuleStats, ReportContext, ReportFormat, ReportFramework,
  ReportMode, render, render_error, render_reactivity_detail,
};
use vue_vet_rules::builtin_registry;
use vue_vet_vize::analyze_sfc_facts_with_environment;

mod fixes;

use fixes::{FixMode, FixOutcome, execute_safe_edits};

#[derive(Debug, Parser)]
#[command(name = "vue-vet", version, about = "Vet your Vue codebase")]
#[expect(clippy::struct_excessive_bools, reason = "clap maps independent CLI flags to bool fields")]
struct Cli {
  #[arg(default_value = ".")]
  path: PathBuf,

  #[arg(long, value_enum, default_value = "text")]
  format: OutputFormat,

  #[arg(long, help = "Return exit code 1 for warnings as well as errors")]
  deny_warnings: bool,

  #[arg(long, value_name = "FILE", help = "Use an explicit vue-vet.toml")]
  config: Option<PathBuf>,

  #[arg(long, help = "Print the effective configuration as JSON and exit")]
  print_config: bool,

  #[arg(long, help = "Print the deterministic project graph as JSON and exit")]
  print_graph: bool,

  #[arg(long, help = "Print a per-module reactivity tracer breakdown after the normal report")]
  print_reactivity: bool,

  #[command(flatten)]
  cache: CacheArgs,

  #[arg(long, value_name = "FILE", help = "Hide diagnostics matching a versioned baseline")]
  baseline: Option<PathBuf>,

  #[arg(long, value_name = "FILE", help = "Write a versioned baseline after scanning")]
  write_baseline: Option<PathBuf>,

  #[arg(long, value_name = "REF", help = "Report changed lines plus all project findings")]
  diff: Option<String>,

  /// Analysis worker threads (oxlint-style file parallelism). Defaults to available parallelism.
  #[arg(
    long,
    value_name = "N",
    help = "Number of analysis threads (default: available parallelism)"
  )]
  threads: Option<usize>,

  #[command(flatten)]
  fix: FixArgs,
}

#[derive(Args, Debug)]
struct CacheArgs {
  #[arg(long, help = "Disable the content-addressed local cache")]
  no_cache: bool,

  #[arg(long, value_name = "DIR", help = "Override the local cache directory")]
  cache_dir: Option<PathBuf>,

  #[arg(long, help = "Print cache hit, miss, or recovery status on stderr")]
  cache_stats: bool,
}

#[derive(Args, Debug)]
struct FixArgs {
  #[arg(
    long,
    conflicts_with = "fix_safe",
    conflicts_with_all = [
      "baseline",
      "write_baseline",
      "diff",
      "print_config",
      "print_graph",
      "print_reactivity"
    ],
    help = "Validate and preview explicitly safe edits without writing files"
  )]
  fix_dry_run: bool,

  #[arg(
    long,
    conflicts_with = "fix_dry_run",
    conflicts_with_all = [
      "baseline",
      "write_baseline",
      "diff",
      "print_config",
      "print_graph",
      "print_reactivity"
    ],
    help = "Atomically apply explicitly safe edits and report a fresh rescan"
  )]
  fix_safe: bool,
}

impl FixArgs {
  const fn mode(&self) -> Option<FixMode> {
    if self.fix_dry_run {
      Some(FixMode::DryRun)
    } else if self.fix_safe {
      Some(FixMode::Apply)
    } else {
      None
    }
  }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
  Text,
  Json,
  Sarif,
  Github,
}

impl From<OutputFormat> for ReportFormat {
  fn from(format: OutputFormat) -> Self {
    match format {
      OutputFormat::Text => Self::Text,
      OutputFormat::Json => Self::Json,
      OutputFormat::Sarif => Self::Sarif,
      OutputFormat::Github => Self::Github,
    }
  }
}

#[expect(
  clippy::print_stderr,
  clippy::print_stdout,
  reason = "a CLI must emit requested output and report operational errors"
)]
fn main() -> ExitCode {
  let cli = Cli::parse();
  let config = match load_config(&cli.path, cli.config.as_deref()) {
    Ok(config) => config,
    Err(error) => return operational_failure(&cli, &error),
  };
  if cli.print_config {
    return match serde_json::to_string_pretty(&config) {
      Ok(output) => {
        println!("{output}");
        ExitCode::SUCCESS
      }
      Err(error) => {
        operational_failure(&cli, &format!("failed to serialize effective config: {error}"))
      }
    };
  }
  match cached_scan(&cli, &config) {
    Ok((mut result, cache_status)) => {
      if cli.cache.cache_stats {
        eprintln!("vue-vet cache: {cache_status}");
      }
      if let Err(error) = run_requested_fixes(&cli, &config, &mut result) {
        return operational_failure(&cli, &error);
      }
      if let Some(path) = &cli.baseline {
        let baseline = match Baseline::read(path) {
          Ok(baseline) => baseline,
          Err(error) => return operational_failure(&cli, &error.to_string()),
        };
        result.summary = baseline.filter(result.summary);
      }
      if let Some(reference) = &cli.diff {
        let directory = scan_directory(&cli.path);
        let changed = match read_git_diff(directory, reference) {
          Ok(changed) => changed,
          Err(error) => return operational_failure(&cli, &error.to_string()),
        };
        result.summary = filter_diff(result.summary, &changed);
      }
      if let Some(path) = &cli.write_baseline
        && let Err(error) = Baseline::from_summary(&result.summary).write(path)
      {
        return operational_failure(&cli, &error.to_string());
      }
      if cli.print_graph {
        return match serde_json::to_string_pretty(&result.graph) {
          Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
          }
          Err(error) => {
            operational_failure(&cli, &format!("failed to serialize project graph: {error}"))
          }
        };
      }
      let report_context = report_context(&cli, &result);
      if let Err(error) = print_summary(&result.summary, cli.format, &report_context) {
        return operational_failure(&cli, &format!("failed to serialize report: {error}"));
      }
      if cli.print_reactivity
        && matches!(cli.format, OutputFormat::Text)
        && let Some(digest) = &report_context.reactivity
      {
        print!("{}", render_reactivity_detail(digest));
      }
      if result.summary.fails(cli.deny_warnings) { ExitCode::from(1) } else { ExitCode::SUCCESS }
    }
    Err(error) => operational_failure(&cli, &error),
  }
}

struct ScanResult {
  summary: ScanSummary,
  graph: ProjectGraph,
}

fn cached_scan(cli: &Cli, config: &Config) -> Result<(ScanResult, &'static str), String> {
  if cli.cache.no_cache || cli.fix.mode().is_some() {
    return scan_with_threads(&cli.path, config, cli.threads).map(|result| (result, "disabled"));
  }
  let files = cache_inputs(&cli.path)?;
  let serialized_config =
    serde_json::to_vec(config).map_err(|error| format!("failed to hash config: {error}"))?;
  let key = content_key(&files, &serialized_config);
  let store = CacheStore::new(cli.cache.cache_dir.clone().unwrap_or_else(default_cache_dir));
  match store.load(&key) {
    CacheLookup::Hit(payload) => {
      Ok((ScanResult { summary: payload.summary, graph: payload.graph }, "hit"))
    }
    CacheLookup::Miss => fill_cache(&store, &key, &cli.path, config, "miss", cli.threads),
    CacheLookup::RecoveredCorruption => {
      fill_cache(&store, &key, &cli.path, config, "recovered-corruption", cli.threads)
    }
  }
}

fn run_requested_fixes(cli: &Cli, config: &Config, result: &mut ScanResult) -> Result<(), String> {
  let Some(mode) = cli.fix.mode() else {
    return Ok(());
  };
  let edits = result
    .summary
    .diagnostics
    .iter()
    .flat_map(|diagnostic| diagnostic.edits.iter().cloned())
    .collect::<Vec<_>>();
  let outcome = execute_safe_edits(&cli.path, edits, mode).map_err(|error| error.to_string())?;
  print_fix_outcome(mode, outcome);
  if mode == FixMode::Apply && outcome.changed() {
    *result = scan_with_threads(&cli.path, config, cli.threads)?;
  }
  Ok(())
}

#[expect(clippy::print_stderr, reason = "fix summaries belong on CLI stderr")]
fn print_fix_outcome(mode: FixMode, outcome: FixOutcome) {
  let action = if mode == FixMode::DryRun { "would apply" } else { "applied" };
  let edit_label = if outcome.edit_count() == 1 { "edit" } else { "edits" };
  let file_label = if outcome.file_count() == 1 { "file" } else { "files" };
  eprintln!(
    "vue-vet fixes: {action} {} safe {edit_label} to {} {file_label}",
    outcome.edit_count(),
    outcome.file_count()
  );
}

fn fill_cache(
  store: &CacheStore,
  key: &str,
  root: &Path,
  config: &Config,
  status: &'static str,
  threads: Option<usize>,
) -> Result<(ScanResult, &'static str), String> {
  let result = scan_with_threads(root, config, threads)?;
  store
    .store(key, &CachePayload { summary: result.summary.clone(), graph: result.graph.clone() })
    .map_err(|error| error.to_string())?;
  Ok((result, status))
}

fn cache_inputs(root: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
  let mut files = Vec::new();
  for entry in project_walk(root) {
    let entry = entry.map_err(|error| error.to_string())?;
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
    let content = fs::read(path)
      .map_err(|error| format!("failed to read {} for cache key: {error}", path.display()))?;
    files.push((logical_path(root, path).to_string_lossy().replace('\\', "/"), content));
  }
  if root.is_file()
    && let Some(package) = nearest_package_json(root, scan_directory(root))
  {
    let content = fs::read(&package)
      .map_err(|error| format!("failed to read {} for cache key: {error}", package.display()))?;
    files.push(("package.json".into(), content));
  }
  let boundary = scan_directory(root);
  for relative in resolver_config_inputs(boundary) {
    let path = boundary.join(&relative);
    if !path.is_file() {
      continue;
    }
    let content = fs::read(&path)
      .map_err(|error| format!("failed to read {} for cache key: {error}", path.display()))?;
    files.push((relative, content));
  }
  files.sort_by(|left, right| left.0.cmp(&right.0));
  files.dedup_by(|left, right| left.0 == right.0);
  Ok(files)
}

fn scan_directory(path: &Path) -> &Path {
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

/// Oxlint-style scan: walk/collect paths sequentially, analyze files and run
/// seed-aware rules in parallel, then sort diagnostics for determinism.
fn scan_with_threads(
  root: &Path,
  config: &Config,
  threads: Option<usize>,
) -> Result<ScanResult, String> {
  if !root.exists() {
    return Err(format!("path does not exist: {}", root.display()));
  }

  let run = || scan_parallel(root, config);
  match threads {
    Some(threads) => {
      let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads.max(1))
        .build()
        .map_err(|error| format!("failed to configure analysis threads: {error}"))?;
      pool.install(run)
    }
    None => run(),
  }
}

fn scan_parallel(root: &Path, config: &Config) -> Result<ScanResult, String> {
  use rayon::prelude::*;

  let filter = config.path_filter().map_err(|error| error.to_string())?;
  let boundary = scan_directory(root);

  // Phase 0: collect candidates (sequential; ignore crate walk is not parallel-safe).
  let mut candidates = Vec::new();
  for entry in project_walk(root) {
    let entry = entry.map_err(|error| error.to_string())?;
    let path = entry.path().to_path_buf();
    // Follow symlinks so directory packages / symlink installs are skipped.
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
    .map(|candidate| analyze_candidate(candidate, boundary))
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
      Ok::<_, String>((pending.logical_path, diagnostics))
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
) -> Result<AnalyzedCandidate, String> {
  match candidate {
    ScanCandidate::Vue { path, logical_path } => {
      let source = read_source(path)?;
      let environment = RuleEnvironment { vue_version: vue_version_for(path, boundary) };
      let analysis = analyze_sfc_facts_with_environment(path, &source)
        .map_err(|error| format!("failed to analyze {}: {error}", path.display()))?;
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
      let source = read_source(path)?;
      let block = analyze_module(&source, language)
        .map_err(|error| format!("failed to analyze {}: {error}", path.display()))?;
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

fn report_context(cli: &Cli, result: &ScanResult) -> ReportContext {
  let mut skipped_check_reasons = BTreeMap::new();
  if let Some(error) = &result.graph.reactivity_error {
    skipped_check_reasons.insert("module_reactivity".into(), error.clone());
  }
  let module_stats = reactivity_module_stats(&result.graph.module_reactivity);
  let mut digest =
    ReactivityDigest::from_modules(&module_stats, result.graph.reactivity_error.clone());
  if cli.print_reactivity {
    digest = digest.with_modules_detail(&module_stats);
  }
  ReportContext {
    mode: report_mode(cli),
    framework: report_framework(&cli.path),
    project_root: report_root(&cli.path),
    analyzed_files: result.graph.invalidation_inputs.clone(),
    complete: skipped_check_reasons.is_empty(),
    skipped_check_reasons,
    reactivity: Some(digest),
  }
}

fn reactivity_module_stats(modules: &[ModuleReactivity]) -> Vec<ReactivityModuleStats> {
  let mut stats = modules
    .iter()
    .map(|module| {
      let mut binding_labels = module
        .graph
        .bindings
        .iter()
        .map(|binding| format!("{}:{}", binding.name, binding_kind_label(binding.kind)))
        .collect::<Vec<_>>();
      binding_labels.sort();
      let mut scope_labels = module
        .graph
        .scopes
        .iter()
        .map(|scope| {
          let kind = scope_kind_label(scope.kind);
          scope.binding.as_ref().map_or_else(
            || format!("{kind}({})", scope.callee),
            |binding| format!("{kind}({binding})"),
          )
        })
        .collect::<Vec<_>>();
      scope_labels.sort();
      let mut edge_labels = module
        .graph
        .edges
        .iter()
        .map(|edge| format!("{} -> {}", edge.from, edge.to))
        .collect::<Vec<_>>();
      edge_labels.sort();
      let mut template_labels = module
        .graph
        .template_reads
        .iter()
        .map(|read| format!("{}@{}", read.binding, read.surface))
        .collect::<Vec<_>>();
      template_labels.sort();
      ReactivityModuleStats {
        id: module.id.clone(),
        bindings: module.graph.bindings.len(),
        scopes: module.graph.scopes.len(),
        edges: module.graph.edges.len(),
        template_reads: module.graph.template_reads.len(),
        binding_labels,
        scope_labels,
        edge_labels,
        template_labels,
      }
    })
    .collect::<Vec<_>>();
  stats.sort_by(|left, right| left.id.cmp(&right.id));
  stats
}

const fn binding_kind_label(kind: ReactiveBindingKind) -> &'static str {
  match kind {
    ReactiveBindingKind::Ref => "ref",
    ReactiveBindingKind::ShallowRef => "shallow_ref",
    ReactiveBindingKind::Computed => "computed",
    ReactiveBindingKind::Reactive => "reactive",
    ReactiveBindingKind::ShallowReactive => "shallow_reactive",
    ReactiveBindingKind::Readonly => "readonly",
    ReactiveBindingKind::ShallowReadonly => "shallow_readonly",
    ReactiveBindingKind::CustomRef => "custom_ref",
    ReactiveBindingKind::ToRef => "to_ref",
    ReactiveBindingKind::TemplateRef => "template_ref",
    ReactiveBindingKind::ModelRef => "model_ref",
  }
}

const fn scope_kind_label(kind: TrackingScopeKind) -> &'static str {
  match kind {
    TrackingScopeKind::WatchEffect => "watch_effect",
    TrackingScopeKind::WatchPostEffect => "watch_post_effect",
    TrackingScopeKind::WatchSyncEffect => "watch_sync_effect",
    TrackingScopeKind::Computed => "computed",
    TrackingScopeKind::WatchSources => "watch_sources",
    TrackingScopeKind::WatchCallback => "watch_callback",
    TrackingScopeKind::EffectScope => "effect_scope",
    TrackingScopeKind::OnScopeDispose => "on_scope_dispose",
  }
}

const fn report_mode(cli: &Cli) -> ReportMode {
  if cli.diff.is_some() {
    ReportMode::Diff
  } else if cli.baseline.is_some() {
    ReportMode::Baseline
  } else {
    ReportMode::Full
  }
}

fn report_root(path: &Path) -> String {
  let root = scan_directory(path).to_string_lossy().replace('\\', "/");
  if root.is_empty() { ".".into() } else { root }
}

fn report_framework(root: &Path) -> ReportFramework {
  let package = if root.is_dir() {
    root.join("package.json")
  } else {
    root.parent().unwrap_or(root).join("package.json")
  };
  let Ok(source) = fs::read_to_string(package) else {
    return ReportFramework::Vue;
  };
  let Ok(package) = serde_json::from_str::<serde_json::Value>(&source) else {
    return ReportFramework::Vue;
  };
  let is_nuxt = ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"]
    .iter()
    .filter_map(|section| package.get(section))
    .any(|section| section.get("nuxt").is_some());
  if is_nuxt { ReportFramework::Nuxt } else { ReportFramework::Vue }
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

fn read_source(path: &Path) -> Result<String, String> {
  fs::read_to_string(path).map_err(|error| format!("failed to read {}: {error}", path.display()))
}

fn logical_path<'a>(root: &'a Path, path: &'a Path) -> &'a Path {
  if root.is_file() {
    path.file_name().map_or(path, |name| Path::new(name))
  } else {
    path.strip_prefix(root).unwrap_or(path)
  }
}

fn load_config(root: &Path, explicit: Option<&Path>) -> Result<Config, String> {
  let discovered = explicit.map_or_else(
    || {
      let directory = if root.is_dir() { root } else { root.parent().unwrap_or(root) };
      let candidate = directory.join(CONFIG_FILE);
      candidate.exists().then_some(candidate)
    },
    |explicit| Some(explicit.to_path_buf()),
  );
  let config = if let Some(path) = discovered {
    let source = fs::read_to_string(&path)
      .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    Config::parse(&source).map_err(|error| format!("{}: {error}", path.display()))?
  } else {
    Config::default()
  };
  config
    .validate_rules(
      builtin_registry().metadata().into_iter().map(|meta| meta.id).chain(PROJECT_RULE_IDS),
    )
    .map_err(|error| error.to_string())?;
  Ok(config)
}

#[expect(
  clippy::print_stderr,
  clippy::print_stdout,
  reason = "a CLI must emit structured failures or human-readable operational errors"
)]
fn operational_failure(cli: &Cli, message: &str) -> ExitCode {
  if matches!(cli.format, OutputFormat::Json) {
    let context = ReportContext {
      mode: report_mode(cli),
      framework: report_framework(&cli.path),
      project_root: report_root(&cli.path),
      analyzed_files: Vec::new(),
      complete: false,
      skipped_check_reasons: BTreeMap::from([("scan".into(), message.into())]),
      reactivity: None,
    };
    match render_error(message, &context) {
      Ok(output) => println!("{output}"),
      Err(error) => eprintln!("vue-vet: {message}; failed to serialize error report: {error}"),
    }
  } else {
    eprintln!("vue-vet: {message}");
  }
  ExitCode::from(2)
}

#[expect(clippy::print_stdout, reason = "a CLI must emit requested reports on stdout")]
fn print_summary(
  summary: &ScanSummary,
  format: OutputFormat,
  context: &ReportContext,
) -> Result<(), serde_json::Error> {
  let output = render(summary, format.into(), context)?;
  println!("{output}");
  Ok(())
}
