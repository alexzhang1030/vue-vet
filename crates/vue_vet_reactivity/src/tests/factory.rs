use super::helpers::*;

#[test]
fn return_call_forwards_same_file_composable_shape() {
  let graph = graph(
    "import { ref, computed } from 'vue';\n\
     function useInner() { return { data: ref(0), ready: ref(false) }; }\n\
     function useOuter() { return useInner(); }\n\
     const { data, ready } = useOuter();\n\
     const rows = computed(() => data.value);\n\
     const pending = computed(() => ready.value);",
  );
  assert!(
    graph
      .bindings
      .iter()
      .any(|binding| binding.name == "data" && binding.kind == ReactiveBindingKind::Ref)
      && graph
        .bindings
        .iter()
        .any(|binding| binding.name == "ready" && binding.kind == ReactiveBindingKind::Ref),
    "return useInner() must forward bag fields; bindings={:?}",
    graph.bindings
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.iter().any(|read| read.binding == "data")
        && scope.uncertain_accesses.is_empty()
    }) && graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.iter().any(|read| read.binding == "ready")
        && scope.uncertain_accesses.is_empty()
    }),
    "forwarded destructure .value must classify; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn mapped_ref_return_type_opens_destructure_keys() {
  let modules = [
    ModuleSource::standalone(
      "producer.d.ts",
      "import type { Ref } from 'vue';\n\
       type Result = { data: string; isLoading: boolean; refetch: () => void };\n\
       type OpenBag = { [K in keyof Result]: K extends 'refetch' ? Result[K] : Ref<Result[K]> };\n\
       export declare function useRemote(): OpenBag;\n",
      "d.ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { useRemote } from './producer';\n\
       const { data, isLoading } = useRemote();\n\
       const rows = computed(() => data.value);\n\
       const pending = computed(() => isLoading.value);\n",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "./producer".into(),
    to: "producer.d.ts".into(),
  }];
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
    "mapped Ref return must open destructure keys; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn return_call_forwards_imported_composable_across_modules() {
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
      "wrapper.ts",
      "import { useRemote } from './producer';\n\
       export function useWrapped() { return useRemote(); }\n",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { useWrapped } from './wrapper';\n\
       const { data, isLoading } = useWrapped();\n\
       const rows = computed(() => data.value);\n\
       const pending = computed(() => isLoading.value);\n",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links = [
    ModuleLink {
      from: "wrapper.ts".into(),
      specifier: "./producer".into(),
      to: "producer.d.ts".into(),
    },
    ModuleLink {
      from: "consumer.ts".into(),
      specifier: "./wrapper".into(),
      to: "wrapper.ts".into(),
    },
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
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope.reads.iter().any(|read| read.binding == "data")
            && scope.uncertain_accesses.is_empty()
        })
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope.reads.iter().any(|read| read.binding == "isLoading")
            && scope.uncertain_accesses.is_empty()
        })
    }),
    "return useRemote() forward must seed consumer; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn to_refs_return_opens_destructure_keys() {
  let graph = graph(
    "import { reactive, toRefs, computed } from 'vue';\n\
     function useStateBag() {\n\
       const state = reactive({ data: 1, isLoading: false });\n\
       return toRefs(state);\n\
     }\n\
     const { data, isLoading } = useStateBag();\n\
     const rows = computed(() => data.value);\n\
     const pending = computed(() => isLoading.value);",
  );
  assert!(
    graph
      .bindings
      .iter()
      .any(|binding| binding.name == "data" && binding.kind == ReactiveBindingKind::Ref)
      && graph.scopes.iter().any(|scope| {
        scope.kind == TrackingScopeKind::Computed
          && scope.reads.iter().any(|read| read.binding == "data")
          && scope.uncertain_accesses.is_empty()
      }),
    "return toRefs(state) must open destructure; bindings={:?} scopes={:?}",
    graph.bindings,
    graph.scopes
  );
}

