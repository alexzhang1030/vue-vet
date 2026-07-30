//! `StructuralLink` — per-file import/component/composable edges + Nuxt seed hook.

use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
  sync::Arc,
};

use vue_vet_core::{Diagnostic, FileId, ModuleId, ScriptFacts, SfcFacts, SourceSpan};
use vue_vet_reactivity::ModuleLink;

use crate::conventions::{
  NuxtImportTarget, convention_component_name, strip_lazy_component_prefix,
};
use crate::model::{EdgeKind, GraphEdge, GraphNode, NodeKind, ProjectFile};
use crate::passes::{ExternalReactivityRoot, NuxtImportsSeedPass};
use crate::resolve::{ProjectResolver, Resolution, normalized_path};
use crate::rules::unresolved_diagnostic;

#[derive(Clone, Debug, Default)]
pub struct StructuralProjectState {
  pub context: Option<StructuralContextKey>,
  pub files: BTreeMap<FileId, StructuralFileCache>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructuralContextKey {
  pub root: PathBuf,
  pub revision: u64,
  pub nodes: Vec<GraphNode>,
  pub module_ids: BTreeSet<ModuleId>,
  pub nuxt_import_names: BTreeMap<String, NuxtImportTarget>,
}

#[derive(Clone, Debug)]
pub struct StructuralFileCache {
  pub facts: Arc<SfcFacts>,
  pub output: Arc<StructuralFileOutput>,
}

#[derive(Debug, Default)]
pub struct StructuralFileOutput {
  pub external_nodes: Vec<GraphNode>,
  pub edges: Vec<GraphEdge>,
  pub diagnostics: Vec<Diagnostic>,
  pub module_links: Vec<ModuleLink>,
  /// External package files that may contribute reactivity Factory/Composable seeds.
  pub external_reactivity_roots: Vec<ExternalReactivityRoot>,
}

#[must_use]
pub fn file_node(file: &ProjectFile) -> GraphNode {
  let path = normalized_path(file.path.as_path());
  let kind = node_kind(&path);
  let name = if kind == NodeKind::Component {
    convention_component_name(&path).unwrap_or_else(|| file_stem(&path))
  } else {
    file_stem(&path)
  };
  GraphNode { id: file_id(&path), kind, name, path }
}

pub fn insert_component_name(map: &mut BTreeMap<String, Vec<String>>, name: &str, id: &str) {
  let key = comparable_name(name);
  let entry = map.entry(key).or_default();
  if !entry.iter().any(|existing| existing == id) {
    entry.push(id.to_owned());
  }
}

#[expect(
  clippy::too_many_arguments,
  reason = "structural analysis needs graph indexes plus Nuxt import maps"
)]
#[must_use]
pub fn analyze_structural_file(
  file: &ProjectFile,
  resolver: &ProjectResolver,
  known: &BTreeSet<String>,
  node_by_path: &BTreeMap<String, String>,
  component_by_name: &BTreeMap<String, Vec<String>>,
  composable_by_name: &BTreeMap<String, String>,
  module_ids: &BTreeSet<ModuleId>,
  nuxt_import_names: &BTreeMap<String, NuxtImportTarget>,
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
      Resolution::External { package, resolved_path } => {
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
        if let Some(resolved_path) = resolved_path {
          for module_from in [ModuleId::primary(&file.path), ModuleId::ordinary(&file.path)] {
            if module_ids.contains(&module_from) {
              output.external_reactivity_roots.push(ExternalReactivityRoot {
                from: module_from,
                specifier: import.source.clone(),
                resolved_path: resolved_path.clone(),
              });
            }
          }
        }
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
  // Enrichment (`StructuralLink`): bare Nuxt / Vite auto-imports → `#nuxt-imports:` seeds.
  let nuxt_delta = NuxtImportsSeedPass::run(file, resolver, known, module_ids, nuxt_import_names);
  output.module_links.extend(nuxt_delta.module_links);
  output.external_nodes.extend(nuxt_delta.external_nodes);
  output.external_reactivity_roots.extend(nuxt_delta.external_reactivity_roots);
  output
}

fn all_imports(script: &ScriptFacts) -> Vec<&vue_vet_core::ScriptImportFact> {
  script.blocks.iter().flat_map(|block| &block.imports).collect()
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
