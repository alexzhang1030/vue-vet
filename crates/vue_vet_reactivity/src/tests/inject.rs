use super::helpers::*;

#[test]
fn same_file_provide_inject_seeds_reactive_binding() {
  let graph = graph(
    "import { provide, inject, ref, computed } from 'vue';\n\
     const count = ref(1);\n\
     provide('count', count);\n\
     const injected = inject('count');\n\
     const doubled = computed(() => injected.value * 2);\n\
     void doubled.value;",
  );
  assert!(
    graph
      .bindings
      .iter()
      .any(|binding| { binding.name == "injected" && binding.kind == ReactiveBindingKind::Ref }),
    "unique same-file provide must seed inject binding; bindings={:?}",
    graph.bindings
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "injected" && read.property.as_deref() == Some("value"))
    }),
    "computed must track injected.value; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn inject_default_seeds_when_provide_missing() {
  let graph = graph(
    "import { inject, ref, computed } from 'vue';\n\
     const fallback = ref(0);\n\
     const count = inject('missing', fallback);\n\
     const doubled = computed(() => count.value * 2);\n\
     void doubled.value;",
  );
  assert!(
    graph
      .bindings
      .iter()
      .any(|binding| { binding.name == "count" && binding.kind == ReactiveBindingKind::Ref }),
    "inject default ref must seed when no provide exists; bindings={:?}",
    graph.bindings
  );
}

#[test]
fn ambiguous_provide_keys_stay_quiet() {
  let graph = graph(
    "import { provide, inject, ref, computed } from 'vue';\n\
     provide('count', ref(1));\n\
     provide('count', ref(2));\n\
     const count = inject('count');\n\
     const doubled = computed(() => count.value * 2);\n\
     void doubled.value;",
  );
  assert!(
    !graph.bindings.iter().any(|binding| binding.name == "count"),
    "multiple provides of the same key must not invent an inject binding; bindings={:?}",
    graph.bindings
  );
}

#[test]
fn cross_module_provide_inject_unique_key_seeds() {
  let modules = [
    ModuleSource::standalone(
      "provider.ts",
      "import { provide, ref } from 'vue'; const count = ref(1); provide('count', count);",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { inject, computed } from 'vue'; const count = inject('count'); const d = computed(() => count.value); void d.value;",
      "ts",
      ScriptKind::Script,
    ),
  ];
  // No import link required — provide index is project-wide by key.
  let traced = traced_modules(&modules, &[]);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "count" && binding.kind == ReactiveBindingKind::Ref)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope
              .reads
              .iter()
              .any(|read| read.binding == "count" && read.property.as_deref() == Some("value"))
        })
    }),
    "unique project provide must seed cross-module inject; consumer={consumer:?}"
  );
}

#[test]
fn inject_as_typed_bag_seeds_without_known_provide() {
  let graph = graph(
    "import type { Ref } from 'vue';\n\
     import { inject, computed } from 'vue';\n\
     interface MapCtx { mapId: Ref<number | undefined> }\n\
     const KEY = Symbol('map');\n\
     const ctx = inject(KEY) as MapCtx;\n\
     const d = computed(() => ctx.mapId.value);\n\
     void d.value;",
  );
  assert!(
    graph
      .composable_instances
      .get("ctx")
      .is_some_and(|shape| shape.get("mapId") == Some(&ReactiveBindingKind::Ref)),
    "inject(key) as Ctx must seed bag from asserted interface; instances={:?}",
    graph.composable_instances
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "mapId" && read.property.as_deref() == Some("value"))
    }),
    "computed must track ctx.mapId.value via asserted inject bag; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn generic_context_factory_instantiates_inject_bag() {
  let modules = [
    ModuleSource::standalone(
      "common.ts",
      "import { inject, provide } from 'vue';\n\
       export function createContext<T>(identifier: string) {\n\
         const key = Symbol(identifier);\n\
         const useProvide = (value: T) => { provide(key, value); };\n\
         const useInject = () => {\n\
           const value = inject(key);\n\
           return value as T;\n\
         };\n\
         return { useProvide, useInject };\n\
       }\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "ctx.ts",
      "import type { Ref } from 'vue';\n\
       import { createContext } from './common';\n\
       interface MapCtx { mapId: Ref<number | undefined> }\n\
       export const {\n\
         useProvide: provideMapCtx,\n\
         useInject: useMapCtx,\n\
       } = createContext<MapCtx>('MAP');\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { useMapCtx } from './ctx';\n\
       const { mapId } = useMapCtx();\n\
       const d = computed(() => mapId.value);\n\
       void d.value;",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links = [
    ModuleLink { from: "ctx.ts".into(), specifier: "./common".into(), to: "common.ts".into() },
    ModuleLink { from: "consumer.ts".into(), specifier: "./ctx".into(), to: "ctx.ts".into() },
  ];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "mapId" && binding.kind == ReactiveBindingKind::Ref)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope
              .reads
              .iter()
              .any(|read| read.binding == "mapId" && read.property.as_deref() == Some("value"))
        })
    }),
    "createContext<Ctx> useInject destructure must seed mapId; consumer={consumer:?}"
  );
}

