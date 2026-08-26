use super::helpers::*;

#[test]
fn traces_core_reactivity_primitives() {
  let graph = graph(
    "import { ref, shallowRef, computed, reactive, shallowReactive } from 'vue';\n\
     const a = ref(0); const b = shallowRef(0); const c = computed(() => 0);\n\
     const d = reactive({ value: 0 }); const e = shallowReactive({ value: 0 });",
  );
  assert_eq!(
    graph.bindings.iter().map(|binding| binding.kind).collect::<Vec<_>>(),
    [
      ReactiveBindingKind::Ref,
      ReactiveBindingKind::ShallowRef,
      ReactiveBindingKind::Computed,
      ReactiveBindingKind::Reactive,
      ReactiveBindingKind::ShallowReactive,
    ],
    "all core primitives must become graph nodes"
  );
}

#[test]
fn traces_extended_reactivity_primitives() {
  let graph = graph(
    "import { readonly, shallowReadonly, customRef, toRef, useTemplateRef } from 'vue';\n\
     const a = readonly({ value: 0 }); const b = shallowReadonly({ value: 0 });\n\
     const c = customRef(() => ({ get: () => 0, set: () => {} }));\n\
     const d = toRef({ value: 0 }, 'value'); const e = useTemplateRef('input');",
  );
  assert_eq!(
    graph.bindings.iter().map(|binding| binding.kind).collect::<Vec<_>>(),
    [
      ReactiveBindingKind::Readonly,
      ReactiveBindingKind::ShallowReadonly,
      ReactiveBindingKind::CustomRef,
      ReactiveBindingKind::ToRef,
      ReactiveBindingKind::TemplateRef,
    ],
    "extended Vue primitives must become graph nodes"
  );
}

#[test]
fn resolves_aliased_primitives_and_effects() {
  let graph = graph(
    "import { ref as signal, watchEffect as effect } from 'vue';\n\
     const value = signal(0); effect(() => value.value);",
  );
  assert_eq!(
    graph.bindings.first().map(|binding| binding.kind),
    Some(ReactiveBindingKind::Ref),
    "aliased ref must resolve through import facts"
  );
  assert_eq!(
    graph.effects.first().map(|effect| effect.callee.as_str()),
    Some("watchEffect"),
    "aliased effect must retain its canonical callee"
  );
}

#[test]
fn resolves_vue_namespace_calls() {
  let graph = graph(
    "import * as Vue from 'vue';\n\
     const value = Vue.ref(0); Vue.watchEffect(() => value.value);",
  );
  assert_eq!(graph.bindings.len(), 1, "namespace primitive must be traced");
  assert_eq!(graph.effects.len(), 1, "namespace effect must be traced");
}

#[test]
fn resolves_explicit_nuxt_imports() {
  let graph = graph(
    "import { ref, watchEffect } from '#imports';\n\
     const value = ref(0); watchEffect(() => value.value);",
  );
  assert_eq!(graph.bindings.len(), 1, "explicit Nuxt imports must be traced");
  assert_eq!(graph.effects.len(), 1, "explicit Nuxt effects must be traced");
}

#[test]
fn resolves_bare_vue_auto_imports_without_import() {
  let graph = graph(
    "const host = ref<HTMLElement | null>(null);\n\
     const failed = ref(false);\n\
     watchEffect(() => { void failed.value; });",
  );
  assert!(
    graph
      .bindings
      .iter()
      .any(|binding| { binding.name == "host" && binding.kind == ReactiveBindingKind::Ref }),
    "Nuxt-style bare ref() must create bindings: {:?}",
    graph.bindings
  );
  assert!(
    graph
      .bindings
      .iter()
      .any(|binding| { binding.name == "failed" && binding.kind == ReactiveBindingKind::Ref }),
    "second bare ref() must also bind: {:?}",
    graph.bindings
  );
  assert_eq!(graph.effects.len(), 1, "bare watchEffect must create a tracking scope");
}

#[test]
fn ignores_local_lookalike_functions() {
  let graph = graph(
    "function ref(value: number) { return { value }; }\n\
     function watchEffect(callback: () => void) { callback(); }\n\
     const value = ref(0); watchEffect(() => value.value);",
  );
  assert!(graph.bindings.is_empty(), "local ref lookalikes must not create nodes");
  assert!(graph.effects.is_empty(), "local effect lookalikes must not create edges");
}

#[test]
fn expands_to_refs_destructuring() {
  let graph = graph(
    "import { toRefs } from 'vue';\n\
     const props = { foo: 1, bar: 2 }; const { foo, bar: renamed } = toRefs(props);",
  );
  assert_eq!(
    graph.bindings.iter().map(|binding| binding.name.as_str()).collect::<Vec<_>>(),
    ["foo", "renamed"],
    "every toRefs binding must receive its own ref node"
  );
  assert!(
    graph.bindings.iter().all(|binding| binding.kind == ReactiveBindingKind::ToRef),
    "toRefs destructuring must produce ref-like nodes"
  );
}

#[test]
fn traces_define_model_in_script_setup() {
  let graph = graph("const model = defineModel<string>();");
  assert_eq!(
    graph.bindings.first().map(|binding| binding.kind),
    Some(ReactiveBindingKind::ModelRef),
    "defineModel must be recognized as a setup compiler macro"
  );
}

#[test]
fn expands_define_models_destructuring() {
  let graph = graph(
    "const { modelValue, open: isOpen } = defineModels<{\n\
       modelValue: string\n\
       open: boolean\n\
     }>()",
  );
  assert_eq!(
    graph.bindings.iter().map(|binding| binding.name.as_str()).collect::<Vec<_>>(),
    ["modelValue", "isOpen"],
    "defineModels destructure must seed each local model ref"
  );
  assert!(
    graph.bindings.iter().all(|binding| binding.kind == ReactiveBindingKind::ModelRef),
    "defineModels locals must be ModelRef; got {:?}",
    graph.bindings
  );
}

#[test]
fn ignores_define_model_outside_script_setup() {
  let source = "const model = defineModel<string>();";
  let graph = trace(source, source, 0, ScriptKind::Script);
  assert!(
    graph.bindings.is_empty(),
    "defineModel must not be assumed to be a compiler macro in a normal script"
  );
}

#[test]
fn ignores_define_models_outside_script_setup() {
  let source = "const { modelValue } = defineModels<{ modelValue: string }>()";
  let graph = trace(source, source, 0, ScriptKind::Script);
  assert!(
    graph.bindings.is_empty(),
    "defineModels must not be assumed to be a compiler macro in a normal script"
  );
}

