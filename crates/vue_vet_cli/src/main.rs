//! `vue-vet` CLI binary — thin clap front-end over `vue_vet_session`.
//!
//! Owns flags, stdout/stderr policy, exit codes, fix apply, and `--lsp` /
//! `--mcp` handoff. Do not grow analysis logic here.

use std::{io::IsTerminal, path::PathBuf, process::ExitCode, sync::Arc};

use clap::{Args, Parser, ValueEnum};
use vue_vet_cache::{Baseline, filter_diff, read_git_diff};
use vue_vet_reporters::{ReportFormat, render_reactivity_detail};
use vue_vet_session::{AnalysisSnapshot, ProgressEvent, ProjectSession, SessionOptions};

mod explain;
mod fixes;
mod progress;
mod reactivity_tui;
mod report;

use explain::{run_explain, run_explain_scope};
use fixes::{FixMode, FixOutcome, execute_safe_edits};
use progress::{ProgressController, detect_style};
use reactivity_tui::run_reactivity_tui;
use report::{
  component_nav_digest, operational_failure, print_summary, reactivity_module_stats, report_context,
};

#[derive(Debug, Parser)]
#[command(name = "vue-vet", version, about = "Vet your Vue codebase")]
#[expect(clippy::struct_excessive_bools, reason = "clap maps independent CLI flags to bool fields")]
struct Cli {
  #[arg(default_value = ".")]
  path: PathBuf,

  #[arg(long, value_enum, default_value = "text")]
  format: OutputFormat,

  #[arg(
    long,
    value_enum,
    default_value = "auto",
    value_name = "WHEN",
    help = "When to color text reports: auto (TTY; honors NO_COLOR / FORCE_COLOR), always, or never"
  )]
  color: ColorWhen,

  #[arg(
    long,
    value_enum,
    default_value = "auto",
    value_name = "WHEN",
    help = "Scan progress on stderr: auto (live TTY stderr and not CI), always (TTY status or compact logs), or never"
  )]
  progress: ProgressWhen,

  #[arg(long, help = "Return exit code 1 for warnings as well as errors")]
  deny_warnings: bool,

  #[arg(long, value_name = "FILE", help = "Use an explicit vue-vet.toml")]
  config: Option<PathBuf>,

  #[arg(long, help = "Print the effective configuration as JSON and exit")]
  print_config: bool,

  #[arg(
    long,
    conflicts_with = "mcp",
    help = "Run the language server on stdio and exit when the client shuts down"
  )]
  lsp: bool,

  #[arg(
    long,
    conflicts_with = "lsp",
    help = "Run the MCP server on stdio (scan / explain / explain-scope / safe-fix preview) and exit when the client closes"
  )]
  mcp: bool,

  #[arg(
    long,
    value_name = "RULE_OR_FINDING",
    conflicts_with = "explain_scope",
    help = "Print rule docs, or scan and explain a finding id, then exit"
  )]
  explain: Option<String>,

  #[arg(
    long,
    value_name = "QUERY",
    conflicts_with = "explain",
    help = "Scan and explain tracking scope deps (binding, file:binding, @offset start-or-covering), then exit"
  )]
  explain_scope: Option<String>,

  #[arg(long, help = "Print the deterministic project graph as JSON and exit")]
  print_graph: bool,

  #[arg(long, help = "Print a per-module reactivity tracer breakdown after the normal report")]
  print_reactivity: bool,

  #[arg(
    long,
    help = "Browse per-module reactivity facts in an interactive TUI after the normal report"
  )]
  reactivity_tui: bool,

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
      "lsp",
      "mcp",
      "explain",
      "explain_scope",
      "print_graph",
      "print_reactivity",
      "reactivity_tui"
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
      "lsp",
      "mcp",
      "explain",
      "explain_scope",
      "print_graph",
      "print_reactivity",
      "reactivity_tui"
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

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ColorWhen {
  Auto,
  Always,
  Never,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ProgressWhen {
  Auto,
  Always,
  Never,
}

fn color_enabled(when: ColorWhen) -> bool {
  match when {
    ColorWhen::Always => true,
    ColorWhen::Never => false,
    ColorWhen::Auto => {
      if std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
        return false;
      }
      if std::env::var_os("FORCE_COLOR").is_some_and(|value| !value.is_empty())
        || std::env::var_os("CLICOLOR_FORCE").is_some_and(|value| !value.is_empty())
      {
        return true;
      }
      std::io::stdout().is_terminal()
    }
  }
}

