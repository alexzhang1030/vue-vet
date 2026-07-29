use crate::resolve::normalized_path;
use crate::{
  EdgeKind, PROJECT_RULE_IDS, ProjectContext, ProjectFile, ProjectGraph, ProjectGraphState,
  build_project_graph, build_project_graph_incremental_with_options, project_context_from_inputs,
};
use std::{
  collections::BTreeSet,
  fs,
  path::{Path, PathBuf},
  sync::atomic::{AtomicUsize, Ordering},
};

use vue_vet_core::{
  FileId, ScriptBlockFacts, ScriptCallFact, ScriptFacts, ScriptImportFact, ScriptKind, SfcFacts,
  SourceSpan, TemplateElementFact, TemplateFacts,
};
use vue_vet_reactivity::{ModuleSource, TraceModulesOptions};

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
    None,
  );
  assert_eq!(state.last_stats().structural_files_rebuilt, 2);

  let _unchanged = build_project_graph_incremental_with_options(
    project.root(),
    &[first.clone(), second],
    TraceModulesOptions { max_workers: 1, ..Default::default() },
    &context,
    &mut state,
    None,
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
    None,
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
  let ids =
    graph.diagnostics.iter().map(|diagnostic| diagnostic.rule_id.as_str()).collect::<BTreeSet<_>>();
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
          top_level_await_ends: Vec::new(),
          operands: Vec::new(),
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

#[test]
fn external_package_factory_ref_return_seeds_computed() {
  let project = TempProject::new("vueuse-factory");
  project.write(
      "node_modules/@vueuse/core/package.json",
      r#"{"name":"@vueuse/core","version":"1.0.0","types":"./index.d.ts","exports":{".":{"types":"./index.d.ts","import":"./index.js","default":"./index.js"}}}"#,
    );
  project.write(
    "node_modules/@vueuse/core/index.js",
    "export { useMediaQuery } from './useMediaQuery.js'\n",
  );
  project.write(
    "node_modules/@vueuse/core/index.d.ts",
    "export { useMediaQuery } from './useMediaQuery'\n",
  );
  project.write(
    "node_modules/@vueuse/core/useMediaQuery.js",
    "export function useMediaQuery() { return { value: false } }\n",
  );
  project.write(
      "node_modules/@vueuse/core/useMediaQuery.d.ts",
      "import type { Ref } from 'vue'\nexport declare function useMediaQuery(query: string): Ref<boolean>\n",
    );

  let script = "import { computed } from 'vue'\n\
import { useMediaQuery } from '@vueuse/core'\n\
const isCoarsePointer = useMediaQuery('(pointer: coarse)')\n\
const hint = computed(() => (isCoarsePointer.value ? 'a' : 'b'))\n";
  let prefix = "<script setup lang=\"ts\">\n";
  let sfc = format!("{prefix}{script}</script>\n<template><p>{{{{ hint }}}}</p></template>\n");
  let script_offset = prefix.len();
  project.write("components/ViewportDemo.vue", &sfc);

  let consumer = ProjectFile {
    path: "components/ViewportDemo.vue".into(),
    source_len: sfc.len(),
    facts: SfcFacts {
      template: TemplateFacts { elements: Vec::new(), expressions: Vec::new() },
      script: ScriptFacts {
        blocks: vec![ScriptBlockFacts {
          kind: ScriptKind::Setup,
          language: "ts".into(),
          imports: vec![
            ScriptImportFact {
              source: "vue".into(),
              imported: "computed".into(),
              local: "computed".into(),
              span: span(0),
            },
            ScriptImportFact {
              source: "@vueuse/core".into(),
              imported: "useMediaQuery".into(),
              local: "useMediaQuery".into(),
              span: span(1),
            },
          ],
          bindings: Vec::new(),
          calls: Vec::new(),
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
      "components/ViewportDemo.vue",
      script,
      "ts",
      ScriptKind::Setup,
      script_offset,
      sfc,
    )),
    ordinary_module_source: None,
  };

  let graph = build_project_graph(project.root(), &[consumer]);
  assert!(
    graph.reactivity_error.is_none(),
    "external factory tracing must succeed: {:?}",
    graph.reactivity_error
  );
  let demo =
    graph.module_reactivity.iter().find(|module| module.id == "components/ViewportDemo.vue");
  assert!(
    demo.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| {
        binding.name == "isCoarsePointer" && binding.kind == vue_vet_core::ReactiveBindingKind::Ref
      }) && module.graph.scopes.iter().any(|scope| {
        scope.kind == vue_vet_core::TrackingScopeKind::Computed
          && scope.reads.iter().any(|read| {
            read.binding == "isCoarsePointer" && read.property.as_deref() == Some("value")
          })
      })
    }),
    "external @vueuse-style Ref factory must seed computed dependency; got {:?}",
    demo.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn external_package_composable_object_return_seeds_destructure_watch() {
  let project = TempProject::new("vueuse-element-size");
  project.write(
      "node_modules/@vueuse/core/package.json",
      r#"{"name":"@vueuse/core","version":"1.0.0","types":"./index.d.ts","exports":{".":{"types":"./index.d.ts","import":"./index.js","default":"./index.js"}}}"#,
    );
  project.write(
    "node_modules/@vueuse/core/index.js",
    "export { useElementSize } from './useElementSize.js'\n",
  );
  project.write(
    "node_modules/@vueuse/core/index.d.ts",
    "export { useElementSize } from './useElementSize'\n",
  );
  project.write(
      "node_modules/@vueuse/core/useElementSize.js",
      "export function useElementSize() { return { width: { value: 0 }, height: { value: 0 }, stop() {} } }\n",
    );
  project.write(
    "node_modules/@vueuse/core/useElementSize.d.ts",
    "import type { Ref } from 'vue'\n\
export declare function useElementSize(): {\n\
  width: Ref<number>\n\
  height: Ref<number>\n\
  stop: () => void\n\
}\n",
  );

  let script = "import { watch } from 'vue'\n\
import { useElementSize } from '@vueuse/core'\n\
const { width: hostWidth, height: hostHeight } = useElementSize()\n\
watch([hostWidth, hostHeight], () => {})\n";
  let prefix = "<script setup lang=\"ts\">\n";
  let sfc = format!("{prefix}{script}</script>\n<template><p>ok</p></template>\n");
  let script_offset = prefix.len();
  project.write("components/ViewportDemo.client.vue", &sfc);

  let consumer = ProjectFile {
    path: "components/ViewportDemo.client.vue".into(),
    source_len: sfc.len(),
    facts: SfcFacts {
      template: TemplateFacts { elements: Vec::new(), expressions: Vec::new() },
      script: ScriptFacts {
        blocks: vec![ScriptBlockFacts {
          kind: ScriptKind::Setup,
          language: "ts".into(),
          imports: vec![
            ScriptImportFact {
              source: "vue".into(),
              imported: "watch".into(),
              local: "watch".into(),
              span: span(0),
            },
            ScriptImportFact {
              source: "@vueuse/core".into(),
              imported: "useElementSize".into(),
              local: "useElementSize".into(),
              span: span(1),
            },
          ],
          bindings: Vec::new(),
          calls: Vec::new(),
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
      "components/ViewportDemo.client.vue",
      script,
      "ts",
      ScriptKind::Setup,
      script_offset,
      sfc,
    )),
    ordinary_module_source: None,
  };

  let graph = build_project_graph(project.root(), &[consumer]);
  assert!(
    graph.reactivity_error.is_none(),
    "external object-bag tracing must succeed: {:?}",
    graph.reactivity_error
  );
  let demo =
    graph.module_reactivity.iter().find(|module| module.id == "components/ViewportDemo.client.vue");
  assert!(
    demo.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| {
        binding.name == "hostWidth" && binding.kind == vue_vet_core::ReactiveBindingKind::Ref
      }) && module.graph.bindings.iter().any(|binding| {
        binding.name == "hostHeight" && binding.kind == vue_vet_core::ReactiveBindingKind::Ref
      }) && module.graph.scopes.iter().any(|scope| {
        scope.kind == vue_vet_core::TrackingScopeKind::WatchSources
          && scope.reads.iter().any(|read| read.binding == "hostWidth")
          && scope.reads.iter().any(|read| read.binding == "hostHeight")
          && scope.uncertain_accesses.is_empty()
      })
    }),
    "external useElementSize object bag must seed renamed destructure watch; got {:?}",
    demo.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn bare_nuxt_imports_dts_color_mode_seeds_reactive_watch() {
  let project = TempProject::new("nuxt-color-mode");
  write_color_mode_package(&project);
  project.write(
    ".nuxt/imports.d.ts",
    "export { useColorMode } from '../node_modules/@nuxtjs/color-mode/dist/runtime/composables';\n",
  );

  let graph = color_mode_consumer_graph(&project);
  assert_color_mode_reactive_watch(&graph);
}

/// Real Nuxt emits both maps; types/imports uses one extra `../`. Sorted
/// cache inputs used to overwrite the re-export with the types specifier and
/// still resolve from `.nuxt/imports.d.ts` → Unresolved → colorMode FP.
#[test]
fn bare_nuxt_types_imports_dts_does_not_break_color_mode_seed() {
  let project = TempProject::new("nuxt-color-mode-types");
  write_color_mode_package(&project);
  let imports =
    "export { useColorMode } from '../node_modules/@nuxtjs/color-mode/dist/runtime/composables';\n";
  let types = "export {}\n\
declare global {\n\
  const useColorMode: typeof import('../../node_modules/@nuxtjs/color-mode/dist/runtime/composables').useColorMode\n\
}\n\
export {}\n";
  project.write(".nuxt/imports.d.ts", imports);
  project.write(".nuxt/types/imports.d.ts", types);

  // Mimic discovery: sorted cache_inputs process types after imports.
  let known = FileId::from("components/ViewportDemo.client.vue");
  let context = project_context_from_inputs(
    project.root(),
    [&known],
    [(".nuxt/imports.d.ts", imports.as_bytes()), (".nuxt/types/imports.d.ts", types.as_bytes())],
    1,
  );
  assert_eq!(
    context.nuxt_import_names.get("useColorMode").map(|target| target.importer.as_str()),
    Some(".nuxt/imports.d.ts"),
    "prefer imports.d.ts re-export over types/imports overwrite"
  );
  assert_eq!(
    context.nuxt_import_names.get("useColorMode").map(|target| target.specifier.as_str()),
    Some("../node_modules/@nuxtjs/color-mode/dist/runtime/composables")
  );

  let graph = color_mode_consumer_graph(&project);
  assert_color_mode_reactive_watch(&graph);
}

/// When only the types map exists, resolve from that importer (extra `../`).
#[test]
fn bare_nuxt_types_imports_only_resolves_from_types_importer() {
  let project = TempProject::new("nuxt-color-mode-types-only");
  write_color_mode_package(&project);
  project.write(
      ".nuxt/types/imports.d.ts",
      "export {}\n\
declare global {\n\
  const useColorMode: typeof import('../../node_modules/@nuxtjs/color-mode/dist/runtime/composables').useColorMode\n\
}\n",
    );

  let graph = color_mode_consumer_graph(&project);
  assert_color_mode_reactive_watch(&graph);
}

fn write_color_mode_package(project: &TempProject) {
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

fn color_mode_consumer_graph(project: &TempProject) -> ProjectGraph {
  let script = "import { watch } from 'vue'\n\
const colorMode = useColorMode()\n\
watch(() => colorMode.value, () => {})\n";
  let prefix = "<script setup lang=\"ts\">\n";
  let sfc = format!("{prefix}{script}</script>\n<template><p>ok</p></template>\n");
  let script_offset = prefix.len();
  project.write("components/ViewportDemo.client.vue", &sfc);

  let consumer = ProjectFile {
    path: "components/ViewportDemo.client.vue".into(),
    source_len: sfc.len(),
    facts: SfcFacts {
      template: TemplateFacts { elements: Vec::new(), expressions: Vec::new() },
      script: ScriptFacts {
        blocks: vec![ScriptBlockFacts {
          kind: ScriptKind::Setup,
          language: "ts".into(),
          imports: vec![ScriptImportFact {
            source: "vue".into(),
            imported: "watch".into(),
            local: "watch".into(),
            span: span(0),
          }],
          bindings: Vec::new(),
          calls: vec![ScriptCallFact {
            callee: "useColorMode".into(),
            assigned_to: Some("colorMode".into()),
            resolved_import: None,
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
    module_source: Some(ModuleSource::sfc_script(
      "components/ViewportDemo.client.vue",
      script,
      "ts",
      ScriptKind::Setup,
      script_offset,
      sfc,
    )),
    ordinary_module_source: None,
  };

  build_project_graph(project.root(), &[consumer])
}

fn assert_color_mode_reactive_watch(graph: &ProjectGraph) {
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

#[test]
fn non_provisional_external_dts_skips_huge_companion_js() {
  let project = TempProject::new("huge-companion");
  project.write(
    "node_modules/heavy-lib/package.json",
    r#"{"name":"heavy-lib","version":"1.0.0","types":"index.d.ts","main":"index.js"}"#,
  );
  project.write("node_modules/heavy-lib/index.d.ts", "export declare function heavy(): number;\n");
  // ~3 MiB pad — parsing this as a companion would dominate scan time (0.1.16 regression
  // on `import … from 'typescript'`). Non-provisional .d.ts must skip it.
  let mut huge = String::from("export function heavy() { return 1 }\n");
  huge.push_str(&"// pad\n".repeat(400_000));
  project.write("node_modules/heavy-lib/index.js", &huge);
  project.write("consumer.ts", "import { heavy } from 'heavy-lib';\nexport const n = heavy();\n");

  let consumer = ProjectFile {
    path: "consumer.ts".into(),
    source_len: 64,
    facts: SfcFacts {
      template: TemplateFacts { elements: Vec::new(), expressions: Vec::new() },
      script: ScriptFacts {
        blocks: vec![ScriptBlockFacts {
          kind: ScriptKind::Script,
          language: "ts".into(),
          imports: vec![ScriptImportFact {
            source: "heavy-lib".into(),
            imported: "heavy".into(),
            local: "heavy".into(),
            span: span(0),
          }],
          bindings: Vec::new(),
          calls: Vec::new(),
          member_writes: Vec::new(),
          destructures: Vec::new(),
          top_level_await_ends: Vec::new(),
          operands: Vec::new(),
          reactivity_graph: std::sync::Arc::new(vue_vet_core::ReactivityGraph::default()),
        }],
      },
    }
    .into(),
    module_source: Some(ModuleSource::standalone(
      "consumer.ts",
      "import { heavy } from 'heavy-lib';\nexport const n = heavy();\n",
      "ts",
      ScriptKind::Script,
    )),
    ordinary_module_source: None,
  };

  let started = std::time::Instant::now();
  let graph = build_project_graph(project.root(), &[consumer]);
  let elapsed = started.elapsed();
  assert!(graph.reactivity_error.is_none(), "graph must succeed: {:?}", graph.reactivity_error);
  assert!(
    elapsed.as_secs_f64() < 2.0,
    "non-provisional external .d.ts must not parse multi-MB companion .js; elapsed={elapsed:?}"
  );
}
