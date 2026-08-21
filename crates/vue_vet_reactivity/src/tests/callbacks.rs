use super::helpers::*;

#[test]
fn options_object_callback_ref_bag_seeds_same_file() {
  let graph = graph(
    "import { computed, type Ref } from 'vue';\n\
     interface FormCtx { values: Ref<{ name: string }> }\n\
     type FormSetup = (ctx: FormCtx) => void;\n\
     function defineFormProps(props: { setup?: FormSetup }) {\n\
       props.setup?.({ values: null as unknown as Ref<{ name: string }> });\n\
     }\n\
     defineFormProps({\n\
       setup({ values }) {\n\
         return computed(() => values.value.name);\n\
       },\n\
     });",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.iter().any(|read| read.binding == "values")
        && scope.uncertain_accesses.is_empty()
    }),
    "options callback ObjectPattern Ref field must seed; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn options_object_callback_ref_bag_seeds_across_modules_with_extends() {
  let modules = [
    ModuleSource::standalone(
      "form.d.ts",
      "import type { Ref } from 'vue';\n\
       export interface FormContext { values: Ref<{ name: string }> }\n\
       export interface FormSetupContext extends FormContext { ready: boolean }\n\
       export type FormSetupFn = (ctx: FormSetupContext) => void;\n\
       export interface FormProps { setup?: FormSetupFn }\n\
       export declare function defineFormProps(props: FormProps): void;\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { defineFormProps } from './form';\n\
       defineFormProps({\n\
         setup({ values }) {\n\
           return computed(() => values.value.name);\n\
         },\n\
       });\n",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links =
    [ModuleLink { from: "consumer.ts".into(), specifier: "./form".into(), to: "form.d.ts".into() }];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.scopes.iter().any(|scope| {
        scope.kind == TrackingScopeKind::Computed
          && scope.reads.iter().any(|read| read.binding == "values")
          && scope.uncertain_accesses.is_empty()
      })
    }),
    "cross-module options callback with interface extends must seed; got {:?}",
    consumer.map(|module| &module.graph.scopes)
  );
}

#[test]
fn options_object_callback_type_param_constraint_seeds_across_modules() {
  let modules = [
    ModuleSource::standalone(
      "form.d.ts",
      "import type { Ref } from 'vue';\n\
       export interface FormContext { values: Ref<{ name: string }> }\n\
       export interface FormSetupContext extends FormContext { ready: boolean }\n\
       export type FormSetupFn = (ctx: FormSetupContext) => void;\n\
       export interface FormProps<Setup extends FormSetupFn> { setup?: Setup }\n\
       export declare function defineFormProps<Setup extends FormSetupFn>(\n\
         props: FormProps<Setup>,\n\
       ): void;\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { defineFormProps } from './form';\n\
       defineFormProps({\n\
         setup: ({ values }) => {\n\
           return computed(() => values.value.name);\n\
         },\n\
       });\n",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links =
    [ModuleLink { from: "consumer.ts".into(), specifier: "./form".into(), to: "form.d.ts".into() }];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.scopes.iter().any(|scope| {
        scope.kind == TrackingScopeKind::Computed
          && scope.reads.iter().any(|read| read.binding == "values")
          && scope.uncertain_accesses.is_empty()
      })
    }),
    "type-param `Setup extends Fn` options callback must seed; got {:?}",
    consumer.map(|module| &module.graph.scopes)
  );
}

#[test]
fn options_object_callback_without_ref_fields_stays_quiet() {
  let graph = graph(
    "import { computed } from 'vue';\n\
     function definePlain(props: { setup?: (ctx: { name: string }) => void }) {\n\
       props.setup?.({ name: '' });\n\
     }\n\
     definePlain({\n\
       setup({ name }) {\n\
         return computed(() => name.length);\n\
       },\n\
     });",
  );
  assert!(
    !graph.bindings.iter().any(|binding| binding.name == "name")
      && graph.scopes.iter().all(|scope| {
        scope.kind != TrackingScopeKind::Computed
          || !scope.reads.iter().any(|read| read.binding == "name")
      }),
    "non-Ref options callback fields must not seed; bindings={:?} scopes={:?}",
    graph.bindings,
    graph.scopes
  );
}

