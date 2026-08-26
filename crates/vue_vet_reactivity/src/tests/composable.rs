use super::helpers::*;

#[test]
fn joins_composable_instance_member_chains_from_template() {
  use std::collections::BTreeMap;

  let mut graph = graph(
    "import { watchEffect } from 'vue'; const bag = { signal: null }; watchEffect(() => {});",
  );
  // Simulate a module-seeded instance bag without inventing top-level field bindings.
  graph
    .composable_instances
    .insert("bag".into(), BTreeMap::from([("signal".into(), ReactiveBindingKind::Ref)]));
  let template = TemplateFacts {
    elements: Vec::new(),
    expressions: vec![
      TemplateExpressionFact {
        surface: "interpolation".into(),
        expression: "bag.signal".into(),
        span: test_span(0),
        identifiers: Some(vec!["bag".into()]),
      },
      TemplateExpressionFact {
        surface: "if".into(),
        expression: "bag.signal.value".into(),
        span: test_span(10),
        identifiers: Some(vec!["bag".into()]),
      },
      TemplateExpressionFact {
        surface: "interpolation".into(),
        expression: "bag.signal + other".into(),
        span: test_span(20),
        identifiers: Some(vec!["bag".into(), "other".into()]),
      },
    ],
  };
  graph.join_template_reads(&template);
  assert!(
    graph.template_reads.iter().any(|read| read.binding == "signal"
      && read.surface == "interpolation"
      && read.span.offset == 0),
    "pure bag.signal must join the shape field"
  );
  assert!(
    graph.template_reads.iter().any(|read| read.binding == "signal" && read.surface == "if"),
    "pure bag.signal.value must join the shape field"
  );
  assert!(
    !graph.template_reads.iter().any(|read| {
      read.binding == "signal" && read.surface == "interpolation" && read.span.offset == 20
    }),
    "operator-bearing expressions must stay quiet for instance field joins"
  );
}

#[test]
fn return_renames_destructured_composable_field() {
  // `const { items } = useBag(); return { rows: items }` — pending composable field
  // resolves at link time so consumers see `rows` as Ref-like.
  let bag = prepared_standalone(
    "bag.ts",
    "import { computed } from 'vue';
     export function useBag() {
       const items = computed(() => [1, 2]);
       return { items };
     }
",
    "ts",
  );
  let wrapper = prepared_standalone(
    "wrapper.ts",
    "export function useList() {
       const { items } = useBag();
       return { rows: items };
     }
",
    "ts",
  );
  let consumer = prepared_standalone(
    "consumer.ts",
    "import { computed } from 'vue';
     const list = useList();
     const count = computed(() => list.rows.value.length);
",
    "ts",
  );
  let links = [
    ModuleLink {
      from: "wrapper.ts".into(),
      specifier: "#nuxt-imports:useBag".into(),
      to: "bag.ts".into(),
    },
    ModuleLink {
      from: "consumer.ts".into(),
      specifier: "#nuxt-imports:useList".into(),
      to: "wrapper.ts".into(),
    },
  ];
  let traced = traced_modules(&[bag, wrapper, consumer], &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.composable_instances.get("list").is_some_and(|shape| {
        shape.get("rows").is_some_and(|kind| {
          matches!(
            kind,
            ReactiveBindingKind::Ref
              | ReactiveBindingKind::ShallowRef
              | ReactiveBindingKind::Computed
          )
        })
      }) && (module.graph.edges.iter().any(|edge| edge.from == "count" && edge.to == "rows")
        || module.graph.scopes.iter().any(|scope| {
          scope.binding.as_deref() == Some("count")
            && scope.reads.iter().any(|read| read.binding == "rows" || read.binding == "list")
        }))
    }),
    "renamed destructured composable field must seed instance bag; got {:?}",
    consumer.map(|module| {
      (
        &module.graph.composable_instances,
        &module.graph.bindings,
        &module.graph.edges,
        &module.graph.scopes,
      )
    })
  );
}

