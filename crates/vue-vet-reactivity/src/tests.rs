use std::collections::BTreeSet;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use vue_vet_core::{ReactiveBindingKind, ReactiveReadKind, ReactivityGraph, ScriptKind};

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

#[derive(Clone, Copy)]
struct PrimitiveAxis {
  name: &'static str,
  constructor: &'static str,
  access: &'static str,
}

#[derive(Clone, Copy)]
struct FlowAxis {
  name: &'static str,
  body: fn(&str) -> String,
}

#[derive(Clone, Copy)]
struct EffectAxis {
  name: &'static str,
  callee: &'static str,
  function_callback: bool,
}

#[derive(Clone, Copy)]
enum ImportAxis {
  Named,
  Namespace,
}

impl ImportAxis {
  const fn name(self) -> &'static str {
    match self {
      Self::Named => "named",
      Self::Namespace => "namespace",
    }
  }
}

const PRIMITIVE_AXES: [PrimitiveAxis; 5] = [
  PrimitiveAxis { name: "ref", constructor: "ref", access: "guard.value" },
  PrimitiveAxis { name: "computed", constructor: "computed", access: "guard.value" },
  PrimitiveAxis { name: "reactive", constructor: "reactive", access: "guard.active" },
  PrimitiveAxis { name: "readonly", constructor: "readonly", access: "guard.active" },
  PrimitiveAxis { name: "custom_ref", constructor: "customRef", access: "guard.value" },
];

const FLOW_AXES: [FlowAxis; 5] = [
  FlowAxis { name: "early_return", body: early_return_body },
  FlowAxis { name: "if_consequent", body: if_consequent_body },
  FlowAxis { name: "if_alternate", body: if_alternate_body },
  FlowAxis { name: "logical_rhs", body: logical_rhs_body },
  FlowAxis { name: "ternary_branch", body: ternary_branch_body },
];

const EFFECT_AXES: [EffectAxis; 2] = [
  EffectAxis { name: "watch_effect_arrow", callee: "watchEffect", function_callback: false },
  EffectAxis { name: "watch_effect_function", callee: "watchEffect", function_callback: true },
];

const IMPORT_AXES: [ImportAxis; 2] = [ImportAxis::Named, ImportAxis::Namespace];

fn early_return_body(access: &str) -> String {
  format!("if (!{access}) return; payload.value;")
}

fn if_consequent_body(access: &str) -> String {
  format!("if ({access}) payload.value;")
}

fn if_alternate_body(access: &str) -> String {
  format!("if ({access}) {{ sink(); }} else {{ payload.value; }}")
}

fn logical_rhs_body(access: &str) -> String {
  format!("{access} && payload.value;")
}

fn ternary_branch_body(access: &str) -> String {
  format!("{access} ? payload.value : sink();")
}

fn primitive_initializer(axis: PrimitiveAxis, prefix: &str) -> String {
  match axis.name {
    "ref" => format!("{prefix}ref(true)"),
    "computed" => format!("{prefix}computed(() => true)"),
    "reactive" => format!("{prefix}reactive({{ active: true }})"),
    "readonly" => format!("{prefix}readonly({{ active: true }})"),
    "custom_ref" => {
      format!("{prefix}customRef(() => ({{ get: () => true, set: (_value: boolean) => {{}} }}))")
    }
    _ => String::new(),
  }
}

fn systematic_source(
  primitive: PrimitiveAxis,
  flow: FlowAxis,
  effect: EffectAxis,
  import: ImportAxis,
) -> String {
  let body = (flow.body)(primitive.access);
  let callback = if effect.function_callback {
    format!("function () {{ {body} }}")
  } else {
    format!("() => {{ {body} }}")
  };
  match import {
    ImportAxis::Named => {
      let mut names = BTreeSet::from(["ref", primitive.constructor, effect.callee]);
      let imports = names.iter().copied().collect::<Vec<_>>().join(", ");
      names.clear();
      format!(
        "import {{ {imports} }} from 'vue'; const guard = {}; const payload = ref(0); {}({callback});",
        primitive_initializer(primitive, ""),
        effect.callee
      )
    }
    ImportAxis::Namespace => format!(
      "import * as Vue from 'vue'; const guard = {}; const payload = Vue.ref(0); Vue.{}({callback});",
      primitive_initializer(primitive, "Vue."),
      effect.callee
    ),
  }
}

