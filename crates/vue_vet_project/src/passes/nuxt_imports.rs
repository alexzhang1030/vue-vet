//! [`NuxtImportsSeedPass`] — [`StructuralLink`](super::EnrichmentStage::StructuralLink)
//! enrichment for bare Nuxt auto-imports.

use std::collections::{BTreeMap, BTreeSet};

use vue_vet_core::{FileId, ModuleId, ScriptCallFact, ScriptFacts, ScriptImportFact};

use super::types::ExternalReactivityRoot;
use crate::{
  GraphNode, NodeKind, NuxtImportTarget, ProjectFile, ProjectResolver, Resolution,
  conventions::nuxt_imports_link_specifier,
};

/// Deterministic delta produced by [`NuxtImportsSeedPass::run`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NuxtImportsSeedDelta {
  pub module_links: Vec<vue_vet_reactivity::ModuleLink>,
  pub external_nodes: Vec<GraphNode>,
  pub external_reactivity_roots: Vec<ExternalReactivityRoot>,
}

/// Bare `.nuxt` auto-import → `#nuxt-imports:` reactivity seeds.
pub struct NuxtImportsSeedPass;

impl NuxtImportsSeedPass {
  pub const NAME: &'static str = "nuxt_imports_seed";
  pub const STAGE: super::EnrichmentStage = super::EnrichmentStage::StructuralLink;

  /// Wire bare script calls listed in `.nuxt` imports maps to `#nuxt-imports:` seeds.
  ///
  /// Specifiers resolve from the **declaring** dts importer (`NuxtImportTarget::importer`).
  /// Local imports shadow auto-import names. Unresolved / quiet virtuals stay silent.
  #[must_use]
  pub fn run(
    file: &ProjectFile,
    resolver: &ProjectResolver,
    known: &BTreeSet<String>,
    module_ids: &BTreeSet<ModuleId>,
    nuxt_import_names: &BTreeMap<String, NuxtImportTarget>,
  ) -> NuxtImportsSeedDelta {
    let imports = all_imports(&file.facts.script);
    let mut delta = NuxtImportsSeedDelta::default();
    for call in file.facts.script.blocks.iter().flat_map(|block| &block.calls) {
      append_bare_nuxt_seed(
        file,
        call,
        &imports,
        resolver,
        known,
        module_ids,
        nuxt_import_names,
        &mut delta,
      );
    }
    delta
  }
}

#[expect(
  clippy::too_many_arguments,
  reason = "seed wiring needs file, call, imports, resolver indexes, and out delta"
)]
fn append_bare_nuxt_seed(
  file: &ProjectFile,
  call: &ScriptCallFact,
  imports: &[&ScriptImportFact],
  resolver: &ProjectResolver,
  known: &BTreeSet<String>,
  module_ids: &BTreeSet<ModuleId>,
  nuxt_import_names: &BTreeMap<String, NuxtImportTarget>,
  delta: &mut NuxtImportsSeedDelta,
) {
  if imports.iter().any(|import| import.local == call.callee) {
    return;
  }
  let Some(target) = nuxt_import_names.get(&call.callee) else {
    return;
  };
  let link_specifier = nuxt_imports_link_specifier(&call.callee);
  match resolver.resolve(&target.importer, &target.specifier, known) {
    Resolution::File(workspace_path) => {
      let target_id = ModuleId::primary(&FileId::from(workspace_path.as_str()));
      for module_from in [ModuleId::primary(&file.path), ModuleId::ordinary(&file.path)] {
        if module_ids.contains(&module_from) && module_ids.contains(&target_id) {
          delta.module_links.push(vue_vet_reactivity::ModuleLink {
            from: module_from,
            specifier: link_specifier.clone(),
            to: target_id.clone(),
          });
        }
      }
    }
    Resolution::External { package, resolved_path: Some(resolved_path) } => {
      let id = format!("external:{package}");
      delta.external_nodes.push(GraphNode {
        id,
        kind: NodeKind::External,
        path: package.clone(),
        name: package,
      });
      for module_from in [ModuleId::primary(&file.path), ModuleId::ordinary(&file.path)] {
        if module_ids.contains(&module_from) {
          delta.external_reactivity_roots.push(ExternalReactivityRoot {
            from: module_from,
            specifier: link_specifier.clone(),
            resolved_path: resolved_path.clone(),
          });
        }
      }
    }
    Resolution::External { resolved_path: None, .. } | Resolution::Unresolved => {
      // Virtual `#app/…` / unresolved auto-imports stay quiet (no unresolved-import).
    }
  }
}

fn all_imports(script: &ScriptFacts) -> Vec<&ScriptImportFact> {
  script.blocks.iter().flat_map(|block| &block.imports).collect()
}

#[cfg(test)]
mod tests {
  use std::collections::{BTreeMap, BTreeSet};

  use vue_vet_core::{
    FileId, ModuleId, ScriptBlockFacts, ScriptCallFact, ScriptFacts, ScriptImportFact, ScriptKind,
    SfcFacts, SourceSpan, TemplateFacts,
  };
  use vue_vet_reactivity::ModuleSource;

