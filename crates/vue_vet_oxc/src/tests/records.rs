use super::support::*;

#[test]
fn records_runtime_export_spans_and_skips_type_only_comments_and_strings() {
  let runtime = analyze("export default { name: 'X' }\nexport const n = 1\n", "ts");
  assert_eq!(runtime.runtime_export_spans.len(), 2, "{:?}", runtime.runtime_export_spans);
  assert!(
    runtime
      .runtime_export_spans
      .first()
      .is_some_and(|span| span.offset == 0 && span.length >= "export default".len()),
    "{:?}",
    runtime.runtime_export_spans
  );

  let type_only =
    analyze("export type Foo = string\nexport interface Bar { n: number }\nconst n = 0\n", "ts");
  assert!(type_only.runtime_export_spans.is_empty(), "{:?}", type_only.runtime_export_spans);

  let decoys = analyze(
    "// export default { name: 'commented' }\nconst label = \"export default\"\nconst n = 0\n",
    "ts",
  );
  assert!(decoys.runtime_export_spans.is_empty(), "{:?}", decoys.runtime_export_spans);
}

#[test]
fn records_new_expressions_as_call_facts() {
  let facts = analyze(
    "const io = new IntersectionObserver(() => {});\
     const ro = new ResizeObserver(() => {}); io.disconnect();",
    "ts",
  );
  assert!(
    facts.calls.iter().any(|call| {
      call.callee == "IntersectionObserver" && call.assigned_to.as_deref() == Some("io")
    }),
    "new IntersectionObserver must become a ScriptCallFact; got {:?}",
    facts.calls
  );
  assert!(
    facts
      .calls
      .iter()
      .any(|call| { call.callee == "ResizeObserver" && call.assigned_to.as_deref() == Some("ro") }),
    "new ResizeObserver must become a ScriptCallFact; got {:?}",
    facts.calls
  );
  assert!(
    facts
      .calls
      .iter()
      .any(|call| call.callee == "disconnect" || call.callee.ends_with(".disconnect")),
    "member disconnect calls must remain queryable"
  );
}

#[test]
fn records_member_callees_assignment_targets_and_identifier_args() {
  let facts = analyze(
    "let timer; clearTimeout(timer); timer = setTimeout(() => {}, 0);\
     window.addEventListener('resize', () => {});",
    "ts",
  );
  assert!(
    facts
      .calls
      .iter()
      .any(|call| { call.callee == "setTimeout" && call.assigned_to.as_deref() == Some("timer") }),
    "assignment targets must populate ScriptCallFact.assigned_to"
  );
  assert!(
    facts.calls.iter().any(|call| {
      call.callee == "clearTimeout" && call.argument_identifiers.iter().any(|name| name == "timer")
    }),
    "identifier call arguments must remain queryable without exposing Oxc nodes"
  );
  assert!(
    facts.calls.iter().any(|call| call.callee == "window.addEventListener"),
    "static member callees must remain queryable without exposing Oxc nodes"
  );
}

#[test]
fn records_props_destructures_and_null_template_refs() {
  let facts = analyze(
    "import { ref } from 'vue'; const { title } = defineProps(); const input = ref(null);",
    "ts",
  );
  assert_eq!(facts.destructures.len(), 1);
  assert!(
    facts
      .reactivity_graph
      .bindings
      .iter()
      .any(|binding| binding.name == "input" && binding.initialized_with_null)
  );
}

#[test]
fn records_plain_initializer_and_object_escape() {
  let facts = analyze(
    "let text = '';\n\
     const form = source.form;\n\
     const target = 1;\n\
     const state = { target };\n\
     function useIt() { return target; }",
    "ts",
  );
  let text = facts.bindings.iter().find(|binding| binding.name == "text");
  assert!(
    text.is_some_and(|binding| binding.plain_initializer && binding.mutable && !binding.escaped),
    "literal local must be a proven plain initializer: {text:?}"
  );
  let form = facts.bindings.iter().find(|binding| binding.name == "form");
  assert!(
    form.is_some_and(|binding| !binding.plain_initializer),
    "unknown member provenance must not look plain: {form:?}"
  );
  let target = facts.bindings.iter().find(|binding| binding.name == "target");
  assert!(
    target.is_some_and(|binding| binding.escaped && !binding.mutable),
    "object/return uses must mark the binding escaped: {target:?}"
  );
}

