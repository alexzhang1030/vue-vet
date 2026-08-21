use super::helpers::*;

#[test]
fn value_bag_member_call_seeds_destructure() {
  let graph = graph(
    "import { ref, computed } from 'vue';\n\
     function useMapsGet() { return { data: ref(0), isLoading: ref(false) }; }\n\
     function createApi() {\n\
       return { maps: { useMapsGet } };\n\
     }\n\
     const api = createApi();\n\
     const { data, isLoading } = api.maps.useMapsGet();\n\
     const rows = computed(() => data.value);\n\
     const pending = computed(() => isLoading.value);",
  );
  assert!(
    graph
      .bindings
      .iter()
      .any(|binding| binding.name == "data" && binding.kind == ReactiveBindingKind::Ref)
      && graph
        .bindings
        .iter()
        .any(|binding| binding.name == "isLoading" && binding.kind == ReactiveBindingKind::Ref),
    "api.maps.useMapsGet() must seed destructure; bindings={:?}",
    graph.bindings
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.iter().any(|read| read.binding == "data")
        && scope.uncertain_accesses.is_empty()
    }) && graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.iter().any(|read| read.binding == "isLoading")
        && scope.uncertain_accesses.is_empty()
    }),
    "value-bag member destructure .value must classify; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn value_bag_nested_function_member_call_seeds_destructure() {
  let graph = graph(
    "import { ref, computed } from 'vue';\n\
     function useQuery() { return { data: ref(0), isLoading: ref(false) }; }\n\
     const createDeviceApiQuery = (api) => {\n\
       function useDevicesGet() { return useQuery(); }\n\
       return { useDevicesGet };\n\
     };\n\
     const createApiQuery = (api) => {\n\
       return { device: createDeviceApiQuery(api) };\n\
     };\n\
     const api = createApiQuery({});\n\
     const { data, isLoading } = api.device.useDevicesGet();\n\
     const rows = computed(() => data.value);\n\
     const pending = computed(() => isLoading.value);",
  );
  assert!(
    graph
      .bindings
      .iter()
      .any(|binding| binding.name == "data" && binding.kind == ReactiveBindingKind::Ref)
      && graph
        .bindings
        .iter()
        .any(|binding| binding.name == "isLoading" && binding.kind == ReactiveBindingKind::Ref),
    "nested-fn value-bag member call must seed; bindings={:?}",
    graph.bindings
  );
}

