mod conventions;
mod resolve;

use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use vue_vet_core::{Confidence, Diagnostic, ScriptFacts, Severity, SfcFacts, SourceSpan};
use vue_vet_reactivity::{ModuleLink, ModuleReactivity, ModuleSource, trace_modules};

pub use resolve::{OXC_RESOLVER_VERSION, normalize_project_root, resolver_config_inputs};

use conventions::{
  convention_component_name, load_nuxt_component_dts_names, strip_lazy_component_prefix,
};
use resolve::{ProjectResolver, Resolution, normalized_path};

pub const CONVENTIONS_VERSION: u32 = 3;
pub const PROJECT_RULE_IDS: [&str; 2] =
  ["vue-vet/project/unresolved-import", "vue-vet/project/unused-component"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectFile {
  pub path: PathBuf,
  pub source_len: usize,
  pub facts: SfcFacts,
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
  pub reactivity_error: Option<String>,
}

#[must_use]
pub fn build_project_graph(root: &Path, files: &[ProjectFile]) -> ProjectGraph {
  let root = normalize_project_root(root);
  let mut ordered = files.iter().collect::<Vec<_>>();
  ordered.sort_by_key(|file| normalized_path(&file.path));
  let known = ordered.iter().map(|file| normalized_path(&file.path)).collect::<BTreeSet<_>>();
  let resolver = ProjectResolver::new(&root);
  let mut nodes = ordered.iter().map(|file| file_node(file)).collect::<Vec<_>>();
  let node_by_path =
    nodes.iter().map(|node| (node.path.clone(), node.id.clone())).collect::<BTreeMap<_, _>>();
  let dts_names = load_nuxt_component_dts_names(&root, &known);
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
  for (name, path) in &dts_names {
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
      [file.module_source.clone(), file.ordinary_module_source.clone()].into_iter().flatten()
    })
    .map(|mut module| {
      // Preserve `#script` dual suffix while normalizing the path prefix.
      if let Some((base, suffix)) = module.id.rsplit_once('#') {
        module.id = format!("{}#{suffix}", normalized_path(Path::new(base)));
      } else {
        module.id = normalized_path(Path::new(&module.id));
      }
      module
    })
    .collect::<Vec<_>>();
  let module_ids = module_sources.iter().map(|module| module.id.clone()).collect::<BTreeSet<_>>();
  let mut module_links = Vec::new();
  let mut external_nodes = BTreeMap::new();
  let mut edges = Vec::new();
  let mut diagnostics = Vec::new();

  for file in &ordered {
    let path = normalized_path(&file.path);
    let from = file_id(&path);
    let imports = all_imports(&file.facts.script);
    for import in &imports {
      match resolver.resolve(&path, &import.source, &known) {
        Resolution::File(target) => {
          if let Some(to) = node_by_path.get(&target) {
            edges.push(edge(&from, to, EdgeKind::Import, &import.source, import.span.clone()));
          }
          // Link primary module id and dual `#script` companion when both re-trace.
          for module_from in [&path, &format!("{path}#script")] {
            if module_ids.contains(module_from.as_str()) && module_ids.contains(&target) {
              module_links.push(ModuleLink {
                from: module_from.clone(),
                specifier: import.source.clone(),
                to: target.clone(),
              });
            }
          }
        }
        Resolution::External(package) => {
          let id = format!("external:{package}");
          external_nodes.entry(id.clone()).or_insert_with(|| GraphNode {
            id: id.clone(),
            kind: NodeKind::External,
            path: package.clone(),
            name: package.clone(),
          });
          edges.push(edge(
            &from,
            &id,
            EdgeKind::ExternalImport,
            &import.source,
            import.span.clone(),
          ));
        }
        Resolution::Unresolved => {
          diagnostics.push(unresolved_diagnostic(&file.path, &import.source, import.span.clone()));
        }
      }
    }

    for element in &file.facts.template.elements {
      let tag = comparable_name(&element.tag);
      if let Some(import) = imports.iter().find(|import| comparable_name(&import.local) == tag) {
        if let Resolution::File(target) = resolver.resolve(&path, &import.source, &known)
          && let Some(to) = node_by_path.get(&target)
        {
          edges.push(edge(&from, to, EdgeKind::ComponentUsage, &element.tag, element.span.clone()));
        }
      } else {
        for to in auto_component_targets(&element.tag, &component_by_name) {
          edges.push(edge(&from, &to, EdgeKind::AutoComponent, &element.tag, element.span.clone()));
        }
      }
    }

    for call in file.facts.script.blocks.iter().flat_map(|block| &block.calls) {
      if let Some(to) = composable_by_name.get(&call.callee) {
        edges.push(edge(&from, to, EdgeKind::AutoComposable, &call.callee, call.span.clone()));
      }
    }
  }

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
  let (mut module_reactivity, reactivity_error) =
    match trace_modules(&module_sources, &module_links) {
      Ok(reactivity) => (reactivity, None),
      Err(error) => (Vec::new(), Some(error.to_string())),
    };
  // Re-apply SFC template joins onto module graphs so cross-file seeds and
  // template reads share one fact surface. Spans stay SFC-absolute when the
  // module carried `source_offset` + `span_source`.
  let templates = ordered
    .iter()
    .map(|file| (normalized_path(&file.path), &file.facts.template))
    .collect::<BTreeMap<_, _>>();
  for module in &mut module_reactivity {
    if let Some(template) = templates.get(&module.id) {
      module.graph.join_template_reads(template);
    }
  }
  let mut invalidation_inputs = known.into_iter().collect::<Vec<_>>();
  invalidation_inputs.extend(resolver_config_inputs(&root));
  invalidation_inputs.sort();
  invalidation_inputs.dedup();
  ProjectGraph {
    conventions_version: CONVENTIONS_VERSION,
    nodes,
    edges,
    diagnostics,
    invalidation_inputs,
    module_reactivity,
    reactivity_error,
  }
}