#[test]
fn local_factory_ref_return_seeds_computed_dependency() {
  for source in [
    "import { ref, computed } from 'vue'; function useFlag() { const flag = ref(false); return flag; } const isCoarse = useFlag(); const hint = computed(() => isCoarse.value ? 'a' : 'b');",
    "import { ref, computed } from 'vue'; const useFlag = () => ref(false); const isCoarse = useFlag(); const hint = computed(() => (isCoarse.value ? 'a' : 'b'));",
    "import { computed } from 'vue'; function useFlag(): Ref<boolean> { return null as any; } const isCoarse = useFlag(); const hint = computed(() => isCoarse.value ? 'a' : 'b');",
  ] {
    let graph = graph(source);
    assert!(
      graph
        .bindings
        .iter()
        .any(|binding| { binding.name == "isCoarse" && binding.kind == ReactiveBindingKind::Ref }),
      "factory call must seed Ref binding; source={source}; bindings={:?}",
      graph.bindings
    );
    assert!(
      graph.scopes.iter().any(|scope| {
        scope.kind == TrackingScopeKind::Computed
          && scope
            .reads
            .iter()
            .any(|read| read.binding == "isCoarse" && read.property.as_deref() == Some("value"))
      }),
      "computed must read factory ref; source={source}; scopes={:?}",
      graph.scopes
    );
  }
}

#[test]
fn cross_module_factory_ref_return_seeds_consumer() {
  let modules = [
    ModuleSource::standalone(
      "producer.ts",
      "import { ref } from 'vue'; export function useFlag() { const flag = ref(false); return flag; }",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue'; import { useFlag } from './producer'; const isCoarse = useFlag(); const hint = computed(() => isCoarse.value ? 'a' : 'b');",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "./producer".into(),
    to: "producer.ts".into(),
  }];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "isCoarse" && binding.kind == ReactiveBindingKind::Ref)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope
              .reads
              .iter()
              .any(|read| read.binding == "isCoarse" && read.property.as_deref() == Some("value"))
        })
    }),
    "cross-module factory Ref return must seed consumer computed; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn dts_factory_return_type_seeds_consumer() {
  let modules = [
    ModuleSource::standalone(
      "producer.d.ts",
      "import type { Ref } from 'vue'; export declare function useMediaQuery(query: string): Ref<boolean>;",
      "d.ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue'; import { useMediaQuery } from './producer'; const isCoarse = useMediaQuery('(pointer: coarse)'); const hint = computed(() => isCoarse.value ? 'a' : 'b');",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "./producer".into(),
    to: "producer.d.ts".into(),
  }];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "isCoarse" && binding.kind == ReactiveBindingKind::Ref)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope
              .reads
              .iter()
              .any(|read| read.binding == "isCoarse" && read.property.as_deref() == Some("value"))
        })
    }),
    ".d.ts Ref return must seed factory call; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn dts_composable_object_return_seeds_destructure() {
  for producer in [
    "import type { Ref } from 'vue'; export declare function useElementSize(): { width: Ref<number>; height: Ref<number>; stop: () => void };",
    "import type { ShallowRef } from 'vue'; export interface UseElementSizeReturn { width: ShallowRef<number>; height: ShallowRef<number>; stop: () => void } export declare function useElementSize(): UseElementSizeReturn;",
    "import type { Ref } from 'vue'; export type UseElementSizeReturn = { width: Ref<number>; height: Ref<number> }; export declare function useElementSize(): UseElementSizeReturn;",
  ] {
    let modules = [
      ModuleSource::standalone("producer.d.ts", producer, "d.ts", ScriptKind::Script),
      ModuleSource::standalone(
        "consumer.ts",
        "import { watch } from 'vue'; import { useElementSize } from './producer'; const { width: hostWidth, height: hostHeight } = useElementSize(); watch([hostWidth, hostHeight], () => {});",
        "ts",
        ScriptKind::Script,
      ),
    ];
    let links = [ModuleLink {
      from: "consumer.ts".into(),
      specifier: "./producer".into(),
      to: "producer.d.ts".into(),
    }];
    let traced = traced_modules(&modules, &links);
    let consumer = traced.iter().find(|module| module.id == "consumer.ts");
    assert!(
      consumer.is_some_and(|module| {
        module.graph.bindings.iter().any(|binding| {
          binding.name == "hostWidth"
            && matches!(binding.kind, ReactiveBindingKind::Ref | ReactiveBindingKind::ShallowRef)
        }) && module.graph.bindings.iter().any(|binding| {
          binding.name == "hostHeight"
            && matches!(binding.kind, ReactiveBindingKind::Ref | ReactiveBindingKind::ShallowRef)
        }) && !module.graph.bindings.iter().any(|binding| binding.name == "stop")
          && module.graph.scopes.iter().any(|scope| {
            scope.kind == TrackingScopeKind::WatchSources
              && scope.reads.iter().any(|read| read.binding == "hostWidth")
              && scope.reads.iter().any(|read| read.binding == "hostHeight")
              && scope.uncertain_accesses.is_empty()
          })
      }),
      ".d.ts object-bag return must seed renamed destructure + watch; producer={producer}; got {:?}",
      consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
    );
  }
}

