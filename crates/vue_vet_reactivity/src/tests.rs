use std::{collections::BTreeSet, path::Path, sync::Arc};

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use vue_vet_core::{
  ReactiveBindingKind, ReactiveDependencyKind, ReactiveGuardRole, ReactiveReadKind,
  ReactivityGraph, ScriptKind, SourceSpan, TemplateDirectiveFact, TemplateElementFact,
  TemplateExpressionFact, TemplateFacts, TrackingScopeKind,
};

use super::{
  ModuleLink, ModuleReactivity, ModuleSource, ModuleTraceState, TraceModulesOptions,
  merge_declaration_implementation_summary, prepare_module_summary,
  prepare_standalone_module_source, trace_modules, trace_modules_incremental_with_options,
  trace_reactivity,
};

fn trace(
  sfc_source: &str,
  script_source: &str,
  script_offset: usize,
  kind: ScriptKind,
) -> ReactivityGraph {
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, script_source, SourceType::ts()).parse();
  assert!(parsed.errors.is_empty(), "script parsing unexpectedly failed: {:?}", parsed.errors);
  let built = SemanticBuilder::new().with_check_syntax_error(true).build(&parsed.program);
  assert!(built.errors.is_empty(), "semantic analysis unexpectedly failed: {:?}", built.errors);
  trace_reactivity(&built.semantic, sfc_source, script_offset, kind)
}

fn graph(source: &str) -> ReactivityGraph {
  trace(source, source, 0, ScriptKind::Setup)
}

