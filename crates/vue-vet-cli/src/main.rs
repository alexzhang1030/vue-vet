use std::{
  fs,
  path::{Path, PathBuf},
  process::ExitCode,
};

use clap::{Args, Parser, ValueEnum};
use ignore::WalkBuilder;
use vue_vet_cache::{
  Baseline, CacheLookup, CachePayload, CacheStore, content_key, default_cache_dir, filter_diff,
  read_git_diff,
};
use vue_vet_config::{CONFIG_FILE, Config, apply_suppressions};
use vue_vet_core::{
  RuleEnvironment, ScanSummary, ScriptFacts, SfcFacts, TemplateFacts, VueVersion,
};
use vue_vet_oxc::analyze_module;
use vue_vet_project::{PROJECT_RULE_IDS, ProjectFile, ProjectGraph, build_project_graph};
use vue_vet_reactivity::ModuleSource;
use vue_vet_reporters::{ReportFormat, render};
use vue_vet_rules::builtin_registry;
use vue_vet_vize::analyze_sfc_with_environment;

#[derive(Debug, Parser)]
#[command(name = "vue-vet", version, about = "Vet your Vue codebase")]
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

  #[command(flatten)]
  cache: CacheArgs,

  #[arg(long, value_name = "FILE", help = "Hide diagnostics matching a versioned baseline")]
  baseline: Option<PathBuf>,

  #[arg(long, value_name = "FILE", help = "Write a versioned baseline after scanning")]
  write_baseline: Option<PathBuf>,

  #[arg(long, value_name = "REF", help = "Report changed lines plus all project findings")]
  diff: Option<String>,
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

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
  Text,
  Json,
}

impl From<OutputFormat> for ReportFormat {
  fn from(format: OutputFormat) -> Self {
    match format {
      OutputFormat::Text => Self::Text,
      OutputFormat::Json => Self::Json,
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
    Err(error) => {
      eprintln!("vue-vet: {error}");
      return ExitCode::from(2);
    }
  };
  if cli.print_config {
    return match serde_json::to_string_pretty(&config) {
      Ok(output) => {
        println!("{output}");
        ExitCode::SUCCESS
      }
      Err(error) => {
        eprintln!("vue-vet: failed to serialize effective config: {error}");
        ExitCode::from(2)
      }
    };
  }
  match cached_scan(&cli, &config) {
    Ok((mut result, cache_status)) => {
      if cli.cache.cache_stats {
        eprintln!("vue-vet cache: {cache_status}");
      }
      if let Some(path) = &cli.baseline {
        let baseline = match Baseline::read(path) {
          Ok(baseline) => baseline,
          Err(error) => {
            eprintln!("vue-vet: {error}");
            return ExitCode::from(2);
          }
        };
        result.summary = baseline.filter(result.summary);
      }
      if let Some(reference) = &cli.diff {
        let directory = scan_directory(&cli.path);
        let changed = match read_git_diff(directory, reference) {
          Ok(changed) => changed,
          Err(error) => {
            eprintln!("vue-vet: {error}");
            return ExitCode::from(2);
          }
        };
        result.summary = filter_diff(result.summary, &changed);
      }
      if let Some(path) = &cli.write_baseline
        && let Err(error) = Baseline::from_summary(&result.summary).write(path)
      {
        eprintln!("vue-vet: {error}");
        return ExitCode::from(2);
      }
      if cli.print_graph {
        return match serde_json::to_string_pretty(&result.graph) {
          Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
          }
          Err(error) => {
            eprintln!("vue-vet: failed to serialize project graph: {error}");
            ExitCode::from(2)
          }
        };
      }
      if let Err(error) = print_summary(&result.summary, cli.format) {
        eprintln!("vue-vet: failed to serialize report: {error}");
        ExitCode::from(2)
      } else if result.summary.fails(cli.deny_warnings) {
        ExitCode::from(1)
      } else {
        ExitCode::SUCCESS
      }
    }
    Err(error) => {
      eprintln!("vue-vet: {error}");
      ExitCode::from(2)
    }
  }
}

struct ScanResult {
  summary: ScanSummary,
  graph: ProjectGraph,
}