#[test]
fn records_enclosing_callees_and_wrapped_callable_arguments() {
  let facts = analyze(
    "function consume(value) { return value }\n\
     const enabled = consume(computed(() => true));\n\
     onMounted(() => { requestAnimationFrame(() => {});\n\
       watch(value, () => { setTimeout(() => {}, 0) }) })\n\
     unref(input)\n\
     unref((() => 1))\n\
     watchEffect(() => {}, { flush: 'post' })",
    "ts",
  );
  assert!(
    facts.calls.iter().any(|call| {
      call.callee == "requestAnimationFrame"
        && call.enclosing_callees.iter().any(|name| name == "onMounted")
    }),
    "rAF inside onMounted must record enclosing onMounted; got {:?}",
    facts.calls
  );
  assert!(
    facts.calls.iter().any(|call| {
      call.callee == "setTimeout" && call.enclosing_callees.iter().any(|name| name == "watch")
    }),
    "setTimeout inside watch must record enclosing watch; got {:?}",
    facts.calls
  );
  assert!(
    facts.calls.iter().any(|call| call.callee == "unref" && !call.has_function_argument),
    "identifier unref must not look like a getter argument"
  );
  assert!(
    facts.calls.iter().any(|call| call.callee == "unref" && call.has_function_argument),
    "parenthesized getter must set has_function_argument; got {:?}",
    facts.calls
  );
  assert!(
    facts
      .calls
      .iter()
      .any(|call| { call.callee == "watchEffect" && call.flush_option.as_deref() == Some("post") }),
    "watchEffect flush literal must be recorded; got {:?}",
    facts.calls
  );
}

#[test]
fn records_self_scheduling_raf_and_preserves_shadowing() {
  let facts = analyze(
    "onMounted(() => {\n\
       const loop = () => { requestAnimationFrame(loop) }\n\
       requestAnimationFrame(loop)\n\
       requestAnimationFrame(function tick() { requestAnimationFrame(tick) })\n\
       function render() {}\n\
       requestAnimationFrame(render)\n\
       requestAnimationFrame(() => { requestAnimationFrame(render) })\n\
       function tick() { requestAnimationFrame(tick) }\n\
       {\n\
         function tick() {}\n\
         requestAnimationFrame(tick)\n\
       }\n\
     })",
    "ts",
  );
  let rafs: Vec<_> =
    facts.calls.iter().filter(|call| call.callee == "requestAnimationFrame").collect();
  assert!(
    rafs.iter().any(|call| call.callback_reschedules_self
      && call.argument_identifiers.iter().any(|name| name == "loop")),
    "const loop that schedules itself must set callback_reschedules_self; got {:?}",
    facts.calls
  );
  assert!(
    rafs.iter().any(|call| call.callback_reschedules_self && call.has_function_argument),
    "named function tick that schedules itself must set callback_reschedules_self; got {:?}",
    facts.calls
  );
  assert!(
    rafs.iter().any(|call| {
      call.argument_identifiers.iter().any(|name| name == "render")
        && !call.callback_reschedules_self
    }),
    "one-shot named render must not look like a loop; got {:?}",
    facts.calls
  );
  let shadowed = rafs.iter().filter(|call| {
    call.argument_identifiers.iter().any(|name| name == "tick") && !call.has_function_argument
  });
  assert!(
    shadowed.clone().any(|call| !call.callback_reschedules_self),
    "inner shadowed tick must not inherit the outer recursive tick; got {:?}",
    facts.calls
  );
}