#[test]
fn covers_one_hundred_systematic_scenarios() {
  let mut names = BTreeSet::new();
  let mut sources = BTreeSet::new();
  let mut scenario_count = 0_usize;

  for primitive in PRIMITIVE_AXES {
    for flow in FLOW_AXES {
      for effect in EFFECT_AXES {
        for import in IMPORT_AXES {
          let name =
            format!("{}::{}::{}::{}", primitive.name, flow.name, effect.name, import.name());
          let source = systematic_source(primitive, flow, effect, import);
          assert!(names.insert(name.clone()), "duplicate systematic scenario name: {name}");
          assert!(sources.insert(source.clone()), "duplicate systematic scenario source: {name}");

          let graph = graph(&source);
          let traced_effect = graph.effects.first();
          assert_eq!(
            traced_effect.map(|effect| effect.callee.as_str()),
            Some(effect.callee),
            "wrong effect resolution in {name}"
          );
          let payload = traced_effect
            .into_iter()
            .flat_map(|effect| &effect.reads)
            .find(|read| read.binding == "payload");
          assert_eq!(
            payload.map(|read| read.kind),
            Some(ReactiveReadKind::Conditional),
            "payload must be conditional in {name}"
          );
          assert_eq!(
            payload.and_then(|read| read.guards.first()).map(|guard| guard.binding.as_str()),
            Some("guard"),
            "guard evidence must be retained in {name}"
          );
          scenario_count = scenario_count.saturating_add(1);
        }
      }
    }
  }

  assert_eq!(scenario_count, 100, "the systematic corpus must contain exactly 100 cases");
  assert_eq!(names.len(), 100, "all systematic scenario names must be unique");
  assert_eq!(sources.len(), 100, "all systematic scenario sources must be unique");
}

#[derive(Clone, Copy)]
struct PayloadAxis {
  name: &'static str,
  constructor: &'static str,
  access: &'static str,
}

#[derive(Clone, Copy)]
enum AliasAxis {
  Direct,
  Aliased,
}

impl AliasAxis {
  const fn name(self) -> &'static str {
    match self {
      Self::Direct => "direct",
      Self::Aliased => "aliased",
    }
  }
}

const COMPLEX_PAYLOAD_AXES: [PayloadAxis; 5] = [
  PayloadAxis { name: "ref", constructor: "ref", access: "payload.value" },
  PayloadAxis { name: "shallow_ref", constructor: "shallowRef", access: "payload.value" },
  PayloadAxis { name: "computed", constructor: "computed", access: "payload.value" },
  PayloadAxis { name: "reactive", constructor: "reactive", access: "payload.count" },
  PayloadAxis { name: "shallow_reactive", constructor: "shallowReactive", access: "payload.count" },
];

const COMPLEX_CONTROLS: [&str; 10] = [
  "sequential_early_returns",
  "nested_if",
  "if_logical",
  "logical_chain",
  "nested_ternary",
  "early_return_then_if",
  "else_if",
  "try_finally_in_branch",
  "switch_in_branch",
  "loop_in_branch",
];

const ALIAS_AXES: [AliasAxis; 2] = [AliasAxis::Direct, AliasAxis::Aliased];

fn payload_initializer(axis: PayloadAxis, constructor: &str) -> String {
  match axis.name {
    "ref" | "shallow_ref" => format!("{constructor}(0)"),
    "computed" => format!("{constructor}(() => 1)"),
    "reactive" | "shallow_reactive" => format!("{constructor}({{ count: 0 }})"),
    _ => String::new(),
  }
}

fn complex_control_body(control: &str, access: &str) -> String {
  match control {
    "sequential_early_returns" => {
      format!("if (!ready.value) return; if (!enabled.value) return; {access};")
    }
    "nested_if" => format!("if (ready.value) {{ if (enabled.value) {{ {access}; }} }}"),
    "if_logical" => format!("if (ready.value) {{ enabled.value && {access}; }}"),
    "logical_chain" => format!("ready.value && enabled.value && {access};"),
    "nested_ternary" => {
      format!("ready.value ? (enabled.value ? {access} : sink()) : sink();")
    }
    "early_return_then_if" => {
      format!("if (!ready.value) return; if (enabled.value) {{ {access}; }}")
    }
    "else_if" => {
      format!("if (!ready.value) {{ sink(); }} else if (enabled.value) {{ {access}; }}")
    }
    "try_finally_in_branch" => {
      format!("if (ready.value) {{ try {{ {access}; }} finally {{ sink(); }} }}")
    }
    "switch_in_branch" => format!(
      "if (ready.value) {{ switch (enabled.value) {{ case true: {access}; break; default: sink(); }} }}"
    ),
    "loop_in_branch" => format!(
      "if (ready.value) {{ for (const item of [enabled.value]) {{ if (item) {{ {access}; }} }} }}"
    ),
    _ => String::new(),
  }
}

