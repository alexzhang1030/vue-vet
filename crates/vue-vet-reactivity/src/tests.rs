use std::{collections::BTreeSet, path::Path};

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use vue_vet_core::{
  ReactiveBindingKind, ReactiveGuardRole, ReactiveReadKind, ReactivityGraph, ScriptKind,
  TrackingScopeKind,
};

use super::{ModuleLink, ModuleReactivity, ModuleSource, trace_modules, trace_reactivity};

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
  assert_eq!(
    scope.map(|scope| { scope.reads.iter().map(|read| read.binding.as_str()).collect::<Vec<_>>() }),
    Some(vec!["a", "b"]),
    "watch source arrays must record each reactive source"
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
      scope
        .reads
        .iter()
        .any(|read| read.binding == "value" && read.kind == ReactiveReadKind::Unconditional)
    }),
    "watch source getters must track reactive reads"
  );
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
fn seeds_composable_instance_member_access() {
  let modules = [
    ModuleSource {
      id: "producer.ts".into(),
      source: "import { ref } from 'vue'; export function useSignal() { const signal = ref(0); return { signal }; }".into(),
      language: "ts".into(),
      kind: ScriptKind::Script,
    },
    ModuleSource {
      id: "consumer.ts".into(),
      source: "import { watchEffect } from 'vue'; import { useSignal } from './producer'; const bag = useSignal(); watchEffect(() => bag.signal.value);".into(),
      language: "ts".into(),
      kind: ScriptKind::Script,
    },
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

#[derive(serde::Deserialize)]
struct LocalExpectation {
  effect: String,
  binding: String,
  kind: ReactiveReadKind,
  guards: Vec<String>,
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
  ModuleSource {
    id: id.into(),
    source: source.into(),
    language: "ts".into(),
    kind: ScriptKind::Script,
  }
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
      ModuleSource { id: file.id.clone(), source, language: file.language.clone(), kind: file.kind }
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