#[test]
fn local_composable_instance_member_access() {
  for source in [
    "import { ref, watchEffect } from 'vue'; function useSignal() { const signal = ref(0); return { signal }; } const bag = useSignal(); watchEffect(() => bag.signal.value);",
    "import { ref, watchEffect } from 'vue'; const useSignal = () => { const signal = ref(0); return { signal }; }; const bag = useSignal(); watchEffect(() => bag.signal.value);",
    "import { ref, watchEffect } from 'vue'; function useSignal() { return { signal: ref(0) }; } const bag = useSignal(); watchEffect(() => bag.signal.value);",
    "import { ref, watchEffect } from 'vue'; const useSignal = () => ({ signal: ref(0) }); const bag = useSignal(); watchEffect(() => bag.signal.value);",
  ] {
    let graph = graph(source);
    assert!(
      graph.composable_instances.contains_key("bag"),
      "same-file useX() must record the instance bag; source={source}"
    );
    assert!(
      graph.effects.iter().any(|effect| {
        effect.reads.iter().any(|read| {
          read.binding == "signal"
            && read.property.as_deref() == Some("value")
            && read.kind == ReactiveReadKind::Unconditional
        })
      }),
      "bag.signal.value must track for same-file composables; source={source}"
    );
    assert!(
      !graph.bindings.iter().any(|binding| binding.name == "signal"),
      "function-local signal must not become a top-level binding; source={source}"
    );
  }
}

