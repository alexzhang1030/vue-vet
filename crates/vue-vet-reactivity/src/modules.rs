use std::collections::{BTreeMap, btree_map::Entry};

use oxc_allocator::Allocator;
use oxc_ast::{
  AstKind,
  ast::{ExportDefaultDeclarationKind, ImportDeclarationSpecifier},
};
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::{SourceType, Span};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vue_vet_core::{ReactiveBindingFact, ReactiveBindingKind, ReactivityGraph, ScriptKind};

use super::{module_export_name, source_span, trace_reactivity_seeded};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleSource {
  pub id: String,
  pub source: String,
  pub language: String,
  pub kind: ScriptKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleLink {
  pub from: String,
  pub specifier: String,
  pub to: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleReactivity {
  pub id: String,
  pub graph: ReactivityGraph,
}

#[derive(Debug, Error)]
pub enum TraceModulesError {
  #[error("duplicate reactivity module id `{0}`")]
  DuplicateModule(String),
  #[error("module `{module}` uses unsupported language `{language}`")]
  UnsupportedLanguage { module: String, language: String },
  #[error("could not parse reactivity module `{module}`: {message}")]
  Parse { module: String, message: String },
  #[error("could not build semantics for reactivity module `{module}`: {message}")]
  Semantic { module: String, message: String },
  #[error("reactivity module link {from} -> {to} references an unknown module")]
  UnknownLink { from: String, to: String },
  #[error("reactivity module `{from}` resolves `{specifier}` to multiple targets")]
  AmbiguousLink { from: String, specifier: String },
}

#[derive(Clone, Debug)]
struct ImportSummary {
  local: String,
  imported: String,
  source: String,
  span: Span,
}

#[derive(Clone, Debug)]
enum ExportSummary {
  Local { local: String, exported: String },
  Reexport { source: String, imported: String, exported: String },
  Star { source: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExportState {
  Known(ReactiveBindingKind),
  Ambiguous,
}

#[derive(Clone, Debug)]
struct ModuleSummary {
  module: ModuleSource,
  local_graph: ReactivityGraph,
  imports: Vec<ImportSummary>,
  exports: Vec<ExportSummary>,
}

/// Traces local and linked reactivity across a resolved module graph.
///
/// # Errors
///
/// Returns an error when a module cannot be parsed or analyzed, module identifiers
/// are duplicated, or a supplied resolved link is unknown or ambiguous.
pub fn trace_modules(
  modules: &[ModuleSource],
  links: &[ModuleLink],
) -> Result<Vec<ModuleReactivity>, TraceModulesError> {
  let mut summaries = BTreeMap::new();
  for module in modules {
    if summaries.contains_key(&module.id) {
      return Err(TraceModulesError::DuplicateModule(module.id.clone()));
    }
    summaries.insert(module.id.clone(), analyze_module(module, &[])?);
  }
  let resolved_links = resolved_links(&summaries, links)?;
  let exports = resolve_exports(&summaries, &resolved_links);

  let mut traced = Vec::with_capacity(summaries.len());
  for (id, summary) in &summaries {
    let seeds = imported_bindings(summary, &exports, &resolved_links);
    let analysis = analyze_module(&summary.module, &seeds)?;
    traced.push(ModuleReactivity { id: id.clone(), graph: analysis.local_graph });
  }
  Ok(traced)
}

fn source_type(module: &ModuleSource) -> Result<SourceType, TraceModulesError> {
  match module.language.as_str() {
    "js" | "javascript" => Ok(SourceType::mjs()),
    "jsx" => Ok(SourceType::jsx()),
    "ts" | "typescript" => Ok(SourceType::ts()),
    "tsx" => Ok(SourceType::tsx()),
    language => Err(TraceModulesError::UnsupportedLanguage {
      module: module.id.clone(),
      language: language.into(),
    }),
  }
}

fn analyze_module(
  module: &ModuleSource,
  seeds: &[ReactiveBindingFact],
) -> Result<ModuleSummary, TraceModulesError> {
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, &module.source, source_type(module)?).parse();
  if !parsed.errors.is_empty() {
    return Err(TraceModulesError::Parse {
      module: module.id.clone(),
      message: join_errors(&parsed.errors),
    });
  }
  let built = SemanticBuilder::new().with_check_syntax_error(true).build(&parsed.program);
  if !built.errors.is_empty() {
    return Err(TraceModulesError::Semantic {
      module: module.id.clone(),
      message: join_errors(&built.errors),
    });
  }
  let semantic = built.semantic;
  let local_graph = trace_reactivity_seeded(&semantic, &module.source, 0, module.kind, seeds);
  let imports = collect_imports(&semantic);
  let exports = collect_exports(&semantic, &local_graph);
  Ok(ModuleSummary { module: module.clone(), local_graph, imports, exports })
}

fn collect_imports(semantic: &oxc_semantic::Semantic<'_>) -> Vec<ImportSummary> {
  let mut imports = Vec::new();
  for node in semantic.nodes() {
    let AstKind::ImportDeclaration(declaration) = node.kind() else {
      continue;
    };
    let Some(specifiers) = &declaration.specifiers else {
      continue;
    };
    let source = declaration.source.value.to_string();
    for specifier in specifiers {
      let (local, imported, span) = match specifier {
        ImportDeclarationSpecifier::ImportSpecifier(specifier) => (
          specifier.local.name.to_string(),
          module_export_name(&specifier.imported),
          specifier.local.span,
        ),
        ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
          (specifier.local.name.to_string(), "default".into(), specifier.local.span)
        }
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
          (specifier.local.name.to_string(), "*".into(), specifier.local.span)
        }
      };
      imports.push(ImportSummary { local, imported, source: source.clone(), span });
    }
  }
  imports.sort_by_key(|import| import.span.start);
  imports
}

fn collect_exports(
  semantic: &oxc_semantic::Semantic<'_>,
  graph: &ReactivityGraph,
) -> Vec<ExportSummary> {
  let mut exports = Vec::new();
  for node in semantic.nodes() {
    match node.kind() {
      AstKind::ExportNamedDeclaration(declaration) => {
        if declaration.declaration.is_some() {
          for binding in &graph.bindings {
            let offset = u32::try_from(binding.span.offset).unwrap_or(u32::MAX);
            if declaration.span.start <= offset && declaration.span.end >= offset {
              exports.push(ExportSummary::Local {
                local: binding.name.clone(),
                exported: binding.name.clone(),
              });
            }
          }
        }
        for specifier in &declaration.specifiers {
          let local = module_export_name(&specifier.local);
          let exported = module_export_name(&specifier.exported);
          if let Some(source) = &declaration.source {
            exports.push(ExportSummary::Reexport {
              source: source.value.to_string(),
              imported: local,
              exported,
            });
          } else {
            exports.push(ExportSummary::Local { local, exported });
          }
        }
      }
      AstKind::ExportDefaultDeclaration(declaration) => {
        if let ExportDefaultDeclarationKind::Identifier(identifier) = &declaration.declaration {
          exports.push(ExportSummary::Local {
            local: identifier.name.to_string(),
            exported: "default".into(),
          });
        }
      }
      AstKind::ExportAllDeclaration(declaration) if declaration.exported.is_none() => {
        exports.push(ExportSummary::Star { source: declaration.source.value.to_string() });
      }
      _ => {}
    }
  }
  exports
}

fn resolved_links(
  summaries: &BTreeMap<String, ModuleSummary>,
  links: &[ModuleLink],
) -> Result<BTreeMap<(String, String), String>, TraceModulesError> {
  let mut resolved = BTreeMap::new();
  for link in links {
    if !summaries.contains_key(&link.from) || !summaries.contains_key(&link.to) {
      return Err(TraceModulesError::UnknownLink { from: link.from.clone(), to: link.to.clone() });
    }
    let key = (link.from.clone(), link.specifier.clone());
    match resolved.entry(key) {
      Entry::Vacant(entry) => {
        entry.insert(link.to.clone());
      }
      Entry::Occupied(entry) if entry.get() == &link.to => {}
      Entry::Occupied(_) => {
        return Err(TraceModulesError::AmbiguousLink {
          from: link.from.clone(),
          specifier: link.specifier.clone(),
        });
      }
    }
  }
  Ok(resolved)
}

fn resolve_exports(
  summaries: &BTreeMap<String, ModuleSummary>,
  links: &BTreeMap<(String, String), String>,
) -> BTreeMap<String, BTreeMap<String, ExportState>> {
  let mut resolved =
    summaries.keys().map(|id| (id.clone(), BTreeMap::new())).collect::<BTreeMap<_, _>>();

  for (id, summary) in summaries {
    for export in &summary.exports {
      let ExportSummary::Local { local, exported } = export else {
        continue;
      };
      if let Some(binding) =
        summary.local_graph.bindings.iter().find(|binding| &binding.name == local)
      {
        insert_export(&mut resolved, id, exported, ExportState::Known(binding.kind));
      }
    }
  }

  loop {
    let snapshot = resolved.clone();
    let mut changed = false;
    for (id, summary) in summaries {
      for export in &summary.exports {
        match export {
          ExportSummary::Local { .. } => {}
          ExportSummary::Reexport { source, imported, exported } => {
            let Some(target) = links.get(&(id.clone(), source.clone())) else {
              continue;
            };
            let Some(state) = snapshot.get(target).and_then(|exports| exports.get(imported)) else {
              continue;
            };
            changed |= insert_export(&mut resolved, id, exported, *state);
          }
          ExportSummary::Star { source } => {
            let Some(target) = links.get(&(id.clone(), source.clone())) else {
              continue;
            };
            let Some(target_exports) = snapshot.get(target) else {
              continue;
            };
            for (exported, state) in target_exports {
              if exported != "default" {
                changed |= insert_export(&mut resolved, id, exported, *state);
              }
            }
          }
        }
      }
    }
    if !changed {
      break;
    }
  }

  resolved
}

fn insert_export(
  resolved: &mut BTreeMap<String, BTreeMap<String, ExportState>>,
  module: &str,
  exported: &str,
  state: ExportState,
) -> bool {
  let Some(module_exports) = resolved.get_mut(module) else {
    return false;
  };
  match module_exports.entry(exported.into()) {
    Entry::Vacant(entry) => {
      entry.insert(state);
      true
    }
    Entry::Occupied(mut entry)
      if *entry.get() != state && *entry.get() != ExportState::Ambiguous =>
    {
      entry.insert(ExportState::Ambiguous);
      true
    }
    Entry::Occupied(_) => false,
  }
}

fn imported_bindings(
  summary: &ModuleSummary,
  exports: &BTreeMap<String, BTreeMap<String, ExportState>>,
  links: &BTreeMap<(String, String), String>,
) -> Vec<ReactiveBindingFact> {
  summary
    .imports
    .iter()
    .filter(|import| import.imported != "*")
    .filter_map(|import| {
      let target = links.get(&(summary.module.id.clone(), import.source.clone()))?;
      let ExportState::Known(kind) = exports.get(target)?.get(&import.imported)? else {
        return None;
      };
      Some(ReactiveBindingFact {
        name: import.local.clone(),
        kind: *kind,
        initialized_with_null: false,
        span: source_span(&summary.module.source, 0, import.span),
      })
    })
    .collect()
}

fn join_errors(errors: &[impl ToString]) -> String {
  errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
}
