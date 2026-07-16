use std::{
  fs,
  path::{Path, PathBuf},
  process::ExitCode,
};

use clap::{Parser, ValueEnum};
use ignore::WalkBuilder;
use vue_vet_config::{CONFIG_FILE, Config, apply_suppressions};
use vue_vet_core::{ScanSummary, ScriptFacts, Severity, SfcFacts, TemplateFacts};
use vue_vet_oxc::analyze_module;
use vue_vet_project::{PROJECT_RULE_IDS, ProjectFile, ProjectGraph, build_project_graph};
use vue_vet_rules::builtin_registry;
use vue_vet_vize::analyze_sfc_with_facts;

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
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
  Text,
  Json,
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
  match scan(&cli.path, &config) {
    Ok(result) => {
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
        let analysis = analyze_sfc_with_facts(path, &source)
          .map_err(|error| format!("failed to analyze {}: {error}", path.display()))?;
        let diagnostics = config.apply(analysis.diagnostics);
        let diagnostics = apply_suppressions(path, &source, diagnostics);
        summary.files_scanned = summary.files_scanned.saturating_add(1);
        summary.diagnostics.extend(diagnostics);
        project_files.push(ProjectFile {
          path: logical_path.to_path_buf(),
          source_len: source.len(),
          facts: analysis.facts,
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
  match format {
    OutputFormat::Json => {
      println!("{}", serde_json::to_string_pretty(summary)?);
    }
    OutputFormat::Text => {
      for diagnostic in &summary.diagnostics {
        let severity = match diagnostic.severity {
          Severity::Info => "info",
          Severity::Warning => "warning",
          Severity::Error => "error",
        };
        println!(
          "{}:{}:{}  {}  {}  {}",
          diagnostic.file.display(),
          diagnostic.span.line,
          diagnostic.span.column,
          severity,
          diagnostic.rule_id,
          diagnostic.message
        );
        if let Some(help) = &diagnostic.help {
          println!("  help: {help}");
        }
      }
      println!(
        "\nVue Vet score: {}/100 — {} file(s), {} finding(s)",
        summary.score,
        summary.files_scanned,
        summary.diagnostics.len()
      );
    }
  }
  Ok(())
}