#[test]
fn retains_all_watch_effect_families() {
  let graph = graph(
    "import { ref, watchEffect, watchPostEffect, watchSyncEffect } from 'vue';\n\
     const value = ref(0); watchEffect(() => value.value);\n\
     watchPostEffect(() => value.value); watchSyncEffect(() => value.value);",
  );
  assert_eq!(
    graph.effects.iter().map(|effect| effect.callee.as_str()).collect::<Vec<_>>(),
    ["watchEffect", "watchPostEffect", "watchSyncEffect"],
    "all watchEffect timing variants must be traced"
  );
}

#[test]
fn supports_function_expression_callbacks() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue';\n\
     const value = ref(0); watchEffect(function () { console.log(value.value); });",
  );
  assert_eq!(
    graph.effects.first().map(|effect| effect.reads.len()),
    Some(1),
    "function expression callbacks must be analyzed"
  );
}

#[test]
fn retains_unconditional_reads() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue';\n\
     const value = ref(0); watchEffect(() => console.log(value.value));",
  );
  let read = graph.effects.first().and_then(|effect| effect.reads.first());
  assert_eq!(
    read.map(|read| (read.binding.as_str(), read.property.as_deref(), read.kind)),
    Some(("value", Some("value"), ReactiveReadKind::Unconditional)),
    "unconditional dependencies must remain visible"
  );
}

#[test]
fn classifies_single_early_return_guard() {
  let graph = graph(
    "import { computed, ref, watchEffect } from 'vue';\n\
     const ready = computed(() => true); const value = ref(0);\n\
     watchEffect(() => { if (!ready.value) return; console.log(value.value); });",
  );
  let value = graph
    .effects
    .first()
    .into_iter()
    .flat_map(|effect| &effect.reads)
    .find(|read| read.binding == "value");
  assert_eq!(
    value.map(|read| (read.kind, read.guards.first().map(|guard| guard.binding.as_str()))),
    Some((ReactiveReadKind::Conditional, Some("ready"))),
    "the downstream dependency must retain guard evidence"
  );
}

#[test]
fn retains_all_sequential_guards() {
  let graph = graph(
    "import { computed, ref, watchEffect } from 'vue';\n\
     const ready = computed(() => true); const enabled = ref(true); const value = ref(0);\n\
     watchEffect(() => { if (!ready.value) return; if (!enabled.value) return; value.value; });",
  );
  let guards = graph
    .effects
    .first()
    .into_iter()
    .flat_map(|effect| &effect.reads)
    .find(|read| read.binding == "value")
    .map(|read| read.guards.iter().map(|guard| guard.binding.as_str()).collect::<Vec<_>>());
  assert_eq!(
    guards,
    Some(vec!["ready", "enabled"]),
    "sequential early returns must preserve every guard in source order"
  );
}

#[test]
fn classifies_if_consequent_reads() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; const ready = ref(false); const value = ref(0);\n\
     watchEffect(() => { if (ready.value) console.log(value.value); });",
  );
  assert!(
    graph.effects.first().is_some_and(|effect| {
      effect
        .reads
        .iter()
        .any(|read| read.binding == "value" && read.kind == ReactiveReadKind::Conditional)
    }),
    "reads in an if consequent must be conditional"
  );
}

#[test]
fn classifies_if_alternate_reads() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; const ready = ref(false); const fallback = ref(0);\n\
     watchEffect(() => { if (ready.value) return; else console.log(fallback.value); });",
  );
  assert!(
    graph.effects.first().is_some_and(|effect| {
      effect
        .reads
        .iter()
        .any(|read| read.binding == "fallback" && read.kind == ReactiveReadKind::Conditional)
    }),
    "reads in an if alternate must be conditional"
  );
}

#[test]
fn classifies_logical_short_circuit_reads() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; const ready = ref(false); const value = ref(0);\n\
     watchEffect(() => ready.value && console.log(value.value));",
  );
  let value = graph
    .effects
    .first()
    .into_iter()
    .flat_map(|effect| &effect.reads)
    .find(|read| read.binding == "value");
  assert_eq!(
    value.map(|read| read.guarded_by.as_deref()),
    Some(Some("ready")),
    "the logical right-hand side must retain the left-hand dependency as its guard"
  );
}

#[test]
fn classifies_ternary_branch_reads() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; const ready = ref(false);\n\
     const yes = ref(1); const no = ref(0);\n\
     watchEffect(() => ready.value ? yes.value : no.value);",
  );
  assert_eq!(
    graph
      .effects
      .first()
      .into_iter()
      .flat_map(|effect| &effect.reads)
      .filter(|read| read.kind == ReactiveReadKind::Conditional)
      .count(),
    2,
    "both ternary branches must be conditional"
  );
}

#[test]
fn same_binding_on_both_ternary_arms_is_unconditional() {
  // `cond ? x.value : x.value` always tracks `x` — not a conditional-only dependency.
  let nested = graph(
    "import { ref, computed } from 'vue';\n\
     const prefer = ref(false);\n\
     const account = ref({ avatar: 'a', alt: 'b' });\n\
     const src = computed(() =>\n\
       prefer.value ? account.value.avatar : account.value.alt);\n",
  );
  // account.value is read in both arms (object of .avatar / .alt) — every path tracks it.
  assert!(
    nested.scopes.iter().any(|scope| {
      scope.binding.as_deref() == Some("src")
        && scope.reads.iter().any(|read| {
          read.binding == "account"
            && read.property.as_deref() == Some("value")
            && read.kind == ReactiveReadKind::Unconditional
        })
        && !scope.reads.iter().any(|read| {
          read.binding == "account"
            && read.property.as_deref() == Some("value")
            && read.kind == ReactiveReadKind::Conditional
        })
    }),
    "account.value on both ternary arms must be unconditional; got {:?}",
    nested.scopes.iter().find(|scope| scope.binding.as_deref() == Some("src"))
  );

  let same = graph(
    "import { ref, computed } from 'vue';\n\
     const prefer = ref(false);\n\
     const label = ref('a');\n\
     const out = computed(() => (prefer.value ? label.value : label.value));\n",
  );
  assert!(
    same.scopes.iter().any(|scope| {
      scope.binding.as_deref() == Some("out")
        && scope.reads.iter().any(|read| {
          read.binding == "label"
            && read.property.as_deref() == Some("value")
            && read.kind == ReactiveReadKind::Unconditional
        })
        && !scope.reads.iter().any(|read| {
          read.binding == "label"
            && read.property.as_deref() == Some("value")
            && read.kind == ReactiveReadKind::Conditional
        })
    }),
    "identical arm reads must be unconditional; got {:?}",
    same.scopes.iter().find(|scope| scope.binding.as_deref() == Some("out"))
  );
}