fn all_imports(script: &ScriptFacts) -> Vec<&vue_vet_core::ScriptImportFact> {
  script.blocks.iter().flat_map(|block| &block.imports).collect()
}

fn file_node(file: &ProjectFile) -> GraphNode {
  let path = normalized_path(&file.path);
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
    file: file.to_path_buf(),
    span,
    edits: Vec::new(),
  }
}

fn unused_component_diagnostics(
  files: &[&ProjectFile],
  nodes: &[GraphNode],
  edges: &[GraphEdge],
) -> Vec<Diagnostic> {
  nodes
    .iter()
    .filter(|node| node.kind == NodeKind::Component)
    .filter(|node| {
      !edges.iter().any(|edge| {
        edge.to == node.id
          && matches!(
            edge.kind,
            EdgeKind::Import | EdgeKind::ComponentUsage | EdgeKind::AutoComponent
          )
      })
    })
    .filter_map(|node| {
      let file = files.iter().find(|file| normalized_path(&file.path) == node.path)?;
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
      })
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::{
    fs,
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
            span: span(index.saturating_add(10)),
          })
          .collect(),
        member_writes: Vec::new(),
        destructures: Vec::new(),
        reactivity_graph: vue_vet_core::ReactivityGraph::default(),
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
        })
        .collect(),
      expressions: Vec::new(),
    };
    ProjectFile {
      path: path.into(),
      source_len: 100,
      facts: SfcFacts { template, script },
      module_source: None,
      ordinary_module_source: None,
    }
  }

  fn materialize(project: &TempProject, files: &[ProjectFile]) {
    for file in files {
      let relative = normalized_path(&file.path);
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
      facts: SfcFacts { template: TemplateFacts::default(), script: ScriptFacts::default() },
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
            reactivity_graph: vue_vet_core::ReactivityGraph::default(),
          }],
        },
      },
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
