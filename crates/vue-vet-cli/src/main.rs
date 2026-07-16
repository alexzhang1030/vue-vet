use std::{
  fs,
  path::{Path, PathBuf},
  process::ExitCode,
};

use clap::{Parser, ValueEnum};
use ignore::WalkBuilder;
use vue_vet_config::{CONFIG_FILE, Config, apply_suppressions};
use vue_vet_core::{ScanSummary, Severity};
use vue_vet_rules::builtin_registry;
use vue_vet_vize::analyze_sfc;

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
    Ok(summary) => {
      if let Err(error) = print_summary(&summary, cli.format) {
        eprintln!("vue-vet: failed to serialize report: {error}");
        ExitCode::from(2)
      } else if summary.fails(cli.deny_warnings) {
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

fn scan(root: &Path, config: &Config) -> Result<ScanSummary, String> {
  if !root.exists() {
    return Err(format!("path does not exist: {}", root.display()));
  }

  let filter = config.path_filter().map_err(|error| error.to_string())?;

  let mut summary = ScanSummary::default();
  for entry in WalkBuilder::new(root).standard_filters(true).build() {
    let entry = entry.map_err(|error| error.to_string())?;
    let path = entry.path();
    if entry.file_type().is_some_and(|kind| kind.is_dir())
      || path.extension().and_then(|extension| extension.to_str()) != Some("vue")
    {
      continue;
    }
    let logical_path = logical_path(root, path);
    if !filter.matches(logical_path) {
      continue;
    }

    let source = fs::read_to_string(path)
      .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let diagnostics = analyze_sfc(path, &source)
      .map_err(|error| format!("failed to analyze {}: {error}", path.display()))?;
    let diagnostics = config.apply(diagnostics);
    let diagnostics = apply_suppressions(path, &source, diagnostics);
    summary.files_scanned += 1;
    summary.diagnostics.extend(diagnostics);
  }

  Ok(summary.finish())
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
    .validate_rules(builtin_registry().metadata().into_iter().map(|meta| meta.id))
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
