use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  io::{IsTerminal, Write},
  path::{Path, PathBuf},
  process::ExitCode,
  sync::{Arc, Mutex},
};

use clap::{Args, Parser, ValueEnum};
use vue_vet_cache::{Baseline, filter_diff, read_git_diff};
use vue_vet_core::{ReactiveBindingKind, ReactiveDependencyKind, ScanSummary, TrackingScopeKind};
use vue_vet_project::{EdgeKind, ProjectGraph};
use vue_vet_reactivity::ModuleReactivity;
use vue_vet_reporters::{
  ComponentNavDigest, ComponentNavEdgeInput, ReactivityDigest, ReactivityModuleStats,
  ReactivitySpanRef, ReportContext, ReportFormat, ReportFramework, ReportMode, ScopeExplain,
  binding_detail, component_nav_from_edges, edge_detail, render, render_error,
  render_finding_explain_json, render_finding_explain_text, render_reactivity_detail,
  render_rule_explain_json, render_rule_explain_text, render_scope_explain_json,
  render_scope_explain_text, render_text_diagnostics, render_text_score_footer,
  scope_detail_with_uncertain, scope_label_with_uncertain, template_read_detail,
  to_span_from_identity,
};
use vue_vet_session::{
  AnalysisSnapshot, Explained, ProgressEvent, ProgressReporter, ProjectSession, SessionOptions,
  scan_directory,
};

mod fixes;
mod reactivity_tui;

use fixes::{FixMode, FixOutcome, execute_safe_edits};
use reactivity_tui::run_reactivity_tui;

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
    help = "Stream stages on stderr and per-file analyzed lines; text also streams findings as each file finishes: auto (TTY stderr and not CI), always, or never"
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
    help = "Run the MCP server on stdio (scan / explain / safe-fix preview) and exit when the client closes"
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
    help = "Scan and explain tracking scope deps (binding, file:binding, @offset), then exit"
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

struct StreamState {
  /// Files whose rule diagnostics were already printed (text stream).
  streamed_files: Mutex<BTreeSet<String>>,
}

