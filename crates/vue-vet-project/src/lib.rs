use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use vue_vet_core::{Diagnostic, ScriptFacts, Severity, SfcFacts, SourceSpan};

pub const CONVENTIONS_VERSION: u32 = 1;
pub const PROJECT_RULE_IDS: [&str; 2] =
  ["vue-vet/project/unresolved-import", "vue-vet/project/unused-component"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectFile {
  pub path: PathBuf,
  pub source_len: usize,
  pub facts: SfcFacts,
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
}

#[must_use]
pub fn build_project_graph(files: &[ProjectFile]) -> ProjectGraph {
  let mut ordered = files.iter().collect::<Vec<_>>();
  ordered.sort_by_key(|file| normalized_path(&file.path));
  let known = ordered.iter().map(|file| normalized_path(&file.path)).collect::<BTreeSet<_>>();
  let mut nodes = ordered.iter().map(|file| file_node(file)).collect::<Vec<_>>();
  let node_by_path =
    nodes.iter().map(|node| (node.path.clone(), node.id.clone())).collect::<BTreeMap<_, _>>();
  let component_by_name = nodes
    .iter()
    .filter(|node| node.kind == NodeKind::Component)
    .map(|node| (comparable_name(&node.name), node.id.clone()))
    .collect::<BTreeMap<_, _>>();
  let composable_by_name = nodes
    .iter()
    .filter(|node| node.kind == NodeKind::Composable)
    .map(|node| (node.name.clone(), node.id.clone()))
    .collect::<BTreeMap<_, _>>();
  let mut external_nodes = BTreeMap::new();
  let mut edges = Vec::new();
  let mut diagnostics = Vec::new();

  for file in &ordered {
    let path = normalized_path(&file.path);
    let from = file_id(&path);
    let imports = all_imports(&file.facts.script);
    for import in &imports {
      match resolve_import(&path, &import.source, &known) {
        Resolution::File(target) => {
          if let Some(to) = node_by_path.get(&target) {
            edges.push(edge(&from, to, EdgeKind::Import, &import.source, import.span.clone()));
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
        if let Resolution::File(target) = resolve_import(&path, &import.source, &known)
          && let Some(to) = node_by_path.get(&target)
        {
          edges.push(edge(&from, to, EdgeKind::ComponentUsage, &element.tag, element.span.clone()));
        }
      } else if let Some(to) = component_by_name.get(&tag) {
        edges.push(edge(&from, to, EdgeKind::AutoComponent, &element.tag, element.span.clone()));
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
  ProjectGraph {
    conventions_version: CONVENTIONS_VERSION,
    nodes,
    edges,
    diagnostics,
    invalidation_inputs: known.into_iter().collect(),
  }
}

fn all_imports(script: &ScriptFacts) -> Vec<&vue_vet_core::ScriptImportFact> {
  script.blocks.iter().flat_map(|block| &block.imports).collect()
}

fn file_node(file: &ProjectFile) -> GraphNode {
  let path = normalized_path(&file.path);
  GraphNode { id: file_id(&path), kind: node_kind(&path), name: file_stem(&path), path }
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

enum Resolution {
  File(String),
  External(String),
  Unresolved,
}

fn resolve_import(from: &str, specifier: &str, known: &BTreeSet<String>) -> Resolution {
  if specifier == "#imports"
    || (!specifier.starts_with('.')
      && !specifier.starts_with('@')
      && !specifier.starts_with('~')
      && !specifier.starts_with('#'))
  {
    return Resolution::External(specifier.into());
  }
  let base = if let Some(relative) = specifier.strip_prefix("@/") {
    format!("src/{relative}")
  } else if let Some(relative) = specifier.strip_prefix("~/") {
    relative.into()
  } else if specifier.starts_with('.') {
    let parent = Path::new(from).parent().unwrap_or_else(|| Path::new(""));
    normalized_path(&parent.join(specifier))
  } else {
    return Resolution::Unresolved;
  };
  resolution_candidates(&base)
    .into_iter()
    .find(|candidate| known.contains(candidate))
    .map_or(Resolution::Unresolved, Resolution::File)
}

fn resolution_candidates(base: &str) -> Vec<String> {
  let base = normalized_path(Path::new(base));
  let mut candidates = vec![base.clone()];
  if Path::new(&base).extension().is_none() {
    for extension in ["vue", "ts", "tsx", "js", "jsx"] {
      candidates.push(format!("{base}.{extension}"));
    }
    for extension in ["vue", "ts", "tsx", "js", "jsx"] {
      candidates.push(format!("{base}/index.{extension}"));
    }
  }
  candidates
}

fn normalized_path(path: &Path) -> String {
  let mut parts = Vec::new();
  for component in path.components() {
    match component {
      Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
      Component::ParentDir => {
        parts.pop();
      }
      Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
    }
  }
  parts.join("/")
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
    message: format!("cannot resolve project import `{specifier}`"),
    help: Some(
      "Use a relative path, the @/ or ~/ project aliases, or a supported external package import."
        .into(),
    ),
    file: file.to_path_buf(),
    span,
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
        message: format!("component `{}` is never referenced", node.name),
        help: Some("Remove it or reference it from a template or script import.".into()),
        file: file.path.clone(),
        span: SourceSpan { offset: 0, length: file.source_len.min(1), line: 1, column: 1 },
      })
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use vue_vet_core::{
    ScriptBlockFacts, ScriptCallFact, ScriptImportFact, ScriptKind, TemplateElementFact,
    TemplateFacts,
  };

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
    };
    ProjectFile { path: path.into(), source_len: 100, facts: SfcFacts { template, script } }
  }

  #[test]
  fn graph_is_deterministic_and_preserves_cycles() {
    let first = file("src/a.ts", &[("./b", "b")], &[], &[]);
    let second = file("src/b.ts", &[("./a", "a")], &[], &[]);
    let forward = build_project_graph(&[first.clone(), second.clone()]);
    let reverse = build_project_graph(&[second, first]);
    assert_eq!(forward, reverse, "input traversal order must not affect the graph");
    assert_eq!(forward.edges.len(), 2, "both sides of an import cycle must be represented");
  }

  #[test]
  fn resolves_aliases_and_nuxt_auto_imports() {
    let page = file(
      "pages/index.vue",
      &[("@/components/AppCard", "Card")],
      &["Card", "AutoButton"],
      &["useAccount"],
    );
    let imported = file("src/components/AppCard.vue", &[], &[], &[]);
    let automatic = file("components/AutoButton.vue", &[], &[], &[]);
    let composable = file("composables/useAccount.ts", &[], &[], &[]);
    let graph = build_project_graph(&[page, imported, automatic, composable]);
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
    let page = file("pages/index.vue", &[("./missing", "missing")], &[], &[]);
    let component = file("components/UnusedPanel.vue", &[], &[], &[]);
    let graph = build_project_graph(&[page, component]);
    let ids = graph
      .diagnostics
      .iter()
      .map(|diagnostic| diagnostic.rule_id.as_str())
      .collect::<BTreeSet<_>>();
    assert_eq!(ids, PROJECT_RULE_IDS.into_iter().collect());
  }
}