fn progress_enabled(when: ProgressWhen) -> bool {
  match when {
    ProgressWhen::Always => true,
    ProgressWhen::Never => false,
    ProgressWhen::Auto => {
      if std::env::var_os("CI").is_some_and(|value| !value.is_empty()) {
        return false;
      }
      std::io::stderr().is_terminal()
    }
  }
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
  if cli.lsp {
    return match vue_vet_lsp::run_stdio() {
      Ok(()) => ExitCode::SUCCESS,
      Err(error) => {
        eprintln!("vue-vet: failed to start language server: {error}");
        ExitCode::from(2)
      }
    };
  }
  if cli.mcp {
    return match vue_vet_mcp::run_stdio(cli.path) {
      Ok(()) => ExitCode::SUCCESS,
      Err(error) => {
        eprintln!("vue-vet: failed to start MCP server: {error}");
        ExitCode::from(2)
      }
    };
  }
  if let Some(target) = cli.explain.as_deref() {
    return run_explain(&cli, target);
  }
  if let Some(query) = cli.explain_scope.as_deref() {
    return run_explain_scope(&cli, query);
  }
  let (session, mut progress) = match open_session(&cli) {
    Ok(opened) => opened,
    Err(error) => return operational_failure(&cli, &error),
  };
  if cli.print_config {
    progress.stop();
    return match serde_json::to_string_pretty(session.config()) {
      Ok(output) => {
        println!("{output}");
        ExitCode::SUCCESS
      }
      Err(error) => {
        operational_failure(&cli, &format!("failed to serialize effective config: {error}"))
      }
    };
  }
  match session.analyze() {
    Ok(mut snapshot) => {
      let fix_outcome = match run_requested_fixes(&cli, &session, &mut snapshot) {
        Ok(outcome) => outcome,
        Err(error) => return fail_with_progress(&cli, &mut progress, &error),
      };
      if let Some(path) = &cli.baseline {
        let baseline = match Baseline::read(path) {
          Ok(baseline) => baseline,
          Err(error) => {
            return fail_with_progress(&cli, &mut progress, &error.to_string());
          }
        };
        snapshot.summary = Arc::new(baseline.filter(Arc::unwrap_or_clone(snapshot.summary)));
      }
      if let Some(reference) = &cli.diff {
        let directory = session.workspace_root();
        let changed = match read_git_diff(directory, reference) {
          Ok(changed) => changed,
          Err(error) => {
            return fail_with_progress(&cli, &mut progress, &error.to_string());
          }
        };
        snapshot.summary = Arc::new(filter_diff(Arc::unwrap_or_clone(snapshot.summary), &changed));
      }
      if let Some(path) = &cli.write_baseline
        && let Err(error) = Baseline::from_summary(&snapshot.summary).write(path)
      {
        return fail_with_progress(&cli, &mut progress, &error.to_string());
      }
      progress.emit(&ProgressEvent::WritingReport);
      progress.stop();
      if cli.cache.cache_stats {
        eprintln!("vue-vet cache: {}", snapshot.cache_status);
      }
      if let Some((mode, outcome)) = fix_outcome {
        print_fix_outcome(mode, outcome);
      }
      if cli.print_graph {
        return match serde_json::to_string_pretty(&snapshot.graph) {
          Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
          }
          Err(error) => {
            operational_failure(&cli, &format!("failed to serialize project graph: {error}"))
          }
        };
      }
      if cli.reactivity_tui && !matches!(cli.format, OutputFormat::Text) {
        return operational_failure(&cli, "--reactivity-tui requires --format text");
      }
      let report_context = report_context(&cli, &snapshot);
      if let Err(error) = print_summary(&snapshot.summary, cli.format, &report_context) {
        return operational_failure(&cli, &format!("failed to serialize report: {error}"));
      }
      if cli.print_reactivity
        && !cli.reactivity_tui
        && matches!(cli.format, OutputFormat::Text)
        && let Some(digest) = &report_context.reactivity
      {
        print!("{}", render_reactivity_detail(digest));
      }
      if cli.reactivity_tui {
        let module_stats = reactivity_module_stats(&snapshot.graph.module_reactivity);
        let component_nav = component_nav_digest(&snapshot.graph);
        if let Err(error) =
          run_reactivity_tui(&module_stats, snapshot.graph.reactivity_error.clone(), &component_nav)
        {
          return operational_failure(&cli, &error);
        }
      }
      if snapshot.summary.fails(cli.deny_warnings) { ExitCode::from(1) } else { ExitCode::SUCCESS }
    }
    Err(error) => fail_with_progress(&cli, &mut progress, &error.to_string()),
  }
}

pub(crate) fn open_session(cli: &Cli) -> Result<(ProjectSession, ProgressController), String> {
  let session = ProjectSession::open(SessionOptions {
    root: cli.path.clone(),
    config_path: cli.config.clone(),
    cache_dir: cli.cache.cache_dir.clone(),
    no_cache: cli.cache.no_cache || cli.fix.mode().is_some(),
    threads: cli.threads,
  })
  .map_err(|error| error.to_string())?;
  let progress = ProgressController::start(detect_style(progress_enabled(cli.progress)));
  let session = match progress.reporter() {
    Some(reporter) => session.with_progress(reporter),
    None => session,
  };
  Ok((session, progress))
}

fn fail_with_progress(cli: &Cli, progress: &mut ProgressController, message: &str) -> ExitCode {
  progress.stop();
  operational_failure(cli, message)
}

fn run_requested_fixes(
  cli: &Cli,
  session: &ProjectSession,
  snapshot: &mut AnalysisSnapshot,
) -> Result<Option<(FixMode, FixOutcome)>, String> {
  let Some(mode) = cli.fix.mode() else {
    return Ok(None);
  };
  let edits = snapshot
    .summary
    .diagnostics
    .iter()
    .flat_map(|diagnostic| diagnostic.edits.iter().cloned())
    .collect::<Vec<_>>();
  let outcome = execute_safe_edits(&cli.path, edits, mode).map_err(|error| error.to_string())?;
  if mode == FixMode::Apply && outcome.changed() {
    *snapshot = session.analyze_fresh().map_err(|error| error.to_string())?;
  }
  Ok(Some((mode, outcome)))
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