fn graph_tsx(source: &str) -> ReactivityGraph {
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, source, SourceType::tsx()).parse();
  assert!(parsed.errors.is_empty(), "tsx parsing unexpectedly failed: {:?}", parsed.errors);
  let built = SemanticBuilder::new().with_check_syntax_error(true).build(&parsed.program);
  assert!(built.errors.is_empty(), "tsx semantic analysis unexpectedly failed: {:?}", built.errors);
  trace_reactivity(&built.semantic, source, 0, ScriptKind::Script)
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
fn ignores_define_model_outside_script_setup() {
  let source = "const model = defineModel<string>();";
  let graph = trace(source, source, 0, ScriptKind::Script);
  assert!(
    graph.bindings.is_empty(),
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
fn builds_computed_dependency_edges() {
  let graph = graph(
    "import { computed, ref } from 'vue';\n\
     const source = ref(1);\n\
     const doubled = computed(() => source.value * 2);",
  );
  assert!(
    graph.edges.iter().any(|edge| {
      edge.kind == ReactiveDependencyKind::Computed && edge.from == "doubled" && edge.to == "source"
    }),
    "computed scopes must invert into depends-on edges"
  );
}

fn test_span(offset: usize) -> SourceSpan {
  SourceSpan { offset, length: 1, line: 1, column: offset.saturating_add(1) }
}

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
fn joins_template_reads_onto_script_bindings() {
  let mut graph = graph("import { ref } from 'vue'; const count = ref(0);");
  let Some(binding_span) = graph.bindings.first().map(|binding| binding.span.clone()) else {
    assert!(!graph.bindings.is_empty(), "count binding missing");
    return;
  };
  let template = TemplateFacts {
    elements: vec![TemplateElementFact {
      tag: "div".into(),
      span: binding_span.clone(),
      attributes: Vec::new(),
      directives: vec![TemplateDirectiveFact {
        name: "if".into(),
        raw_name: "v-if".into(),
        argument: None,
        expression: Some("count > 0".into()),
        modifiers: Vec::new(),
        span: binding_span.clone(),
      }],
      has_children: false,
      has_accessible_content: false,
      has_labelable_descendant: false,
      has_label_ancestor: false,
    }],
    expressions: vec![vue_vet_core::TemplateExpressionFact {
      surface: "if".into(),
      expression: "count > 0".into(),
      span: binding_span,
      identifiers: Some(vec!["count".into()]),
    }],
  };
  graph.join_template_reads(&template);
  assert!(
    graph.template_reads.iter().any(|read| read.binding == "count" && read.surface == "if"),
    "template v-if expressions must join onto reactive bindings"
  );
  assert!(
    graph
      .edges
      .iter()
      .any(|edge| edge.kind == ReactiveDependencyKind::Template && edge.to == "count"),
    "template joins must appear in the inverted edge list"
  );
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
fn partial_module_failure_preserves_healthy_cross_module_links() {
  let modules = [
    ModuleSource::standalone(
      "producer.ts",
      "import { ref } from 'vue'; export const count = ref(0);",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { watchEffect } from 'vue'; import { count } from './producer'; watchEffect(() => count.value);",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone("broken.ts", "const = ;", "ts", ScriptKind::Script),
  ];
  let links = [
    ModuleLink {
      from: "consumer.ts".into(),
      specifier: "./producer".into(),
      to: "producer.ts".into(),
    },
    ModuleLink {
      from: "broken.ts".into(),
      specifier: "./producer".into(),
      to: "producer.ts".into(),
    },
  ];
  let mut state = ModuleTraceState::default();
  let report = trace_modules_incremental_with_options(
    &modules,
    &links,
    TraceModulesOptions { max_workers: 2, ..Default::default() },
    &mut state,
  );
  assert!(
    report.issues.iter().any(|issue| issue.module_id().is_some_and(|id| id == "broken.ts")),
    "the malformed module must produce a scoped issue: {:?}",
    report.issues
  );
  let consumer = report.modules.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .effects
        .iter()
        .any(|effect| effect.reads.iter().any(|read| read.binding == "count"))
    }),
    "an unrelated parse failure must not discard the healthy producer → consumer seed"
  );
}

#[test]
fn incremental_module_trace_reuses_unchanged_seeded_graphs() {
  let modules = [
    ModuleSource::standalone(
      "producer.ts",
      "import { ref } from 'vue'; export const count = ref(0);",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { watchEffect } from 'vue'; import { count } from './producer'; watchEffect(() => count.value);",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "./producer".into(),
    to: "producer.ts".into(),
  }];
  let mut state = ModuleTraceState::default();
  let first = trace_modules_incremental_with_options(
    &modules,
    &links,
    TraceModulesOptions { max_workers: 2, ..Default::default() },
    &mut state,
  );
  assert!(first.issues.is_empty());
  assert_eq!(first.stats.seeded_reparses, 1);
  assert!(first.stats.export_resolve_ran);
  assert_eq!(first.stats.seed_plans_recomputed, 2);
  let second = trace_modules_incremental_with_options(
    &modules,
    &links,
    TraceModulesOptions { max_workers: 2, ..Default::default() },
    &mut state,
  );
  assert!(second.issues.is_empty());
  assert_eq!(second.stats.seeded_reparses, 0);
  assert_eq!(second.stats.reused_graphs, 2);
  assert!(!second.stats.export_resolve_ran);
  assert_eq!(second.stats.seed_plans_recomputed, 0);
  assert_eq!(first.modules, second.modules);
}

#[test]
fn seed_plans_recompute_only_export_closure() {
  let modules_v1 = [
    ModuleSource::standalone(
      "producer.ts",
      "import { ref } from 'vue'; export const count = ref(0);",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { watchEffect } from 'vue'; import { count } from './producer'; watchEffect(() => count.value);",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "unrelated.ts",
      "import { ref } from 'vue'; export const other = ref(1);",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "./producer".into(),
    to: "producer.ts".into(),
  }];
  let mut state = ModuleTraceState::default();
  let first = trace_modules_incremental_with_options(
    &modules_v1,
    &links,
    TraceModulesOptions { max_workers: 2, ..Default::default() },
    &mut state,
  );
  assert!(first.issues.is_empty());
  assert_eq!(first.stats.seed_plans_recomputed, 3);

  // New named export changes producer linking surface + consumer import closure.
  let modules_v2 = [
    ModuleSource::standalone(
      "producer.ts",
      "import { ref } from 'vue'; export const count = ref(0); export const flag = ref(true);",
      "ts",
      ScriptKind::Script,
    ),
    modules_v1[1].clone(),
    modules_v1[2].clone(),
  ];
  let second = trace_modules_incremental_with_options(
    &modules_v2,
    &links,
    TraceModulesOptions { max_workers: 2, ..Default::default() },
    &mut state,
  );
  assert!(second.issues.is_empty());
  assert!(second.stats.export_resolve_ran);
  assert_eq!(
    second.stats.seed_plans_recomputed, 2,
    "producer surface + consumer importer; unrelated must keep prior seed plan"
  );
}

#[test]
fn incremental_linking_skips_export_resolve_when_only_local_graph_changes() {
  use std::sync::Arc;

  use crate::{TraceSeeds, prepare_module_summary, trace_reactivity_seeded};

  fn summary_for(source: &str) -> Arc<crate::ModuleSummary> {
    let allocator = oxc_allocator::Allocator::default();
    let source_type = oxc_span::SourceType::default().with_module(true).with_typescript(true);
    let parsed = oxc_parser::Parser::new(&allocator, source, source_type).parse();
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let semantic = oxc_semantic::SemanticBuilder::new().build(&parsed.program).semantic;
    let graph = Arc::new(trace_reactivity_seeded(
      &semantic,
      source,
      0,
      ScriptKind::Script,
      &TraceSeeds::default(),
    ));
    Arc::new(prepare_module_summary(&semantic, source, 0, ScriptKind::Script, graph))
  }

  let producer_src = "import { ref } from 'vue'; export const count = ref(0);";
  let consumer_v1 = "import { watchEffect } from 'vue'; import { count } from './producer'; watchEffect(() => count.value);";
  let consumer_v2 = "import { watchEffect } from 'vue'; import { count } from './producer'; watchEffect(() => { void count.value; });";

  let producer = ModuleSource::standalone("producer.ts", producer_src, "ts", ScriptKind::Script);
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "./producer".into(),
    to: "producer.ts".into(),
  }];
  let mut state = ModuleTraceState::default();
  let first_modules = [
    producer.clone(),
    ModuleSource::standalone("consumer.ts", consumer_v1, "ts", ScriptKind::Script)
      .with_module_summary(summary_for(consumer_v1)),
  ];
  let first = trace_modules_incremental_with_options(
    &first_modules,
    &links,
    TraceModulesOptions { max_workers: 2, ..Default::default() },
    &mut state,
  );
  assert!(first.issues.is_empty());
  assert!(first.stats.export_resolve_ran);

  // Same import/export/provide surface; only local tracking body (local_graph) differs.
  let second_modules = [
    producer,
    ModuleSource::standalone("consumer.ts", consumer_v2, "ts", ScriptKind::Script)
      .with_module_summary(summary_for(consumer_v2)),
  ];
  let second = trace_modules_incremental_with_options(
    &second_modules,
    &links,
    TraceModulesOptions { max_workers: 2, ..Default::default() },
    &mut state,
  );
  assert!(second.issues.is_empty());
  assert!(!second.stats.export_resolve_ran, "linking surface unchanged → skip export fixed point");
  assert_eq!(second.stats.seed_plans_recomputed, 0);
}

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
fn string_replace_callback_tracks_nested_reactive_reads() {
  for (label, method) in [("replace", "replace"), ("replaceAll", "replaceAll")] {
    let graph = graph(&format!(
      "import {{ ref, computed }} from 'vue';\n\
       const text = ref('ab');\n\
       const flag = ref(true);\n\
       const d = computed(() => text.value.{method}(/./g, c => flag.value ? c : ''));\n\
       void d.value;"
    ));
    assert!(
      graph.scopes.iter().any(|scope| {
        scope.kind == TrackingScopeKind::Computed
          && scope
            .reads
            .iter()
            .any(|read| read.binding == "flag" && read.property.as_deref() == Some("value"))
          && scope
            .reads
            .iter()
            .any(|read| read.binding == "text" && read.property.as_deref() == Some("value"))
      }),
      "String#{label} replacer must track nested reactive reads; scopes={:?}",
      graph.scopes
    );
  }
}