#[test]
fn plain_object_declaration_plus_unwrapped_call_seeds_reactive_factory() {
  let declaration = prepared_standalone(
    "producer.d.ts",
    "export interface ColorModeInstance {\n\
       preference: string;\n\
       value: string;\n\
       unknown: boolean;\n\
       forced: boolean;\n\
     }\n\
     export declare const useColorMode: () => ColorModeInstance;\n",
    "d.ts",
  );
  let implementation = prepared_standalone(
    "producer.js",
    "import { useState } from '#imports';\n\
     export const useColorMode = () => {\n\
       return useState('color-mode').value;\n\
     };\n",
    "js",
  );
  let merged = merge_declaration_implementation_summary(
    attached_summary(&declaration).as_ref(),
    attached_summary(&implementation).as_ref(),
  );
  let producer = ModuleSource::standalone(
    "producer.d.ts",
    "export declare const useColorMode: () => ColorModeInstance;",
    "d.ts",
    ScriptKind::Script,
  )
  .with_module_summary(merged);
  let consumer = ModuleSource::standalone(
    "consumer.ts",
    "import { watch } from 'vue';\n\
     import { useColorMode } from './producer';\n\
     const colorMode = useColorMode();\n\
     watch(() => colorMode.value, () => {});\n",
    "ts",
    ScriptKind::Script,
  );
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "./producer".into(),
    to: "producer.d.ts".into(),
  }];
  let traced = traced_modules(&[producer, consumer], &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "colorMode" && binding.kind == ReactiveBindingKind::Reactive)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::WatchSources
            && scope
              .reads
              .iter()
              .any(|read| read.binding == "colorMode" && read.property.as_deref() == Some("value"))
            && scope.uncertain_accesses.is_empty()
        })
    }),
    "plain object + unwrapped call must seed Reactive factory watch; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn needs_implementation_merge_only_for_provisional_halves() {
  let plain = prepared_standalone(
    "plain.d.ts",
    "export interface Mode { value: string; forced: boolean }\n\
     export declare const useColorMode: () => Mode;\n",
    "d.ts",
  );
  assert!(
    attached_summary(&plain).needs_implementation_merge(),
    "DeclaredPlainObjectFactory must request companion merge"
  );

  let number_only =
    prepared_standalone("number.d.ts", "export declare const useFlag: () => number;\n", "d.ts");
  assert!(
    !attached_summary(&number_only).needs_implementation_merge(),
    "non-provisional .d.ts must not pull companion .js (e.g. typescript.js)"
  );

  let ref_factory = prepared_standalone(
    "ref.d.ts",
    "import type { Ref } from 'vue';\n\
     export declare function useFlag(): Ref<boolean>;\n",
    "d.ts",
  );
  assert!(
    !attached_summary(&ref_factory).needs_implementation_merge(),
    "finished Factory seed must not request companion merge"
  );
}

#[test]
fn unwrapped_call_without_plain_object_declaration_stays_quiet() {
  let implementation = prepared_standalone(
    "producer.js",
    "import { useState } from '#imports';\n\
     export const useFlag = () => useState('flag').value;\n",
    "js",
  );
  let declaration =
    prepared_standalone("producer.d.ts", "export declare const useFlag: () => number;\n", "d.ts");
  let merged = merge_declaration_implementation_summary(
    attached_summary(&declaration).as_ref(),
    attached_summary(&implementation).as_ref(),
  );
  assert!(
    !merged.has_reactivity_export_seeds(),
    "number return + unwrapped call must not invent Reactive; summary={merged:?}"
  );
}

