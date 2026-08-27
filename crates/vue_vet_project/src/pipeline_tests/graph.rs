use super::helpers::*;

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
    &trace_opts_workers(1),
    &context,
    &mut state,
    None,
  );
  assert_eq!(state.last_stats().structural_files_rebuilt, 2);

  let _unchanged = build_project_graph_incremental_with_options(
    project.root(),
    &[first.clone(), second],
    &trace_opts_workers(1),
    &context,
    &mut state,
    None,
  );
  assert_eq!(state.last_stats().structural_files_reused, 2);
  assert_eq!(state.last_stats().structural_files_rebuilt, 0);
  assert!(
    !state.last_stats().export_resolve_ran,
    "unchanged linking surface must skip export resolve"
  );
  assert!(
    state.last_export_closure.is_empty(),
    "warm linking cache hit leaves export_closure empty"
  );

  let changed = file("src/b.vue", &[], &["section"], &[]);
  let _changed = build_project_graph_incremental_with_options(
    project.root(),
    &[first, changed],
    &trace_opts_workers(1),
    &context,
    &mut state,
    None,
  );
  assert_eq!(state.last_stats().structural_files_reused, 1);
  assert_eq!(state.last_stats().structural_files_rebuilt, 1);
}

#[test]
fn incremental_leaf_edit_visits_one_module_summary() {
  let project = TempProject::new("leaf-subset");
  let first = standalone_ts("src/a.ts", "import { ref } from 'vue'; export const a = ref(1);");
  let second = standalone_ts("src/b.ts", "import { ref } from 'vue'; export const b = ref(2);");
  let third = standalone_ts("src/c.ts", "import { ref } from 'vue'; export const c = ref(3);");
  materialize(&project, &[first.clone(), second.clone(), third.clone()]);
  let mut state = ProjectGraphState::default();
  let context = ProjectContext { revision: 1, ..ProjectContext::default() };
  let initial = build_project_graph_incremental_with_options(
    project.root(),
    &[first.clone(), second, third.clone()],
    &trace_opts_workers(1),
    &context,
    &mut state,
    None,
  );
  assert_eq!(initial.module_reactivity.len(), 3);
  assert_eq!(state.last_stats().module_summaries_visited, 3);

  let edited = standalone_ts("src/b.ts", "import { ref } from 'vue'; export const b = ref(20);");
  let after = build_project_graph_incremental_with_options(
    project.root(),
    &[first, edited, third],
    &trace_opts_workers(1),
    &context,
    &mut state,
    None,
  );
  assert_eq!(after.module_reactivity.len(), 3);
  assert_eq!(state.last_stats().module_summaries_visited, 1);
  assert_eq!(state.last_stats().module_graphs_reused, 2);
  assert!(
    !state.last_stats().export_resolve_ran,
    "literal-only leaf edit must skip export resolve"
  );
  for id in ["src/a.ts", "src/c.ts"] {
    let before = initial
      .module_reactivity
      .iter()
      .find(|module| module.id.as_str() == id)
      .map(|module| std::sync::Arc::as_ptr(&module.graph));
    let kept = after
      .module_reactivity
      .iter()
      .find(|module| module.id.as_str() == id)
      .map(|module| std::sync::Arc::as_ptr(&module.graph));
    assert_eq!(before, kept, "unchanged module {id} must keep its layered graph Arc");
  }
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
fn named_barrel_imports_mark_component_name_targets_used() {
  let project = TempProject::new("barrel-named");
  // Import local name matches the component convention name (barrel path alias).
  let page = file("pages/index.vue", &[("@components", "PageContainer")], &[], &[]);
  let component = file("components/PageContainer/index.tsx", &[], &[], &[]);
  let story = file("components/PageContainer/PageContainer.story.vue", &[], &[], &[]);
  project.write("components/PageContainer/index.tsx", "export const PageContainer = {}\n");
  project
    .write("components/PageContainer/PageContainer.story.vue", "<template><div /></template>\n");
  let graph = build_project_graph(project.root(), &[page, component, story]);
  assert!(
    graph.diagnostics.iter().all(|diagnostic| diagnostic.rule_id != PROJECT_RULE_IDS[1]),
    "named import + story files must not report unused-component: {:?}",
    graph.diagnostics
  );
  assert!(
    graph
      .edges
      .iter()
      .any(|edge| { edge.kind == EdgeKind::ComponentUsage && edge.to.contains("PageContainer") }),
    "named import must create a ComponentUsage edge by component name: {:?}",
    graph.edges
  );
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
  project.write("composables/useField.ts", producer_source);
  let (sfc, script_offset) = write_setup_sfc(
    &project,
    "pages/index.vue",
    consumer_script,
    "<template>\n  <p>{{ title }}</p>\n</template>\n",
  );
  let producer = standalone_ts("composables/useField.ts", producer_source);
  let consumer = setup_sfc_file(
    "pages/index.vue",
    consumer_script,
    sfc,
    script_offset,
    &[("../composables/useField", "useField", "useField")],
    &[],
    vec![vue_vet_core::TemplateExpressionFact {
      surface: "interpolation".into(),
      expression: "title".into(),
      span: span(script_offset.saturating_add(consumer_script.len().saturating_add(40))),
      identifiers: Some(vec!["title".into()]),
    }],
  );
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
