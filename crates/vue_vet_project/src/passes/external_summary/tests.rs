use std::{collections::BTreeSet, path::PathBuf};

use vue_vet_core::ModuleId;
use vue_vet_reactivity::{ModuleSource, trace_modules};

use super::{paths::node_modules_package_key, *};
use crate::resolve::{ProjectResolver, Resolution};

#[test]
fn duplicate_type_import_enrichment_falls_back_to_raw_dts() {
  let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-vue-query-enrich");
  let _ = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(root.join("node_modules/@tanstack/vue-query/build/modern")).unwrap();
  std::fs::create_dir_all(root.join("src")).unwrap();
  std::fs::write(root.join("package.json"), r#"{"name":"vue-query-enrich"}"#).unwrap();
  // Barrel that import-types two files both declaring `QueryClient` — concat fails.
  std::fs::write(
    root.join("node_modules/@tanstack/vue-query/package.json"),
    r#"{"name":"@tanstack/vue-query","types":"./build/modern/index.d.ts","exports":{".":{"types":"./build/modern/index.d.ts","import":"./build/modern/index.js"}}}"#,
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@tanstack/vue-query/build/modern/index.js"),
    "export { useQuery } from './queryClient.js'\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@tanstack/vue-query/build/modern/a.d.ts"),
    "export declare class QueryClient {}\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@tanstack/vue-query/build/modern/b.d.ts"),
    "export declare class QueryClient {}\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@tanstack/vue-query/build/modern/queryClient.js"),
    "export function useQuery() { return {} }\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@tanstack/vue-query/build/modern/queryClient.d.ts"),
    "import type { Ref } from 'vue'\n\
     type Bag = { [K in 'data' | 'isLoading']: Ref<unknown> }\n\
     declare function useQuery(): Bag\n\
     export { useQuery as u }\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@tanstack/vue-query/build/modern/index.d.ts"),
    "import type { QueryClient as A } from './a.js'\n\
     import type { QueryClient as B } from './b.js'\n\
     export { u as useQuery } from './queryClient.js'\n\
     export type { A, B }\n",
  )
  .unwrap();
  // Companions so `import type … from './a.js'` resolve during enrich attempts.
  std::fs::write(
    root.join("node_modules/@tanstack/vue-query/build/modern/a.js"),
    "export class QueryClient {}\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@tanstack/vue-query/build/modern/b.js"),
    "export class QueryClient {}\n",
  )
  .unwrap();
  std::fs::write(
    root.join("src/direct.ts"),
    "import { computed } from 'vue'\n\
     import { useQuery } from '@tanstack/vue-query'\n\
     const { data, isLoading } = useQuery()\n\
     export const a = computed(() => data.value)\n\
     export const b = computed(() => isLoading.value)\n",
  )
  .unwrap();

  let resolver = ProjectResolver::new(&root);
  let known = BTreeSet::new();
  let Resolution::External { resolved_path: Some(path), .. } =
    resolver.resolve("src/direct.ts", "@tanstack/vue-query", &known)
  else {
    panic!("expected external resolve");
  };
  let roots = [ExternalReactivityRoot {
    from: ModuleId::from("src/direct.ts"),
    specifier: "@tanstack/vue-query".into(),
    resolved_path: path,
  }];
  let (sources, links) = ExternalSummaryLoadPass::run(&root, &resolver, &roots, None);
  assert!(!sources.is_empty(), "enriched duplicate imports must not drop the whole package");
  assert!(
    links.iter().any(|link| link.specifier.contains("queryClient")),
    "raw barrel must still follow leaf re-exports; links={links:?}"
  );
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn typeof_forward_follows_bare_package_import() {
  let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-typeof-bare");
  let _ = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(root.join("node_modules/@ui")).unwrap();
  std::fs::create_dir_all(root.join("node_modules/field-kit")).unwrap();
  std::fs::create_dir_all(root.join("src")).unwrap();
  std::fs::write(root.join("package.json"), r#"{"name":"typeof-bare"}"#).unwrap();
  std::fs::write(
    root.join("node_modules/field-kit/package.json"),
    r#"{"name":"field-kit","types":"./index.d.ts","exports":{".":{"types":"./index.d.ts","import":"./index.js"}}}"#,
  )
  .unwrap();
  std::fs::write(root.join("node_modules/field-kit/index.js"), "export {}\n").unwrap();
  std::fs::write(
    root.join("node_modules/field-kit/index.d.ts"),
    "import type { Ref } from 'vue'\n\
     export interface FieldListContext { fields: Ref<{ key: string }[]> }\n\
     export declare function useFieldList(): FieldListContext\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@ui/package.json"),
    r#"{"name":"@ui","types":"./index.d.ts","exports":{".":{"types":"./index.d.ts","import":"./index.js"}}}"#,
  )
  .unwrap();
  std::fs::write(root.join("node_modules/@ui/index.js"), "export {}\n").unwrap();
  std::fs::write(
    root.join("node_modules/@ui/index.d.ts"),
    "import { useFieldList } from 'field-kit'\n\
     export declare const useFormFieldList: typeof useFieldList\n",
  )
  .unwrap();
  std::fs::write(
    root.join("src/consumer.ts"),
    "import { computed } from 'vue'\n\
     import { useFormFieldList } from '@ui'\n\
     const ctx = useFormFieldList()\n\
     const keys = computed(() => ctx.fields.value.map((row) => row.key))\n",
  )
  .unwrap();

  let resolver = ProjectResolver::new(&root);
  let known = BTreeSet::new();
  let Resolution::External { resolved_path: Some(path), .. } =
    resolver.resolve("src/consumer.ts", "@ui", &known)
  else {
    panic!("expected external resolve");
  };
  let roots = [ExternalReactivityRoot {
    from: ModuleId::from("src/consumer.ts"),
    specifier: "@ui".into(),
    resolved_path: path,
  }];
  let ui_path = root.join("node_modules/@ui/index.d.ts");
  let Ok(ui_module) = prepare_standalone_module_source(
    ModuleId::from("ui"),
    std::fs::read_to_string(&ui_path).unwrap(),
    "d.ts",
  ) else {
    panic!("parse ui typeof alias");
  };
  let Some(ui_summary) = ui_module.module_summary() else {
    panic!("ui summary");
  };
  let typeof_sources = ui_summary.typeof_forward_sources();
  assert!(
    typeof_sources.contains("field-kit"),
    "ui alias must publish typeof forward source; got {typeof_sources:?}"
  );
  match resolver.resolve_from_absolute(&ui_path, "field-kit") {
    Resolution::External { resolved_path: Some(path), .. } => {
      assert!(
        path.ends_with("field-kit/index.d.ts") || path.ends_with("field-kit/index.js"),
        "unexpected field-kit path {path:?}"
      );
    }
    Resolution::External { resolved_path: None, package } => {
      panic!("quiet external for typeof target {package}")
    }
    Resolution::File(path) => panic!("unexpected project file resolve {path}"),
    Resolution::Unresolved => panic!("unresolved field-kit from {ui_path:?}"),
  }

  let (sources, links) = ExternalSummaryLoadPass::run(&root, &resolver, &roots, None);
  assert!(
    sources.iter().any(|module| module.id.as_str().contains("field-kit")),
    "typeof forward must load bare package; sources={:?}",
    sources.iter().map(|module| module.id.as_str()).collect::<Vec<_>>()
  );
  assert!(
    links.iter().any(|link| link.specifier == "field-kit"),
    "typeof forward must link bare package; links={links:?}"
  );
  let mut modules = sources;
  modules.push(ModuleSource::standalone(
    "src/consumer.ts",
    std::fs::read_to_string(root.join("src/consumer.ts")).unwrap(),
    "ts",
    vue_vet_core::ScriptKind::Script,
  ));
  let Ok(traced) = trace_modules(&modules, &links) else {
    panic!("trace typeof bare modules");
  };
  let consumer = traced.iter().find(|module| module.id.as_str() == "src/consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.composable_instances.contains_key("ctx")
        && module.graph.scopes.iter().any(|scope| {
          scope
            .reads
            .iter()
            .any(|read| read.binding == "fields" && read.property.as_deref() == Some("value"))
        })
    }),
    "typeof bare package must seed instance bag; got {:?}",
    consumer.map(|module| { (&module.graph.composable_instances, &module.graph.scopes) })
  );
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn export_star_follows_bare_package_reexport() {
  // Generic: entry package re-exports helpers via `export * from 'shared-kit'`.
  // Without following that bare star, `useTimer` never becomes Factory on the entry.
  let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-star-bare-pkg");
  let _ = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(root.join("node_modules/shared-kit")).unwrap();
  std::fs::create_dir_all(root.join("node_modules/entry-kit")).unwrap();
  std::fs::create_dir_all(root.join("src")).unwrap();
  std::fs::write(root.join("package.json"), r#"{"name":"star-bare-pkg"}"#).unwrap();
  std::fs::write(
    root.join("node_modules/shared-kit/package.json"),
    r#"{"name":"shared-kit","types":"./index.d.ts","exports":{".":{"types":"./index.d.ts","import":"./index.js"}}}"#,
  )
  .unwrap();
  std::fs::write(root.join("node_modules/shared-kit/index.js"), "export {}\n").unwrap();
  std::fs::write(
    root.join("node_modules/shared-kit/index.d.ts"),
    "import type { ComputedRef } from 'vue'\n\
     export declare function useTimer(ms?: number): ComputedRef<boolean>\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/entry-kit/package.json"),
    r#"{"name":"entry-kit","types":"./index.d.ts","exports":{".":{"types":"./index.d.ts","import":"./index.js"}}}"#,
  )
  .unwrap();
  std::fs::write(root.join("node_modules/entry-kit/index.js"), "export {}\n").unwrap();
  std::fs::write(root.join("node_modules/entry-kit/index.d.ts"), "export * from 'shared-kit'\n")
    .unwrap();
  std::fs::write(
    root.join("src/consumer.ts"),
    "import { computed } from 'vue'\n\
     import { useTimer } from 'entry-kit'\n\
     const done = useTimer(100)\n\
     const label = computed(() => (done.value ? 'yes' : 'no'))\n",
  )
  .unwrap();

  let resolver = ProjectResolver::new(&root);
  let known = BTreeSet::new();
  let Resolution::External { resolved_path: Some(path), .. } =
    resolver.resolve("src/consumer.ts", "entry-kit", &known)
  else {
    panic!("expected external resolve for entry-kit");
  };
  let roots = [ExternalReactivityRoot {
    from: ModuleId::from("src/consumer.ts"),
    specifier: "entry-kit".into(),
    resolved_path: path,
  }];
  let (sources, links) = ExternalSummaryLoadPass::run(&root, &resolver, &roots, None);
  assert!(
    sources.iter().any(|module| module.id.as_str().contains("shared-kit")),
    "export * bare package must load shared target; sources={:?}",
    sources.iter().map(|module| module.id.as_str()).collect::<Vec<_>>()
  );
  assert!(
    links.iter().any(|link| link.specifier == "shared-kit"),
    "export * bare package must link shared target; links={links:?}"
  );
  let mut modules = sources;
  modules.push(ModuleSource::standalone(
    "src/consumer.ts",
    std::fs::read_to_string(root.join("src/consumer.ts")).unwrap(),
    "ts",
    vue_vet_core::ScriptKind::Script,
  ));
  let Ok(traced) = trace_modules(&modules, &links) else {
    panic!("trace export-star bare modules");
  };
  let consumer = traced.iter().find(|module| module.id.as_str() == "src/consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| {
        binding.name == "done"
          && matches!(
            binding.kind,
            vue_vet_core::ReactiveBindingKind::Ref
              | vue_vet_core::ReactiveBindingKind::ShallowRef
              | vue_vet_core::ReactiveBindingKind::Computed
          )
      }) && (module.graph.edges.iter().any(|edge| edge.from == "label" && edge.to == "done")
        || module.graph.scopes.iter().any(|scope| {
          scope.binding.as_deref() == Some("label")
            && scope.reads.iter().any(|read| read.binding == "done")
        }))
    }),
    "export * bare reexport must seed Factory return; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.edges, &module.graph.scopes))
  );
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn options_callback_slots_follow_package_export_star_barrel() {
  let root =
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-options-callback-barrel");
  let _ = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(root.join("node_modules/@ui/dist/types/components/Form")).unwrap();
  std::fs::create_dir_all(root.join("src")).unwrap();
  std::fs::write(root.join("package.json"), r#"{"name":"options-callback-barrel"}"#).unwrap();
  std::fs::write(
    root.join("node_modules/@ui/package.json"),
    r#"{"name":"@ui","types":"./dist/types/index.d.ts","exports":{".":{"types":"./dist/types/index.d.ts","import":"./dist/index.js"}}}"#,
  )
  .unwrap();
  std::fs::write(root.join("node_modules/@ui/dist/index.js"), "export {}\n").unwrap();
  std::fs::write(
    root.join("node_modules/@ui/dist/types/index.d.ts"),
    "export * from './components'\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@ui/dist/types/components/index.d.ts"),
    "export * from './Form'\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@ui/dist/types/components/Form/index.d.ts"),
    "export { defineStdFormProps } from './utils'\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@ui/dist/types/components/Form/composables.d.ts"),
    "import type { Ref } from 'vue'\n\
     export interface StdFormContext { values: Ref<unknown>; form: unknown }\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@ui/dist/types/components/Form/types.d.ts"),
    "import { StdFormContext } from './composables'\n\
     export interface StdFormGlobalSetupContext extends StdFormContext { schema: unknown }\n\
     export type StdFormGlobalSetupFn = (ctx: StdFormGlobalSetupContext) => unknown\n\
     export interface StdFormProps<Setup extends StdFormGlobalSetupFn> { setup?: Setup }\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@ui/dist/types/components/Form/utils.d.ts"),
    "import { StdFormGlobalSetupFn, StdFormProps } from './types'\n\
     export declare function defineStdFormProps<Setup extends StdFormGlobalSetupFn>(\n\
       props: StdFormProps<Setup>,\n\
     ): StdFormProps<Setup>\n",
  )
  .unwrap();
  std::fs::write(
    root.join("src/consumer.ts"),
    "import { computed } from 'vue'\n\
     import { defineStdFormProps } from '@ui'\n\
     defineStdFormProps({\n\
       setup: ({ values }) => computed(() => values.value),\n\
     })\n",
  )
  .unwrap();

  let resolver = ProjectResolver::new(&root);
  let known = BTreeSet::new();
  let Resolution::External { resolved_path: Some(path), .. } =
    resolver.resolve("src/consumer.ts", "@ui", &known)
  else {
    panic!("expected external resolve");
  };
  let roots = [ExternalReactivityRoot {
    from: ModuleId::from("src/consumer.ts"),
    specifier: "@ui".into(),
    resolved_path: path,
  }];
  let (sources, links) = ExternalSummaryLoadPass::run(&root, &resolver, &roots, None);
  assert!(
    sources.iter().any(|source| source.id.as_str().contains("Form/utils")),
    "package follow must load Form/utils.d.ts; sources={:?}",
    sources.iter().map(|source| source.id.as_str()).collect::<Vec<_>>()
  );
  let mut modules = sources;
  modules.push(ModuleSource::standalone(
    "src/consumer.ts",
    std::fs::read_to_string(root.join("src/consumer.ts")).unwrap(),
    "ts",
    vue_vet_core::ScriptKind::Script,
  ));
  let Ok(traced) = trace_modules(&modules, &links) else {
    panic!("trace options-callback barrel modules");
  };
  let consumer = traced.iter().find(|module| module.id.as_str() == "src/consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.scopes.iter().any(|scope| {
        scope.reads.iter().any(|read| read.binding == "values")
          && scope.uncertain_accesses.is_empty()
      })
    }),
    "export* barrel must surface options-callback slots; got {:?}",
    consumer.map(|module| &module.graph.scopes)
  );
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn enrich_strips_bare_imports_so_duplicate_vue_types_still_seed() {
  // Real UI Form chain: utils/types/composables each `import { MaybeRefOrGetter } from 'vue'`.
  let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-dts-dup-vue-import");
  let _ = std::fs::remove_dir_all(&root);
  let form = root.join("node_modules/@ui/dist/types/components/Form");
  std::fs::create_dir_all(&form).unwrap();
  std::fs::create_dir_all(root.join("src")).unwrap();
  std::fs::write(root.join("package.json"), r#"{"name":"dup-vue-import"}"#).unwrap();
  std::fs::write(
    root.join("node_modules/@ui/package.json"),
    r#"{"name":"@ui","types":"./dist/types/index.d.ts","exports":{".":{"types":"./dist/types/index.d.ts","import":"./dist/index.js"}}}"#,
  )
  .unwrap();
  std::fs::write(root.join("node_modules/@ui/dist/index.js"), "export {}\n").unwrap();
  std::fs::write(
    root.join("node_modules/@ui/dist/types/index.d.ts"),
    "export * from './components'\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@ui/dist/types/components/index.d.ts"),
    "export * from './Form'\n",
  )
  .unwrap();
  std::fs::write(form.join("index.d.ts"), "export { defineStdFormProps } from './utils'\n")
    .unwrap();
  std::fs::write(
    form.join("composables.d.ts"),
    "import { MaybeRefOrGetter, Ref } from 'vue'\n\
     export interface StdFormContext { values: Ref<unknown>; form: unknown }\n",
  )
  .unwrap();
  std::fs::write(
    form.join("types.d.ts"),
    "import { MaybeRefOrGetter } from 'vue'\n\
     import { StdFormContext } from './composables'\n\
     export interface StdFormGlobalSetupContext extends StdFormContext {}\n\
     export type StdFormGlobalSetupFn = (ctx: StdFormGlobalSetupContext) => unknown\n\
     export interface StdFormProps<Setup extends StdFormGlobalSetupFn> { setup?: Setup }\n",
  )
  .unwrap();
  std::fs::write(
    form.join("utils.d.ts"),
    "import { MaybeRefOrGetter } from 'vue'\n\
     import { StdFormGlobalSetupFn, StdFormProps } from './types'\n\
     export declare function defineStdFormProps<Setup extends StdFormGlobalSetupFn>(\n\
       props: StdFormProps<Setup>,\n\
     ): StdFormProps<Setup>\n",
  )
  .unwrap();
  std::fs::write(
    root.join("src/consumer.ts"),
    "import { computed } from 'vue'\n\
     import { defineStdFormProps } from '@ui'\n\
     defineStdFormProps({\n\
       setup: ({ values }) => computed(() => values.value),\n\
     })\n",
  )
  .unwrap();

  let utils = form.join("utils.d.ts");
  let enriched =
    enrich_dts_with_relative_type_imports(&utils, &std::fs::read_to_string(&utils).unwrap());
  assert!(
    prepare_standalone_module_source(ModuleId::from("utils.d.ts"), enriched.clone(), "d.ts")
      .is_ok(),
    "enrich must strip duplicate vue imports; enriched:\n{enriched}"
  );

  let resolver = ProjectResolver::new(&root);
  let known = BTreeSet::new();
  let Resolution::External { resolved_path: Some(path), .. } =
    resolver.resolve("src/consumer.ts", "@ui", &known)
  else {
    panic!("expected external resolve");
  };
  let roots = [ExternalReactivityRoot {
    from: ModuleId::from("src/consumer.ts"),
    specifier: "@ui".into(),
    resolved_path: path,
  }];
  let (sources, links) = ExternalSummaryLoadPass::run(&root, &resolver, &roots, None);
  let mut modules = sources;
  modules.push(ModuleSource::standalone(
    "src/consumer.ts",
    std::fs::read_to_string(root.join("src/consumer.ts")).unwrap(),
    "ts",
    vue_vet_core::ScriptKind::Script,
  ));
  let Ok(traced) = trace_modules(&modules, &links) else {
    panic!("trace dup-vue-import modules");
  };
  let consumer = traced.iter().find(|module| module.id.as_str() == "src/consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.scopes.iter().any(|scope| {
        scope.reads.iter().any(|read| read.binding == "values")
          && scope.uncertain_accesses.is_empty()
      })
    }),
    "duplicate vue imports must not block options-callback seeds; got {:?}",
    consumer.map(|module| &module.graph.scopes)
  );
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn pnpm_store_path_uses_real_package_budget_key() {
  let path = PathBuf::from(
    "/proj/node_modules/.pnpm/@standard-design+ui@1.0.0/node_modules/@standard-design/ui/dist/types/index.d.ts",
  );
  assert_eq!(
    node_modules_package_key(&path).as_deref(),
    Some("@standard-design/ui"),
    "pnpm store paths must not budget under `.pnpm`"
  );
  let plain = PathBuf::from("/proj/node_modules/vue/dist/vue.d.ts");
  assert_eq!(node_modules_package_key(&plain).as_deref(), Some("vue"));
}