#[test]
fn same_binding_on_both_if_else_arms_is_unconditional() {
  let traced = graph(
    "import { ref, computed } from 'vue';\n\
     const ready = ref(false);\n\
     const count = ref(0);\n\
     const label = computed(() => {\n\
       if (ready.value) return String(count.value);\n\
       else return String(count.value);\n\
     });\n",
  );
  assert!(
    traced.scopes.iter().any(|scope| {
      scope.binding.as_deref() == Some("label")
        && scope.reads.iter().any(|read| {
          read.binding == "count"
            && read.property.as_deref() == Some("value")
            && read.kind == ReactiveReadKind::Unconditional
        })
        && !scope.reads.iter().any(|read| {
          read.binding == "count"
            && read.property.as_deref() == Some("value")
            && read.kind == ReactiveReadKind::Conditional
        })
    }),
    "if/else both arms reading count.value must be unconditional; got {:?}",
    traced.scopes.iter().find(|scope| scope.binding.as_deref() == Some("label"))
  );
}

#[test]
fn seeds_ternary_init_when_both_arms_are_ref_like() {
  // `const flag = cond ? ref(false) : shallowRef(true)` — both arms reactive.
  let graph = graph(
    "import { ref, shallowRef, computed } from 'vue';\n\
     const ssr = true;\n\
     const flag = ssr ? ref(false) : shallowRef(true);\n\
     const label = computed(() => (flag.value ? 'a' : 'b'));",
  );
  assert!(
    graph.bindings.iter().any(|binding| {
      binding.name == "flag"
        && matches!(binding.kind, ReactiveBindingKind::Ref | ReactiveBindingKind::ShallowRef)
    }),
    "ternary of ref-like arms must seed the binding; got {:?}",
    graph.bindings
  );
  assert!(
    graph.edges.iter().any(|edge| edge.from == "label" && edge.to == "flag")
      || graph.scopes.iter().any(|scope| {
        scope.binding.as_deref() == Some("label")
          && scope.reads.iter().any(|read| read.binding == "flag")
      }),
    "ternary-seeded flag must be a computed dependency; got {:?}",
    (&graph.edges, &graph.scopes)
  );
}

#[test]
fn ternary_init_stays_quiet_when_one_arm_is_plain() {
  // Under-approx: do not invent a Ref binding from a single reactive arm.
  let graph = graph(
    "import { ref, computed } from 'vue';\n\
     const ssr = true;\n\
     const flag = ssr ? ref(false) : false;\n\
     const label = computed(() => String(flag));",
  );
  assert!(
    !graph.bindings.iter().any(|binding| binding.name == "flag"),
    "mixed reactive/plain ternary must stay quiet; got {:?}",
    graph.bindings
  );
}

#[test]
fn excludes_reads_inside_nested_callbacks() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; const outer = ref(0); const nested = ref(0);\n\
     watchEffect(() => { outer.value; const later = () => nested.value; void later; });",
  );
  assert_eq!(
    graph
      .effects
      .first()
      .into_iter()
      .flat_map(|effect| &effect.reads)
      .map(|read| read.binding.as_str())
      .collect::<Vec<_>>(),
    ["outer"],
    "nested callbacks execute outside the parent effect's direct tracking context"
  );
}

#[test]
fn excludes_simple_assignment_targets() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; const value = ref(0);\n\
     watchEffect(() => { value.value = 1; });",
  );
  assert!(
    graph.effects.first().is_some_and(|effect| effect.reads.is_empty()),
    "a simple assignment target is write-only"
  );
}

#[test]
fn retains_compound_and_update_reads() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; const value = ref(0);\n\
     watchEffect(() => { value.value += 1; value.value++; });",
  );
  assert_eq!(
    graph.effects.first().map(|effect| effect.reads.len()),
    Some(2),
    "compound assignments and updates both read their previous value"
  );
}

#[test]
fn records_compound_and_update_writes() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; const value = ref(0);\n\
     watchEffect(() => { value.value += 1; value.value++; });",
  );
  assert_eq!(graph.version, vue_vet_core::REACTIVITY_GRAPH_VERSION);
  let scope = graph.scopes.iter().find(|scope| scope.kind == TrackingScopeKind::WatchEffect);
  let writes = scope.map(|scope| {
    scope
      .writes
      .iter()
      .filter(|write| write.binding == "value" && write.property.as_deref() == Some("value"))
      .count()
  });
  assert_eq!(writes, Some(2), "+= and ++ must record writes; scopes={:?}", graph.scopes);
  assert_eq!(scope.map(|scope| scope.assignment_only), Some(true));
}

#[test]
fn logical_compound_assignment_does_not_invent_a_write() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; const value = ref(0);\n\
     watchEffect(() => { value.value &&= 1; });",
  );
  let scope = graph.scopes.iter().find(|scope| scope.kind == TrackingScopeKind::WatchEffect);
  assert!(
    scope.is_some_and(|scope| scope.writes.iter().all(|write| write.binding != "value")),
    "logical &&= may not write; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn classifies_reads_after_top_level_await() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; const before = ref(0); const after = ref(0);\n\
     watchEffect(async () => { before.value; await Promise.resolve(); after.value; });",
  );
  let kinds = graph
    .effects
    .first()
    .into_iter()
    .flat_map(|effect| &effect.reads)
    .map(|read| (read.binding.as_str(), read.kind))
    .collect::<Vec<_>>();
  assert_eq!(
    kinds,
    [("before", ReactiveReadKind::Unconditional), ("after", ReactiveReadKind::AfterAwait),],
    "only reads after the synchronous tracking boundary must be marked after-await"
  );
}

#[test]
fn ignores_await_inside_nested_callbacks() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; const value = ref(0);\n\
     watchEffect(() => { const later = async () => { await Promise.resolve(); };\n\
       value.value; void later; });",
  );
  assert_eq!(
    graph.effects.first().and_then(|effect| effect.reads.first()).map(|read| read.kind),
    Some(ReactiveReadKind::Unconditional),
    "nested async work must not create a tracking boundary in the parent callback"
  );
}

#[test]
fn retains_static_and_dynamic_properties() {
  let graph = graph(
    "import { reactive, watchEffect } from 'vue'; const state = reactive({ count: 0 });\n\
     const key = 'count'; watchEffect(() => { state.count; state[key]; });",
  );
  assert_eq!(
    graph
      .effects
      .first()
      .into_iter()
      .flat_map(|effect| &effect.reads)
      .map(|read| read.property.as_deref())
      .collect::<Vec<_>>(),
    [Some("count"), None],
    "static and dynamic property edges must remain distinguishable"
  );
}

#[test]
fn retains_read_before_a_later_conditional_read() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; const ready = ref(false); const value = ref(0);\n\
     watchEffect(() => { value.value; if (ready.value) value.value; });",
  );
  let kinds = graph
    .effects
    .first()
    .into_iter()
    .flat_map(|effect| &effect.reads)
    .filter(|read| read.binding == "value")
    .map(|read| read.kind)
    .collect::<Vec<_>>();
  assert_eq!(
    kinds,
    [ReactiveReadKind::Unconditional, ReactiveReadKind::Conditional],
    "the graph must retain occurrences so rule consumers can suppress already-tracked dependencies"
  );
}