#[test]
fn bare_nuxt_imports_link_seeds_reactive_factory_call() {
  let declaration = prepared_standalone(
    "producer.d.ts",
    "export interface Mode { value: string; forced: boolean }\n\
     export declare const useColorMode: () => Mode;\n",
    "d.ts",
  );
  let implementation = prepared_standalone(
    "producer.js",
    "import { useState } from '#imports';\n\
     export const useColorMode = () => useState('color-mode').value;\n",
    "js",
  );
  let merged = merge_declaration_implementation_summary(
    attached_summary(&declaration).as_ref(),
    attached_summary(&implementation).as_ref(),
  );
  let producer = ModuleSource::standalone(
    "producer.d.ts",
    "export declare const useColorMode: () => Mode;",
    "d.ts",
    ScriptKind::Script,
  )
  .with_module_summary(merged);
  let consumer = ModuleSource::standalone(
    "consumer.ts",
    "import { watch } from 'vue';\n\
     const colorMode = useColorMode();\n\
     watch(() => colorMode.value, () => {});\n",
    "ts",
    ScriptKind::Script,
  );
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "#nuxt-imports:useColorMode".into(),
    to: "producer.d.ts".into(),
  }];
  let traced = traced_modules(&[producer, consumer], &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "colorMode" && binding.kind == ReactiveBindingKind::Reactive)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::WatchSources
            && scope.reads.iter().any(|read| read.binding == "colorMode")
            && scope.uncertain_accesses.is_empty()
        })
    }),
    "bare #nuxt-imports Factory(Reactive) must seed watch; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn bare_nuxt_imports_link_seeds_known_exported_const() {
  // `export const sharedHandle = computed(...)` auto-imported as a bare id.
  let producer = prepared_standalone(
    "shared.ts",
    "import { computed, ref } from 'vue';\n\
     const handle = ref('a');\n\
     export const sharedHandle = computed(() => handle.value);\n",
    "ts",
  );
  let consumer = prepared_standalone(
    "consumer.ts",
    "import { computed } from 'vue';\n\
     const key = computed(() => sharedHandle.value ?? '');\n",
    "ts",
  );
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "#nuxt-imports:sharedHandle".into(),
    to: "shared.ts".into(),
  }];
  let traced = traced_modules(&[producer, consumer], &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| {
        binding.name == "sharedHandle" && binding.kind == ReactiveBindingKind::Computed
      }) && (module.graph.edges.iter().any(|edge| edge.from == "key" && edge.to == "sharedHandle")
        || module.graph.scopes.iter().any(|scope| {
          scope.binding.as_deref() == Some("key")
            && scope.reads.iter().any(|read| read.binding == "sharedHandle")
        }))
    }),
    "bare #nuxt-imports Known(Computed) must seed sharedHandle; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.edges, &module.graph.scopes))
  );
}

#[test]
fn return_of_call_initialized_local_forwards_factory() {
  // Same-module: wrapper returns a local filled by another factory call.
  // `export function usePersisted(): Ref<T>` +
  // `export function useSettings() { const s = usePersisted(); return s }`
  let producer = prepared_standalone(
    "storage.ts",
    "import type { Ref } from 'vue';
     import { shallowRef } from 'vue';
     export function usePersisted<T>(init: T): Ref<T> {
       return shallowRef(init);
     }
     export function useSettings() {
       const storage = usePersisted({ theme: 'light' });
       return storage;
     }
",
    "ts",
  );
  let consumer = prepared_standalone(
    "consumer.ts",
    "import { computed } from 'vue';
     const settings = useSettings();
     const theme = computed(() => settings.value.theme);
",
    "ts",
  );
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "#nuxt-imports:useSettings".into(),
    to: "storage.ts".into(),
  }];
  let traced = traced_modules(&[producer, consumer], &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| {
        binding.name == "settings"
          && matches!(
            binding.kind,
            ReactiveBindingKind::Ref
              | ReactiveBindingKind::ShallowRef
              | ReactiveBindingKind::Computed
          )
      }) && module.graph.edges.iter().any(|edge| edge.from == "theme" && edge.to == "settings")
    }),
    "return-of-call-init local must forward factory kind; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.edges))
  );
}