#[test]
fn array_from_mapfn_tracks_nested_reactive_reads() {
  let graph = graph(
    "import { ref, computed } from 'vue';\n\
     const list = ref([1, 2]);\n\
     const factor = ref(2);\n\
     const d = computed(() => Array.from(list.value, x => x * factor.value));\n\
     void d.value;",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "factor" && read.property.as_deref() == Some("value"))
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "list" && read.property.as_deref() == Some("value"))
    }),
    "Array.from mapFn must track nested reactive reads; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn json_parse_reviver_tracks_nested_reactive_reads() {
  let graph = graph(
    "import { ref, computed } from 'vue';\n\
     const raw = ref('{\"a\":1}');\n\
     const flag = ref(true);\n\
     const d = computed(() => JSON.parse(raw.value, (k, v) => flag.value ? v : v));\n\
     void d.value;",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "flag" && read.property.as_deref() == Some("value"))
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "raw" && read.property.as_deref() == Some("value"))
    }),
    "JSON.parse reviver must track nested reactive reads; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn sync_hof_first_arg_function_does_not_invent_tracking() {
  // Callback-at-index-1 callees must not treat a sole first-arg function as the
  // sync callback (runtime never invokes it as mapFn/reviver/replacer).
  for (label, source) in [
    (
      "Array.from",
      "import { ref, computed } from 'vue';\n\
       const factor = ref(2);\n\
       const d = computed(() => Array.from(() => factor.value));\n\
       void d.value;",
    ),
    (
      "JSON.parse",
      "import { ref, computed } from 'vue';\n\
       const flag = ref(true);\n\
       const d = computed(() => JSON.parse(() => flag.value));\n\
       void d.value;",
    ),
    (
      "String.replace",
      "import { ref, computed } from 'vue';\n\
       const text = ref('ab');\n\
       const flag = ref(true);\n\
       const d = computed(() => text.value.replace(() => flag.value));\n\
       void d.value;",
    ),
  ] {
    let graph = graph(source);
    let invented = graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope.reads.iter().any(|read| {
          matches!(read.binding.as_str(), "factor" | "flag")
            && read.property.as_deref() == Some("value")
        })
    });
    assert!(
      !invented,
      "{label} first-arg function must not invent nested reactive reads; scopes={:?}",
      graph.scopes
    );
  }
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
fn dependency_edges_include_span_qualified_to_id() {
  let graph = graph(
    "import { ref, computed } from 'vue'; const source = ref(1); const doubled = computed(() => source.value * 2);",
  );
  let edge = graph.edges.iter().find(|edge| {
    edge.kind == ReactiveDependencyKind::Computed && edge.from == "doubled" && edge.to == "source"
  });
  assert!(
    edge.is_some_and(|edge| {
      edge.to_id.as_deref().is_some_and(|id| id.starts_with("source@"))
        && edge.to_identity().split('@').next() == Some("source")
    }),
    "anonymous traces keep name@offset to_id; got {:?}",
    edge.map(|edge| &edge.to_id)
  );
}

#[test]
fn module_traces_qualify_to_id_with_module_prefix() {
  let modules = [ModuleSource::standalone(
    "producer.ts",
    "import { ref, computed } from 'vue'; export const source = ref(1); export const doubled = computed(() => source.value * 2);",
    "ts",
    ScriptKind::Script,
  )];
  let traced = traced_modules(&modules, &[]);
  let producer = traced.iter().find(|module| module.id == "producer.ts");
  let edge = producer.and_then(|module| {
    module.graph.edges.iter().find(|edge| {
      edge.kind == ReactiveDependencyKind::Computed && edge.from == "doubled" && edge.to == "source"
    })
  });
  assert!(
    edge.is_some_and(|edge| {
      edge.to_id.as_deref().is_some_and(|id| id.starts_with("producer.ts:source@"))
    }),
    "v8 module traces must prefix to_id with module id; got {:?}",
    edge.map(|edge| &edge.to_id)
  );
}

