use super::helpers::*;

#[test]
fn external_package_factory_ref_return_seeds_computed() {
  let project = TempProject::new("vueuse-factory");
  write_vueuse_core(
    &project,
    "useMediaQuery",
    "export function useMediaQuery() { return { value: false } }\n",
    "import type { Ref } from 'vue'\nexport declare function useMediaQuery(query: string): Ref<boolean>\n",
  );

  let script = "import { computed } from 'vue'\n\
import { useMediaQuery } from '@vueuse/core'\n\
const isCoarsePointer = useMediaQuery('(pointer: coarse)')\n\
const hint = computed(() => (isCoarsePointer.value ? 'a' : 'b'))\n";
  let (sfc, script_offset) = write_setup_sfc(
    &project,
    "components/ViewportDemo.vue",
    script,
    "<template><p>{{ hint }}</p></template>\n",
  );
  let consumer = setup_sfc_file(
    "components/ViewportDemo.vue",
    script,
    sfc,
    script_offset,
    &[("vue", "computed", "computed"), ("@vueuse/core", "useMediaQuery", "useMediaQuery")],
    &[],
    Vec::new(),
  );

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
  write_vueuse_core(
    &project,
    "useElementSize",
    "export function useElementSize() { return { width: { value: 0 }, height: { value: 0 }, stop() {} } }\n",
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
  let (sfc, script_offset) = write_setup_sfc(
    &project,
    "components/ViewportDemo.client.vue",
    script,
    "<template><p>ok</p></template>\n",
  );
  let consumer = setup_sfc_file(
    "components/ViewportDemo.client.vue",
    script,
    sfc,
    script_offset,
    &[("vue", "watch", "watch"), ("@vueuse/core", "useElementSize", "useElementSize")],
    &[],
    Vec::new(),
  );

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

/// Same as Vite auto-import seeding, but the composable is **not** in the scanned
/// file set — only reached via `ExternalSummaryLoad` (single-file / IDE path).
#[test]
fn bare_vite_auto_imports_external_spread_seeds_is_loading() {
  let project = TempProject::new("vite-auto-imports-external");
  let producer_source = "import { computed, ref, watch } from 'vue'\n\
export function useTableQuery(tableQuery) {\n\
  const page = ref(1)\n\
  const queryResult = tableQuery()\n\
  const list = computed(() => queryResult.data.value?.records || [])\n\
  watch(() => queryResult.isSuccess.value && !queryResult.isFetching.value, () => {})\n\
  return { page, list, ...queryResult }\n\
}\n";
  project.write("src/composables/useTable.ts", producer_source);
  write_vite_auto_import(&project, "./src/composables/useTable", "useTableQuery");

  let script = "import { computed } from 'vue'\n\
const { list: rows, isLoading: queryLoading } = useTableQuery(() => ({\n\
  data: { value: { records: [] } },\n\
  isSuccess: { value: true },\n\
  isFetching: { value: false },\n\
  isLoading: { value: false },\n\
}))\n\
const isLoading = computed(() => queryLoading.value)\n";
  let (sfc, script_offset) =
    write_setup_sfc(&project, "pages/index.vue", script, "<template><p>ok</p></template>\n");

  // Intentionally omit producer from ProjectFile list — external seed only.
  let consumer = setup_sfc_file(
    "pages/index.vue",
    script,
    sfc,
    script_offset,
    &[("vue", "computed", "computed")],
    &[("useTableQuery", None)],
    Vec::new(),
  );

  let graph = build_project_graph(project.root(), &[consumer]);
  assert!(
    graph.reactivity_error.is_none(),
    "external Vite auto-import tracing must succeed: {:?}",
    graph.reactivity_error
  );
  let page = graph.module_reactivity.iter().find(|module| module.id == "pages/index.vue");
  assert!(
    page.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| {
        binding.name == "queryLoading" && binding.kind == vue_vet_core::ReactiveBindingKind::Ref
      }) && module.graph.scopes.iter().any(|scope| {
        scope.kind == vue_vet_core::TrackingScopeKind::Computed
          && scope
            .reads
            .iter()
            .any(|read| read.binding == "queryLoading" && read.property.as_deref() == Some("value"))
          && scope.uncertain_accesses.is_empty()
      })
    }),
    "external ...queryResult must seed isLoading via auto-imports; got {:?}",
    page.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

/// Vite unplugin-auto-import writes root `auto-imports.d.ts` with
/// `typeof import('…')['name']` — seed bare composable destructure like Nuxt maps.
#[test]
fn bare_vite_auto_imports_dts_seeds_composable_destructure() {
  let project = TempProject::new("vite-auto-imports");
  let producer_source = "import { computed } from 'vue'\n\
export function useTableQuery() {\n\
  const list = computed(() => [] as number[])\n\
  return { list }\n\
}\n";
  project.write("src/composables/useTable.ts", producer_source);
  write_vite_auto_import(&project, "./src/composables/useTable", "useTableQuery");

  let script = "import { computed } from 'vue'\n\
const { list: rows } = useTableQuery()\n\
const all = computed(() => rows.value.length)\n";
  let (sfc, script_offset) =
    write_setup_sfc(&project, "pages/index.vue", script, "<template><p>ok</p></template>\n");

  let producer = standalone_ts("src/composables/useTable.ts", producer_source);
  let consumer = setup_sfc_file(
    "pages/index.vue",
    script,
    sfc,
    script_offset,
    &[("vue", "computed", "computed")],
    &[("useTableQuery", None)],
    Vec::new(),
  );

  let graph = build_project_graph(project.root(), &[producer, consumer]);
  assert!(
    graph.reactivity_error.is_none(),
    "Vite auto-import tracing must succeed: {:?}",
    graph.reactivity_error
  );
  let page = graph.module_reactivity.iter().find(|module| module.id == "pages/index.vue");
  assert!(
    page.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| {
        binding.name == "rows" && binding.kind == vue_vet_core::ReactiveBindingKind::Computed
      }) && module.graph.scopes.iter().any(|scope| {
        scope.kind == vue_vet_core::TrackingScopeKind::Computed
          && scope
            .reads
            .iter()
            .any(|read| read.binding == "rows" && read.property.as_deref() == Some("value"))
          && scope.uncertain_accesses.is_empty()
      })
    }),
    "bare Vite useTableQuery destructure must seed Computed without uncertain; got {:?}",
    page.map(|module| (&module.graph.bindings, &module.graph.scopes))
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
    module_source: Some(std::sync::Arc::new(ModuleSource::standalone(
      "consumer.ts",
      "import { heavy } from 'heavy-lib';\nexport const n = heavy();\n",
      "ts",
      ScriptKind::Script,
    ))),
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