#[test]
fn records_watch_effect_flush_certainty_and_resets() {
  let facts = analyze(
    "watchEffect(() => {})\n\
     watchEffect(() => {}, options)\n\
     watchEffect(() => {}, { flush: 'pre', ...rest })\n\
     watchEffect(() => {}, { ...rest, flush: 'sync' })\n\
     watchEffect(() => {}, { flush: 'sync', [key]: true })\n\
     watch(source, () => {}, { flush: 'post' })\n",
    "ts",
  );
  let effects: Vec<_> = facts.calls.iter().filter(|call| call.callee == "watchEffect").collect();
  assert_eq!(effects.len(), 5, "got {:?}", facts.calls);
  let expected = [
    (None, false, "sole callback is default options"),
    (None, true, "nonliteral options are unknown"),
    (None, true, "trailing spread resets known flush"),
    (Some("sync"), false, "final explicit flush restores certainty"),
    (None, true, "unknown computed key resets prior flush"),
  ];
  for (call, (flush, unresolved, label)) in effects.iter().zip(expected) {
    assert_eq!(call.flush_option.as_deref(), flush, "{label}; got {:?}", facts.calls);
    assert_eq!(call.flush_option_unresolved, unresolved, "{label}; got {:?}", facts.calls);
  }
  assert!(
    facts
      .calls
      .iter()
      .any(|call| { call.callee == "watch" && call.flush_option.as_deref() == Some("post") }),
    "watch options are the third argument; got {:?}",
    facts.calls
  );
}

#[test]
fn records_aliased_and_namespace_watch_flush() {
  let facts = analyze(
    "import { watchEffect as effect, watch as observe } from 'vue'\n\
     import { watchEffect as nuxtEffect } from '#imports'\n\
     import * as Vue from 'vue'\n\
     const rest = {}\n\
     const options = { flush: 'post' }\n\
     effect(() => {}, { flush: 'sync' })\n\
     effect(() => {}, options)\n\
     effect(() => {}, { flush: 'sync', ...rest })\n\
     nuxtEffect(() => {}, { flush: 'sync' })\n\
     Vue.watchEffect(() => {}, { flush: 'post' })\n\
     observe(source, () => {}, { flush: 'sync' })\n\
     effect(() => {}, ...spread)\n",
    "ts",
  );
  let effects: Vec<_> = facts.calls.iter().filter(|call| call.callee == "effect").collect();
  assert_eq!(effects.len(), 4, "got {:?}", facts.calls);
  let expected = [
    (Some("sync"), false, "aliased watchEffect records explicit sync"),
    (None, true, "opaque options stay unresolved"),
    (None, true, "trailing object spread resets known flush"),
    (None, true, "spread argument at the options index is unresolved"),
  ];
  for (call, (flush, unresolved, label)) in effects.iter().zip(expected) {
    assert_eq!(call.flush_option.as_deref(), flush, "{label}; got {:?}", facts.calls);
    assert_eq!(call.flush_option_unresolved, unresolved, "{label}; got {:?}", facts.calls);
  }
  assert!(
    facts
      .calls
      .iter()
      .any(|call| { call.callee == "nuxtEffect" && call.flush_option.as_deref() == Some("sync") }),
    "#imports alias must use resolved watchEffect identity; got {:?}",
    facts.calls
  );
  assert!(
    facts.calls.iter().any(|call| {
      call.callee == "Vue.watchEffect" && call.flush_option.as_deref() == Some("post")
    }),
    "namespace Vue.watchEffect must keep supported flush forms; got {:?}",
    facts.calls
  );
  assert!(
    facts
      .calls
      .iter()
      .any(|call| { call.callee == "observe" && call.flush_option.as_deref() == Some("sync") }),
    "aliased watch options are the third argument; got {:?}",
    facts.calls
  );
}