#[test]
fn maps_read_and_guard_spans_to_the_sfc() {
  let script = "import { ref, watchEffect } from 'vue'; const ready = ref(false); const value = ref(0);\n\
     watchEffect(() => { if (!ready.value) return; value.value; });";
  let sfc = format!("<template /><script setup lang=\"ts\">{script}</script>");
  let offset = sfc.find(script).unwrap_or_default();
  let graph = trace(&sfc, script, offset, ScriptKind::Setup);
  let read = graph
    .effects
    .first()
    .into_iter()
    .flat_map(|effect| &effect.reads)
    .find(|read| read.binding == "value");
  assert_eq!(
    read.map(|read| read.span.offset),
    sfc.rfind("value.value"),
    "read spans must use original SFC byte offsets"
  );
  assert_eq!(
    read.and_then(|read| read.guards.first()).map(|guard| guard.span.offset),
    sfc.find("ready.value"),
    "guard spans must use original SFC byte offsets"
  );
}

#[test]
fn serializes_deterministically() {
  let source = "import { ref, watchEffect } from 'vue'; const ready = ref(false); const value = ref(0);\n\
     watchEffect(() => { if (!ready.value) return; value.value; });";
  let first = serde_json::to_string(&graph(source));
  let second = serde_json::to_string(&graph(source));
  assert!(first.is_ok(), "the reactivity graph must be serializable");
  assert!(
    matches!((&first, &second), (Ok(first), Ok(second)) if first == second),
    "equivalent graphs must serialize identically"
  );
}

#[test]
fn supports_expression_body_arrows() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; const value = ref(0);\n\
     watchEffect(() => value.value);",
  );
  assert_eq!(
    graph.effects.first().map(|effect| effect.reads.len()),
    Some(1),
    "expression-body arrows must retain their dependency"
  );
}

#[test]
fn retains_empty_effect_nodes() {
  let graph = graph("import { watchEffect } from 'vue'; watchEffect(() => console.log('ready'));");
  assert!(
    graph.effects.first().is_some_and(|effect| effect.reads.is_empty()),
    "recognized effects must remain visible even when they have no reactive reads"
  );
}

#[test]
fn traces_computed_tracking_scopes() {
  let graph = graph(
    "import { computed, ref } from 'vue';\n\
     const ready = ref(false); const value = ref(0);\n\
     const doubled = computed(() => { if (!ready.value) return 0; return value.value; });",
  );
  let scope = graph.scopes.iter().find(|scope| scope.kind == TrackingScopeKind::Computed);
  assert!(scope.is_some(), "computed must become a tracking scope");
  assert!(
    scope.is_some_and(|scope| {
      scope
        .reads
        .iter()
        .any(|read| read.binding == "value" && read.kind == ReactiveReadKind::Conditional)
    }),
    "computed bodies must classify conditional reactive reads"
  );
  assert!(
    graph.effects.iter().all(|effect| effect.callee != "computed"),
    "computed scopes must not project into legacy effects"
  );
}

#[test]
fn traces_watch_source_arrays() {
  let graph = graph(
    "import { ref, watch } from 'vue';\n\
     const a = ref(0); const b = ref(1);\n\
     watch([a, b], () => {});",
  );
  let scope = graph.scopes.iter().find(|scope| scope.kind == TrackingScopeKind::WatchSources);
  let reads = scope.map(|scope| {
    scope
      .reads
      .iter()
      .map(|read| (read.binding.as_str(), read.property.as_deref(), read.kind))
      .collect::<Vec<_>>()
  });
  assert_eq!(
    reads,
    Some(vec![
      ("a", Some("value"), ReactiveReadKind::Unconditional),
      ("b", Some("value"), ReactiveReadKind::Unconditional),
    ]),
    "watch source arrays must record each ref with the runtime .value dep key"
  );
}

#[test]
fn traces_watch_source_getters() {
  let graph = graph(
    "import { ref, watch } from 'vue';\n\
     const value = ref(0); watch(() => value.value, () => {});",
  );
  let scope = graph.scopes.iter().find(|scope| scope.kind == TrackingScopeKind::WatchSources);
  assert!(
    scope.is_some_and(|scope| {
      scope.reads.iter().any(|read| {
        read.binding == "value"
          && read.property.as_deref() == Some("value")
          && read.kind == ReactiveReadKind::Unconditional
      })
    }),
    "watch source getters must track reactive reads"
  );
}

#[test]
fn bare_reactive_watch_source_records_deep_root() {
  let graph = graph(
    "import { reactive, watch } from 'vue';\n\
     const state = reactive({ n: 1 });\n\
     watch(state, () => {});",
  );
  let scope = graph.scopes.iter().find(|scope| scope.kind == TrackingScopeKind::WatchSources);
  assert!(
    scope.is_some_and(|scope| {
      scope.reads.len() == 1
        && scope.reads.iter().any(|read| {
          read.binding == "state"
            && read.property.as_deref() == Some(crate::DEEP_WATCH_PROPERTY)
            && read.kind == ReactiveReadKind::Unconditional
        })
    }),
    "bare reactive watch sources must record a deep-root sentinel, not nested keys; got {:?}",
    scope.map(|scope| &scope.reads)
  );
  assert!(
    !graph.edges.iter().any(|edge| {
      edge.to == "state" && edge.property.as_deref().is_some_and(|property| property != "*")
    }),
    "must not invent concrete nested keys for deep watch(reactive)"
  );
}

#[test]
fn records_assignment_only_writes_on_watch_effect() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue';\n\
     const first = ref('a'); const last = ref('b'); const full = ref('');\n\
     watchEffect(() => { full.value = first.value + last.value; });",
  );
  let effect = graph.effects.first();
  assert!(
    effect.is_some_and(|effect| effect.reads.len() >= 2),
    "derived assignment must still track source reads"
  );
  let scope = graph.scopes.iter().find(|scope| scope.kind == TrackingScopeKind::WatchEffect);
  assert_eq!(scope.map(|scope| scope.assignment_only), Some(true));
  assert!(
    scope.is_some_and(|scope| {
      scope
        .writes
        .iter()
        .any(|write| write.binding == "full" && write.property.as_deref() == Some("value"))
    }),
    "assignment-only bodies must record reactive writes"
  );
}