#[test]
fn records_composable_instance_field_writes() {
  #[derive(Clone, Copy)]
  enum Want {
    FieldValue,
    Quiet,
  }
  struct Case {
    label: &'static str,
    source: &'static str,
    want: Want,
  }
  let cases = [
    Case {
      label: "computed writes bag.field.value",
      source: "import { ref, computed } from 'vue';\n\
               function useX() { const field = ref(0); return { field }; }\n\
               const a = ref(0); const bag = useX();\n\
               const c = computed(() => { bag.field.value = a.value; return a.value; });\n\
               void c.value;",
      want: Want::FieldValue,
    },
    Case {
      label: "+= writes bag.field.value",
      source: "import { ref, computed } from 'vue';\n\
               function useX() { const field = ref(0); return { field }; }\n\
               const a = ref(0); const bag = useX();\n\
               const c = computed(() => { bag.field.value += a.value; return a.value; });\n\
               void c.value;",
      want: Want::FieldValue,
    },
    Case {
      label: "++ writes bag.field.value",
      source: "import { ref, computed } from 'vue';\n\
               function useX() { const field = ref(0); return { field }; }\n\
               const a = ref(0); const bag = useX();\n\
               const c = computed(() => { bag.field.value++; return a.value; });\n\
               void c.value;",
      want: Want::FieldValue,
    },
    Case {
      label: "peeled (bag.field).value write",
      source: "import { ref, computed } from 'vue';\n\
               function useX() { const field = ref(0); return { field }; }\n\
               const a = ref(0); const bag = useX();\n\
               const c = computed(() => { (bag.field).value = a.value; return a.value; });\n\
               void c.value;",
      want: Want::FieldValue,
    },
    Case {
      label: "TS-wrapped (bag.field as any).value write",
      source: "import { ref, computed } from 'vue';\n\
               function useX() { const field = ref(0); return { field }; }\n\
               const a = ref(0); const bag = useX();\n\
               const c = computed(() => { (bag.field as any).value = a.value; return a.value; });\n\
               void c.value;",
      want: Want::FieldValue,
    },
    Case {
      label: "helper load() writes bag.field.value",
      source: "import { ref, computed } from 'vue';\n\
               function useX() { const field = ref(0); return { field }; }\n\
               const a = ref(0); const bag = useX();\n\
               function load() { bag.field.value = a.value; return a.value; }\n\
               const c = computed(() => load());\n\
               void c.value;",
      want: Want::FieldValue,
    },
    Case {
      label: "identifier getter writes bag.field.value",
      source: "import { ref, computed } from 'vue';\n\
               function useX() { const field = ref(0); return { field }; }\n\
               const a = ref(0); const bag = useX();\n\
               function load() { bag.field.value = a.value; return a.value; }\n\
               const c = computed(load);\n\
               void c.value;",
      want: Want::FieldValue,
    },
    Case {
      label: "replacing the ref bag.field = … stays quiet",
      source: "import { ref, computed } from 'vue';\n\
               function useX() { const field = ref(0); return { field }; }\n\
               const a = ref(0); const bag = useX();\n\
               const c = computed(() => { bag.field = a; return a.value; });\n\
               void c.value;",
      want: Want::Quiet,
    },
    Case {
      label: "computed key bag['field'].value stays quiet",
      source: "import { ref, computed } from 'vue';\n\
               function useX() { const field = ref(0); return { field }; }\n\
               const a = ref(0); const bag = useX();\n\
               const c = computed(() => { bag['field'].value = a.value; return a.value; });\n\
               void c.value;",
      want: Want::Quiet,
    },
    Case {
      label: "unknown bag stays quiet",
      source: "import { ref, computed } from 'vue';\n\
               const a = ref(0);\n\
               const other = { field: { value: 0 } };\n\
               const c = computed(() => { other.field.value = a.value; return a.value; });\n\
               void c.value;",
      want: Want::Quiet,
    },
    Case {
      label: "unknown field stays quiet",
      source: "import { ref, computed } from 'vue';\n\
               function useX() { const field = ref(0); return { field }; }\n\
               const bag = useX();\n\
               const c = computed(() => { bag.missing.value = 1; return 1; });\n\
               void c.value;",
      want: Want::Quiet,
    },
    Case {
      label: "logical &&= on bag.field.value stays quiet",
      source: "import { ref, computed } from 'vue';\n\
               function useX() { const field = ref(0); return { field }; }\n\
               const bag = useX();\n\
               const c = computed(() => { bag.field.value &&= 1; return 1; });\n\
               void c.value;",
      want: Want::Quiet,
    },
    Case {
      label: "three-level bag.nested.field.value stays quiet",
      source: "import { ref, computed } from 'vue';\n\
               function useX() { const field = ref(0); return { nested: { field } }; }\n\
               const bag = useX();\n\
               const c = computed(() => { bag.nested.field.value = 1; return 1; });\n\
               void c.value;",
      want: Want::Quiet,
    },
  ];
  for case in cases {
    let graph = graph(case.source);
    assert_eq!(graph.version, vue_vet_core::REACTIVITY_GRAPH_VERSION);
    let scope = helper_follow_scope(&graph, TrackingScopeKind::Computed);
    match case.want {
      Want::FieldValue => {
        assert!(
          scope.is_some_and(|scope| {
            scope
              .writes
              .iter()
              .any(|write| write.binding == "field" && write.property.as_deref() == Some("value"))
          }),
          "{}: scopes={:?}",
          case.label,
          graph.scopes
        );
      }
      Want::Quiet => {
        assert!(
          scope.is_none_or(|scope| scope.writes.iter().all(|write| write.binding != "field")),
          "{}: scopes={:?}",
          case.label,
          graph.scopes
        );
      }
    }
  }
}