fn complex_source(payload: PayloadAxis, control: &str, alias: AliasAxis) -> String {
  match alias {
    AliasAxis::Direct => {
      let imports =
        BTreeSet::from(["ref", "watchEffect", payload.constructor]).into_iter().collect::<Vec<_>>();
      format!(
        "import {{ {} }} from 'vue'; const ready = ref(false); const enabled = ref(false); \
         const payload = {}; watchEffect(() => {{ {} }});",
        imports.join(", "),
        payload_initializer(payload, payload.constructor),
        complex_control_body(control, payload.access)
      )
    }
    AliasAxis::Aliased => {
      let (payload_import, payload_constructor) = if payload.constructor == "ref" {
        (String::new(), "makeRef")
      } else {
        (format!(", {} as makePayload", payload.constructor), "makePayload")
      };
      format!(
        "import {{ ref as makeRef, watchEffect as runEffect{payload_import} }} from 'vue'; \
         const ready = makeRef(false); const enabled = makeRef(false); const payload = {}; \
         runEffect(function () {{ {} }});",
        payload_initializer(payload, payload_constructor),
        complex_control_body(control, payload.access)
      )
    }
  }
}

#[test]
fn covers_one_hundred_complex_single_module_scenarios() {
  let mut names = BTreeSet::new();
  let mut sources = BTreeSet::new();
  let mut scenario_count = 0_usize;

  for payload_axis in COMPLEX_PAYLOAD_AXES {
    for control in COMPLEX_CONTROLS {
      for alias in ALIAS_AXES {
        let name = format!("complex::{control}::{}::{}", payload_axis.name, alias.name());
        let source = complex_source(payload_axis, control, alias);
        assert!(names.insert(name.clone()), "duplicate complex scenario name: {name}");
        assert!(sources.insert(source.clone()), "duplicate complex scenario source: {name}");

        let graph = graph(&source);
        let payload = graph
          .effects
          .first()
          .into_iter()
          .flat_map(|effect| &effect.reads)
          .find(|read| read.binding == "payload");
        assert_eq!(
          payload.map(|read| read.kind),
          Some(ReactiveReadKind::Conditional),
          "complex payload must remain conditional in {name}"
        );
        assert!(
          payload.is_some_and(|read| read.guards.iter().any(|guard| guard.binding == "ready")),
          "outer ready guard evidence must survive in {name}"
        );
        scenario_count = scenario_count.saturating_add(1);
      }
    }
  }

  assert_eq!(scenario_count, 100, "the complex corpus must contain exactly 100 cases");
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

const CROSS_PRIMITIVES: [PayloadAxis; 5] = [
  PayloadAxis { name: "ref", constructor: "ref", access: "payload.value" },
  PayloadAxis { name: "shallow_ref", constructor: "shallowRef", access: "payload.value" },
  PayloadAxis { name: "computed", constructor: "computed", access: "payload.value" },
  PayloadAxis { name: "reactive", constructor: "reactive", access: "payload.count" },
  PayloadAxis { name: "readonly", constructor: "readonly", access: "payload.count" },
];

const CROSS_TOPOLOGIES: [&str; 8] = [
  "direct_named",
  "composable_alias",
  "default_export",
  "star_barrel",
  "named_multihop",
  "cycle",
  "unresolved",
  "conflicting_star",
];

const CROSS_FLOWS: [&str; 2] = ["nested_guards", "logical_ternary"];

fn module_source(id: &str, source: String) -> ModuleSource {
  ModuleSource { id: id.into(), source, language: "ts".into(), kind: ScriptKind::Script }
}

fn module_link(from: &str, specifier: &str, to: &str) -> ModuleLink {
  ModuleLink { from: from.into(), specifier: specifier.into(), to: to.into() }
}

fn producer_initializer(axis: PayloadAxis) -> String {
  match axis.name {
    "ref" | "shallow_ref" => format!("{}(0)", axis.constructor),
    "computed" => format!("{}(() => 1)", axis.constructor),
    "reactive" | "readonly" => format!("{}({{ count: 0 }})", axis.constructor),
    _ => String::new(),
  }
}

fn consumer_source(setup: &str, access: &str, flow: &str) -> String {
  let body = if flow == "nested_guards" {
    format!(
      "if (!ready.value) return; if (enabled.value) {{ try {{ for (const item of [1]) {{ \
       if (item) {{ {access}; }} }} }} finally {{ sink(); }} }}"
    )
  } else {
    format!("ready.value && (enabled.value ? {access} : sink());")
  };
  format!(
    "import {{ ref, watchEffect }} from 'vue'; {setup} \
     const ready = ref(false); const enabled = ref(false); \
     watchEffect(() => {{ {body} }});"
  )
}

fn cross_module_case(
  topology: &str,
  primitive: PayloadAxis,
  flow: &str,
) -> (Vec<ModuleSource>, Vec<ModuleLink>, bool) {
  let producer = format!(
    "import {{ {} }} from 'vue'; export const signal = {};",
    primitive.constructor,
    producer_initializer(primitive)
  );
  match topology {
    "direct_named" => (
      vec![
        module_source("producer.ts", producer),
        module_source(
          "consumer.ts",
          consumer_source(
            "import { signal as payload } from './producer';",
            primitive.access,
            flow,
          ),
        ),
      ],
      vec![module_link("consumer.ts", "./producer", "producer.ts")],
      true,
    ),
    "composable_alias" => (
      vec![
        module_source(
          "producer.ts",
          format!(
            "import {{ {} }} from 'vue'; export function useSignal() {{ \
             const signal = {}; return {{ signal }}; }}",
            primitive.constructor,
            producer_initializer(primitive)
          ),
        ),
        module_source("barrel.ts", "export { useSignal as usePayload } from './producer';".into()),
        module_source(
          "consumer.ts",
          consumer_source(
            "import { usePayload as buildPayload } from './barrel'; \
             const { signal: payload } = buildPayload();",
            primitive.access,
            flow,
          ),
        ),
      ],
      vec![
        module_link("barrel.ts", "./producer", "producer.ts"),
        module_link("consumer.ts", "./barrel", "barrel.ts"),
      ],
      true,
    ),
    "default_export" => (
      vec![
        module_source(
          "producer.ts",
          format!(
            "import {{ {} }} from 'vue'; const signal = {}; export default signal;",
            primitive.constructor,
            producer_initializer(primitive)
          ),
        ),
        module_source(
          "consumer.ts",
          consumer_source("import payload from './producer';", primitive.access, flow),
        ),
      ],
      vec![module_link("consumer.ts", "./producer", "producer.ts")],
      true,
    ),
    "star_barrel" => (
      vec![
        module_source("producer.ts", producer),
        module_source("barrel.ts", "export * from './producer';".into()),
        module_source(
          "consumer.ts",
          consumer_source("import { signal as payload } from './barrel';", primitive.access, flow),
        ),
      ],
      vec![
        module_link("barrel.ts", "./producer", "producer.ts"),
        module_link("consumer.ts", "./barrel", "barrel.ts"),
      ],
      true,
    ),
    "named_multihop" => (
      vec![
        module_source("producer.ts", producer),
        module_source("first.ts", "export { signal as middle } from './producer';".into()),
        module_source("second.ts", "export { middle as finalSignal } from './first';".into()),
        module_source(
          "consumer.ts",
          consumer_source(
            "import { finalSignal as payload } from './second';",
            primitive.access,
            flow,
          ),
        ),
      ],
      vec![
        module_link("first.ts", "./producer", "producer.ts"),
        module_link("second.ts", "./first", "first.ts"),
        module_link("consumer.ts", "./second", "second.ts"),
      ],
      true,
    ),
    "cycle" => (
      vec![
        module_source(
          "producer.ts",
          format!(
            "import {{ loop }} from './barrel'; import {{ {} }} from 'vue'; \
             void loop; export const signal = {};",
            primitive.constructor,
            producer_initializer(primitive)
          ),
        ),
        module_source(
          "barrel.ts",
          "export { signal } from './producer'; export const loop = 1;".into(),
        ),
        module_source(
          "consumer.ts",
          consumer_source("import { signal as payload } from './barrel';", primitive.access, flow),
        ),
      ],
      vec![
        module_link("producer.ts", "./barrel", "barrel.ts"),
        module_link("barrel.ts", "./producer", "producer.ts"),
        module_link("consumer.ts", "./barrel", "barrel.ts"),
      ],
      true,
    ),
    "conflicting_star" => (
      vec![
        module_source("producer.ts", producer),
        module_source(
          "conflict.ts",
          "import { shallowReadonly } from 'vue'; \
           export const signal = shallowReadonly({ value: 0, count: 0 });"
            .into(),
        ),
        module_source(
          "barrel.ts",
          "export * from './producer'; export * from './conflict';".into(),
        ),
        module_source(
          "consumer.ts",
          consumer_source("import { signal as payload } from './barrel';", primitive.access, flow),
        ),
      ],
      vec![
        module_link("barrel.ts", "./producer", "producer.ts"),
        module_link("barrel.ts", "./conflict", "conflict.ts"),
        module_link("consumer.ts", "./barrel", "barrel.ts"),
      ],
      false,
    ),
    _ => (
      vec![
        module_source("producer.ts", producer),
        module_source(
          "consumer.ts",
          consumer_source("import { signal as payload } from './missing';", primitive.access, flow),
        ),
      ],
      Vec::new(),
      false,
    ),
  }
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
  let mut names = BTreeSet::new();
  let mut signatures = BTreeSet::new();
  let mut scenario_count = 0_usize;

  for topology in CROSS_TOPOLOGIES {
    for primitive in CROSS_PRIMITIVES {
      for flow in CROSS_FLOWS {
        let name = format!("modules::{topology}::{}::{flow}", primitive.name);
        let (modules, links, should_trace) = cross_module_case(topology, primitive, flow);
        assert!(modules.len() >= 2, "cross-module case must contain separate files: {name}");
        let signature = modules
          .iter()
          .map(|module| format!("{}\n{}", module.id, module.source))
          .collect::<Vec<_>>()
          .join("\n---module---\n");
        assert!(names.insert(name.clone()), "duplicate module scenario name: {name}");
        assert!(signatures.insert(signature), "duplicate module scenario sources: {name}");

        let traced = traced_modules(&modules, &links);
        let consumer = traced.iter().find(|module| module.id == "consumer.ts");
        let payload = consumer
          .into_iter()
          .flat_map(|module| &module.graph.effects)
          .flat_map(|effect| &effect.reads)
          .find(|read| read.binding == "payload");
        if should_trace {
          assert_eq!(
            payload.map(|read| read.kind),
            Some(ReactiveReadKind::Conditional),
            "linked payload must be conditional in {name}"
          );
          assert!(
            payload.is_some_and(|read| read.guards.iter().any(|guard| guard.binding == "ready")),
            "linked payload must retain its local guard in {name}"
          );
        } else {
          assert!(payload.is_none(), "unresolved module shapes must stay quiet in {name}");
        }
        scenario_count = scenario_count.saturating_add(1);
      }
    }
  }

  assert_eq!(scenario_count, 80, "the module corpus must contain exactly 80 cases");
  assert_eq!(names.len(), 80, "all module scenario names must be unique");
  assert_eq!(signatures.len(), 80, "all module scenario sources must be unique");
}

#[test]
fn does_not_export_function_local_refs_as_module_bindings() {
  let modules = vec![
    module_source(
      "producer.ts",
      "import { ref } from 'vue'; export function useHidden() { \
       const signal = ref(0); return { signal }; }"
        .into(),
    ),
    module_source(
      "consumer.ts",
      consumer_source(
        "import { signal as payload } from './producer';",
        "payload.value",
        "logical_ternary",
      ),
    ),
  ];
  let links = vec![module_link("consumer.ts", "./producer", "producer.ts")];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer
      .into_iter()
      .flat_map(|module| &module.graph.effects)
      .flat_map(|effect| &effect.reads)
      .all(|read| read.binding != "payload"),
    "a composable's function-local ref is not itself a module export"
  );
}

#[test]
fn ignores_shadowed_composable_calls_across_modules() {
  let modules = vec![
    module_source(
      "producer.ts",
      "import { ref } from 'vue'; export function useSignal() { \
       const signal = ref(0); return { signal }; }"
        .into(),
    ),
    module_source(
      "consumer.ts",
      "import { useSignal } from './producer'; import { watchEffect } from 'vue'; \
       function local(useSignal: () => { signal: { value: number } }) { \
       const { signal: payload } = useSignal(); watchEffect(() => payload.value); }"
        .into(),
    ),
  ];
  let links = vec![module_link("consumer.ts", "./producer", "producer.ts")];
  let traced = traced_modules(&modules, &links);
  let consumer = traced.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer
      .into_iter()
      .flat_map(|module| &module.graph.effects)
      .flat_map(|effect| &effect.reads)
      .all(|read| read.binding != "payload"),
    "a parameter shadowing an imported composable must not receive its export shape"
  );
}