  use super::NuxtImportsSeedPass;
  use crate::{
    NuxtImportTarget, ProjectFile, ProjectResolver, conventions::NUXT_IMPORTS_SPECIFIER_PREFIX,
  };

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 1, line: 1, column: 1 }
  }

  #[test]
  #[expect(clippy::panic, reason = "test setup failures must fail the unit test")]
  fn bare_call_seeds_external_root_from_declaring_importer() {
    let dir = std::env::temp_dir().join(format!(
      "vue-vet-nuxt-pass-{}-{}",
      std::process::id(),
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
    ));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(dir.join("node_modules/@nuxtjs/color-mode/dist/runtime"))
      .unwrap_or_else(|error| panic!("fixture dir: {error}"));
    std::fs::write(
      dir.join("node_modules/@nuxtjs/color-mode/dist/runtime/composables.js"),
      "export const useColorMode = () => ({ value: 'light' })\n",
    )
    .unwrap_or_else(|error| panic!("write js: {error}"));
    std::fs::write(
      dir.join("node_modules/@nuxtjs/color-mode/dist/runtime/composables.d.ts"),
      "export declare const useColorMode: () => { value: string };\n",
    )
    .unwrap_or_else(|error| panic!("write dts: {error}"));
    std::fs::create_dir_all(dir.join(".nuxt")).unwrap_or_else(|error| panic!(".nuxt: {error}"));
    std::fs::write(
      dir.join(".nuxt/imports.d.ts"),
      "export { useColorMode } from '../node_modules/@nuxtjs/color-mode/dist/runtime/composables';\n",
    )
    .unwrap_or_else(|error| panic!("imports.d.ts: {error}"));

    let script = "const colorMode = useColorMode()\n";
    let prefix = "<script setup lang=\"ts\">\n";
    let sfc = format!("{prefix}{script}</script>\n<template><p>ok</p></template>\n");
    let file = ProjectFile {
      path: "components/Demo.vue".into(),
      source_len: sfc.len(),
      facts: SfcFacts {
        template: TemplateFacts { elements: Vec::new(), expressions: Vec::new() },
        script: ScriptFacts {
          blocks: vec![ScriptBlockFacts {
            kind: ScriptKind::Setup,
            language: "ts".into(),
            imports: Vec::new(),
            bindings: Vec::new(),
            calls: vec![ScriptCallFact {
              callee: "useColorMode".into(),
              assigned_to: Some("colorMode".into()),
              resolved_import: None,
              argument_identifiers: Vec::new(),
              span: span(0),
            }],
            member_writes: Vec::new(),
            destructures: Vec::new(),
            top_level_await_ends: Vec::new(),
            operands: Vec::new(),
            reactivity_graph: std::sync::Arc::new(vue_vet_core::ReactivityGraph::default()),
          }],
        },
      }
      .into(),
      module_source: Some(ModuleSource::sfc_script(
        "components/Demo.vue",
        script,
        "ts",
        ScriptKind::Setup,
        prefix.len(),
        sfc,
      )),
      ordinary_module_source: None,
    };

    let mut names = BTreeMap::new();
    names.insert(
      "useColorMode".into(),
      NuxtImportTarget {
        specifier: "../node_modules/@nuxtjs/color-mode/dist/runtime/composables".into(),
        importer: ".nuxt/imports.d.ts".into(),
      },
    );
    let resolver = ProjectResolver::new(&dir);
    let module_ids = BTreeSet::from([ModuleId::primary(&FileId::from("components/Demo.vue"))]);
    let delta = NuxtImportsSeedPass::run(&file, &resolver, &BTreeSet::new(), &module_ids, &names);
    drop(std::fs::remove_dir_all(&dir));

    assert!(
      delta.external_reactivity_roots.iter().any(|root| {
        root.specifier == format!("{NUXT_IMPORTS_SPECIFIER_PREFIX}useColorMode")
          && root.from == ModuleId::primary(&FileId::from("components/Demo.vue"))
      }),
      "bare useColorMode must seed external reactivity root: {delta:?}"
    );
    assert!(
      delta.external_nodes.iter().any(|node| node.kind == crate::NodeKind::External),
      "external node expected: {delta:?}"
    );
  }

  #[test]
  fn local_import_shadows_nuxt_auto_import() {
    let file = ProjectFile {
      path: "components/Demo.vue".into(),
      source_len: 1,
      facts: SfcFacts {
        template: TemplateFacts { elements: Vec::new(), expressions: Vec::new() },
        script: ScriptFacts {
          blocks: vec![ScriptBlockFacts {
            kind: ScriptKind::Setup,
            language: "ts".into(),
            imports: vec![ScriptImportFact {
              source: "./local".into(),
              imported: "useColorMode".into(),
              local: "useColorMode".into(),
              span: span(0),
            }],
            bindings: Vec::new(),
            calls: vec![ScriptCallFact {
              callee: "useColorMode".into(),
              assigned_to: Some("colorMode".into()),
              resolved_import: Some(("./local".into(), "useColorMode".into())),
              argument_identifiers: Vec::new(),
              span: span(1),
            }],
            member_writes: Vec::new(),
            destructures: Vec::new(),
            top_level_await_ends: Vec::new(),
            operands: Vec::new(),
            reactivity_graph: std::sync::Arc::new(vue_vet_core::ReactivityGraph::default()),
          }],
        },
      }
      .into(),
      module_source: None,
      ordinary_module_source: None,
    };
    let mut names = BTreeMap::new();
    names.insert(
      "useColorMode".into(),
      NuxtImportTarget {
        specifier: "../node_modules/@nuxtjs/color-mode/dist/runtime/composables".into(),
        importer: ".nuxt/imports.d.ts".into(),
      },
    );
    let resolver = ProjectResolver::new(std::env::temp_dir().as_path());
    let delta =
      NuxtImportsSeedPass::run(&file, &resolver, &BTreeSet::new(), &BTreeSet::new(), &names);
    assert!(
      delta.external_reactivity_roots.is_empty() && delta.module_links.is_empty(),
      "local import must shadow Nuxt auto-import: {delta:?}"
    );
  }
}