#[test]
fn composable_instance_field_write_works_with_sfc_script_offset() {
  let script = "import { ref, computed } from 'vue'\n\
     function useX() { const field = ref(0); return { field }; }\n\
     const a = ref(0)\n\
     const bag = useX()\n\
     const c = computed(() => { bag.field.value = a.value; return a.value })\n";
  let prefix = "<script setup lang=\"ts\">\n";
  let sfc = format!("{prefix}{script}</script>\n<template><p>{{{{ c }}}}</p></template>\n");
  let graph = trace(&sfc, script, prefix.len(), ScriptKind::Setup);
  let scope = helper_follow_scope(&graph, TrackingScopeKind::Computed);
  assert!(
    scope.is_some_and(|scope| {
      scope
        .writes
        .iter()
        .any(|write| write.binding == "field" && write.property.as_deref() == Some("value"))
    }),
    "SFC-offset bag.field.value write must record; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn local_composable_instance_works_with_sfc_script_offset() {
  let script = "import { ref, watchEffect } from 'vue'\n\
     function useSignal() { const signal = ref(0); return { signal }; }\n\
     const bag = useSignal()\n\
     watchEffect(() => { void bag.signal.value })\n";
  let prefix = "<script setup lang=\"ts\">\n";
  let sfc =
    format!("{prefix}{script}</script>\n<template><p>{{{{ bag.signal }}}}</p></template>\n");
  let graph = trace(&sfc, script, prefix.len(), ScriptKind::Setup);
  assert!(
    graph.composable_instances.contains_key("bag"),
    "SFC-offset same-file useX() must record bag; instances={:?}",
    graph.composable_instances
  );
  assert!(
    graph.effects.iter().any(|effect| {
      effect.reads.iter().any(|read| {
        read.binding == "signal"
          && read.property.as_deref() == Some("value")
          && read.kind == ReactiveReadKind::Unconditional
      })
    }),
    "SFC-offset bag.signal.value must track; effects={:?}",
    graph.effects
  );
  // Template join for pure bag.signal after instances are retained.
  let template = TemplateFacts {
    elements: Vec::new(),
    expressions: vec![TemplateExpressionFact {
      surface: "interpolation".into(),
      expression: "bag.signal".into(),
      span: test_span(sfc.find("bag.signal").unwrap_or(0)),
      identifiers: Some(vec!["bag".into()]),
    }],
  };
  let mut joined = graph;
  joined.join_template_reads(&template);
  assert!(
    joined
      .template_reads
      .iter()
      .any(|read| read.binding == "signal" && read.surface == "interpolation"),
    "SFC-offset instance bags must join pure template bag.signal"
  );
}

#[test]
fn local_composable_destructure_seeds_fields() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; function useSignal() { const signal = ref(0); return { signal }; } const { signal } = useSignal(); watchEffect(() => signal.value);",
  );
  assert!(
    graph
      .bindings
      .iter()
      .any(|binding| { binding.name == "signal" && binding.kind == ReactiveBindingKind::Ref }),
    "same-file destructure must seed the field binding"
  );
  assert!(
    graph.effects.iter().any(|effect| {
      effect.reads.iter().any(|read| {
        read.binding == "signal"
          && read.property.as_deref() == Some("value")
          && read.kind == ReactiveReadKind::Unconditional
      })
    }),
    "destructured same-file field must track .value"
  );
}

#[test]
fn return_reactive_spread_opens_unknown_destructure_keys() {
  let graph = graph(
    "import { computed } from 'vue';\n\
     function useTableQuery(run) {\n\
       const list = computed(() => []);\n\
       const queryResult = run();\n\
       void queryResult.data.value;\n\
       return { list, ...queryResult };\n\
     }\n\
     const { list, isLoading } = useTableQuery(() => ({ data: { value: 1 }, isLoading: { value: false } }));\n\
     const ready = computed(() => !isLoading.value && list.value.length >= 0);\n",
  );
  assert!(
    graph
      .bindings
      .iter()
      .any(|binding| { binding.name == "isLoading" && binding.kind == ReactiveBindingKind::Ref }),
    "open reactive spread must seed unknown destructured isLoading; bindings={:?}",
    graph.bindings
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "isLoading" && read.property.as_deref() == Some("value"))
        && scope.uncertain_accesses.is_empty()
    }),
    "isLoading.value must be a proven computed read; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn return_plain_spread_does_not_open_destructure_keys() {
  let graph = graph(
    "import { computed } from 'vue';\n\
     function usePlain() {\n\
       const list = computed(() => []);\n\
       const extras = { flag: true };\n\
       return { list, ...extras };\n\
     }\n\
     const { list, flag } = usePlain();\n\
     const label = computed(() => (flag ? list.value : list.value));\n",
  );
  assert!(
    !graph.bindings.iter().any(|binding| binding.name == "flag"),
    "plain object spread must not invent Ref seeds; bindings={:?}",
    graph.bindings
  );
}