#[test]
fn dependency_edges_carry_member_property_for_props_bag() {
  let graph = graph(
    "import { computed } from 'vue'; const props = defineProps<{ count: number; mode: string }>(); const label = computed(() => props.count + props.mode);",
  );
  assert_eq!(graph.version, vue_vet_core::REACTIVITY_GRAPH_VERSION);
  let count = graph.edges.iter().find(|edge| {
    edge.from == "label" && edge.to == "props" && edge.property.as_deref() == Some("count")
  });
  let mode = graph.edges.iter().find(|edge| {
    edge.from == "label" && edge.to == "props" && edge.property.as_deref() == Some("mode")
  });
  assert!(
    count.is_some_and(|edge| edge.to_path() == "props.count"),
    "v7 edges must carry property for props.count; got {:?}",
    graph.edges
  );
  assert!(
    mode.is_some_and(|edge| edge.to_path() == "props.mode"),
    "v7 edges must carry property for props.mode; got {:?}",
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
fn define_component_props_member_reads_track_in_computed() {
  let options = graph(
    "import { computed, defineComponent } from 'vue';\n\
     export default defineComponent({\n\
       props: { displayMode: String },\n\
       setup(props) {\n\
         const mode = computed(() => props.displayMode || 'whiteboard');\n\
         return () => mode.value;\n\
       },\n\
     });",
  );
  assert!(
    options.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "props" && read.property.as_deref() == Some("displayMode"))
        && scope.uncertain_accesses.is_empty()
    }),
    "setup(props).displayMode must track; got {:?}",
    options.scopes
  );

  let functional = graph_tsx(
    "import { computed, defineComponent } from 'vue';\n\
     export default defineComponent((props: { title: string }) => {\n\
       const label = computed(() => props.title);\n\
       return () => <p>{label.value}</p>;\n\
     });",
  );
  assert!(
    functional.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "props" && read.property.as_deref() == Some("title"))
        && scope.uncertain_accesses.is_empty()
    }),
    "defineComponent((props) => props.title) must track; got {:?}",
    functional.scopes
  );

  // Opaque project helper — no Vue `defineComponent` link ⇒ quiet (under-approx).
  let opaque = graph_tsx(
    "import { computed } from 'vue';\n\
     declare function defineTypedComponent<P>(setup: (props: P) => unknown): unknown;\n\
     export const Panel = defineTypedComponent<{ open: boolean }>((props) => {\n\
       const shown = computed(() => props.open);\n\
       return () => <div>{shown.value}</div>;\n\
     });",
  );
  assert!(
    opaque.scopes.iter().all(|scope| {
      scope.kind != TrackingScopeKind::Computed
        || !scope
          .reads
          .iter()
          .any(|read| read.binding == "props" && read.property.as_deref() == Some("open"))
    }),
    "opaque defineTypedComponent must not invent props tracking; got {:?}",
    opaque.scopes
  );

  // Same-file identity forwarder to Vue `defineComponent` ⇒ props seed.
  let forwarded = graph_tsx(
    "import { computed, defineComponent } from 'vue';\n\
     const defineTypedComponent = <P,>(setup: (props: P) => unknown) => defineComponent(setup);\n\
     export const Panel = defineTypedComponent((props: { open: boolean }) => {\n\
       const shown = computed(() => props.open);\n\
       return () => <div>{shown.value}</div>;\n\
     });",
  );
  assert!(
    forwarded.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "props" && read.property.as_deref() == Some("open"))
        && scope.uncertain_accesses.is_empty()
    }),
    "defineComponent identity forwarder must seed props; got {:?}",
    forwarded.scopes
  );

  // Same-file multi-arg / alias wrap ⇒ ComponentFactory props seed.
  let multi_arg = graph_tsx(
    "import { computed, defineComponent } from 'vue';\n\
     function defineTypedComponent<P>(setup: (props: P) => unknown, extra?: object) {\n\
       const _setup = setup as any;\n\
       const _props = extra as any;\n\
       return defineComponent(_setup, _props) as unknown as (props: P) => unknown;\n\
     }\n\
     export const Panel = defineTypedComponent((props: { open: boolean }) => {\n\
       const shown = computed(() => props.open);\n\
       return () => <div>{shown.value}</div>;\n\
     });",
  );
  assert!(
    multi_arg.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "props" && read.property.as_deref() == Some("open"))
        && scope.uncertain_accesses.is_empty()
    }),
    "multi-arg defineComponent wrap must seed props; got {:?}",
    multi_arg.scopes
  );
}

#[test]
fn cross_module_component_factory_wrapper_seeds_props() {
  let modules = [
    ModuleSource::standalone(
      "factory.ts",
      "import { defineComponent } from 'vue';\n\
       export function defineTypedComponent(setup, extra) {\n\
         const _setup = setup;\n\
         const _props = extra;\n\
         return defineComponent(_setup, _props);\n\
       }",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.tsx",
      "import { computed } from 'vue';\n\
       import { defineTypedComponent } from './factory';\n\
       export const Panel = defineTypedComponent((props: { open: boolean }) => {\n\
         const shown = computed(() => props.open);\n\
         return () => shown.value;\n\
       });",
      "tsx",
      ScriptKind::Script,
    ),
  ];
  let links = [ModuleLink {
    from: "consumer.tsx".into(),
    specifier: "./factory".into(),
    to: "factory.ts".into(),
  }];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.tsx");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "props" && binding.kind == ReactiveBindingKind::Reactive)
        && module.graph.scopes.iter().any(|scope| {
          scope.kind == TrackingScopeKind::Computed
            && scope
              .reads
              .iter()
              .any(|read| read.binding == "props" && read.property.as_deref() == Some("open"))
            && scope.uncertain_accesses.is_empty()
        })
    }),
    "cross-module ComponentFactory must seed props; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