#[test]
fn traces_watch_callback_as_outside_tracking() {
  let graph = graph(
    "import { ref, watch } from 'vue';\n\
     const source = ref(0); const other = ref(1);\n\
     watch(source, () => { other.value; });",
  );
  let callback = graph.scopes.iter().find(|scope| scope.kind == TrackingScopeKind::WatchCallback);
  assert!(
    callback.is_some_and(|scope| {
      scope
        .reads
        .iter()
        .any(|read| read.binding == "other" && read.kind == ReactiveReadKind::OutsideTracking)
    }),
    "watch job bodies must not collect dependencies"
  );
  assert_eq!(graph.version, vue_vet_core::REACTIVITY_GRAPH_VERSION);
}

#[test]
fn classifies_then_callbacks_as_outside_tracking() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue';\n\
     const value = ref(0);\n\
     watchEffect(() => { Promise.resolve().then(() => value.value); });",
  );
  assert!(
    graph.effects.first().is_some_and(|effect| {
      effect
        .reads
        .iter()
        .any(|read| read.binding == "value" && read.kind == ReactiveReadKind::OutsideTracking)
    }),
    "promise then callbacks must be outside synchronous tracking"
  );
}

#[test]
fn records_guard_roles_for_early_exit() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue';\n\
     const ready = ref(false); const value = ref(0);\n\
     watchEffect(() => { if (!ready.value) return; value.value; });",
  );
  let value = graph
    .effects
    .first()
    .into_iter()
    .flat_map(|effect| &effect.reads)
    .find(|read| read.binding == "value");
  assert_eq!(
    value.and_then(|read| read.guards.first().map(|guard| guard.role)),
    Some(ReactiveGuardRole::EarlyExit),
    "early-return guards must retain their role"
  );
}

#[test]
fn classifies_pause_tracking_regions() {
  let graph = graph(
    "import { ref, watchEffect, pauseTracking, enableTracking } from 'vue';\n\
     const value = ref(0);\n\
     watchEffect(() => { pauseTracking(); value.value; enableTracking(); });",
  );
  assert!(
    graph.effects.first().is_some_and(|effect| {
      effect
        .reads
        .iter()
        .any(|read| read.binding == "value" && read.kind == ReactiveReadKind::OutsideTracking)
    }),
    "reads after pauseTracking must not collect dependencies"
  );
}

#[test]
fn enable_tracking_resumes_dependency_collection() {
  let graph = graph(
    "import { ref, watchEffect, pauseTracking, enableTracking } from 'vue';\n\
     const paused = ref(0); const resumed = ref(1);\n\
     watchEffect(() => { pauseTracking(); paused.value; enableTracking(); resumed.value; });",
  );
  let effect = graph.effects.first();
  assert!(
    effect.is_some_and(|effect| {
      effect
        .reads
        .iter()
        .any(|read| read.binding == "paused" && read.kind == ReactiveReadKind::OutsideTracking)
        && effect
          .reads
          .iter()
          .any(|read| read.binding == "resumed" && read.kind == ReactiveReadKind::Unconditional)
    }),
    "enableTracking must resume collection; got {:?}",
    effect.map(|effect| &effect.reads)
  );
}

#[test]
fn reset_tracking_resumes_dependency_collection() {
  let graph = graph(
    "import { ref, watchEffect, pauseTracking, resetTracking } from 'vue';\n\
     const paused = ref(0); const resumed = ref(1);\n\
     watchEffect(() => { pauseTracking(); paused.value; resetTracking(); resumed.value; });",
  );
  let effect = graph.effects.first();
  assert!(
    effect.is_some_and(|effect| {
      effect
        .reads
        .iter()
        .any(|read| read.binding == "paused" && read.kind == ReactiveReadKind::OutsideTracking)
        && effect
          .reads
          .iter()
          .any(|read| read.binding == "resumed" && read.kind == ReactiveReadKind::Unconditional)
    }),
    "resetTracking must resume collection; got {:?}",
    effect.map(|effect| &effect.reads)
  );
}

#[test]
fn next_tick_callback_is_outside_tracking() {
  let graph = graph(
    "import { ref, watchEffect, nextTick } from 'vue';\n\
     const value = ref(0);\n\
     watchEffect(() => { nextTick(() => { value.value; }); });",
  );
  let effect = graph.effects.first();
  assert!(
    effect.is_some_and(|effect| {
      effect
        .reads
        .iter()
        .any(|read| read.binding == "value" && read.kind == ReactiveReadKind::OutsideTracking)
    }),
    "nextTick callbacks are outside synchronous tracking; got {:?}",
    effect.map(|effect| &effect.reads)
  );
}

#[test]
fn traces_effect_scope_run_callbacks() {
  let graph = graph(
    "import { effectScope, ref, watchEffect } from 'vue';\n\
     const value = ref(0);\n\
     const scope = effectScope();\n\
     scope.run(() => { watchEffect(() => value.value); });",
  );
  assert!(
    graph.scopes.iter().any(|scope| scope.kind == TrackingScopeKind::EffectScope),
    "effectScope.run must produce an EffectScope fact"
  );
  assert!(
    graph.effects.iter().any(|effect| { effect.reads.iter().any(|read| read.binding == "value") }),
    "nested watchEffect inside effectScope.run must still track reads"
  );
}

#[test]
fn does_not_invent_effect_scope_for_arbitrary_run_methods() {
  let graph = graph(
    "import { ref } from 'vue';\n\
     const count = ref(0);\n\
     const runner = { run(fn) { fn(); } };\n\
     runner.run(() => count.value);",
  );
  assert!(
    !graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::EffectScope || scope.callee.contains("effectScope")
    }),
    "arbitrary `.run` must not invent effectScope edges; got {:?}",
    graph.scopes.iter().map(|scope| (&scope.callee, scope.kind)).collect::<Vec<_>>()
  );
  assert!(graph.effects.is_empty(), "invented effect-family scopes must not project into effects");
}

#[test]
fn seeds_parametric_composable_to_ref_fields() {
  let modules = [
    ModuleSource::standalone(
      "producer.ts",
      "import { toRef } from 'vue'; export function useField(props) { return { title: toRef(props, 'title') }; }",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { watchEffect } from 'vue'; import { useField } from './producer'; const props = { title: 'x' }; const { title } = useField(props); watchEffect(() => title.value);",
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
        .any(|binding| binding.name == "title" && binding.kind == ReactiveBindingKind::ToRef)
        && module
          .graph
          .effects
          .iter()
          .any(|effect| effect.reads.iter().any(|read| read.binding == "title"))
    }),
    "toRef(param, key) composable fields must seed consumers"
  );
}

