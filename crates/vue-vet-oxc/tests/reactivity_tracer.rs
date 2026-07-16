use vue_vet_core::{
  ReactiveBindingKind, ReactiveReadKind, ReactivityGraph, ScriptBlockFacts, ScriptKind,
};
use vue_vet_oxc::analyze_script;

#[expect(clippy::panic, reason = "unexpected Oxc errors must fail tracer tests")]
fn analyze(source: &str) -> ScriptBlockFacts {
  match analyze_script(source, source, 0, "ts", ScriptKind::Setup) {
    Ok(facts) => facts,
    Err(error) => panic!("script analysis unexpectedly failed: {error}"),
  }
}

fn graph(source: &str) -> ReactivityGraph {
  analyze(source).reactivity_graph
}

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
fn ignores_define_model_outside_script_setup() {
  let source = "const model = defineModel<string>();";
  let facts = analyze_script(source, source, 0, "ts", ScriptKind::Script);
  assert!(
    facts.is_ok_and(|facts| facts.reactivity_graph.bindings.is_empty()),
    "defineModel must not be assumed to be a compiler macro in a normal script"
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
  let facts = analyze_script(&sfc, script, offset, "ts", ScriptKind::Setup);
  assert!(facts.is_ok(), "the embedded script must be analyzable");
  if let Ok(facts) = facts {
    let read = facts
      .reactivity_graph
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