#[expect(
  clippy::print_stderr,
  clippy::print_stdout,
  reason = "progress on stderr; text findings stream on stdout by design"
)]
fn progress_reporter(
  stderr_stages: bool,
  stream_text: bool,
  color: bool,
  stream_state: Arc<StreamState>,
) -> ProgressReporter {
  ProgressReporter::new(move |event: &ProgressEvent| match event {
    ProgressEvent::FileRules { path, done, total, diagnostics } => {
      if stderr_stages {
        eprintln!("vue-vet: analyzed {path} ({done}/{total})");
      }
      if stream_text {
        if let Ok(mut files) = stream_state.streamed_files.lock() {
          files.insert(path.clone());
        }
        let chunk = render_text_diagnostics(diagnostics, color);
        if !chunk.is_empty() {
          print!("{chunk}");
          #[expect(
            clippy::let_underscore_must_use,
            reason = "best-effort flush while streaming text findings"
          )]
          let _ = std::io::stdout().flush();
        }
      }
    }
    other if stderr_stages => {
      eprintln!("vue-vet: {}", other.message());
    }
    _ => {}
  })
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
  let (session, stream_state, text_streamed) = match open_session(&cli) {
    Ok(opened) => opened,
    Err(error) => return operational_failure(&cli, &error),
  };
  if cli.print_config {
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
      if cli.cache.cache_stats {
        eprintln!("vue-vet cache: {}", snapshot.cache_status);
      }
      if let Err(error) = run_requested_fixes(&cli, &session, &mut snapshot) {
        return operational_failure(&cli, &error);
      }
      if let Some(path) = &cli.baseline {
        let baseline = match Baseline::read(path) {
          Ok(baseline) => baseline,
          Err(error) => return operational_failure(&cli, &error.to_string()),
        };
        snapshot.summary = Arc::new(baseline.filter(Arc::unwrap_or_clone(snapshot.summary)));
      }
      if let Some(reference) = &cli.diff {
        let directory = session.workspace_root();
        let changed = match read_git_diff(directory, reference) {
          Ok(changed) => changed,
          Err(error) => return operational_failure(&cli, &error.to_string()),
        };
        snapshot.summary = Arc::new(filter_diff(Arc::unwrap_or_clone(snapshot.summary), &changed));
      }
      if let Some(path) = &cli.write_baseline
        && let Err(error) = Baseline::from_summary(&snapshot.summary).write(path)
      {
        return operational_failure(&cli, &error.to_string());
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
      if progress_enabled(cli.progress) {
        eprintln!("vue-vet: {}", ProgressEvent::WritingReport.message());
      }
      let streamed_files = stream_state
        .as_ref()
        .and_then(|state| state.streamed_files.lock().ok().map(|files| files.clone()))
        .unwrap_or_default();
      if let Err(error) = print_summary(
        &snapshot.summary,
        cli.format,
        &report_context,
        text_streamed,
        &streamed_files,
      ) {
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
    Err(error) => operational_failure(&cli, &error.to_string()),
  }
}

fn open_session(cli: &Cli) -> Result<(ProjectSession, Option<Arc<StreamState>>, bool), String> {
  let session = ProjectSession::open(SessionOptions {
    root: cli.path.clone(),
    config_path: cli.config.clone(),
    cache_dir: cli.cache.cache_dir.clone(),
    no_cache: cli.cache.no_cache || cli.fix.mode().is_some(),
    threads: cli.threads,
  })
  .map_err(|error| error.to_string())?;
  let stderr_stages = progress_enabled(cli.progress);
  // Stream text findings as each file finishes unless baseline/diff would hide them.
  let stream_text =
    matches!(cli.format, OutputFormat::Text) && cli.baseline.is_none() && cli.diff.is_none();
  if !stderr_stages && !stream_text {
    return Ok((session, None, false));
  }
  let stream_state = Arc::new(StreamState { streamed_files: Mutex::new(BTreeSet::new()) });
  let session = session.with_progress(progress_reporter(
    stderr_stages,
    stream_text,
    color_enabled(cli.color),
    Arc::clone(&stream_state),
  ));
  Ok((session, Some(stream_state), stream_text))
}

fn run_requested_fixes(
  cli: &Cli,
  session: &ProjectSession,
  snapshot: &mut AnalysisSnapshot,
) -> Result<(), String> {
  let Some(mode) = cli.fix.mode() else {
    return Ok(());
  };
  let edits = snapshot
    .summary
    .diagnostics
    .iter()
    .flat_map(|diagnostic| diagnostic.edits.iter().cloned())
    .collect::<Vec<_>>();
  let outcome = execute_safe_edits(&cli.path, edits, mode).map_err(|error| error.to_string())?;
  print_fix_outcome(mode, outcome);
  if mode == FixMode::Apply && outcome.changed() {
    *snapshot = session.analyze_fresh().map_err(|error| error.to_string())?;
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

fn report_context(cli: &Cli, snapshot: &AnalysisSnapshot) -> ReportContext {
  let mut skipped_check_reasons = BTreeMap::new();
  if let Some(error) = &snapshot.graph.reactivity_error {
    skipped_check_reasons.insert("module_reactivity".into(), error.clone());
  }
  for (index, issue) in snapshot.issues.iter().enumerate() {
    skipped_check_reasons
      .entry(format!("analysis_{index}"))
      .or_insert_with(|| issue.message.clone());
  }
  let module_stats = reactivity_module_stats(&snapshot.graph.module_reactivity);
  let mut digest =
    ReactivityDigest::from_modules(&module_stats, snapshot.graph.reactivity_error.clone());
  if cli.print_reactivity || cli.reactivity_tui {
    digest = digest.with_modules_detail(&module_stats);
  }
  // Always expose structural component nav in JSON; cheap and editor-facing.
  let component_nav = Some(component_nav_digest(&snapshot.graph));
  ReportContext {
    mode: report_mode(cli),
    framework: report_framework(&cli.path),
    project_root: report_root(&cli.path),
    analyzed_files: snapshot.analyzed_files.as_ref().to_vec(),
    complete: snapshot.complete(),
    skipped_check_reasons,
    reactivity: Some(digest),
    component_nav,
    color: color_enabled(cli.color),
  }
}

fn component_nav_digest(graph: &ProjectGraph) -> ComponentNavDigest {
  let edges = graph.edges.iter().filter_map(|edge| {
    let kind = match edge.kind {
      EdgeKind::ComponentUsage => "component_usage",
      EdgeKind::AutoComponent => "auto_component",
      _ => return None,
    };
    Some(ComponentNavEdgeInput {
      from: edge.from.clone(),
      to: edge.to.clone(),
      kind: kind.into(),
      specifier: edge.specifier.clone(),
      span: ReactivitySpanRef::new(edge.evidence.offset, edge.evidence.length.max(1)),
    })
  });
  component_nav_from_edges(edges)
}

fn reactivity_module_stats(modules: &[ModuleReactivity]) -> Vec<ReactivityModuleStats> {
  let mut stats = modules
    .iter()
    .map(|module| {
      let binding_len_by_name = module
        .graph
        .bindings
        .iter()
        .map(|binding| (binding.name.as_str(), binding.span.length.max(binding.name.len())))
        .collect::<BTreeMap<_, _>>();
      let mut binding_details = module
        .graph
        .bindings
        .iter()
        .map(|binding| {
          binding_detail(
            binding.name.clone(),
            binding_kind_label(binding.kind),
            ReactivitySpanRef::new(binding.span.offset, binding.span.length.max(1)),
          )
        })
        .collect::<Vec<_>>();
      binding_details.sort_by(|left, right| {
        (left.name.as_str(), left.kind.as_str()).cmp(&(right.name.as_str(), right.kind.as_str()))
      });
      let binding_labels =
        binding_details.iter().map(|detail| format!("{}:{}", detail.name, detail.kind)).collect();

      let mut scope_details = module
        .graph
        .scopes
        .iter()
        .map(|scope| {
          let mut uncertain = scope.uncertain_accesses.clone();
          uncertain.sort();
          uncertain.dedup();
          scope_detail_with_uncertain(
            scope_kind_label(scope.kind),
            scope.callee.clone(),
            scope.binding.clone(),
            ReactivitySpanRef::new(scope.span.offset, scope.span.length.max(1)),
            uncertain,
          )
        })
        .collect::<Vec<_>>();
      scope_details.sort_by(|left, right| {
        (left.kind.as_str(), left.callee.as_str(), left.binding.as_deref()).cmp(&(
          right.kind.as_str(),
          right.callee.as_str(),
          right.binding.as_deref(),
        ))
      });
      let scope_labels = scope_details.iter().map(scope_label_with_uncertain).collect();

      let mut edge_details = module
        .graph
        .edges
        .iter()
        .map(|edge| {
          let to_span = to_span_from_identity(edge.to_id.as_deref(), |name| {
            binding_len_by_name.get(name).copied()
          });
          edge_detail(
            edge.from.clone(),
            edge.to.clone(),
            edge.to_id.clone(),
            edge.property.clone(),
            dependency_kind_label(edge.kind),
            ReactivitySpanRef::new(edge.span.offset, edge.span.length.max(1)),
            to_span,
          )
        })
        .collect::<Vec<_>>();
      edge_details.sort_by(|left, right| {
        (left.from.as_str(), left.to.as_str(), left.property.as_deref(), left.span.offset).cmp(&(
          right.from.as_str(),
          right.to.as_str(),
          right.property.as_deref(),
          right.span.offset,
        ))
      });
      let edge_labels = edge_details
        .iter()
        .map(|detail| format!("{} -> {}", detail.from, detail.to_path))
        .collect();

      let mut template_details = module
        .graph
        .template_reads
        .iter()
        .map(|read| {
          template_read_detail(
            read.binding.clone(),
            read.surface.clone(),
            ReactivitySpanRef::new(read.span.offset, read.span.length.max(1)),
          )
        })
        .collect::<Vec<_>>();
      template_details.sort_by(|left, right| {
        (left.binding.as_str(), left.surface.as_str(), left.span.offset).cmp(&(
          right.binding.as_str(),
          right.surface.as_str(),
          right.span.offset,
        ))
      });
      let template_labels = template_details
        .iter()
        .map(|detail| format!("{}@{}", detail.binding, detail.surface))
        .collect();

      ReactivityModuleStats {
        id: module.id.to_string(),
        bindings: module.graph.bindings.len(),
        scopes: module.graph.scopes.len(),
        edges: module.graph.edges.len(),
        template_reads: module.graph.template_reads.len(),
        binding_labels,
        scope_labels,
        edge_labels,
        template_labels,
        binding_details,
        scope_details,
        edge_details,
        template_details,
      }
    })
    .collect::<Vec<_>>();
  stats.sort_by(|left, right| left.id.cmp(&right.id));
  stats
}

const fn dependency_kind_label(kind: ReactiveDependencyKind) -> &'static str {
  match kind {
    ReactiveDependencyKind::Computed => "computed",
    ReactiveDependencyKind::Effect => "effect",
    ReactiveDependencyKind::Template => "template",
    ReactiveDependencyKind::Prop => "prop",
  }
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
    TrackingScopeKind::Render => "render",
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

#[expect(clippy::print_stderr, reason = "cache stats for finding explain belong on stderr")]
fn run_explain(cli: &Cli, target: &str) -> ExitCode {
  let (session, _, _) = match open_session(cli) {
    Ok(opened) => opened,
    Err(error) => return operational_failure(cli, &error),
  };
  let explained = match session.explain(target) {
    Ok(explained) => explained,
    Err(error) => return operational_failure(cli, &error.to_string()),
  };
  if cli.cache.cache_stats
    && let Explained::Finding { cache_status, .. } = &explained
  {
    eprintln!("vue-vet cache: {cache_status}");
  }
  let output = match (&cli.format, explained) {
    (OutputFormat::Text, Explained::Rule(explain)) => Ok(render_rule_explain_text(&explain)),
    (OutputFormat::Json, Explained::Rule(explain)) => render_rule_explain_json(&explain),
    (OutputFormat::Text, Explained::Finding { explain, .. }) => {
      Ok(render_finding_explain_text(&explain))
    }
    (OutputFormat::Json, Explained::Finding { explain, .. }) => {
      render_finding_explain_json(&explain)
    }
    (OutputFormat::Sarif | OutputFormat::Github, _) => {
      return operational_failure(cli, "--explain supports --format text or json only");
    }
  };
  print_explain(cli, output)
}

#[expect(clippy::print_stderr, reason = "cache stats for scope explain belong on stderr")]
fn run_explain_scope(cli: &Cli, query: &str) -> ExitCode {
  let (session, _, _) = match open_session(cli) {
    Ok(opened) => opened,
    Err(error) => return operational_failure(cli, &error),
  };
  let (explains, cache_status) = match session.explain_scope(query) {
    Ok(result) => result,
    Err(error) => return operational_failure(cli, &error.to_string()),
  };
  if cli.cache.cache_stats {
    eprintln!("vue-vet cache: {cache_status}");
  }
  let output = match cli.format {
    OutputFormat::Text => Ok(render_scope_explains_text(&explains)),
    OutputFormat::Json => render_scope_explains_json(&explains),
    OutputFormat::Sarif | OutputFormat::Github => {
      return operational_failure(cli, "--explain-scope supports --format text or json only");
    }
  };
  print_explain(cli, output)
}

fn render_scope_explains_text(explains: &[ScopeExplain]) -> String {
  let mut output = String::new();
  for (index, explain) in explains.iter().enumerate() {
    if index > 0 {
      output.push('\n');
    }
    output.push_str(&render_scope_explain_text(explain));
  }
  output
}

fn render_scope_explains_json(explains: &[ScopeExplain]) -> Result<String, serde_json::Error> {
  match explains {
    [single] => render_scope_explain_json(single),
    _ => serde_json::to_string_pretty(explains),
  }
}

#[expect(clippy::print_stdout, reason = "explain is an early-exit CLI surface")]
fn print_explain(cli: &Cli, output: Result<String, serde_json::Error>) -> ExitCode {
  match output {
    Ok(rendered) => {
      print!("{rendered}");
      ExitCode::SUCCESS
    }
    Err(error) => operational_failure(cli, &format!("failed to serialize explain output: {error}")),
  }
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
      component_nav: None,
      color: false,
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
  text_streamed: bool,
  streamed_files: &BTreeSet<String>,
) -> Result<(), serde_json::Error> {
  if text_streamed && matches!(format, OutputFormat::Text) {
    let remaining: Vec<_> = summary
      .diagnostics
      .iter()
      .filter(|diagnostic| !streamed_files.contains(diagnostic.file.as_str()))
      .cloned()
      .collect();
    let leftover = render_text_diagnostics(&remaining, context.color);
    if !leftover.is_empty() {
      print!("{leftover}");
    }
    if !streamed_files.is_empty() || !leftover.is_empty() {
      println!();
    }
    println!("{}", render_text_score_footer(summary, context));
    return Ok(());
  }
  let output = render(summary, format.into(), context)?;
  println!("{output}");
  Ok(())
}