#[test]
fn value_bag_member_call_reexport_via_wrapper_composable() {
  // Wrapper destructures api.ns.useX() and returns { isLoading } for consumers.
  let modules = [
    ModuleSource::standalone(
      "producer.d.ts",
      "import type { Ref } from 'vue';\n\
       type Result = { data: number; isLoading: boolean };\n\
       type OpenBag = { [K in keyof Result]: Ref<Result[K]> };\n\
       export declare function useQuery(): OpenBag;\n",
      "d.ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "queries.ts",
      "import { useQuery } from './producer';\n\
       export const createApiQuery = (api) => {\n\
         function useDevicesGet() { return useQuery(); }\n\
         return { device: { useDevicesGet } };\n\
       };\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "api.ts",
      "import { createApiQuery } from './queries';\n\
       export const appApi = createApiQuery({});\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "device.ts",
      "import { computed } from 'vue';\n\
       import { appApi } from './api';\n\
       export function useDeviceDetail() {\n\
         const { data, isLoading } = appApi.device.useDevicesGet();\n\
         const detail = computed(() => data.value);\n\
         return { detail, isLoading };\n\
       }\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { useDeviceDetail } from './device';\n\
       const { detail, isLoading } = useDeviceDetail();\n\
       const pending = computed(() => isLoading.value);\n\
       const title = computed(() => detail.value);\n",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links = [
    ModuleLink {
      from: "queries.ts".into(),
      specifier: "./producer".into(),
      to: "producer.d.ts".into(),
    },
    ModuleLink { from: "api.ts".into(), specifier: "./queries".into(), to: "queries.ts".into() },
    ModuleLink { from: "device.ts".into(), specifier: "./api".into(), to: "api.ts".into() },
    ModuleLink { from: "consumer.ts".into(), specifier: "./device".into(), to: "device.ts".into() },
  ];
  let traced = traced_modules(&modules, &links);
  let device = traced.iter().find(|module| module.id == "device.ts");
  assert!(
    device.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "isLoading" && binding.kind == ReactiveBindingKind::Ref)
    }),
    "wrapper must seed isLoading locally; got {:?}",
    device.map(|module| &module.graph.bindings)
  );
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "isLoading" && binding.kind == ReactiveBindingKind::Ref)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope.reads.iter().any(|read| read.binding == "isLoading")
            && scope.uncertain_accesses.is_empty()
        })
    }),
    "wrapper re-export must seed consumer isLoading; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn create_shared_composable_forwards_factory_bag() {
  // VueUse createSharedComposable<Fn>(Fn): Fn — export keeps the factory bag.
  let modules = [
    ModuleSource::standalone(
      "vueuse.d.ts",
      "export declare function createSharedComposable<Fn extends (...args: any[]) => any>(composable: Fn): Fn;\n",
      "d.ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "permission.ts",
      "import { computed } from 'vue';\n\
       import { createSharedComposable } from '@vueuse/core';\n\
       export const useUserFunctionPermission = createSharedComposable(() => {\n\
         const hasPermission = computed(() => (code) => Boolean(code));\n\
         return { hasPermission };\n\
       });\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { useUserFunctionPermission } from './permission';\n\
       const { hasPermission } = useUserFunctionPermission();\n\
       const canDelete = computed(() => hasPermission.value('delete'));\n",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links = [
    ModuleLink {
      from: "permission.ts".into(),
      specifier: "@vueuse/core".into(),
      to: "vueuse.d.ts".into(),
    },
    ModuleLink {
      from: "consumer.ts".into(),
      specifier: "./permission".into(),
      to: "permission.ts".into(),
    },
  ];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| {
        binding.name == "hasPermission" && binding.kind == ReactiveBindingKind::Computed
      }) && module.graph.scopes.iter().any(|scope| {
        scope.kind == TrackingScopeKind::Computed
          && scope.reads.iter().any(|read| read.binding == "hasPermission")
          && scope.uncertain_accesses.is_empty()
      })
    }),
    "createSharedComposable factory bag must seed; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn create_shared_composable_forwards_named_local_factory() {
  let modules = [
    ModuleSource::standalone(
      "vueuse.d.ts",
      "export declare function createSharedComposable<Fn extends (...args: any[]) => any>(composable: Fn): Fn;\n",
      "d.ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "permission.ts",
      "import { computed } from 'vue';\n\
       import { createSharedComposable } from '@vueuse/core';\n\
       function usePermission() {\n\
         const hasPermission = computed(() => (code) => Boolean(code));\n\
         return { hasPermission };\n\
       }\n\
       export const useUserFunctionPermission = createSharedComposable(usePermission);\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { useUserFunctionPermission } from './permission';\n\
       const { hasPermission } = useUserFunctionPermission();\n\
       const canDelete = computed(() => hasPermission.value('delete'));\n",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links = [
    ModuleLink {
      from: "permission.ts".into(),
      specifier: "@vueuse/core".into(),
      to: "vueuse.d.ts".into(),
    },
    ModuleLink {
      from: "consumer.ts".into(),
      specifier: "./permission".into(),
      to: "permission.ts".into(),
    },
  ];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| binding.name == "hasPermission")
        && module.graph.scopes.iter().any(|scope| {
          scope.reads.iter().any(|read| read.binding == "hasPermission")
            && scope.uncertain_accesses.is_empty()
        })
    }),
    "named factory through createSharedComposable must seed; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn value_bag_imported_factory_call_exports_value_bag() {
  // `export const api = createApi()` in another module — ValueFactoryCall → ValueBag.
  let modules = [
    ModuleSource::standalone(
      "producer.d.ts",
      "import type { Ref } from 'vue';\n\
       type Result = { data: number; isLoading: boolean };\n\
       type OpenBag = { [K in keyof Result]: Ref<Result[K]> };\n\
       export declare function useQuery(): OpenBag;\n",
      "d.ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "queries.ts",
      "import { useQuery } from './producer';\n\
       export const createDeviceApiQuery = (api) => {\n\
         function useDevicesGet() { return useQuery(); }\n\
         function useDevicesPostMutation() { return useMutation(); }\n\
         return { useDevicesGet, useDevicesPostMutation };\n\
       };\n\
       export const createApiQuery = (api) => {\n\
         return { device: createDeviceApiQuery(api) };\n\
       };\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "api.ts",
      "import { createApiQuery } from './queries';\n\
       export const appApi = createApiQuery({});\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { appApi } from './api';\n\
       const { data, isLoading } = appApi.device.useDevicesGet();\n\
       const rows = computed(() => data.value);\n\
       const pending = computed(() => isLoading.value);\n",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links = [
    ModuleLink {
      from: "queries.ts".into(),
      specifier: "./producer".into(),
      to: "producer.d.ts".into(),
    },
    ModuleLink { from: "api.ts".into(), specifier: "./queries".into(), to: "queries.ts".into() },
    ModuleLink { from: "consumer.ts".into(), specifier: "./api".into(), to: "api.ts".into() },
  ];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "data" && binding.kind == ReactiveBindingKind::Ref)
        && module
          .graph
          .bindings
          .iter()
          .any(|binding| binding.name == "isLoading" && binding.kind == ReactiveBindingKind::Ref)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope.reads.iter().any(|read| read.binding == "data")
            && scope.uncertain_accesses.is_empty()
        })
    }),
    "imported createApiQuery() export must seed member destructure; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn value_bag_member_call_forwards_imported_shape_across_modules() {
  let modules = [
    ModuleSource::standalone(
      "producer.d.ts",
      "import type { Ref } from 'vue';\n\
       type Result = { data: number; isLoading: boolean };\n\
       type OpenBag = { [K in keyof Result]: Ref<Result[K]> };\n\
       export declare function useRemote(): OpenBag;\n",
      "d.ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "api.ts",
      "import { useRemote } from './producer';\n\
       function useMapsGet() { return useRemote(); }\n\
       export function createApi() { return { maps: { useMapsGet } }; }\n\
       export const api = createApi();\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { api } from './api';\n\
       const { data, isLoading } = api.maps.useMapsGet();\n\
       const rows = computed(() => data.value);\n\
       const pending = computed(() => isLoading.value);\n",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links = [
    ModuleLink {
      from: "api.ts".into(),
      specifier: "./producer".into(),
      to: "producer.d.ts".into(),
    },
    ModuleLink { from: "consumer.ts".into(), specifier: "./api".into(), to: "api.ts".into() },
  ];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "data" && binding.kind == ReactiveBindingKind::Ref)
        && module
          .graph
          .bindings
          .iter()
          .any(|binding| binding.name == "isLoading" && binding.kind == ReactiveBindingKind::Ref)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope.reads.iter().any(|read| read.binding == "data")
            && scope.uncertain_accesses.is_empty()
        })
    }),
    "cross-module value-bag member call must seed; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}