#[test]
fn imported_symbol_keys_match_across_modules() {
  let modules = [
    ModuleSource::standalone(
      "keys.ts",
      "export const ThemeKey = Symbol('theme');",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "provider.ts",
      "import { provide, ref } from 'vue'; import { ThemeKey } from './keys'; const mode = ref('dark'); provide(ThemeKey, mode);",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { inject, computed } from 'vue'; import { ThemeKey } from './keys'; const mode = inject(ThemeKey); const d = computed(() => mode.value); void d.value;",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let traced = traced_modules(&modules, &[]);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "mode" && binding.kind == ReactiveBindingKind::Ref)
        && module.graph.scopes.iter().any(|scope| {
          scope
            .reads
            .iter()
            .any(|read| read.binding == "mode" && read.property.as_deref() == Some("value"))
        })
    }),
    "imported injection keys must match by specifier+export; consumer={consumer:?}"
  );
}

#[test]
fn distinct_local_symbols_do_not_cross_link() {
  let modules = [
    ModuleSource::standalone(
      "provider.ts",
      "import { provide, ref } from 'vue'; const ThemeKey = Symbol('theme'); const mode = ref('dark'); provide(ThemeKey, mode);",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { inject, computed } from 'vue'; const ThemeKey = Symbol('theme'); const mode = inject(ThemeKey); const d = computed(() => mode.value); void d.value;",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let traced = traced_modules(&modules, &[]);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| !module
      .graph
      .bindings
      .iter()
      .any(|binding| binding.name == "mode")),
    "file-local Symbol() keys must not match across modules; consumer={consumer:?}"
  );
}

#[test]
fn to_value_getter_tracks_nested_reactive_reads() {
  let graph = graph(
    "import { ref, computed, toValue } from 'vue';\n\
     const count = ref(1);\n\
     const d = computed(() => toValue(() => count.value) * 2);\n\
     void d.value;",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "count" && read.property.as_deref() == Some("value"))
    }),
    "toValue(getter) must track reads inside the getter; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn unref_and_to_value_track_ref_like_bindings() {
  for (label, source) in [
    (
      "unref",
      "import { ref, computed, unref } from 'vue'; const count = ref(1); const d = computed(() => unref(count) * 2); void d.value;",
    ),
    (
      "toValue",
      "import { ref, computed, toValue } from 'vue'; const count = ref(1); const d = computed(() => toValue(count) * 2); void d.value;",
    ),
  ] {
    let graph = graph(source);
    assert!(
      graph.scopes.iter().any(|scope| {
        scope.kind == TrackingScopeKind::Computed
          && scope.reads.iter().any(|read| {
            read.binding == "count"
              && read.property.as_deref() == Some("value")
              && read.kind == ReactiveReadKind::Unconditional
          })
      }),
      "{label}(count) must track count.value; scopes={:?}",
      graph.scopes
    );
  }
}

#[test]
fn expands_define_props_destructuring() {
  let graph = graph(
    "import { computed } from 'vue';\n\
     const { account, context: ctx, ...rest } = defineProps<{\n\
       account: { id: string }\n\
       context?: string\n\
     }>();\n\
     const label = computed(() => account.id + (ctx ?? '') + String(rest));",
  );
  let names: Vec<&str> = graph.bindings.iter().map(|binding| binding.name.as_str()).collect();
  for expected in ["account", "ctx", "rest", "label"] {
    assert!(
      names.contains(&expected),
      "defineProps destructure must seed `{expected}`; bindings={names:?}"
    );
  }
  assert!(
    graph
      .bindings
      .iter()
      .filter(|binding| matches!(binding.name.as_str(), "account" | "ctx" | "rest"))
      .all(|binding| binding.kind == ReactiveBindingKind::Reactive),
    "destructured props locals must be Reactive; got {:?}",
    graph.bindings
  );
  assert!(
    graph.edges.iter().any(|edge| edge.from == "label" && edge.to == "account"),
    "computed must track bare destructured prop; edges={:?}",
    graph.edges
  );
}

#[test]
fn expands_with_defaults_define_props_destructuring() {
  let graph = graph(
    "import { computed } from 'vue';\n\
     const { title } = withDefaults(defineProps<{ title?: string }>(), { title: 'hi' });\n\
     const label = computed(() => title);",
  );
  assert!(
    graph
      .bindings
      .iter()
      .any(|binding| binding.name == "title" && binding.kind == ReactiveBindingKind::Reactive),
    "withDefaults(defineProps()) destructure must seed title; got {:?}",
    graph.bindings
  );
  assert!(
    graph.edges.iter().any(|edge| edge.from == "label" && edge.to == "title"),
    "computed must track withDefaults-destructured prop; edges={:?}",
    graph.edges
  );
}

#[test]
fn seeds_await_use_async_data_destructure() {
  let graph = graph(
    "const { data: account, pending } = await useAsyncData('key', () => fetch());
     const label = computed(() => account.value?.name ?? String(pending.value));",
  );
  assert!(
    graph.bindings.iter().any(|b| b.name == "account" && b.kind == ReactiveBindingKind::Ref),
    "useAsyncData data must seed Ref; got {:?}",
    graph.bindings
  );
  assert!(
    graph.bindings.iter().any(|b| b.name == "pending" && b.kind == ReactiveBindingKind::Ref),
    "useAsyncData pending must seed Ref; got {:?}",
    graph.bindings
  );
  assert!(
    graph.edges.iter().any(|e| e.from == "label" && e.to == "account"),
    "computed must track account; edges={:?}",
    graph.edges
  );
}

#[test]
fn seeds_use_route_params_slice() {
  let graph = graph(
    "const params = useRoute().params;
     const handle = computed(() => String(params.account));",
  );
  assert!(
    graph.bindings.iter().any(|b| b.name == "params" && b.kind == ReactiveBindingKind::Reactive),
    "useRoute().params must seed Reactive; got {:?}",
    graph.bindings
  );
  assert!(
    graph.edges.iter().any(|e| e.from == "handle" && e.to == "params"),
    "computed must track params; edges={:?}",
    graph.edges
  );
}

#[test]
fn uncertain_value_access_is_recorded_on_computed_scope() {
  let graph = graph(
    "import { computed } from 'vue';\n\
     declare function useMediaQuery(q: string): { value: boolean };\n\
     const isCoarse = useMediaQuery('(pointer: coarse)');\n\
     const hint = computed(() => (isCoarse.value ? 'a' : 'b'));",
  );
  let scope = graph.scopes.iter().find(|scope| scope.kind == TrackingScopeKind::Computed);
  assert!(
    scope.is_some_and(|scope| {
      scope.reads.is_empty() && scope.uncertain_accesses.iter().any(|name| name == "isCoarse")
    }),
    "unclassified .value must surface as uncertain_accesses (maybe); scopes={:?}",
    graph.scopes
  );
}