#[test]
fn options_object_callback_slots_follow_export_star_barrel() {
  let modules = [
    ModuleSource::standalone(
      "utils.d.ts",
      "import type { Ref } from 'vue';\n\
       export interface FormContext { values: Ref<{ name: string }> }\n\
       export interface FormSetupContext extends FormContext {}\n\
       export type FormSetupFn = (ctx: FormSetupContext) => void;\n\
       export interface FormProps<Setup extends FormSetupFn> { setup?: Setup }\n\
       export declare function defineFormProps<Setup extends FormSetupFn>(\n\
         props: FormProps<Setup>,\n\
       ): void;\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "form.d.ts",
      "export { defineFormProps } from './utils';\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone("index.d.ts", "export * from './form';\n", "ts", ScriptKind::Script),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { defineFormProps } from './index';\n\
       defineFormProps({\n\
         setup: ({ values }) => {\n\
           return computed(() => values.value.name);\n\
         },\n\
       });\n",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links = [
    ModuleLink { from: "form.d.ts".into(), specifier: "./utils".into(), to: "utils.d.ts".into() },
    ModuleLink { from: "index.d.ts".into(), specifier: "./form".into(), to: "form.d.ts".into() },
    ModuleLink { from: "consumer.ts".into(), specifier: "./index".into(), to: "index.d.ts".into() },
  ];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.scopes.iter().any(|scope| {
        scope.kind == TrackingScopeKind::Computed
          && scope.reads.iter().any(|read| read.binding == "values")
          && scope.uncertain_accesses.is_empty()
      })
    }),
    "options callback slots must follow export {{ x }} / export * barrels; got {:?}",
    consumer.map(|module| &module.graph.scopes)
  );
}

#[test]
fn typed_function_callback_param_seeds_same_file() {
  let graph = graph(
    "import type { ComputedRef } from 'vue';\n\
     import { computed } from 'vue';\n\
     function usePagedQuery<T>(\n\
       _init: T,\n\
       run: (state: ComputedRef<T & { page: number }>) => unknown,\n\
     ) {\n\
       void run;\n\
     }\n\
     usePagedQuery({ q: '' }, (state) => {\n\
       const page = computed(() => state.value.page);\n\
       void page.value;\n\
     });",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "state" && read.property.as_deref() == Some("value"))
        && !scope.uncertain_accesses.iter().any(|name| name == "state")
    }),
    "typed (state: ComputedRef) callback formal must classify .value; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn typed_function_callback_param_seeds_across_modules() {
  let modules = [
    ModuleSource::standalone(
      "query.ts",
      "import type { ComputedRef } from 'vue';\n\
       export function usePagedQuery<T>(\n\
         _init: T,\n\
         run: (state: ComputedRef<T & { page: number }>) => unknown,\n\
       ) {\n\
         void run;\n\
       }\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { usePagedQuery } from './query';\n\
       usePagedQuery({ q: '' }, (state) => {\n\
         const page = computed(() => state.value.page);\n\
         void page.value;\n\
       });",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links =
    [ModuleLink { from: "consumer.ts".into(), specifier: "./query".into(), to: "query.ts".into() }];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.scopes.iter().any(|scope| {
        scope.kind == TrackingScopeKind::Computed
          && scope
            .reads
            .iter()
            .any(|read| read.binding == "state" && read.property.as_deref() == Some("value"))
          && scope.uncertain_accesses.iter().all(|name| name != "state")
      })
    }),
    "imported typed callback formal must seed across modules; consumer={consumer:?}"
  );
}

#[test]
fn typed_function_callback_ignores_non_ref_formals() {
  let graph = graph(
    "import { computed } from 'vue';\n\
     function useMapped(\n\
       map: (label: string) => string,\n\
     ) {\n\
       void map;\n\
     }\n\
     useMapped((label) => {\n\
       const upper = computed(() => label.toUpperCase());\n\
       void upper.value;\n\
       return label;\n\
     });",
  );
  assert!(
    !graph.bindings.iter().any(|binding| binding.name == "label"),
    "non-Ref callback formals must not invent bindings; bindings={:?}",
    graph.bindings
  );
}