#[test]
fn cross_module_opaque_component_helper_does_not_seed_props() {
  let modules = [
    ModuleSource::standalone(
      "factory.ts",
      "export function defineTypedComponent(setup) { return setup; }",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.tsx",
      "import { computed } from 'vue';\n\
       import { defineTypedComponent } from './factory';\n\
       export const Panel = defineTypedComponent((props: { open: boolean }) => {\n\
         const shown = computed(() => props.open);\n\
         return () => shown.value;\n\
       });",
      "tsx",
      ScriptKind::Script,
    ),
  ];
  let links = [ModuleLink {
    from: "consumer.tsx".into(),
    specifier: "./factory".into(),
    to: "factory.ts".into(),
  }];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.tsx");
  assert!(
    consumer.is_some_and(|module| {
      !module.graph.bindings.iter().any(|binding| binding.name == "props")
        && module.graph.scopes.iter().all(|scope| {
          scope.kind != TrackingScopeKind::Computed
            || !scope
              .reads
              .iter()
              .any(|read| read.binding == "props" && read.property.as_deref() == Some("open"))
        })
    }),
    "opaque helper must not invent props tracking; got {:?}",
    consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
  );
}

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

#[expect(clippy::panic, reason = "fixture setup failures must fail the unit test")]
fn prepared_standalone(id: &str, source: &str, language: &str) -> ModuleSource {
  prepare_standalone_module_source(id, source, language)
    .unwrap_or_else(|error| panic!("prepare {id}: {error}"))
}