#[test]
fn ternary_export_of_computed_and_factory_seeds_known() {
  // SSR/test: `computed(() => …)`; client: storage factory — both ref-like.
  // `export const bag = cond ? computed(...) : usePersisted(...)` must seed consumers.
  let storage = prepared_standalone(
    "storage.ts",
    "import type { Ref } from 'vue';
     import { shallowRef } from 'vue';
     export function usePersisted<T>(init: T): Ref<T> {
       return shallowRef(init);
     }
",
    "ts",
  );
  let producer = prepared_standalone(
    "producer.ts",
    "import { computed } from 'vue';
     const ssr = false;
     export const sharedBag = ssr
       ? computed(() => ({ a: 1 }))
       : usePersisted({ a: 1 });
",
    "ts",
  );
  let consumer = prepared_standalone(
    "consumer.ts",
    "import { computed } from 'vue';
     const keys = computed(() => Object.keys(sharedBag.value));
",
    "ts",
  );
  let links = [
    ModuleLink {
      from: "producer.ts".into(),
      specifier: "#nuxt-imports:usePersisted".into(),
      to: "storage.ts".into(),
    },
    ModuleLink {
      from: "consumer.ts".into(),
      specifier: "#nuxt-imports:sharedBag".into(),
      to: "producer.ts".into(),
    },
  ];
  let traced = traced_modules(&[storage, producer, consumer], &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| {
        binding.name == "sharedBag"
          && matches!(
            binding.kind,
            ReactiveBindingKind::Ref
              | ReactiveBindingKind::ShallowRef
              | ReactiveBindingKind::Computed
          )
      }) && (module.graph.edges.iter().any(|edge| edge.from == "keys" && edge.to == "sharedBag")
        || module.graph.scopes.iter().any(|scope| {
          scope.binding.as_deref() == Some("keys")
            && scope.reads.iter().any(|read| read.binding == "sharedBag")
        }))
    }),
    "ternary computed|factory export must seed Known ref-like; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.edges, &module.graph.scopes))
  );
}

#[test]
fn overload_prefers_factory_scalar_over_controls_bag() {
  // VueUse-style ambient overloads: default `(): Ref<T>`, controls bag last.
  // Last-wins used to keep only the bag so `const x = useClock()` never seeded Ref.
  let producer = prepared_standalone(
    "clock.d.ts",
    "import type { Ref } from 'vue';
     export interface UseClockOptions { interval?: number; controls?: boolean }
     export declare function useClock(options?: UseClockOptions): Ref<Date>;
     export declare function useClock(options: UseClockOptions & { controls: true }): {
       now: Ref<Date>;
       pause: () => void;
       resume: () => void;
     };
",
    "d.ts",
  );
  let consumer = prepared_standalone(
    "consumer.ts",
    "import { computed } from 'vue';
     const now = useClock({ interval: 1000 });
     const stamp = computed(() => now.value.getTime());
",
    "ts",
  );
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "#nuxt-imports:useClock".into(),
    to: "clock.d.ts".into(),
  }];
  let traced = traced_modules(&[producer, consumer], &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| {
        binding.name == "now"
          && matches!(
            binding.kind,
            ReactiveBindingKind::Ref
              | ReactiveBindingKind::ShallowRef
              | ReactiveBindingKind::Computed
          )
      }) && (module.graph.edges.iter().any(|edge| edge.from == "stamp" && edge.to == "now")
        || module.graph.scopes.iter().any(|scope| {
          scope.binding.as_deref() == Some("stamp")
            && scope.reads.iter().any(|read| read.binding == "now")
        }))
    }),
    "scalar Factory overload must win over controls bag; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.edges, &module.graph.scopes))
  );
}