#[test]
fn typed_inject_helper_forwards_bag_across_modules() {
  let modules = [
    ModuleSource::standalone(
      "ctx.ts",
      "import type { Ref } from 'vue';\n\
       import { inject, provide, shallowRef } from 'vue';\n\
       interface MapCtx { mapId: Ref<number | undefined> }\n\
       const KEY = Symbol('map');\n\
       export function provideMapCtx(mapId: Ref<number | undefined>) {\n\
         const local = { mapId, robots: shallowRef([]) };\n\
         provide(KEY, local);\n\
         return local;\n\
       }\n\
       export function useMapCtx() {\n\
         const ctx = inject(KEY) as MapCtx;\n\
         return ctx;\n\
       }\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { useMapCtx } from './ctx';\n\
       const { mapId } = useMapCtx();\n\
       const d = computed(() => mapId.value);\n\
       void d.value;",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links =
    [ModuleLink { from: "consumer.ts".into(), specifier: "./ctx".into(), to: "ctx.ts".into() }];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "mapId" && binding.kind == ReactiveBindingKind::Ref)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope
              .reads
              .iter()
              .any(|read| read.binding == "mapId" && read.property.as_deref() == Some("value"))
        })
    }),
    "useX() returning inject(key) as Ctx must seed destructured Ref fields; consumer={consumer:?}"
  );
}

#[test]
fn provide_composable_instance_seeds_inject_bag() {
  let graph = graph(
    "import { provide, inject, ref, computed } from 'vue';\n\
     function useCounter() { const count = ref(0); return { count }; }\n\
     const api = useCounter();\n\
     provide('api', api);\n\
     const bag = inject('api');\n\
     const d = computed(() => bag.count.value);\n\
     void d.value;",
  );
  assert!(
    graph
      .composable_instances
      .get("bag")
      .is_some_and(|shape| { shape.get("count") == Some(&ReactiveBindingKind::Ref) }),
    "provide(composable bag) must seed inject as instance shape; instances={:?}",
    graph.composable_instances
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope
        .reads
        .iter()
        .any(|read| read.binding == "count" && read.property.as_deref() == Some("value"))
    }),
    "computed must track bag.count.value via inject instance; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn provide_direct_composable_call_seeds_inject_bag() {
  let graph = graph(
    "import { provide, inject, ref, computed } from 'vue';\n\
     function useCounter() { const count = ref(0); return { count }; }\n\
     provide('api', useCounter());\n\
     const bag = inject('api');\n\
     const d = computed(() => bag.count.value);\n\
     void d.value;",
  );
  assert!(
    graph
      .composable_instances
      .get("bag")
      .is_some_and(|shape| shape.get("count") == Some(&ReactiveBindingKind::Ref)),
    "provide(useX()) must seed inject bag; instances={:?}",
    graph.composable_instances
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope
        .reads
        .iter()
        .any(|read| read.binding == "count" && read.property.as_deref() == Some("value"))
    }),
    "computed must track bag.count.value; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn provide_direct_composable_call_stays_quiet_when_callee_shadowed() {
  // Outer composable has a known bag shape; block-local `useCounter` is a plain
  // function. Name-only seeding would invent bag.count from the outer def.
  let graph = graph(
    "import { provide, inject, ref, computed } from 'vue';\n\
     function useCounter() { const count = ref(0); return { count }; }\n\
     {\n\
       const useCounter = () => ({});\n\
       provide('api', useCounter());\n\
     }\n\
     const bag = inject('api');\n\
     const d = computed(() => bag.count.value);\n\
     void d.value;",
  );
  assert!(
    !graph.composable_instances.contains_key("bag"),
    "shadowed provide(useX()) must not invent outer composable shape; instances={:?}",
    graph.composable_instances
  );
  assert!(
    !graph.scopes.iter().any(|scope| {
      scope
        .reads
        .iter()
        .any(|read| read.binding == "count" && read.property.as_deref() == Some("value"))
    }),
    "must not invent bag.count.value via shadowed provide(useX()); scopes={:?}",
    graph.scopes
  );
}