#[test]
fn return_reactive_spread_matches_vue_query_usetable_shape() {
  // vue-query style table helper: optional-chain on data.value + watch reads + spread.
  let producer = "import { computed, ref, watch } from 'vue';\n\
export function useTableQuery(tableQuery) {\n\
  const page = ref(1);\n\
  const queryResult = tableQuery();\n\
  const list = computed(() => queryResult.data.value?.records || []);\n\
  watch(() => queryResult.isSuccess.value && !queryResult.isFetching.value, () => {});\n\
  return { page, list, ...queryResult };\n\
}\n";
  let consumer = "import { computed } from 'vue';\n\
import { useTableQuery } from './producer';\n\
const { list: rows, isLoading: queryLoading } = useTableQuery(() => ({\n\
  data: { value: { records: [] } },\n\
  isSuccess: { value: true },\n\
  isFetching: { value: false },\n\
  isLoading: { value: false },\n\
}));\n\
const isLoading = computed(() => queryLoading.value);\n\
const all = computed(() => rows.value);\n";
  let modules = [
    ModuleSource::standalone("producer.ts", producer, "ts", ScriptKind::Script),
    ModuleSource::standalone("consumer.ts", consumer, "ts", ScriptKind::Script),
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
        .any(|binding| binding.name == "queryLoading" && binding.kind == ReactiveBindingKind::Ref)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope.reads.iter().any(|read| {
              read.binding == "queryLoading" && read.property.as_deref() == Some("value")
            })
            && scope.uncertain_accesses.is_empty()
        })
    }),
    "cross-module ...queryResult must seed isLoading; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn local_instance_does_not_invent_bare_field_reads() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; function useSignal() { const signal = ref(0); return { signal }; } const bag = useSignal(); watchEffect(() => { signal.value; });",
  );
  assert!(graph.composable_instances.contains_key("bag"), "instance bag must still be recorded");
  assert!(
    graph.effects.iter().all(|effect| effect.reads.iter().all(|read| read.binding != "signal")),
    "bare signal.value must stay quiet without a local signal binding"
  );
}

#[test]
fn seeds_composable_instance_member_access() {
  let modules = [
    ModuleSource::standalone(
      "producer.ts",
      "import { ref } from 'vue'; export function useSignal() { const signal = ref(0); return { signal }; }",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { watchEffect } from 'vue'; import { useSignal } from './producer'; const bag = useSignal(); watchEffect(() => bag.signal.value);",
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
      module.graph.effects.iter().any(|effect| {
        effect
          .reads
          .iter()
          .any(|read| read.binding == "signal" && read.kind == ReactiveReadKind::Unconditional)
      })
    }),
    "const bag = useX(); bag.field.value must seed across modules"
  );
}

#[test]
fn seeds_export_const_arrow_and_function_composable_instances() {
  for (label, producer) in [
    (
      "export-const-arrow",
      "import { ref } from 'vue'; export const useSignal = () => ({ signal: ref(0) });",
    ),
    (
      "export-const-function",
      "import { ref } from 'vue'; export const useSignal = function () { const signal = ref(0); return { signal }; };",
    ),
  ] {
    let modules = [
      ModuleSource::standalone("producer.ts", producer, "ts", ScriptKind::Script),
      ModuleSource::standalone(
        "consumer.ts",
        "import { watchEffect } from 'vue'; import { useSignal } from './producer'; const bag = useSignal(); watchEffect(() => bag.signal.value);",
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
        module.graph.composable_instances.contains_key("bag")
          && module.graph.effects.iter().any(|effect| {
            effect.reads.iter().any(|read| {
              read.binding == "signal"
                && read.property.as_deref() == Some("value")
                && read.kind == ReactiveReadKind::Unconditional
            })
          })
      }),
      "{label}: export const useX must seed bag.signal across modules; got {:?}",
      consumer.map(|module| {
        (
          module.graph.composable_instances.clone(),
          module
            .graph
            .effects
            .iter()
            .flat_map(|effect| effect.reads.iter().map(|read| read.binding.clone()))
            .collect::<Vec<_>>(),
        )
      })
    );
  }
}