#[expect(clippy::panic, reason = "fixture setup failures must fail the unit test")]
fn attached_summary(module: &ModuleSource) -> Arc<super::ModuleSummary> {
  module.module_summary().unwrap_or_else(|| panic!("missing summary for {}", module.id))
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

/// One read in an exhaustive effect read-set assertion.
#[derive(serde::Deserialize)]
struct LocalReadExpectation {
  binding: String,
  kind: ReactiveReadKind,
  #[serde(default)]
  guards: Vec<String>,
}

#[derive(serde::Deserialize)]
struct LocalExpectation {
  effect: String,
  binding: String,
  kind: ReactiveReadKind,
  guards: Vec<String>,
  /// When present, the effect's full read set must match exactly (no missing, no invented).
  #[serde(default)]
  reads: Option<Vec<LocalReadExpectation>>,
}

#[derive(serde::Deserialize)]
struct LocalFixture {
  name: String,
  source: String,
  expected: LocalExpectation,
}

#[derive(serde::Deserialize)]
struct ModuleExpectation {
  module: String,
  binding: String,
  kind: ReactiveReadKind,
  guards: Vec<String>,
  trace: bool,
}

#[derive(serde::Deserialize)]
struct ModuleFixture {
  name: String,
  modules: Vec<ModuleSource>,
  links: Vec<ModuleLink>,
  expected: ModuleExpectation,
}

#[derive(serde::Deserialize)]
struct Provenance {
  repository: String,
  commit: String,
  path: String,
  adaptation: String,
}

#[derive(serde::Deserialize)]
struct RealWorldFixture {
  name: String,
  provenance: Provenance,
  modules: Vec<FixtureModule>,
  links: Vec<ModuleLink>,
  expected: ModuleExpectation,
}

#[derive(serde::Deserialize)]
struct FixtureModule {
  id: String,
  file: String,
  language: String,
  kind: ScriptKind,
}

#[derive(serde::Deserialize)]
struct RegressionManifest {
  name: String,
  expected: ModuleExpectation,
}

macro_rules! corpus_batches {
  ($($path:literal),+ $(,)?) => {
    [$(($path, include_str!(concat!("../fixtures/corpus/", $path)))),+]
  };
}

const SYSTEMATIC_FIXTURES: [(&str, &str); 10] = corpus_batches!(
  "systematic/batch-01.json",
  "systematic/batch-02.json",
  "systematic/batch-03.json",
  "systematic/batch-04.json",
  "systematic/batch-05.json",
  "systematic/batch-06.json",
  "systematic/batch-07.json",
  "systematic/batch-08.json",
  "systematic/batch-09.json",
  "systematic/batch-10.json",
);

const COMPLEX_FIXTURES: [(&str, &str); 10] = corpus_batches!(
  "complex/01-sequential-early-returns.json",
  "complex/02-nested-if.json",
  "complex/03-if-logical.json",
  "complex/04-logical-chain.json",
  "complex/05-nested-ternary.json",
  "complex/06-early-return-then-if.json",
  "complex/07-else-if.json",
  "complex/08-try-finally-in-branch.json",
  "complex/09-switch-in-branch.json",
  "complex/10-loop-in-branch.json",
);

const MODULE_FIXTURES: [(&str, &str); 8] = corpus_batches!(
  "modules/01-direct-named.json",
  "modules/02-composable-alias.json",
  "modules/03-default-export.json",
  "modules/04-star-barrel.json",
  "modules/05-named-multihop.json",
  "modules/06-cycle.json",
  "modules/07-unresolved.json",
  "modules/08-conflicting-star.json",
);

const REAL_WORLD_FIXTURES: [(&str, &str); 5] = [
  ("nuxt-async-data", include_str!("../fixtures/real-world/nuxt-async-data/case.json")),
  ("vueuse-computed-async", include_str!("../fixtures/real-world/vueuse-computed-async/case.json")),
  ("vueuse-computed-eager", include_str!("../fixtures/real-world/vueuse-computed-eager/case.json")),
  (
    "vue-router-current-route",
    include_str!("../fixtures/real-world/vue-router-current-route/case.json"),
  ),
  ("pinia-store-to-refs", include_str!("../fixtures/real-world/pinia-store-to-refs/case.json")),
];

#[expect(clippy::panic, reason = "malformed committed fixtures must fail corpus tests")]
fn parse_fixture_batch<T: serde::de::DeserializeOwned>(path: &str, source: &str) -> Vec<T> {
  match serde_json::from_str(source) {
    Ok(fixtures) => fixtures,
    Err(error) => panic!("could not parse fixture batch {path}: {error}"),
  }
}

#[expect(clippy::panic, reason = "malformed committed fixtures must fail corpus tests")]
fn parse_fixture<T: serde::de::DeserializeOwned>(path: &str, source: &str) -> T {
  match serde_json::from_str(source) {
    Ok(fixture) => fixture,
    Err(error) => panic!("could not parse fixture {path}: {error}"),
  }
}

fn load_fixture_batches<T: serde::de::DeserializeOwned>(batches: &[(&str, &str)]) -> Vec<T> {
  let mut fixtures = Vec::new();
  for (path, source) in batches {
    fixtures.extend(parse_fixture_batch(path, source));
  }
  fixtures
}

fn assert_local_fixture(fixture: &LocalFixture) {
  let graph = graph(&fixture.source);
  let effect = graph.effects.iter().find(|effect| effect.callee == fixture.expected.effect);
  assert!(effect.is_some(), "expected effect must be resolved in {}", fixture.name);
  let payload = effect
    .into_iter()
    .flat_map(|effect| &effect.reads)
    .find(|read| read.binding == fixture.expected.binding);
  assert_eq!(
    payload.map(|read| read.kind),
    Some(fixture.expected.kind),
    "unexpected read classification in {}",
    fixture.name
  );
  assert!(
    payload.is_some_and(|read| {
      fixture
        .expected
        .guards
        .iter()
        .all(|expected| read.guards.iter().any(|guard| guard.binding == *expected))
    }),
    "expected guard evidence must survive in {}",
    fixture.name
  );
  assert!(
    fixture.expected.reads.is_some(),
    "local fixture {} must pin exhaustive expected.reads (regenerate from tracer if adding cases)",
    fixture.name
  );
  if let (Some(effect), Some(expected_reads)) = (effect, fixture.expected.reads.as_ref()) {
    assert_effect_reads_exact(effect, expected_reads, &fixture.name);
  }
}

/// Exact effect read-set: every (binding, kind, guard-names) pair must match.
fn assert_effect_reads_exact(
  effect: &vue_vet_core::ReactivityEffectFact,
  expected: &[LocalReadExpectation],
  name: &str,
) {
  let actual = effect
    .reads
    .iter()
    .map(|read| {
      let guards = read.guards.iter().map(|guard| guard.binding.as_str()).collect::<BTreeSet<_>>();
      (read.binding.as_str(), read.kind, guards)
    })
    .collect::<BTreeSet<_>>();
  let expected = expected
    .iter()
    .map(|read| {
      let guards = read.guards.iter().map(String::as_str).collect::<BTreeSet<_>>();
      (read.binding.as_str(), read.kind, guards)
    })
    .collect::<BTreeSet<_>>();
  assert_eq!(
    actual, expected,
    "effect read set must match exactly in {name} (no missing, no invented)"
  );
}

fn module_fixture_signature(modules: &[ModuleSource], links: &[ModuleLink]) -> String {
  let module_sources = modules
    .iter()
    .map(|module| format!("{}\n{}", module.id, module.source))
    .collect::<Vec<_>>()
    .join("\n---module---\n");
  let resolved_links = links
    .iter()
    .map(|link| format!("{}:{}:{}", link.from, link.specifier, link.to))
    .collect::<Vec<_>>()
    .join("\n");
  format!("{module_sources}\n---links---\n{resolved_links}")
}

fn assert_module_case(
  name: &str,
  modules: &[ModuleSource],
  links: &[ModuleLink],
  expected: &ModuleExpectation,
) {
  assert!(modules.len() >= 2, "cross-module fixture must contain separate files: {name}");
  let traced = traced_modules(modules, links);
  let consumer = traced.iter().find(|module| module.id == expected.module);
  let payload = consumer
    .into_iter()
    .flat_map(|module| &module.graph.effects)
    .flat_map(|effect| &effect.reads)
    .find(|read| read.binding == expected.binding);
  if expected.trace {
    assert_eq!(
      payload.map(|read| read.kind),
      Some(expected.kind),
      "linked payload has the wrong classification in {name}"
    );
    assert!(
      payload.is_some_and(|read| {
        expected
          .guards
          .iter()
          .all(|expected| read.guards.iter().any(|guard| guard.binding == *expected))
      }),
      "linked payload must retain local guard evidence in {name}"
    );
  } else {
    assert!(payload.is_none(), "unsupported or shadowed module shapes must stay quiet in {name}");
  }
}

#[test]
fn covers_one_hundred_systematic_scenarios() {
  let fixtures = load_fixture_batches::<LocalFixture>(&SYSTEMATIC_FIXTURES);
  let names = fixtures.iter().map(|fixture| fixture.name.as_str()).collect::<BTreeSet<_>>();
  let sources = fixtures.iter().map(|fixture| fixture.source.as_str()).collect::<BTreeSet<_>>();
  for fixture in &fixtures {
    assert_local_fixture(fixture);
  }
  assert_eq!(fixtures.len(), 100, "the systematic corpus must contain exactly 100 cases");
  assert_eq!(names.len(), 100, "all systematic scenario names must be unique");
  assert_eq!(sources.len(), 100, "all systematic scenario sources must be unique");
}

#[test]
fn covers_one_hundred_complex_single_module_scenarios() {
  let fixtures = load_fixture_batches::<LocalFixture>(&COMPLEX_FIXTURES);
  let names = fixtures.iter().map(|fixture| fixture.name.as_str()).collect::<BTreeSet<_>>();
  let sources = fixtures.iter().map(|fixture| fixture.source.as_str()).collect::<BTreeSet<_>>();
  for fixture in &fixtures {
    assert_local_fixture(fixture);
  }
  assert_eq!(fixtures.len(), 100, "the complex corpus must contain exactly 100 cases");
  assert_eq!(names.len(), 100, "all complex scenario names must be unique");
  assert_eq!(sources.len(), 100, "all complex scenario sources must be unique");
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

#[expect(clippy::panic, reason = "module tracing errors must fail corpus tests")]
fn traced_modules(modules: &[ModuleSource], links: &[ModuleLink]) -> Vec<ModuleReactivity> {
  match trace_modules(modules, links) {
    Ok(traced) => traced,
    Err(error) => panic!("cross-module tracing unexpectedly failed: {error}"),
  }
}

#[test]
fn covers_eighty_real_cross_module_scenarios() {
  let fixtures = load_fixture_batches::<ModuleFixture>(&MODULE_FIXTURES);
  let names = fixtures.iter().map(|fixture| fixture.name.as_str()).collect::<BTreeSet<_>>();
  let signatures = fixtures
    .iter()
    .map(|fixture| module_fixture_signature(&fixture.modules, &fixture.links))
    .collect::<BTreeSet<_>>();
  for fixture in &fixtures {
    assert_module_case(&fixture.name, &fixture.modules, &fixture.links, &fixture.expected);
  }
  assert_eq!(fixtures.len(), 80, "the module corpus must contain exactly 80 cases");
  assert_eq!(names.len(), 80, "all module scenario names must be unique");
  assert_eq!(signatures.len(), 80, "all module scenario sources must be unique");
}

fn module_source(id: &str, source: &str) -> ModuleSource {
  ModuleSource::standalone(id, source, "ts", ScriptKind::Script)
}

#[test]
fn prepared_phase_one_facts_avoid_an_unseeded_second_parse() {
  let source = "import { ref } from 'vue'; export const count = ref(0);";
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
  assert!(parsed.errors.is_empty());
  let built = SemanticBuilder::new().with_check_syntax_error(true).build(&parsed.program);
  assert!(built.errors.is_empty());
  let local_graph = trace_reactivity(&built.semantic, source, 0, ScriptKind::Script);
  let summary = prepare_module_summary(&built.semantic, source, 0, ScriptKind::Script, local_graph);
  let mut module = ModuleSource::standalone("count.ts", source, "ts", ScriptKind::Script)
    .with_module_summary(summary);

  // If phase one parsed again this deliberate mutation would fail. No seeds
  // means the retained local graph is sufficient.
  module.source = "const = ;".into();
  let traced = trace_modules(&[module], &[]);
  assert!(traced.is_ok(), "prepared phase-one facts should bypass a second parse");
}

#[expect(clippy::panic, reason = "missing committed source files must fail corpus tests")]
fn load_real_world_modules(case_dir: &str, files: &[FixtureModule]) -> Vec<ModuleSource> {
  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/real-world").join(case_dir);
  files
    .iter()
    .map(|file| {
      let path = root.join(&file.file);
      let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => panic!("could not read real-world fixture {}: {error}", path.display()),
      };
      ModuleSource::standalone(file.id.clone(), source, file.language.clone(), file.kind)
    })
    .collect()
}

fn regression_case(
  manifest_path: &str,
  manifest_source: &str,
  producer_source: &str,
  consumer_source: &str,
) {
  let manifest = parse_fixture::<RegressionManifest>(manifest_path, manifest_source);
  let modules = vec![
    module_source("producer.ts", producer_source),
    module_source("consumer.ts", consumer_source),
  ];
  let links = vec![ModuleLink {
    from: "consumer.ts".into(),
    specifier: "./producer".into(),
    to: "producer.ts".into(),
  }];
  assert_module_case(&manifest.name, &modules, &links, &manifest.expected);
}

#[test]
fn does_not_export_function_local_refs_as_module_bindings() {
  regression_case(
    "regressions/function-local-export/case.json",
    include_str!("../fixtures/regressions/function-local-export/case.json"),
    include_str!("../fixtures/regressions/function-local-export/producer.ts"),
    include_str!("../fixtures/regressions/function-local-export/consumer.ts"),
  );
}

#[test]
fn ignores_shadowed_composable_calls_across_modules() {
  regression_case(
    "regressions/shadowed-composable/case.json",
    include_str!("../fixtures/regressions/shadowed-composable/case.json"),
    include_str!("../fixtures/regressions/shadowed-composable/producer.ts"),
    include_str!("../fixtures/regressions/shadowed-composable/consumer.ts"),
  );
}

#[test]
fn validates_real_world_module_patterns() {
  let mut names = BTreeSet::new();
  let mut provenances = BTreeSet::new();
  for (case_dir, source) in REAL_WORLD_FIXTURES {
    let manifest_path = format!("real-world/{case_dir}/case.json");
    let fixture = parse_fixture::<RealWorldFixture>(&manifest_path, source);
    assert!(names.insert(fixture.name.clone()), "real-world fixture names must be unique");
    assert!(
      fixture.provenance.commit.len() == 40
        && fixture.provenance.commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
      "real-world fixture commits must be full hexadecimal SHAs: {}",
      fixture.name
    );
    assert!(
      !fixture.provenance.repository.is_empty()
        && !fixture.provenance.path.is_empty()
        && !fixture.provenance.adaptation.is_empty(),
      "real-world fixture provenance must be complete: {}",
      fixture.name
    );
    let provenance = format!(
      "{}@{}:{}",
      fixture.provenance.repository, fixture.provenance.commit, fixture.provenance.path
    );
    assert!(provenances.insert(provenance), "real-world provenance entries must be unique");
    let modules = load_real_world_modules(case_dir, &fixture.modules);
    assert_module_case(&fixture.name, &modules, &fixture.links, &fixture.expected);
  }
  assert_eq!(names.len(), 5, "the real-world corpus must retain five fixed-source cases");
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

#[test]
fn recognizes_render_scopes_for_jsx_shapes_and_factory_wrappers() {
  let options_render = graph_tsx(
    "import { ref } from 'vue';\n\
     const count = ref(0);\n\
     export default { render() { return <div>{count.value}</div>; } };",
  );
  assert!(
    options_render.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Render
        && scope.reads.iter().any(|read| read.binding == "count")
    }),
    "options render must become a Render scope; got {:?}",
    options_render.scopes
  );

  let setup_return = graph_tsx(
    "import { ref, defineComponent } from 'vue';\n\
     const count = ref(0);\n\
     export default defineComponent({ setup() { return () => <div>{count.value}</div>; } });",
  );
  assert!(
    setup_return.scopes.iter().any(|scope| scope.kind == TrackingScopeKind::Render),
    "defineComponent setup→render must become a Render scope"
  );

  let aliased = graph_tsx(
    "import { ref, defineComponent as dc } from 'vue';\n\
     const count = ref(0);\n\
     export default dc({ setup() { return () => <span>{count.value}</span>; } });",
  );
  assert!(
    aliased.scopes.iter().any(|scope| scope.kind == TrackingScopeKind::Render),
    "defineComponent import aliases must resolve as component factories"
  );

  let wrapper = graph_tsx(
    "import { ref, defineComponent } from 'vue';\n\
     const definePage = (options) => defineComponent(options);\n\
     const count = ref(0);\n\
     export default definePage({ setup() { return () => <p>{count.value}</p>; } });",
  );
  assert!(
    wrapper.scopes.iter().any(|scope| scope.kind == TrackingScopeKind::Render),
    "same-file identity forwarders must resolve as component factories"
  );

  let functional = graph_tsx(
    "import { ref } from 'vue';\n\
     const count = ref(0);\n\
     export function Comp() { return <div>{count.value}</div>; }",
  );
  assert!(
    functional.scopes.iter().any(|scope| scope.kind == TrackingScopeKind::Render),
    "exported functional components returning JSX must become Render scopes"
  );

  // Options-object shapes inside unknown factories are still recognized (structure-
  // first). Opaque factories only stay quiet when there is no options/setup/render
  // object and no exported functional component.
  let opaque = graph_tsx(
    "import { ref } from 'vue';\n\
     import { definePage } from '#imports';\n\
     const count = ref(0);\n\
     export default definePage(() => <div>{count.value}</div>);",
  );
  assert!(
    !opaque.scopes.iter().any(|scope| scope.kind == TrackingScopeKind::Render),
    "unknown factory wrapping a bare render callback must stay quiet; got {:?}",
    opaque.scopes
  );
}

#[test]
fn classifies_conditional_reads_inside_render_scopes() {
  let graph = graph_tsx(
    "import { defineComponent, ref } from 'vue';\n\
     const enabled = ref(false);\n\
     const count = ref(0);\n\
     export default defineComponent(() => {\n\
       return () => {\n\
         if (!enabled.value) return <p>off</p>;\n\
         return <p>{count.value}</p>;\n\
       };\n\
     });",
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == TrackingScopeKind::Render
        && scope.reads.iter().any(|read| {
          read.binding == "count"
            && read.kind == ReactiveReadKind::Conditional
            && read.guards.iter().any(|guard| guard.role == ReactiveGuardRole::EarlyExit)
        })
    }),
    "count behind early-exit in render must be Conditional; got {:?}",
    graph.scopes
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