#[test]
fn optional_sole_value_duck_param_seeds_ref_binding() {
  let graph = graph(
    "import { computed } from 'vue';\n\
     function useScale(doc: { value?: { scale: number } }) {\n\
       return computed(() => doc.value?.scale ?? 1);\n\
     }\n\
     const scale = useScale({ value: { scale: 2 } });\n\
     void scale.value;",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "doc" && read.property.as_deref() == Some("value"))
        && scope.uncertain_accesses.iter().all(|name| name != "doc")
    }),
    "optional sole {{ value?: T }} param must classify .value; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn required_sole_value_type_does_not_seed_ref_binding() {
  let graph = graph(
    "import { computed } from 'vue';\n\
     function useLabel(option: { value: string }) {\n\
       return computed(() => option.value);\n\
     }\n\
     void useLabel({ value: 'a' }).value;",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.is_empty()
        && scope.uncertain_accesses.iter().any(|name| name == "option")
    }),
    "required {{ value: T }} must not invent Ref seeds; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn typed_computed_ref_parameters_classify_value_reads_inside_composables() {
  let graph = graph(
    "import { computed, type ComputedRef } from 'vue';\n\
     function useDetail(type: ComputedRef<string>, deviceKey: ComputedRef<string>) {\n\
       const deviceType = computed(() => type.value);\n\
       const detail = computed(() => (type.value === 'a' ? deviceKey.value : ''));\n\
       return { deviceType, detail };\n\
     }",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.iter().any(|read| read.binding == "type")
        && scope.uncertain_accesses.is_empty()
    }),
    "ComputedRef parameters must classify .value reads; got {:?}",
    graph.scopes
  );
  assert!(
    graph.scopes.iter().all(|scope| {
      scope.kind != TrackingScopeKind::Computed || scope.uncertain_accesses.is_empty()
    }),
    "typed Ref parameters must not remain uncertain; got {:?}",
    graph.scopes
  );
}

#[test]
fn asserted_ref_on_declarator_init_classifies_value_reads() {
  let graph = graph(
    "import type { Ref } from 'vue';\n\
     import { computed } from 'vue';\n\
     declare function useVModel(props: object, key: string, emit: unknown): unknown;\n\
     const props = { modelValue: { id: 1 } };\n\
     const emit = () => {};\n\
     const modelValue = useVModel(props, 'modelValue', emit) as Ref<{ id: number }>;\n\
     const id = computed(() => modelValue.value.id);\n\
     void id.value;",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "modelValue" && read.property.as_deref() == Some("value"))
        && !scope.uncertain_accesses.iter().any(|name| name == "modelValue")
    }),
    "asserted Ref init must classify .value; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn nested_local_refs_classify_inside_composable_computed() {
  let graph = graph(
    "import { ref, computed } from 'vue';\n\
     function useX() {\n\
       const count = ref(0);\n\
       const doubled = computed(() => count.value * 2);\n\
       return { doubled };\n\
     }",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.iter().any(|read| read.binding == "count")
        && !scope.uncertain_accesses.iter().any(|name| name == "count")
    }),
    "function-local ref must classify nested computed reads; got {:?}",
    graph.scopes
  );
}

#[test]
fn to_refs_destructure_inside_setup_classifies_value_reads() {
  let graph = graph(
    "import { computed, toRefs, defineComponent } from 'vue';\n\
     export default defineComponent({\n\
       props: { deviceKey: String, type: String, back: Boolean },\n\
       setup(props) {\n\
         const { deviceKey, type, back } = toRefs(props);\n\
         const label = computed(() => deviceKey.value + type.value);\n\
         const enabled = computed(() => back.value);\n\
         return () => label.value;\n\
       },\n\
     });",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.iter().any(|read| read.binding == "deviceKey")
        && scope.reads.iter().any(|read| read.binding == "type")
        && scope.uncertain_accesses.is_empty()
    }),
    "toRefs() locals inside setup must classify .value; got {:?}",
    graph.scopes
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.iter().any(|read| read.binding == "back")
        && scope.uncertain_accesses.is_empty()
    }),
    "toRefs() back must classify; got {:?}",
    graph.scopes
  );
}

#[test]
fn nested_uncertain_value_and_alias_binding_are_handled() {
  let nested = graph(
    "import { computed } from 'vue';\n\
     declare const bag: { nested: { value: number } };\n\
     const hint = computed(() => bag.nested.value);",
  );
  assert!(
    nested.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.is_empty()
        && scope.uncertain_accesses.iter().any(|name| name == "bag")
    }),
    "nested .value root must be uncertain when unclassified; scopes={:?}",
    nested.scopes
  );

  let aliased = graph(
    "import { ref, computed } from 'vue';\n\
     const count = ref(0);\n\
     const alias = count;\n\
     const doubled = computed(() => alias.value * 2);",
  );
  assert!(
    aliased
      .bindings
      .iter()
      .any(|binding| { binding.name == "alias" && binding.kind == ReactiveBindingKind::Ref }),
    "const alias = knownRef must seed; bindings={:?}",
    aliased.bindings
  );
  assert!(
    aliased.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "alias" && read.property.as_deref() == Some("value"))
    }),
    "alias.value must be a proven read; scopes={:?}",
    aliased.scopes
  );
}

#[test]
fn watch_sources_record_uncertain_bare_identifier() {
  let graph = graph(
    "import { watch } from 'vue';\n\
     declare const mystery: { value: number };\n\
     watch(mystery, () => {});",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::WatchSources
        && scope.reads.is_empty()
        && scope.uncertain_accesses.iter().any(|name| name == "mystery")
    }),
    "bare unknown watch source must be uncertain evidence; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn excludes_shadowed_reactive_symbols() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'; const payload = ref(0); \
     watchEffect((payload) => { payload.value; });",
  );
  assert!(
    graph.effects.first().is_some_and(|effect| effect.reads.is_empty()),
    "a callback parameter shadowing a reactive binding must not resolve to the outer symbol"
  );
}

#[test]
fn does_not_export_function_local_refs_as_module_bindings() {
  regression_case(
    "regressions/function-local-export/case.json",
    include_str!("../../fixtures/regressions/function-local-export/case.json"),
    include_str!("../../fixtures/regressions/function-local-export/producer.ts"),
    include_str!("../../fixtures/regressions/function-local-export/consumer.ts"),
  );
}

#[test]
fn ignores_shadowed_composable_calls_across_modules() {
  regression_case(
    "regressions/shadowed-composable/case.json",
    include_str!("../../fixtures/regressions/shadowed-composable/case.json"),
    include_str!("../../fixtures/regressions/shadowed-composable/producer.ts"),
    include_str!("../../fixtures/regressions/shadowed-composable/consumer.ts"),
  );
}