#[test]
fn seeds_default_export_function_composable_instance() {
  let modules = [
    ModuleSource::standalone(
      "producer.ts",
      "import { ref } from 'vue'; export default function useSignal() { return { signal: ref(0) }; }",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { watchEffect } from 'vue'; import useSignal from './producer'; const bag = useSignal(); watchEffect(() => bag.signal.value);",
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
      module.graph.composable_instances.contains_key("bag")
        && module.graph.effects.iter().any(|effect| {
          effect.reads.iter().any(|read| {
            read.binding == "signal"
              && read.property.as_deref() == Some("value")
              && read.kind == ReactiveReadKind::Unconditional
          })
        })
    }),
    "export default function useX must seed bag.signal; got {:?}",
    consumer.map(|module| {
      (
        module.graph.composable_instances.clone(),
        module
          .graph
          .effects
          .iter()
          .flat_map(|effect| effect.reads.iter().map(|read| read.binding.clone()))
          .collect::<Vec<_>>(),
      )
    })
  );
}

#[test]
fn instance_seed_does_not_pollute_top_level_bindings() {
  let modules = [
    ModuleSource::standalone(
      "producer.ts",
      "import { ref } from 'vue'; export function useSignal() { const signal = ref(0); return { signal }; }",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      // Bare `signal.value` must stay quiet: the consumer never bound `signal`.
      // Only `bag.signal.value` is a real edge (covered by the test above).
      "import { watchEffect } from 'vue'; import { useSignal } from './producer'; const bag = useSignal(); watchEffect(() => { signal.value; });",
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
      !module.graph.bindings.iter().any(|binding| binding.name == "signal")
        && module
          .graph
          .effects
          .iter()
          .all(|effect| effect.reads.iter().all(|read| read.binding != "signal"))
    }),
    "instance seeds must not invent top-level bindings for composable shape fields; got {:?}",
    consumer.map(|module| {
      (
        module.graph.bindings.iter().map(|b| b.name.clone()).collect::<Vec<_>>(),
        module
          .graph
          .effects
          .iter()
          .flat_map(|e| e.reads.iter().map(|r| r.binding.clone()))
          .collect::<Vec<_>>(),
      )
    })
  );
}

#[test]
fn seeds_destructured_composable_fields_in_sfc_script_with_offset() {
  let producer = ModuleSource::standalone(
    "composables/useField.ts",
    "import { toRef } from 'vue'; export function useField(props: { title: string }) { return { title: toRef(props, 'title') }; }",
    "ts",
    ScriptKind::Script,
  );
  let script = "import { watchEffect } from 'vue';\nimport { useField } from './composables/useField';\nconst props = { title: 'x' };\nconst { title } = useField(props);\nwatchEffect(async () => {\n  await Promise.resolve();\n  console.log(title.value);\n});\n";
  let prefix = "<script setup lang=\"ts\">\n";
  let sfc = format!("{prefix}{script}</script>\n<template><p>{{{{ title }}}}</p></template>\n");
  let consumer =
    ModuleSource::sfc_script("App.vue", script, "ts", ScriptKind::Setup, prefix.len(), sfc);
  let links = [ModuleLink {
    from: "App.vue".into(),
    specifier: "./composables/useField".into(),
    to: "composables/useField.ts".into(),
  }];
  let traced = traced_modules(&[producer, consumer], &links);
  let app = traced.iter().find(|module| module.id == "App.vue");
  assert!(
    app.is_some_and(|module| {
      module.graph.bindings.iter().any(|binding| binding.name == "title")
        && module.graph.effects.iter().any(|effect| {
          effect
            .reads
            .iter()
            .any(|read| read.binding == "title" && read.kind == ReactiveReadKind::AfterAwait)
        })
    }),
    "SFC-offset seeds must resolve title.value reads after await; got {:?}",
    app.map(|module| (
      module.graph.bindings.iter().map(|b| (b.name.clone(), b.span.offset)).collect::<Vec<_>>(),
      module
        .graph
        .effects
        .iter()
        .map(|e| e.reads.iter().map(|r| (r.binding.clone(), r.kind)).collect::<Vec<_>>())
        .collect::<Vec<_>>()
    ))
  );
}
