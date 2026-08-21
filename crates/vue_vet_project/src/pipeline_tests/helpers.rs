use crate::resolve::normalized_path;

pub use crate::{
  EdgeKind, PROJECT_RULE_IDS, ProjectContext, ProjectFile, ProjectGraph, ProjectGraphState,
  build_project_graph, build_project_graph_incremental_with_options, project_context_from_inputs,
};
pub use std::{collections::BTreeSet, path::Path};
use std::{
  fs,
  path::PathBuf,
  sync::atomic::{AtomicUsize, Ordering},
};

use vue_vet_plugins::default_trace_modules_options;

pub use vue_vet_core::{
  FileId, ScriptBlockFacts, ScriptCallFact, ScriptFacts, ScriptImportFact, ScriptKind, SfcFacts,
  SourceSpan, TemplateElementFact, TemplateFacts,
};
pub use vue_vet_reactivity::{ModuleSource, TraceModulesOptions};

pub fn trace_opts_workers(max_workers: usize) -> TraceModulesOptions {
  let mut options = default_trace_modules_options();
  options.max_workers = max_workers;
  options
}

pub static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

pub struct TempProject {
  root: PathBuf,
}

impl TempProject {
  #[expect(clippy::panic, reason = "test setup failures must fail the unit test")]
  pub fn new(name: &str) -> Self {
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

  pub fn root(&self) -> &Path {
    &self.root
  }

  #[expect(clippy::panic, reason = "test setup failures must fail the unit test")]
  pub fn write(&self, relative: &str, contents: &str) {
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

pub fn span(offset: usize) -> SourceSpan {
  SourceSpan { offset, length: 1, line: 1, column: offset.saturating_add(1) }
}

pub fn file(
  path: &str,
  imports: &[(&str, &str)],
  tags: &[&str],
  calls: &[&str],
) -> ProjectFile {
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
      top_level_await_ends: Vec::new(),
      operands: Vec::new(),
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
        has_accessible_name_ancestor: false,
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

pub fn empty_graph() -> std::sync::Arc<vue_vet_core::ReactivityGraph> {
  std::sync::Arc::new(vue_vet_core::ReactivityGraph::default())
}

pub fn write_setup_sfc(
  project: &TempProject,
  path: &str,
  script: &str,
  template: &str,
) -> (String, usize) {
  let prefix = "<script setup lang=\"ts\">\n";
  let sfc = format!("{prefix}{script}</script>\n{template}");
  project.write(path, &sfc);
  (sfc, prefix.len())
}

pub fn standalone_ts(path: &str, source: &str) -> ProjectFile {
  ProjectFile {
    path: path.into(),
    source_len: source.len(),
    facts: SfcFacts { template: TemplateFacts::default(), script: ScriptFacts::default() }.into(),
    module_source: Some(ModuleSource::standalone(path, source, "ts", ScriptKind::Script)),
    ordinary_module_source: None,
  }
}

pub fn setup_sfc_file(
  path: &str,
  script: &str,
  sfc: String,
  script_offset: usize,
  imports: &[(&str, &str, &str)],
  calls: &[(&str, Option<&str>)],
  expressions: Vec<vue_vet_core::TemplateExpressionFact>,
) -> ProjectFile {
  ProjectFile {
    path: path.into(),
    source_len: sfc.len(),
    facts: SfcFacts {
      template: TemplateFacts { elements: Vec::new(), expressions },
      script: ScriptFacts {
        blocks: vec![ScriptBlockFacts {
          kind: ScriptKind::Setup,
          language: "ts".into(),
          imports: imports
            .iter()
            .enumerate()
            .map(|(index, (source, imported, local))| ScriptImportFact {
              source: (*source).into(),
              imported: (*imported).into(),
              local: (*local).into(),
              span: span(index),
            })
            .collect(),
          bindings: Vec::new(),
          calls: calls
            .iter()
            .enumerate()
            .map(|(index, (callee, assigned_to))| ScriptCallFact {
              callee: (*callee).into(),
              assigned_to: assigned_to.map(str::to_string),
              resolved_import: None,
              argument_identifiers: Vec::new(),
              span: span(index.saturating_add(1)),
            })
            .collect(),
          member_writes: Vec::new(),
          destructures: Vec::new(),
          top_level_await_ends: Vec::new(),
          operands: Vec::new(),
          reactivity_graph: empty_graph(),
        }],
      },
    }
    .into(),
    module_source: Some(ModuleSource::sfc_script(
      path,
      script,
      "ts",
      ScriptKind::Setup,
      script_offset,
      sfc,
    )),
    ordinary_module_source: None,
  }
}

pub fn write_vueuse_core(project: &TempProject, export: &str, js: &str, dts: &str) {
  project.write(
    "node_modules/@vueuse/core/package.json",
    r#"{"name":"@vueuse/core","version":"1.0.0","types":"./index.d.ts","exports":{".":{"types":"./index.d.ts","import":"./index.js","default":"./index.js"}}}"#,
  );
  project.write(
    "node_modules/@vueuse/core/index.js",
    &format!("export {{ {export} }} from './{export}.js'\n"),
  );
  project.write(
    "node_modules/@vueuse/core/index.d.ts",
    &format!("export {{ {export} }} from './{export}'\n"),
  );
  project.write(&format!("node_modules/@vueuse/core/{export}.js"), js);
  project.write(&format!("node_modules/@vueuse/core/{export}.d.ts"), dts);
}

pub fn write_vite_auto_import(project: &TempProject, specifier: &str, name: &str) {
  project.write(
    "auto-imports.d.ts",
    &format!(
      "export {{}}\n\
declare global {{\n\
  const {name}: typeof import('{specifier}')['{name}']\n\
}}\n"
    ),
  );
}

pub fn materialize(project: &TempProject, files: &[ProjectFile]) {
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

pub fn write_color_mode_package(project: &TempProject) {
  project.write(
      "node_modules/@nuxtjs/color-mode/package.json",
      r#"{"name":"@nuxtjs/color-mode","version":"1.0.0","exports":{"./dist/runtime/composables":{"types":"./dist/runtime/composables.d.ts","import":"./dist/runtime/composables.js","default":"./dist/runtime/composables.js"}}}"#,
    );
  project.write(
    "node_modules/@nuxtjs/color-mode/dist/runtime/types.d.ts",
    "export interface ColorModeInstance {\n\
  preference: string;\n\
  value: string;\n\
  unknown: boolean;\n\
  forced: boolean;\n\
}\n",
  );
  project.write(
    "node_modules/@nuxtjs/color-mode/dist/runtime/composables.d.ts",
    "import type { ColorModeInstance } from './types.js';\n\
export declare const useColorMode: () => ColorModeInstance;\n",
  );
  project.write(
    "node_modules/@nuxtjs/color-mode/dist/runtime/composables.js",
    "import { useState } from '#imports';\n\
export const useColorMode = () => {\n\
  return useState('color-mode').value;\n\
};\n",
  );
}

pub fn color_mode_consumer_graph(project: &TempProject) -> ProjectGraph {
  let script = "import { watch } from 'vue'\n\
const colorMode = useColorMode()\n\
watch(() => colorMode.value, () => {})\n";
  let (sfc, script_offset) = write_setup_sfc(
    project,
    "components/ViewportDemo.client.vue",
    script,
    "<template><p>ok</p></template>\n",
  );
  let consumer = setup_sfc_file(
    "components/ViewportDemo.client.vue",
    script,
    sfc,
    script_offset,
    &[("vue", "watch", "watch")],
    &[("useColorMode", Some("colorMode"))],
    Vec::new(),
  );

  build_project_graph(project.root(), &[consumer])
}

pub fn assert_color_mode_reactive_watch(graph: &ProjectGraph) {
  assert!(
    graph.reactivity_error.is_none(),
    "bare useColorMode tracing must succeed: {:?}",
    graph.reactivity_error
  );
  let demo =
    graph.module_reactivity.iter().find(|module| module.id == "components/ViewportDemo.client.vue");
  assert!(
    demo.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| {
        binding.name == "colorMode" && binding.kind == vue_vet_core::ReactiveBindingKind::Reactive
      }) && module.graph.scopes.iter().any(|scope| {
        scope.kind == vue_vet_core::TrackingScopeKind::WatchSources
          && scope
            .reads
            .iter()
            .any(|read| read.binding == "colorMode" && read.property.as_deref() == Some("value"))
          && scope.uncertain_accesses.is_empty()
      })
    }),
    "bare Nuxt useColorMode must seed Reactive watch without uncertain; got {:?}",
    demo.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}