#[test]
fn use_table_style_query_result_spread_seeds_is_loading() {
  // vue-query style table helper: explicit `list` + `...queryResult` after
  // `queryResult.isLoading.value` (and similar) reads in the same function.
  let producer = "import { computed, ref, watch } from 'vue';\n\
export function useTableQuery(_params, tableQuery) {\n\
  const page = ref(1);\n\
  const queryResult = tableQuery();\n\
  const list = computed(() => queryResult.data.value?.records || []);\n\
  watch(() => queryResult.isSuccess.value && !queryResult.isFetching.value, () => {});\n\
  return { page, list, ...queryResult };\n\
}\n";
  let consumer = "import { computed } from 'vue';\n\
import { useTableQuery } from './useTable';\n\
const { list, isLoading } = useTableQuery({ pageNum: 1 }, () => ({\n\
  data: { value: null },\n\
  isLoading: { value: false },\n\
  isSuccess: { value: true },\n\
  isFetching: { value: false },\n\
}));\n\
const x = computed(() => isLoading.value);\n";
  let modules = [
    ModuleSource::standalone("useTable.ts", producer, "ts", ScriptKind::Script),
    ModuleSource::standalone("consumer.ts", consumer, "ts", ScriptKind::Script),
  ];
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "./useTable".into(),
    to: "useTable.ts".into(),
  }];
  let traced = traced_modules(&modules, &links);
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
            && scope
              .reads
              .iter()
              .any(|read| read.binding == "isLoading" && read.property.as_deref() == Some("value"))
            && scope.uncertain_accesses.is_empty()
        })
    }),
    "useTable-style ...queryResult must seed isLoading; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn typeof_import_reexport_forwards_composable_bag() {
  let modules = [
    ModuleSource::standalone(
      "fields.d.ts",
      "import type { Ref } from 'vue';\n\
       export interface FieldListContext {\n\
         fields: Ref<{ key: string }[]>;\n\
         push(value: { key: string }): void;\n\
       }\n\
       export declare function useFieldList(): FieldListContext;\n",
      "d.ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "alias.d.ts",
      "import { useFieldList } from './fields';\n\
       export declare const useFormFieldList: typeof useFieldList;\n",
      "d.ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { useFormFieldList } from './alias';\n\
       const ctx = useFormFieldList();\n\
       const keys = computed(() => ctx.fields.value.map((row) => row.key));\n",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links = [
    ModuleLink {
      from: "alias.d.ts".into(),
      specifier: "./fields".into(),
      to: "fields.d.ts".into(),
    },
    ModuleLink { from: "consumer.ts".into(), specifier: "./alias".into(), to: "alias.d.ts".into() },
  ];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.composable_instances.contains_key("ctx")
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope
              .reads
              .iter()
              .any(|read| read.binding == "fields" && read.property.as_deref() == Some("value"))
            && scope.uncertain_accesses.iter().all(|name| name != "ctx")
        })
    }),
    "typeof re-export must forward composable instance bag; consumer={consumer:?}"
  );
}

#[test]
fn draggable_return_bag_seeds_destructured_coords() {
  let modules = [
    ModuleSource::standalone(
      "drag.d.ts",
      "import type { Ref, ComputedRef } from 'vue';\n\
       export interface DragReturn {\n\
         x: Ref<number>;\n\
         y: Ref<number>;\n\
         style: ComputedRef<string>;\n\
       }\n\
       export declare function useDrag(\n\
         target: unknown,\n\
         options?: unknown,\n\
       ): DragReturn;\n",
      "d.ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed, ref } from 'vue';\n\
       import { useDrag } from './drag';\n\
       const el = ref<HTMLElement | null>(null);\n\
       const { x, y } = useDrag(el, { axis: 'y' });\n\
       const style = computed(() => `${x.value}px ${y.value}px`);\n",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links =
    [ModuleLink { from: "consumer.ts".into(), specifier: "./drag".into(), to: "drag.d.ts".into() }];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.bindings.iter().any(|b| b.name == "x" && b.kind == ReactiveBindingKind::Ref)
        && module.graph.bindings.iter().any(|b| b.name == "y" && b.kind == ReactiveBindingKind::Ref)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope.reads.iter().any(|read| read.binding == "x")
            && scope.reads.iter().any(|read| read.binding == "y")
            && scope.uncertain_accesses.iter().all(|name| name != "x" && name != "y")
        })
    }),
    "drag return bag must seed destructured x/y; consumer={consumer:?}"
  );
}

#[test]
fn declare_plus_export_list_forwards_composable_bag() {
  let modules = [
    ModuleSource::standalone(
      "drag.d.ts",
      "import type { Ref } from 'vue';\n\
       interface DragReturn {\n\
         x: Ref<number>;\n\
         y: Ref<number>;\n\
       }\n\
       declare function useDrag(target: unknown, options?: unknown): DragReturn;\n\
       export { useDrag };\n",
      "d.ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { computed, ref } from 'vue';\n\
       import { useDrag } from './drag';\n\
       const el = ref(null);\n\
       const { x, y } = useDrag(el);\n\
       const style = computed(() => `${x.value},${y.value}`);\n",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links =
    [ModuleLink { from: "consumer.ts".into(), specifier: "./drag".into(), to: "drag.d.ts".into() }];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module.graph.bindings.iter().any(|b| b.name == "x")
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope.reads.iter().any(|read| read.binding == "x")
            && scope.uncertain_accesses.iter().all(|name| name != "x" && name != "y")
        })
    }),
    "declare + export {{ name }} must publish composable bag; consumer={consumer:?}"
  );
}

#[test]
fn array_hof_callback_plain_value_is_not_uncertain() {
  let plain = graph(
    "import { computed } from 'vue';\n\
     const OPTIONS = [{ value: 'a' }, { value: 'b' }];\n\
     const labels = computed(() => OPTIONS.map((option) => option.value));\n\
     void labels.value;",
  );
  assert!(
    plain.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.is_empty()
        && scope.uncertain_accesses.iter().all(|name| name != "option")
    }),
    "plain option.value in Array#map must not be uncertain; scopes={:?}",
    plain.scopes
  );

  let from_ref = graph(
    "import { computed, ref } from 'vue';\n\
     const items = ref([{ value: 'a' }]);\n\
     const labels = computed(() => items.value.map((option) => option.value));\n\
     void labels.value;",
  );
  assert!(
    from_ref.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "items" && read.property.as_deref() == Some("value"))
        && scope.uncertain_accesses.iter().all(|name| name != "option")
    }),
    "items.value.map(option => option.value) must track items only; scopes={:?}",
    from_ref.scopes
  );
}

#[test]
fn untyped_composable_param_value_stays_uncertain() {
  let graph = graph(
    "import { computed } from 'vue';\n\
     function useLabel(option) {\n\
       return computed(() => option.value);\n\
     }\n\
     void useLabel({ value: 'a' }).value;",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.uncertain_accesses.iter().any(|name| name == "option")
    }),
    "untyped composable params must remain uncertain; scopes={:?}",
    graph.scopes
  );
}
