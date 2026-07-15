use std::{fs, path::PathBuf, process::ExitCode};

use clap::{Parser, ValueEnum};
use ignore::WalkBuilder;
use vue_vet_core::{ScanSummary, Severity};
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
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
  Text,
  Json,
}

#[expect(clippy::print_stderr, reason = "a CLI must report operational errors on stderr")]
fn main() -> ExitCode {
  let cli = Cli::parse();
  match scan(&cli.path) {
    Ok(summary) => {
      print_summary(&summary, cli.format);
      if summary.fails(cli.deny_warnings) { ExitCode::from(1) } else { ExitCode::SUCCESS }
    }
    Err(error) => {
      eprintln!("vue-vet: {error}");
      ExitCode::from(2)
    }
  }
}

fn scan(root: &PathBuf) -> Result<ScanSummary, String> {
  if !root.exists() {
    return Err(format!("path does not exist: {}", root.display()));
  }

  let mut summary = ScanSummary::default();
  for entry in WalkBuilder::new(root).standard_filters(true).build() {
    let entry = entry.map_err(|error| error.to_string())?;
    let path = entry.path();
    if entry.file_type().is_some_and(|kind| kind.is_dir())
      || path.extension().and_then(|extension| extension.to_str()) != Some("vue")
    {
      continue;
    }

    let source = fs::read_to_string(path)
      .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let diagnostics = analyze_sfc(path, &source)
      .map_err(|error| format!("failed to analyze {}: {error}", path.display()))?;
    summary.files_scanned += 1;
    summary.diagnostics.extend(diagnostics);
  }

  Ok(summary.finish())
}

#[expect(clippy::print_stdout, reason = "a CLI must emit requested reports on stdout")]
fn print_summary(summary: &ScanSummary, format: OutputFormat) {
  match format {
    OutputFormat::Json => {
      println!("{}", serde_json::to_string_pretty(summary).expect("ScanSummary is serializable"));
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
}