fn cached_scan(cli: &Cli, config: &Config) -> Result<(ScanResult, &'static str), String> {
  if cli.cache.no_cache {
    return scan(&cli.path, config).map(|result| (result, "disabled"));
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
    CacheLookup::Miss => fill_cache(&store, &key, &cli.path, config, "miss"),
    CacheLookup::RecoveredCorruption => {
      fill_cache(&store, &key, &cli.path, config, "recovered-corruption")
    }
  }
}

fn fill_cache(
  store: &CacheStore,
  key: &str,
  root: &Path,
  config: &Config,
  status: &'static str,
) -> Result<(ScanResult, &'static str), String> {
  let result = scan(root, config)?;
  store
    .store(key, &CachePayload { summary: result.summary.clone(), graph: result.graph.clone() })
    .map_err(|error| error.to_string())?;
  Ok((result, status))
}

fn cache_inputs(root: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
  let mut files = Vec::new();
  for entry in WalkBuilder::new(root).standard_filters(true).build() {
    let entry = entry.map_err(|error| error.to_string())?;
    if entry.file_type().is_some_and(|kind| kind.is_dir()) {
      continue;
    }
    let path = entry.path();
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
  files.sort_by(|left, right| left.0.cmp(&right.0));
  files.dedup_by(|left, right| left.0 == right.0);
  Ok(files)
}

fn scan_directory(path: &Path) -> &Path {
  if path.is_dir() { path } else { path.parent().unwrap_or(path) }
}

fn scan(root: &Path, config: &Config) -> Result<ScanResult, String> {
  if !root.exists() {
    return Err(format!("path does not exist: {}", root.display()));
  }

  let filter = config.path_filter().map_err(|error| error.to_string())?;

  let mut summary = ScanSummary::default();
  let mut project_files = Vec::new();
  for entry in WalkBuilder::new(root).standard_filters(true).build() {
    let entry = entry.map_err(|error| error.to_string())?;
    let path = entry.path();
    if entry.file_type().is_some_and(|kind| kind.is_dir()) {
      continue;
    }
    let logical_path = logical_path(root, path);
    let extension = path.extension().and_then(|extension| extension.to_str());
    match extension {
      Some("vue") if filter.matches(logical_path) => {
        let source = read_source(path)?;
        let environment =
          RuleEnvironment { vue_version: vue_version_for(path, scan_directory(root)) };
        let analysis = analyze_sfc_with_environment(path, &source, environment)
          .map_err(|error| format!("failed to analyze {}: {error}", path.display()))?;
        let diagnostics = config.apply(analysis.diagnostics);
        let diagnostics = apply_suppressions(path, &source, diagnostics);
        summary.files_scanned = summary.files_scanned.saturating_add(1);
        summary.diagnostics.extend(diagnostics);
        project_files.push(ProjectFile {
          path: logical_path.to_path_buf(),
          source_len: source.len(),
          facts: analysis.facts,
          module_source: None,
        });
      }
      Some(language @ ("js" | "jsx" | "ts" | "tsx")) => {
        let source = read_source(path)?;
        let block = analyze_module(&source, language)
          .map_err(|error| format!("failed to analyze {}: {error}", path.display()))?;
        project_files.push(ProjectFile {
          path: logical_path.to_path_buf(),
          source_len: source.len(),
          facts: SfcFacts {
            template: TemplateFacts::default(),
            script: ScriptFacts { blocks: vec![block] },
          },
          module_source: Some(ModuleSource {
            id: logical_path.to_string_lossy().replace('\\', "/"),
            source,
            language: language.into(),
            kind: vue_vet_core::ScriptKind::Script,
          }),
        });
      }
      _ => {}
    }
  }

  let graph = build_project_graph(&project_files);
  let project_diagnostics = config.apply(graph.diagnostics.clone());
  summary.diagnostics.extend(project_diagnostics);
  Ok(ScanResult { summary: summary.finish(), graph })
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

#[expect(clippy::print_stdout, reason = "a CLI must emit requested reports on stdout")]
fn print_summary(summary: &ScanSummary, format: OutputFormat) -> Result<(), serde_json::Error> {
  let output = render(summary, format.into())?;
  println!("{output}");
  Ok(())
}