#[test]
fn bare_auto_import_factory_seeds_through_ref_like_ternary() {
  // `const flag = ssr ? ref(false) : usePref()` with bare Factory(Computed) helper.
  let helper = prepared_standalone(
    "helper.ts",
    "import type { ComputedRef } from 'vue';
     import { computed } from 'vue';
     export function usePref(): ComputedRef<boolean> {
       return computed(() => false);
     }
",
    "ts",
  );
  let consumer = prepared_standalone(
    "consumer.ts",
    "import { ref, computed } from 'vue';
     const ssr = true;
     const flag = ssr ? ref(false) : usePref();
     const label = computed(() => (flag.value ? 'a' : 'b'));
",
    "ts",
  );
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "#nuxt-imports:usePref".into(),
    to: "helper.ts".into(),
  }];
  let traced = traced_modules(&[helper, consumer], &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| {
        binding.name == "flag"
          && matches!(
            binding.kind,
            ReactiveBindingKind::Ref
              | ReactiveBindingKind::ShallowRef
              | ReactiveBindingKind::Computed
          )
      }) && (module.graph.edges.iter().any(|edge| edge.from == "label" && edge.to == "flag")
        || module.graph.scopes.iter().any(|scope| {
          scope.binding.as_deref() == Some("label")
            && scope.reads.iter().any(|read| read.binding == "flag")
        }))
    }),
    "ref-like ternary with bare factory arm must seed flag; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.edges, &module.graph.scopes))
  );
}

#[test]
fn return_of_call_init_forwards_via_bare_auto_import_callee() {
  // Cross-module: wrapper calls a bare auto-import helper, returns the local.
  // ForwardReturn(helper) must resolve through `#nuxt-imports:helper`, not only ES imports.
  let helper = prepared_standalone(
    "helper.ts",
    "import type { Ref } from 'vue';
     import { shallowRef } from 'vue';
     export function usePersisted<T>(init: T): Ref<T> {
       return shallowRef(init);
     }
",
    "ts",
  );
  let wrapper = prepared_standalone(
    "wrapper.ts",
    "export function useSettings() {
       const storage = usePersisted({ theme: 'light' });
       return storage;
     }
",
    "ts",
  );
  let consumer = prepared_standalone(
    "consumer.ts",
    "import { computed } from 'vue';
     const settings = useSettings();
     const theme = computed(() => settings.value.theme);
",
    "ts",
  );
  let links = [
    ModuleLink {
      from: "wrapper.ts".into(),
      specifier: "#nuxt-imports:usePersisted".into(),
      to: "helper.ts".into(),
    },
    ModuleLink {
      from: "consumer.ts".into(),
      specifier: "#nuxt-imports:useSettings".into(),
      to: "wrapper.ts".into(),
    },
  ];
  let traced = traced_modules(&[helper, wrapper, consumer], &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| {
        binding.name == "settings"
          && matches!(
            binding.kind,
            ReactiveBindingKind::Ref
              | ReactiveBindingKind::ShallowRef
              | ReactiveBindingKind::Computed
          )
      }) && module.graph.edges.iter().any(|edge| edge.from == "theme" && edge.to == "settings")
    }),
    "ForwardReturn via bare auto-import callee must seed factory; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.edges))
  );
}

#[test]
fn removable_ref_dts_return_seeds_factory_call() {
  let modules = [
    ModuleSource::standalone(
      "storage.d.ts",
      "import type { RemovableRef } from 'vue';\n\
       export declare function usePersistedState<T>(key: string, init: T): RemovableRef<T>;\n",
      "d.ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { usePersistedState } from './storage';\n\
       const state = usePersistedState('k', { on: false });\n\
       const on = computed(() => state.value.on);\n",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "./storage".into(),
    to: "storage.d.ts".into(),
  }];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "state" && binding.kind == ReactiveBindingKind::Ref)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope
              .reads
              .iter()
              .any(|read| read.binding == "state" && read.property.as_deref() == Some("value"))
            && scope.uncertain_accesses.iter().all(|name| name != "state")
        })
    }),
    "RemovableRef .d.ts return must seed Factory(Ref); consumer={consumer:?}"
  );
}