#[test]
fn relative_value_import_in_dts_inlines_and_strips() {
  let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-dts-value-import");
  let _ = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(root.join("pkg")).unwrap();
  std::fs::write(
    root.join("pkg/types.d.ts"),
    "import type { Ref } from 'vue'\n\
     export interface Ctx { values: Ref<number> }\n\
     export type SetupFn = (ctx: Ctx) => void\n\
     export interface Props<S extends SetupFn> { setup?: S }\n",
  )
  .unwrap();
  std::fs::write(
    root.join("pkg/utils.d.ts"),
    "import { SetupFn, Props } from './types'\n\
     export declare function defineFormProps<S extends SetupFn>(props: Props<S>): void\n",
  )
  .unwrap();
  let enriched = enrich_dts_with_relative_type_imports(
    &root.join("pkg/utils.d.ts"),
    &std::fs::read_to_string(root.join("pkg/utils.d.ts")).unwrap(),
  );
  assert!(
    enriched.contains("interface Ctx")
      && enriched.contains("defineFormProps")
      && !enriched.contains("from './types'"),
    "must inline './types' and strip the import; got:\n{enriched}"
  );
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn types_only_chunk_reexport_loads_use_query_seed() {
  // Packaged barrels often re-export `./queryClient-HASH.js` when only `.d.ts` ships.
  let root =
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-vue-query-types-only");
  let _ = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(root.join("node_modules/@tanstack/vue-query/build/modern")).unwrap();
  std::fs::create_dir_all(root.join("src")).unwrap();
  std::fs::write(root.join("package.json"), r#"{"name":"vue-query-types-only"}"#).unwrap();
  std::fs::write(
    root.join("node_modules/@tanstack/vue-query/package.json"),
    r#"{"name":"@tanstack/vue-query","types":"./build/modern/index.d.ts","exports":{".":{"types":"./build/modern/index.d.ts","import":"./build/modern/index.js"}}}"#,
  )
  .unwrap();
  // Entry `.js` exists so the package resolves; the leaf chunk is types-only.
  std::fs::write(
    root.join("node_modules/@tanstack/vue-query/build/modern/index.js"),
    "export { useQuery } from './queryClient-HASH.js'\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@tanstack/vue-query/build/modern/queryClient-HASH.d.ts"),
    "import type { Ref } from 'vue'\n\
     type Bag = { [K in 'data' | 'isLoading']: Ref<unknown> }\n\
     declare function useQuery(): Bag\n\
     export { useQuery as u }\n",
  )
  .unwrap();
  std::fs::write(
    root.join("node_modules/@tanstack/vue-query/build/modern/index.d.ts"),
    "export { u as useQuery } from './queryClient-HASH.js'\n",
  )
  .unwrap();

  let resolver = ProjectResolver::new(&root);
  let known = BTreeSet::new();
  let Resolution::External { resolved_path: Some(path), .. } =
    resolver.resolve("src/consumer.ts", "@tanstack/vue-query", &known)
  else {
    panic!("expected external resolve");
  };
  let roots = [ExternalReactivityRoot {
    from: ModuleId::from("consumer.ts"),
    specifier: "@tanstack/vue-query".into(),
    resolved_path: path,
  }];
  let (sources, links) = ExternalSummaryLoadPass::run(&root, &resolver, &roots, None);
  assert!(!sources.is_empty(), "vue-query index/leaves must load");
  assert!(
    sources.iter().any(|source| source.id.as_str().contains("queryClient-HASH")),
    "types-only chunk follow must load queryClient-HASH.d.ts; sources={:?}",
    sources.iter().map(|source| source.id.as_str()).collect::<Vec<_>>()
  );
  let mut modules = sources;
  modules.push(ModuleSource::standalone(
    "consumer.ts",
    "import { computed } from 'vue';\n\
     import { useQuery } from '@tanstack/vue-query';\n\
     const { data, isLoading } = useQuery({ queryKey: ['x'] as const, queryFn: () => Promise.resolve(1) });\n\
     export const a = computed(() => data.value);\n\
     export const b = computed(() => isLoading.value);\n",
    "ts",
    vue_vet_core::ScriptKind::Script,
  ));
  let Ok(traced) = trace_modules(&modules, &links) else {
    panic!("trace types-only vue-query modules");
  };
  let consumer = traced.iter().find(|module| module.id.as_str() == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| binding.name == "data")
        && module.graph.scopes.iter().any(|scope| {
          scope.reads.iter().any(|read| read.binding == "data")
            && scope.uncertain_accesses.is_empty()
        })
    }),
    "types-only package useQuery must seed; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
  let _ = std::fs::remove_dir_all(&root);
}
