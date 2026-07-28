mod conventions;
mod resolve;

use std::{
  collections::{BTreeMap, BTreeSet},
  path::Path,
  sync::Arc,
};

use serde::{Deserialize, Serialize};
use vue_vet_core::{
  Confidence, Diagnostic, FileId, ModuleId, ScriptFacts, Severity, SfcFacts, SourceSpan,
};
use vue_vet_reactivity::{
  ModuleLink, ModuleReactivity, ModuleSource, ModuleTraceState, PropFlowSite, TraceModulesOptions,
  join_prop_flows, trace_modules_incremental_with_options,
};

pub use resolve::{OXC_RESOLVER_VERSION, normalize_project_root, resolver_config_inputs};

use conventions::{
  NUXT_COMPONENT_DTS_CANDIDATES, convention_component_name, load_nuxt_component_dts_names,
  parse_nuxt_components_dts, strip_lazy_component_prefix,
};
use resolve::{ProjectResolver, Resolution, normalized_path};

pub const CONVENTIONS_VERSION: u32 = 4;
pub const PROJECT_RULE_IDS: [&str; 2] =
  ["vue-vet/project/unresolved-import", "vue-vet/project/unused-component"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectFile {
  pub path: FileId,
  pub source_len: usize,
  pub facts: Arc<SfcFacts>,
  pub module_source: Option<ModuleSource>,
  /// Ordinary `<script>` companion when dual-script SFCs also have setup
  /// (`id` ends with `#script`).
  pub ordinary_module_source: Option<ModuleSource>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
  VueFile,
  Module,
  Component,
  Composable,
  Page,
  Layout,
  Plugin,
  Middleware,
  Store,
  External,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
  Import,
  ExternalImport,
  ComponentUsage,
  AutoComponent,
  AutoComposable,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct GraphNode {
  pub id: String,
  pub kind: NodeKind,
  pub path: String,
  pub name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct GraphEdge {
  pub id: String,
  pub from: String,
  pub to: String,
  pub kind: EdgeKind,
  pub specifier: String,
  pub evidence: SourceSpan,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectGraph {
  pub conventions_version: u32,
  pub nodes: Vec<GraphNode>,
  pub edges: Vec<GraphEdge>,
  pub diagnostics: Vec<Diagnostic>,
  pub invalidation_inputs: Vec<String>,
  pub module_reactivity: Vec<ModuleReactivity>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub reactivity_issues: Vec<ReactivityIssue>,
  /// Compatibility summary for reporters that have not adopted structured issues.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reactivity_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReactivityIssue {
  pub module: Option<ModuleId>,
  pub message: String,
}

/// Reusable project-linking state retained by a long-lived session.
#[derive(Clone, Debug, Default)]
pub struct ProjectGraphState {
  module_trace: ModuleTraceState,
  structural: StructuralProjectState,
  last_stats: ProjectGraphStats,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectGraphStats {
  pub structural_files_rebuilt: usize,
  pub structural_files_reused: usize,
  pub module_graphs_reused: usize,
  pub seeded_module_reparses: usize,
}

/// Why a resolver-context epoch advanced — drives typed incremental invalidation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextChangeKind {
  PackageManifest,
  Lockfile,
  TsConfig,
  NuxtDeclarations,
  SourceMembership,
}

/// Independent epochs so debounced / batched mutations cannot drop a prior kind.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContextEpochs {
  pub package_manifest: u64,
  pub lockfile: u64,
  pub tsconfig: u64,
  pub nuxt_declarations: u64,
  pub source_membership: u64,
}

impl ContextEpochs {
  /// Advance the epoch for `kind`.
  pub const fn bump(&mut self, kind: ContextChangeKind) {
    match kind {
      ContextChangeKind::PackageManifest => {
        self.package_manifest = self.package_manifest.wrapping_add(1);
      }
      ContextChangeKind::Lockfile => {
        self.lockfile = self.lockfile.wrapping_add(1);
      }
      ContextChangeKind::TsConfig => {
        self.tsconfig = self.tsconfig.wrapping_add(1);
      }
      ContextChangeKind::NuxtDeclarations => {
        self.nuxt_declarations = self.nuxt_declarations.wrapping_add(1);
      }
      ContextChangeKind::SourceMembership => {
        self.source_membership = self.source_membership.wrapping_add(1);
      }
    }
  }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectContext {
  pub revision: u64,
  pub nuxt_component_names: BTreeMap<String, String>,
  pub invalidation_inputs: Vec<String>,
  /// Per-kind epochs consumed by long-lived incremental analysis.
  pub epochs: ContextEpochs,
}

impl ProjectContext {
  #[must_use]
  pub fn from_filesystem(root: &Path, known: &BTreeSet<String>) -> Self {
    let root = normalize_project_root(root);
    Self {
      revision: 0,
      nuxt_component_names: load_nuxt_component_dts_names(&root, known),
      invalidation_inputs: resolver_config_inputs(&root),
      epochs: ContextEpochs::default(),
    }
  }
}

/// Build project context from the already-read workspace input snapshot.
#[must_use]
pub fn project_context_from_inputs<'a>(
  root: &Path,
  known_files: impl IntoIterator<Item = &'a FileId>,
  inputs: impl IntoIterator<Item = (&'a str, &'a [u8])>,
  revision: u64,
) -> ProjectContext {
  let root = normalize_project_root(root);
  let known =
    known_files.into_iter().map(|file| normalized_path(file.as_path())).collect::<BTreeSet<_>>();
  let mut nuxt_component_names = BTreeMap::new();
  let mut invalidation_inputs = Vec::new();
  for (relative, bytes) in inputs {
    if is_project_invalidation_input(relative) {
      invalidation_inputs.push(relative.to_owned());
    }
    if !NUXT_COMPONENT_DTS_CANDIDATES.contains(&relative) {
      continue;
    }
    let Ok(source) = std::str::from_utf8(bytes) else {
      continue;
    };
    let path = root.join(relative);
    for (name, target) in parse_nuxt_components_dts(&path, source, &root, &known) {
      nuxt_component_names.insert(name, target);
    }
  }
  invalidation_inputs.sort();
  invalidation_inputs.dedup();
  ProjectContext {
    revision,
    nuxt_component_names,
    invalidation_inputs,
    epochs: ContextEpochs::default(),
  }
}

impl ProjectGraphState {
  #[must_use]
  pub const fn last_stats(&self) -> ProjectGraphStats {
    self.last_stats
  }
}

#[derive(Clone, Debug, Default)]
struct StructuralProjectState {
  context: Option<StructuralContextKey>,
  files: BTreeMap<FileId, StructuralFileCache>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StructuralContextKey {
  root: std::path::PathBuf,
  revision: u64,
  nodes: Vec<GraphNode>,
  module_ids: BTreeSet<ModuleId>,
}

#[derive(Clone, Debug)]
struct StructuralFileCache {
  facts: Arc<SfcFacts>,
  output: StructuralFileOutput,
}

#[derive(Clone, Debug, Default)]
struct StructuralFileOutput {
  external_nodes: Vec<GraphNode>,
  edges: Vec<GraphEdge>,
  diagnostics: Vec<Diagnostic>,
  module_links: Vec<ModuleLink>,
}

#[must_use]
pub fn build_project_graph(root: &Path, files: &[ProjectFile]) -> ProjectGraph {
  build_project_graph_with_options(root, files, TraceModulesOptions::default())
}

#[must_use]
pub fn build_project_graph_with_options(
  root: &Path,
  files: &[ProjectFile],
  trace_options: TraceModulesOptions,
) -> ProjectGraph {
  let root = normalize_project_root(root);
  let known =
    files.iter().map(|file| normalized_path(file.path.as_path())).collect::<BTreeSet<_>>();
  let context = ProjectContext::from_filesystem(&root, &known);
  build_project_graph_incremental_with_options(
    &root,
    files,
    trace_options,
    &context,
    &mut ProjectGraphState::default(),
  )
}

#[must_use]
pub fn build_project_graph_incremental_with_options<'a>(
  root: &Path,
  files: impl IntoIterator<Item = &'a ProjectFile>,
  trace_options: TraceModulesOptions,
  project_context: &ProjectContext,
  state: &mut ProjectGraphState,
) -> ProjectGraph {
  state.last_stats = ProjectGraphStats::default();
  let root = normalize_project_root(root);
  let mut ordered = files.into_iter().collect::<Vec<_>>();
  ordered.sort_by_key(|file| normalized_path(file.path.as_path()));
  let known =
    ordered.iter().map(|file| normalized_path(file.path.as_path())).collect::<BTreeSet<_>>();
  let resolver = ProjectResolver::new(&root);
  let mut nodes = ordered.iter().map(|file| file_node(file)).collect::<Vec<_>>();
  let node_by_path =
    nodes.iter().map(|node| (node.path.clone(), node.id.clone())).collect::<BTreeMap<_, _>>();
  let dts_names = &project_context.nuxt_component_names;
  for node in &mut nodes {
    if node.kind != NodeKind::Component {
      continue;
    }
    if let Some(name) = dts_names
      .iter()
      .find_map(|(name, path)| (path == &node.path).then_some(name.clone()))
      .or_else(|| convention_component_name(&node.path))
    {
      node.name = name;
    }
  }
  let mut component_by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
  for node in nodes.iter().filter(|node| node.kind == NodeKind::Component) {
    insert_component_name(&mut component_by_name, &node.name, &node.id);
  }
  for (name, path) in dts_names {
    if let Some(id) = node_by_path.get(path) {
      insert_component_name(&mut component_by_name, name, id);
    }
  }
  let composable_by_name = nodes
    .iter()
    .filter(|node| node.kind == NodeKind::Composable)
    .map(|node| (node.name.clone(), node.id.clone()))
    .collect::<BTreeMap<_, _>>();
  let module_sources = ordered
    .iter()
    .flat_map(|file| {
      let primary = file.module_source.clone().map(|mut module| {
        module.id = ModuleId::primary(&file.path);
        module
      });
      let ordinary = file.ordinary_module_source.clone().map(|mut module| {
        module.id = ModuleId::ordinary(&file.path);
        module
      });
      [primary, ordinary].into_iter().flatten()
    })
    .collect::<Vec<_>>();
  let module_ids = module_sources.iter().map(|module| module.id.clone()).collect::<BTreeSet<_>>();
  let context = StructuralContextKey {
    root,
    revision: project_context.revision,
    nodes: nodes.clone(),
    module_ids: module_ids.clone(),
  };
  if state.structural.context.as_ref() != Some(&context) {
    state.structural.files.clear();
    state.structural.context = Some(context);
  }
  let mut next_file_cache = BTreeMap::new();
  let mut module_links = Vec::new();
  let mut external_nodes = BTreeMap::new();
  let mut edges = Vec::new();
  let mut diagnostics = Vec::new();
  for file in &ordered {
    let output = if let Some(cached) =
      state.structural.files.get(&file.path).filter(|cached| cached.facts == file.facts)
    {
      state.last_stats.structural_files_reused += 1;
      cached.output.clone()
    } else {
      state.last_stats.structural_files_rebuilt += 1;
      analyze_structural_file(
        file,
        &resolver,
        &known,
        &node_by_path,
        &component_by_name,
        &composable_by_name,
        &module_ids,
      )
    };
    for node in &output.external_nodes {
      external_nodes.entry(node.id.clone()).or_insert_with(|| node.clone());
    }
    edges.extend(output.edges.iter().cloned());
    diagnostics.extend(output.diagnostics.iter().cloned());
    module_links.extend(output.module_links.iter().cloned());
    next_file_cache
      .insert(file.path.clone(), StructuralFileCache { facts: Arc::clone(&file.facts), output });
  }
  state.structural.files = next_file_cache;
  nodes.extend(external_nodes.into_values());
  nodes.sort();
  edges.sort();
  edges.dedup();
  diagnostics.extend(unused_component_diagnostics(&ordered, &nodes, &edges));
  diagnostics.sort_by(|left, right| {
    (&left.file, left.span.offset, &left.rule_id).cmp(&(
      &right.file,
      right.span.offset,
      &right.rule_id,
    ))
  });
  let trace_report = trace_modules_incremental_with_options(
    &module_sources,
    &module_links,
    trace_options,
    &mut state.module_trace,
  );
  state.last_stats.module_graphs_reused = trace_report.stats.reused_graphs;
  state.last_stats.seeded_module_reparses = trace_report.stats.seeded_reparses;
  let mut module_reactivity = trace_report.modules;
  let reactivity_issues = trace_report
    .issues
    .into_iter()
    .map(|error| ReactivityIssue { module: error.module_id().cloned(), message: error.to_string() })
    .collect::<Vec<_>>();
  let reactivity_error = (!reactivity_issues.is_empty()).then(|| {
    reactivity_issues.iter().map(|issue| issue.message.as_str()).collect::<Vec<_>>().join("; ")
  });
  // Re-apply SFC template joins onto module graphs so cross-file seeds and
  // template reads share one fact surface. Spans stay SFC-absolute when the
  // module carried `source_offset` + `span_source`.
  let templates = ordered
    .iter()
    .map(|file| (normalized_path(file.path.as_path()), &file.facts.template))
    .collect::<BTreeMap<_, _>>();
  for module in &mut module_reactivity {
    if let Some(template) = templates.get(module.id.as_str()) {
      std::sync::Arc::make_mut(&mut module.graph).join_template_reads(template);
    }
  }
  // Static parent `:prop="binding"` → child `props.prop` edges (under-approx).
  let graph_snapshots = module_reactivity
    .iter()
    .map(|module| (module.id.clone(), std::sync::Arc::clone(&module.graph)))
    .collect::<BTreeMap<_, _>>();
  let prop_sites = edges
    .iter()
    .filter(|edge| matches!(edge.kind, EdgeKind::ComponentUsage | EdgeKind::AutoComponent))
    .filter_map(|edge| {
      // Graph edges use `file:{path}` node ids; module graphs / templates use bare paths.
      let parent_path = edge.from.strip_prefix("file:").unwrap_or(edge.from.as_str());
      let child_path = edge.to.strip_prefix("file:").unwrap_or(edge.to.as_str());
      let parent_template = *templates.get(parent_path)?;
      let parent_graph = graph_snapshots.get(parent_path)?;
      Some(PropFlowSite {
        element_span: edge.evidence.clone(),
        parent_template,
        parent_graph,
        child_module: child_path,
      })
    })
    .collect::<Vec<_>>();
  join_prop_flows(&mut module_reactivity, &prop_sites);
  let mut invalidation_inputs = known.into_iter().collect::<Vec<_>>();
  invalidation_inputs.extend(project_context.invalidation_inputs.iter().cloned());
  invalidation_inputs.sort();
  invalidation_inputs.dedup();
  ProjectGraph {
    conventions_version: CONVENTIONS_VERSION,
    nodes,
    edges,
    diagnostics,
    invalidation_inputs,
    module_reactivity,
    reactivity_issues,
    reactivity_error,
  }
}

fn is_project_invalidation_input(path: &str) -> bool {
  context_change_kind_for(path).is_some()
}

/// Classify a workspace-relative path as a typed resolver-context change.
#[must_use]
pub fn context_change_kind_for(path: &str) -> Option<ContextChangeKind> {
  let name = Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or(path);
  if name == "package.json" {
    return Some(ContextChangeKind::PackageManifest);
  }
  if matches!(path, "package-lock.json" | "pnpm-lock.yaml" | "yarn.lock" | "bun.lock" | "bun.lockb")
  {
    return Some(ContextChangeKind::Lockfile);
  }
  if matches!(path, ".nuxt/components.d.ts" | ".nuxt/types/components.d.ts") {
    return Some(ContextChangeKind::NuxtDeclarations);
  }
  if matches!(
    path,
    "tsconfig.json" | "tsconfig.app.json" | "tsconfig.node.json" | ".nuxt/tsconfig.json"
  ) || (name.starts_with("tsconfig")
    && Path::new(name).extension().is_some_and(|extension| extension.eq_ignore_ascii_case("json")))
  {
    return Some(ContextChangeKind::TsConfig);
  }
  None
}

fn analyze_structural_file(
  file: &ProjectFile,
  resolver: &ProjectResolver,
  known: &BTreeSet<String>,
  node_by_path: &BTreeMap<String, String>,
  component_by_name: &BTreeMap<String, Vec<String>>,
  composable_by_name: &BTreeMap<String, String>,
  module_ids: &BTreeSet<ModuleId>,
) -> StructuralFileOutput {
  let path = normalized_path(file.path.as_path());
  let from = file_id(&path);
  let imports = all_imports(&file.facts.script);
  let mut output = StructuralFileOutput::default();
  for import in &imports {
    match resolver.resolve(&path, &import.source, known) {
      Resolution::File(target) => {
        if let Some(to) = node_by_path.get(&target) {
          output.edges.push(edge(&from, to, EdgeKind::Import, &import.source, import.span.clone()));
        }
        let target_id = ModuleId::primary(&FileId::from(target.as_str()));
        for module_from in [ModuleId::primary(&file.path), ModuleId::ordinary(&file.path)] {
          if module_ids.contains(&module_from) && module_ids.contains(&target_id) {
            output.module_links.push(ModuleLink {
              from: module_from,
              specifier: import.source.clone(),
              to: target_id.clone(),
            });
          }
        }
      }
      Resolution::External(package) => {
        let id = format!("external:{package}");
        output.external_nodes.push(GraphNode {
          id: id.clone(),
          kind: NodeKind::External,
          path: package.clone(),
          name: package,
        });
        output.edges.push(edge(
          &from,
          &id,
          EdgeKind::ExternalImport,
          &import.source,
          import.span.clone(),
        ));
      }
      Resolution::Unresolved => {
        output.diagnostics.push(unresolved_diagnostic(
          file.path.as_path(),
          &import.source,
          import.span.clone(),
        ));
      }
    }
  }

  for element in &file.facts.template.elements {
    let tag = comparable_name(&element.tag);
    if let Some(import) = imports.iter().find(|import| comparable_name(&import.local) == tag) {
      if let Resolution::File(target) = resolver.resolve(&path, &import.source, known)
        && let Some(to) = node_by_path.get(&target)
      {
        output.edges.push(edge(
          &from,
          to,
          EdgeKind::ComponentUsage,
          &element.tag,
          element.span.clone(),
        ));
      }
    } else {
      for to in auto_component_targets(&element.tag, component_by_name) {
        output.edges.push(edge(
          &from,
          &to,
          EdgeKind::AutoComponent,
          &element.tag,
          element.span.clone(),
        ));
      }
    }
  }

  for call in file.facts.script.blocks.iter().flat_map(|block| &block.calls) {
    if let Some(to) = composable_by_name.get(&call.callee) {
      output.edges.push(edge(&from, to, EdgeKind::AutoComposable, &call.callee, call.span.clone()));
    }
  }
  output
}

fn all_imports(script: &ScriptFacts) -> Vec<&vue_vet_core::ScriptImportFact> {
  script.blocks.iter().flat_map(|block| &block.imports).collect()
}

fn file_node(file: &ProjectFile) -> GraphNode {
  let path = normalized_path(file.path.as_path());
  let kind = node_kind(&path);
  let name = if kind == NodeKind::Component {
    convention_component_name(&path).unwrap_or_else(|| file_stem(&path))
  } else {
    file_stem(&path)
  };
  GraphNode { id: file_id(&path), kind, name, path }
}

fn insert_component_name(map: &mut BTreeMap<String, Vec<String>>, name: &str, id: &str) {
  let key = comparable_name(name);
  let entry = map.entry(key).or_default();
  if !entry.iter().any(|existing| existing == id) {
    entry.push(id.to_owned());
  }
}

fn auto_component_targets(tag: &str, map: &BTreeMap<String, Vec<String>>) -> Vec<String> {
  let mut targets = map.get(&comparable_name(tag)).cloned().unwrap_or_default();
  if let Some(base) = strip_lazy_component_prefix(tag)
    && let Some(more) = map.get(&comparable_name(base))
  {
    for id in more {
      if !targets.iter().any(|existing| existing == id) {
        targets.push(id.clone());
      }
    }
  }
  targets
}

fn node_kind(path: &str) -> NodeKind {
  let segments = path.split('/').collect::<Vec<_>>();
  if segments.contains(&"components") {
    NodeKind::Component
  } else if segments.contains(&"composables") {
    NodeKind::Composable
  } else if segments.contains(&"pages") {
    NodeKind::Page
  } else if segments.contains(&"layouts") {
    NodeKind::Layout
  } else if segments.contains(&"plugins") {
    NodeKind::Plugin
  } else if segments.contains(&"middleware") {
    NodeKind::Middleware
  } else if segments.contains(&"stores") {
    NodeKind::Store
  } else if Path::new(path).extension().and_then(|extension| extension.to_str()) == Some("vue") {
    NodeKind::VueFile
  } else {
    NodeKind::Module
  }
}

fn comparable_name(name: &str) -> String {
  name.chars().filter(char::is_ascii_alphanumeric).flat_map(char::to_lowercase).collect()
}

fn file_stem(path: &str) -> String {
  Path::new(path).file_stem().and_then(|name| name.to_str()).unwrap_or(path).into()
}

fn file_id(path: &str) -> String {
  format!("file:{path}")
}

fn edge(from: &str, to: &str, kind: EdgeKind, specifier: &str, evidence: SourceSpan) -> GraphEdge {
  let id = format!("{kind:?}:{from}->{to}@{}", evidence.offset);
  GraphEdge { id, from: from.into(), to: to.into(), kind, specifier: specifier.into(), evidence }
}

fn unresolved_diagnostic(file: &Path, specifier: &str, span: SourceSpan) -> Diagnostic {
  Diagnostic {
    rule_id: PROJECT_RULE_IDS[0].into(),
    category: "project".into(),
    severity: Severity::Error,
    confidence: Some(Confidence::High),
    documentation: Some("project-graph".into()),
    message: format!("cannot resolve project import `{specifier}`"),
    help: Some(
      "Check that the import resolves under Node/Vite rules: a relative path, tsconfig paths / @ or ~ aliases, or an installed package."
        .into(),
    ),
    file: file.into(),
    span,
    edits: Vec::new(),
  recommendation: None,
  }
}

fn unused_component_diagnostics(
  files: &[&ProjectFile],
  nodes: &[GraphNode],
  edges: &[GraphEdge],
) -> Vec<Diagnostic> {
  let referenced = edges
    .iter()
    .filter(|edge| {
      matches!(edge.kind, EdgeKind::Import | EdgeKind::ComponentUsage | EdgeKind::AutoComponent)
    })
    .map(|edge| edge.to.as_str())
    .collect::<std::collections::HashSet<_>>();
  let file_by_path = files
    .iter()
    .map(|file| (normalized_path(file.path.as_path()), *file))
    .collect::<BTreeMap<_, _>>();
  nodes
    .iter()
    .filter(|node| node.kind == NodeKind::Component)
    .filter(|node| !referenced.contains(node.id.as_str()))
    .filter_map(|node| {
      let file = file_by_path.get(&node.path)?;
      Some(Diagnostic {
        rule_id: PROJECT_RULE_IDS[1].into(),
        category: "project".into(),
        severity: Severity::Warning,
        confidence: Some(Confidence::Medium),
        documentation: Some("project-graph".into()),
        message: format!("component `{}` is never referenced", node.name),
        help: Some("Remove it or reference it from a template or script import.".into()),
        file: file.path.clone(),
        span: SourceSpan { offset: 0, length: file.source_len.min(1), line: 1, column: 1 },
        edits: Vec::new(),
        recommendation: None,
      })
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
  };

  use vue_vet_core::{
    ScriptBlockFacts, ScriptCallFact, ScriptImportFact, ScriptKind, TemplateElementFact,
    TemplateFacts,
  };

  static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

  struct TempProject {
    root: PathBuf,
  }

  impl TempProject {
    #[expect(clippy::panic, reason = "test setup failures must fail the unit test")]
    fn new(name: &str) -> Self {
      let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
      let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("vue-vet-project-{name}-{}-{sequence}", std::process::id()));
      let _ignored = fs::remove_dir_all(&root);
      if let Err(error) = fs::create_dir_all(&root) {
        panic!("failed to create temp project {}: {error}", root.display());
      }
      Self { root }
    }

    fn root(&self) -> &Path {
      &self.root
    }

    #[expect(clippy::panic, reason = "test setup failures must fail the unit test")]
    fn write(&self, relative: &str, contents: &str) {
      let path = self.root.join(relative);
      if let Some(parent) = path.parent()
        && let Err(error) = fs::create_dir_all(parent)
      {
        panic!("failed to create {}: {error}", parent.display());
      }
      if let Err(error) = fs::write(&path, contents) {
        panic!("failed to write {}: {error}", path.display());
      }
    }
  }

  impl Drop for TempProject {
    fn drop(&mut self) {
      let _ignored = fs::remove_dir_all(&self.root);
    }
  }

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 1, line: 1, column: offset.saturating_add(1) }
  }

  fn file(path: &str, imports: &[(&str, &str)], tags: &[&str], calls: &[&str]) -> ProjectFile {
    let script = ScriptFacts {
      blocks: vec![ScriptBlockFacts {
        kind: ScriptKind::Setup,
        language: "ts".into(),
        imports: imports
          .iter()
          .enumerate()
          .map(|(index, (source, local))| ScriptImportFact {
            source: (*source).into(),
            imported: "default".into(),
            local: (*local).into(),
            span: span(index),
          })
          .collect(),
        bindings: Vec::new(),
        calls: calls
          .iter()
          .enumerate()
          .map(|(index, callee)| ScriptCallFact {
            callee: (*callee).into(),
            assigned_to: None,
            resolved_import: None,
            argument_identifiers: Vec::new(),
            span: span(index.saturating_add(10)),
          })
          .collect(),
        member_writes: Vec::new(),
        destructures: Vec::new(),
        reactivity_graph: std::sync::Arc::new(vue_vet_core::ReactivityGraph::default()),
      }],
    };
    let template = TemplateFacts {
      elements: tags
        .iter()
        .enumerate()
        .map(|(index, tag)| TemplateElementFact {
          tag: (*tag).into(),
          span: span(index.saturating_add(20)),
          attributes: Vec::new(),
          directives: Vec::new(),
          has_children: false,
          has_accessible_content: false,
          has_labelable_descendant: false,
          has_label_ancestor: false,
        })
        .collect(),
      expressions: Vec::new(),
    };
    ProjectFile {
      path: path.into(),
      source_len: 100,
      facts: SfcFacts { template, script }.into(),
      module_source: None,
      ordinary_module_source: None,
    }
  }

  fn materialize(project: &TempProject, files: &[ProjectFile]) {
    for file in files {
      let relative = normalized_path(file.path.as_path());
      let stub = if Path::new(&relative)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("vue"))
      {
        "<template><div /></template>\n"
      } else {
        "export {}\n"
      };
      project.write(&relative, stub);
    }
  }

  #[test]
  fn graph_is_deterministic_and_preserves_cycles() {
    let project = TempProject::new("cycles");
    let first = file("src/a.ts", &[("./b", "b")], &[], &[]);
    let second = file("src/b.ts", &[("./a", "a")], &[], &[]);
    materialize(&project, &[first.clone(), second.clone()]);
    let forward = build_project_graph(project.root(), &[first.clone(), second.clone()]);
    let reverse = build_project_graph(project.root(), &[second, first]);
    assert_eq!(forward, reverse, "input traversal order must not affect the graph");
    assert_eq!(forward.edges.len(), 2, "both sides of an import cycle must be represented");
  }

  #[test]
  fn incremental_structure_rebuilds_only_changed_file_facts() {
    let project = TempProject::new("incremental-structure");
    let first = file("src/a.vue", &[], &["main"], &[]);
    let second = file("src/b.vue", &[], &["aside"], &[]);
    materialize(&project, &[first.clone(), second.clone()]);
    let mut state = ProjectGraphState::default();
    let context = ProjectContext { revision: 1, ..ProjectContext::default() };
    let _initial = build_project_graph_incremental_with_options(
      project.root(),
      &[first.clone(), second.clone()],
      TraceModulesOptions { max_workers: 1, ..Default::default() },
      &context,
      &mut state,
    );
    assert_eq!(state.last_stats().structural_files_rebuilt, 2);

    let _unchanged = build_project_graph_incremental_with_options(
      project.root(),
      &[first.clone(), second],
      TraceModulesOptions { max_workers: 1, ..Default::default() },
      &context,
      &mut state,
    );
    assert_eq!(state.last_stats().structural_files_reused, 2);
    assert_eq!(state.last_stats().structural_files_rebuilt, 0);

    let changed = file("src/b.vue", &[], &["section"], &[]);
    let _changed = build_project_graph_incremental_with_options(
      project.root(),
      &[first, changed],
      TraceModulesOptions { max_workers: 1, ..Default::default() },
      &context,
      &mut state,
    );
    assert_eq!(state.last_stats().structural_files_reused, 1);
    assert_eq!(state.last_stats().structural_files_rebuilt, 1);
  }

  #[test]
  fn resolves_aliases_and_nuxt_auto_imports() {
    let project = TempProject::new("aliases");
    let page = file(
      "pages/index.vue",
      &[("@/components/AppCard", "Card")],
      &["Card", "AutoButton"],
      &["useAccount"],
    );
    let imported = file("src/components/AppCard.vue", &[], &[], &[]);
    let automatic = file("components/AutoButton.vue", &[], &[], &[]);
    let composable = file("composables/useAccount.ts", &[], &[], &[]);
    materialize(&project, &[page.clone(), imported.clone(), automatic.clone(), composable.clone()]);
    let graph = build_project_graph(project.root(), &[page, imported, automatic, composable]);
    assert!(
      graph.edges.iter().any(|edge| edge.kind == EdgeKind::ComponentUsage),
      "explicit component imports must connect template usage"
    );
    assert!(
      graph.edges.iter().any(|edge| edge.kind == EdgeKind::AutoComponent),
      "Nuxt component directories must create auto-import usage edges"
    );
    assert!(
      graph.edges.iter().any(|edge| edge.kind == EdgeKind::AutoComposable),
      "Nuxt composable calls must create auto-import usage edges"
    );
  }

  #[test]
  fn reports_broken_imports_and_unused_components() {
    let project = TempProject::new("broken");
    let page = file("pages/index.vue", &[("./missing", "missing")], &[], &[]);
    let component = file("components/UnusedPanel.vue", &[], &[], &[]);
    materialize(&project, &[page.clone(), component.clone()]);
    let graph = build_project_graph(project.root(), &[page, component]);
    let ids = graph
      .diagnostics
      .iter()
      .map(|diagnostic| diagnostic.rule_id.as_str())
      .collect::<BTreeSet<_>>();
    assert_eq!(ids, PROJECT_RULE_IDS.into_iter().collect());
  }

  #[test]
  fn client_suffix_and_lazy_prefix_resolve_auto_imports() {
    let project = TempProject::new("client-lazy");
    let page = file("pages/index.vue", &[], &["HeroDemo", "LazyPlaygroundDemo"], &[]);
    let hero = file("components/HeroDemo.client.vue", &[], &[], &[]);
    let playground = file("components/PlaygroundDemo.client.vue", &[], &[], &[]);
    materialize(&project, &[page.clone(), hero.clone(), playground.clone()]);
    let graph = build_project_graph(project.root(), &[page, hero, playground]);
    assert!(
      graph.edges.iter().filter(|edge| edge.kind == EdgeKind::AutoComponent).count() >= 2,
      "`.client` components must match Nuxt tags / Lazy* tags: {:?}",
      graph.edges
    );
    assert!(
      graph.diagnostics.iter().all(|diagnostic| diagnostic.rule_id != PROJECT_RULE_IDS[1]),
      "referenced `.client` components must not be unused: {:?}",
      graph.diagnostics
    );
    assert!(
      graph
        .nodes
        .iter()
        .any(|node| node.path.ends_with("HeroDemo.client.vue") && node.name == "HeroDemo"),
      "graph node names must use the Nuxt auto-import name"
    );
  }

  #[test]
  fn nested_index_and_paired_client_server_components() {
    let project = TempProject::new("nested-paired");
    let page = file("pages/index.vue", &[], &["BaseButton", "Ui", "Comments"], &[]);
    let button = file("components/base/Button.vue", &[], &[], &[]);
    let ui = file("components/ui/index.vue", &[], &[], &[]);
    let client = file("components/Comments.client.vue", &[], &[], &[]);
    let server = file("components/Comments.server.vue", &[], &[], &[]);
    materialize(
      &project,
      &[page.clone(), button.clone(), ui.clone(), client.clone(), server.clone()],
    );
    let graph = build_project_graph(project.root(), &[page, button, ui, client, server]);
    let auto_targets = graph
      .edges
      .iter()
      .filter(|edge| edge.kind == EdgeKind::AutoComponent)
      .map(|edge| edge.to.as_str())
      .collect::<BTreeSet<_>>();
    assert!(
      auto_targets.contains("file:components/base/Button.vue"),
      "path-prefixed nested components must auto-import: {:?}",
      graph.edges
    );
    assert!(
      auto_targets.contains("file:components/ui/index.vue"),
      "components/*/index.vue must resolve to the folder name: {:?}",
      graph.edges
    );
    assert!(
      auto_targets.contains("file:components/Comments.client.vue")
        && auto_targets.contains("file:components/Comments.server.vue"),
      "paired .client/.server components must both count as referenced: {:?}",
      graph.edges
    );
    assert!(
      graph.diagnostics.iter().all(|diagnostic| diagnostic.rule_id != PROJECT_RULE_IDS[1]),
      "nested and paired components must not be unused: {:?}",
      graph.diagnostics
    );
  }

  #[test]
  fn nuxt_components_dts_overrides_path_prefix_false_names() {
    let project = TempProject::new("dts-override");
    project.write(
      ".nuxt/components.d.ts",
      r#"export const Button: typeof import("../components/base/Button.vue")['default']
export const LazyButton: LazyComponent<typeof import("../components/base/Button.vue")['default']>
"#,
    );
    let page = file("pages/index.vue", &[], &["Button"], &[]);
    let button = file("components/base/Button.vue", &[], &[], &[]);
    materialize(&project, &[page.clone(), button.clone()]);
    let graph = build_project_graph(project.root(), &[page, button]);
    assert!(
      graph
        .edges
        .iter()
        .any(|edge| { edge.kind == EdgeKind::AutoComponent && edge.specifier == "Button" }),
      ".nuxt component dts must supply pathPrefix:false names: {:?}",
      graph.edges
    );
    assert!(
      graph.invalidation_inputs.iter().any(|input| input == ".nuxt/components.d.ts"),
      "component dts must join invalidation inputs: {:?}",
      graph.invalidation_inputs
    );
  }

  #[test]
  fn scoped_package_imports_are_external_not_unresolved() {
    let project = TempProject::new("scoped-pkg");
    project.write(
      "node_modules/@tailwindcss/vite/package.json",
      r#"{"name":"@tailwindcss/vite","version":"1.0.0","exports":{".":"./index.js"}}"#,
    );
    project.write("node_modules/@tailwindcss/vite/index.js", "export default {}\n");
    let config = file("nuxt.config.ts", &[("@tailwindcss/vite", "tailwind")], &[], &[]);
    materialize(&project, std::slice::from_ref(&config));
    let graph = build_project_graph(project.root(), &[config]);
    assert!(
      graph.diagnostics.iter().all(|diagnostic| diagnostic.rule_id != PROJECT_RULE_IDS[0]),
      "scoped packages must not raise unresolved-import: {:?}",
      graph.diagnostics
    );
    assert!(
      graph.edges.iter().any(|edge| {
        edge.kind == EdgeKind::ExternalImport && edge.specifier == "@tailwindcss/vite"
      }),
      "scoped packages must become external import edges"
    );
  }

  #[test]
  #[expect(clippy::panic, reason = "test setup failures must fail the unit test")]
  fn relative_dot_root_resolves_tilde_aliases() {
    use std::sync::{Mutex, OnceLock};
    static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = CWD_LOCK
      .get_or_init(|| Mutex::new(()))
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);

    let project = TempProject::new("tilde-dot");
    let page = file("components/App.vue", &[("~/utils/contract", "Contract")], &[], &[]);
    let contract = file("utils/contract.ts", &[], &[], &[]);
    materialize(&project, &[page.clone(), contract.clone()]);
    let previous = match std::env::current_dir() {
      Ok(cwd) => cwd,
      Err(error) => panic!("failed to read current dir: {error}"),
    };
    if let Err(error) = std::env::set_current_dir(project.root()) {
      panic!("failed to enter temp project: {error}");
    }
    let graph = build_project_graph(Path::new("."), &[page, contract]);
    let _ignored = std::env::set_current_dir(previous);
    assert!(
      graph.diagnostics.iter().all(|diagnostic| diagnostic.rule_id != PROJECT_RULE_IDS[0]),
      "`vue-vet .` style relative roots must still resolve ~/ aliases: {:?}",
      graph.diagnostics
    );
    assert!(
      graph
        .edges
        .iter()
        .any(|edge| { edge.kind == EdgeKind::Import && edge.specifier == "~/utils/contract" }),
      "tilde imports must become project edges from a relative root: {:?}",
      graph.edges
    );
  }

  #[test]
  fn broken_package_exports_are_unresolved() {
    let project = TempProject::new("bad-exports");
    project.write(
      "node_modules/broken-pkg/package.json",
      r#"{"name":"broken-pkg","version":"1.0.0","exports":{".":"./missing.js"}}"#,
    );
    let importer = file("src/main.ts", &[("broken-pkg", "broken")], &[], &[]);
    materialize(&project, std::slice::from_ref(&importer));
    let graph = build_project_graph(project.root(), &[importer]);
    assert!(
      graph.diagnostics.iter().any(|diagnostic| {
        diagnostic.rule_id == PROJECT_RULE_IDS[0] && diagnostic.message.contains("broken-pkg")
      }),
      "broken package exports must be unresolved: {:?}",
      graph.diagnostics
    );
  }

  #[test]
  fn nuxt_tsconfig_paths_resolve_hash_aliases_into_known_files() {
    let project = TempProject::new("nuxt-tsconfig");
    project.write(
      ".nuxt/tsconfig.json",
      r##"{
  "compilerOptions": {
    "baseUrl": "..",
    "paths": {
      "#components/*": ["components/*"]
    }
  }
}"##,
    );
    let page = file("pages/index.vue", &[("#components/Panel", "Panel")], &["Panel"], &[]);
    let component = file("components/Panel.vue", &[], &[], &[]);
    materialize(&project, &[page.clone(), component.clone()]);
    let graph = build_project_graph(project.root(), &[page, component]);
    assert!(
      graph
        .edges
        .iter()
        .any(|edge| edge.kind == EdgeKind::Import || edge.kind == EdgeKind::ComponentUsage),
      "Nuxt tsconfig hash aliases must resolve into known project files: {:?}",
      graph.edges
    );
    assert!(
      graph.diagnostics.iter().all(|diagnostic| diagnostic.rule_id != PROJECT_RULE_IDS[0]),
      "resolved hash aliases must not be unresolved: {:?}",
      graph.diagnostics
    );
  }

  #[test]
  fn project_context_uses_already_read_nuxt_declarations() {
    let project = TempProject::new("snapshot-context");
    let known = [FileId::from("components/Panel.vue")];
    let declaration =
      b"export const CustomPanel: typeof import('../components/Panel.vue')['default']";
    let package = br#"{"name":"fixture"}"#;
    let context = project_context_from_inputs(
      project.root(),
      &known,
      [
        (".nuxt/components.d.ts", declaration.as_slice()),
        ("apps/admin/package.json", package.as_slice()),
      ],
      7,
    );
    assert_eq!(
      context.nuxt_component_names.get("CustomPanel").map(String::as_str),
      Some("components/Panel.vue")
    );
    assert_eq!(context.revision, 7);
    assert_eq!(context.invalidation_inputs, [".nuxt/components.d.ts", "apps/admin/package.json"]);
  }

  #[test]
  fn vue_modules_receive_composable_seeds_and_template_joins() {
    let project = TempProject::new("module-seeds");
    let producer_source = "import { toRef } from 'vue'; export function useField(props) { return { title: toRef(props, 'title') }; }";
    let consumer_script = "import { useField } from '../composables/useField'\nconst props = { title: 'x' }\nconst { title } = useField(props)\n";
    let sfc = format!(
      "<script setup lang=\"ts\">\n{consumer_script}</script>\n<template>\n  <p>{{{{ title }}}}</p>\n</template>\n"
    );
    let script_offset = sfc.find(consumer_script).unwrap_or(0);
    project.write("composables/useField.ts", producer_source);
    project.write("pages/index.vue", &sfc);
    let producer = ProjectFile {
      path: "composables/useField.ts".into(),
      source_len: producer_source.len(),
      facts: SfcFacts { template: TemplateFacts::default(), script: ScriptFacts::default() }.into(),
      module_source: Some(ModuleSource::standalone(
        "composables/useField.ts",
        producer_source,
        "ts",
        ScriptKind::Script,
      )),
      ordinary_module_source: None,
    };
    let consumer = ProjectFile {
      path: "pages/index.vue".into(),
      source_len: sfc.len(),
      facts: SfcFacts {
        template: TemplateFacts {
          elements: Vec::new(),
          expressions: vec![vue_vet_core::TemplateExpressionFact {
            surface: "interpolation".into(),
            expression: "title".into(),
            span: span(script_offset.saturating_add(consumer_script.len().saturating_add(40))),
            identifiers: Some(vec!["title".into()]),
          }],
        },
        script: ScriptFacts {
          blocks: vec![ScriptBlockFacts {
            kind: ScriptKind::Setup,
            language: "ts".into(),
            imports: vec![ScriptImportFact {
              source: "../composables/useField".into(),
              imported: "useField".into(),
              local: "useField".into(),
              span: span(0),
            }],
            bindings: Vec::new(),
            calls: Vec::new(),
            member_writes: Vec::new(),
            destructures: Vec::new(),
            reactivity_graph: std::sync::Arc::new(vue_vet_core::ReactivityGraph::default()),
          }],
        },
      }
      .into(),
      module_source: Some(ModuleSource::sfc_script(
        "pages/index.vue",
        consumer_script,
        "ts",
        ScriptKind::Setup,
        script_offset,
        sfc,
      )),
      ordinary_module_source: None,
    };
    let graph = build_project_graph(project.root(), &[producer, consumer]);
    assert!(
      graph.reactivity_error.is_none(),
      "module tracing must succeed: {:?}",
      graph.reactivity_error
    );
    let page = graph.module_reactivity.iter().find(|module| module.id == "pages/index.vue");
    assert!(
      page.is_some_and(|module| {
        module.graph.bindings.iter().any(|binding| {
          binding.name == "title" && binding.kind == vue_vet_core::ReactiveBindingKind::ToRef
        }) && module
          .graph
          .template_reads
          .iter()
          .any(|read| read.binding == "title" && read.surface == "interpolation")
          && module.graph.bindings.iter().any(|binding| binding.span.offset >= script_offset)
      }),
      "SFC modules must seed composable fields with SFC-absolute spans and join template reads"
    );
  }
}
