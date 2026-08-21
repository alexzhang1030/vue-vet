//! Report context, reactivity digest, and stdout summary.
use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  path::Path,
  process::ExitCode,
};

use vue_vet_core::{ReactiveBindingKind, ReactiveDependencyKind, ScanSummary, TrackingScopeKind};
use vue_vet_project::{EdgeKind, ProjectGraph};
use vue_vet_reactivity::{ModuleReactivity, explain_tracking_scope};
use vue_vet_reporters::{
  ComponentNavDigest, ComponentNavEdgeInput, ReactivityDigest, ReactivityModuleStats,
  ReactivitySpanRef, ReportContext, ReportFramework, ReportMode, binding_detail,
  component_nav_from_edges, edge_detail, render, render_error, render_text_diagnostics,
  render_text_score_footer, scope_detail_with_uncertain, scope_label_with_uncertain,
  template_read_detail, to_span_from_identity,
};
use vue_vet_session::{AnalysisSnapshot, scan_directory};

use crate::{Cli, OutputFormat, color_enabled};

pub fn report_context(cli: &Cli, snapshot: &AnalysisSnapshot) -> ReportContext {
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

pub fn component_nav_digest(graph: &ProjectGraph) -> ComponentNavDigest {
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

pub fn reactivity_module_stats(modules: &[ModuleReactivity]) -> Vec<ReactivityModuleStats> {
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
          let mut detail = scope_detail_with_uncertain(
            scope_kind_label(scope.kind),
            scope.callee.clone(),
            scope.binding.clone(),
            ReactivitySpanRef::new(scope.span.offset, scope.span.length.max(1)),
            uncertain,
          );
          detail.summary = Some(explain_tracking_scope(module.id.as_str(), scope).summary);
          detail
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

pub const fn report_mode(cli: &Cli) -> ReportMode {
  if cli.diff.is_some() {
    ReportMode::Diff
  } else if cli.baseline.is_some() {
    ReportMode::Baseline
  } else {
    ReportMode::Full
  }
}

pub fn report_root(path: &Path) -> String {
  let root = scan_directory(path).to_string_lossy().replace('\\', "/");
  if root.is_empty() { ".".into() } else { root }
}

pub fn report_framework(root: &Path) -> ReportFramework {
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

#[expect(
  clippy::print_stderr,
  clippy::print_stdout,
  reason = "a CLI must emit structured failures or human-readable operational errors"
)]
pub fn operational_failure(cli: &Cli, message: &str) -> ExitCode {
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
pub fn print_summary(
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
