use std::collections::BTreeSet;

use super::*;
use vue_vet_core::ReactiveReadKind;

#[expect(clippy::panic, reason = "unexpected Oxc errors must fail adapter tests")]
fn analyze(source: &str, language: &str) -> ScriptBlockFacts {
  match analyze_script(source, source, 0, language, ScriptKind::Setup) {
    Ok(facts) => facts,
    Err(error) => panic!("script analysis unexpectedly failed: {error}"),
  }
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
fn resolves_aliased_vue_calls_and_member_writes() {
  let facts = analyze(
    "import { ref as makeRef } from 'vue';\
     const props = defineProps(); const x = makeRef(0); props.count += 1;",
    "ts",
  );
  assert!(
    facts.calls.iter().any(|call| {
      call.callee == "makeRef"
        && call
          .resolved_import
          .as_ref()
          .is_some_and(|(source, imported)| source == "vue" && imported == "ref")
    }),
    "aliased Vue imports must resolve at the fact boundary"
  );
  assert_eq!(
    facts
      .calls
      .iter()
      .find(|call| call.callee == "defineProps")
      .and_then(|call| call.assigned_to.as_deref()),
    Some("props"),
    "the identifier assigned from a compiler macro must remain queryable"
  );
  assert!(
    facts
      .member_writes
      .iter()
      .any(|write| { write.object == "props" && write.property.as_deref() == Some("count") }),
    "member writes must be queryable without exposing Oxc AST nodes"
  );
}

#[test]
fn builds_conditional_watch_effect_edges_without_nested_callbacks() {
  let facts = analyze(
    "import { computed, ref, watchEffect } from 'vue';\
     const ready = computed(() => true); const value = ref(0); const nested = ref(0);\
     watchEffect(() => { if (!ready.value) return; console.log(value.value);\
       const later = () => nested.value; void later; });",
    "ts",
  );
  let effect = facts.reactivity_graph.effects.first();
  assert_eq!(effect.map(|effect| effect.callee.as_str()), Some("watchEffect"));
  assert_eq!(
    effect
      .into_iter()
      .flat_map(|effect| &effect.reads)
      .map(|read| (read.binding.as_str(), read.kind, read.guarded_by.as_deref()))
      .collect::<Vec<_>>(),
    [
      ("ready", ReactiveReadKind::Unconditional, None),
      ("value", ReactiveReadKind::Conditional, Some("ready")),
    ]
  );
}

#[test]
fn exported_flag_uses_symbol_id_not_name() {
  let facts = analyze(
    "export const count = 1;\
     export function useInner() { const count = 2; return count; }\
     const local = 3;\
     const \u{8ba1}\u{6570} = 4; export { \u{8ba1}\u{6570} };\
     function hide() { const \u{8ba1}\u{6570} = 5; void \u{8ba1}\u{6570}; }",
    "ts",
  );
  let counts = facts
    .bindings
    .iter()
    .filter(|binding| binding.name == "count")
    .map(|binding| (binding.exported, binding.span.offset))
    .collect::<Vec<_>>();
  assert_eq!(counts.iter().filter(|(exported, _)| *exported).count(), 1);
  assert_eq!(counts.iter().filter(|(exported, _)| !*exported).count(), 1);
  let inner_offset = counts.iter().find(|(exported, _)| !*exported).map(|(_, offset)| *offset);
  let outer_offset = counts.iter().find(|(exported, _)| *exported).map(|(_, offset)| *offset);
  assert!(
    inner_offset.is_some_and(|inner| outer_offset.is_some_and(|outer| inner > outer)),
    "inner shadowed count must not inherit the exported outer symbol; {counts:?}"
  );
  assert!(facts.bindings.iter().any(|binding| binding.name == "useInner" && binding.exported));
  assert!(facts.bindings.iter().any(|binding| binding.name == "local" && !binding.exported));
  let unicode = facts
    .bindings
    .iter()
    .filter(|binding| binding.name == "计数")
    .map(|binding| binding.exported)
    .collect::<Vec<_>>();
  assert_eq!(unicode.iter().filter(|exported| **exported).count(), 1);
  assert_eq!(unicode.iter().filter(|exported| !**exported).count(), 1);
}

#[test]
fn crlf_export_list_marks_root_symbol_only() {
  let facts = analyze(
    "const count = 1;\r\nexport { count };\r\nfunction f(){\r\nconst count = 2;\r\nvoid count;\r\n}\r\n",
    "ts",
  );
  let counts = facts.bindings.iter().filter(|binding| binding.name == "count").collect::<Vec<_>>();
  assert_eq!(counts.iter().filter(|binding| binding.exported).count(), 1);
  assert_eq!(counts.iter().filter(|binding| !binding.exported).count(), 1);
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
fn template_expression_identifiers_use_oxc_ast_not_property_names() {
  assert_eq!(
    template_expression_identifiers("user.name + count", "interpolation"),
    vec!["count".to_owned(), "user".to_owned()],
    "static member properties must not be collected as free reads"
  );
  assert_eq!(
    template_expression_identifiers("item in items", "for"),
    vec!["items".to_owned()],
    "v-for must join only the iterable source, not the alias"
  );
  assert_eq!(
    template_expression_identifiers("(item, index) of list", "for"),
    vec!["list".to_owned()],
    "destructured v-for aliases must not appear as free reads"
  );
  assert_eq!(
    template_expression_identifiers("(item) => item + count", "on"),
    vec!["count".to_owned()],
    "handler parameters must not be treated as free template reads"
  );
  assert_eq!(
    template_expression_identifiers("(item) => { const local = item; return local + total }", "on"),
    vec!["total".to_owned()],
    "inner let/const bindings must be filtered from free reads"
  );
  assert_eq!(
    v_for_alias_identifiers("item in items"),
    vec!["item".to_owned()],
    "simple v-for aliases must be recovered"
  );
  assert_eq!(
    v_for_alias_identifiers("(item, index) of list"),
    vec!["index".to_owned(), "item".to_owned()],
    "paired v-for aliases must be recovered"
  );
  assert_eq!(
    v_for_alias_identifiers("({ id, label }, index) in rows"),
    vec!["id".to_owned(), "index".to_owned(), "label".to_owned()],
    "destructured v-for aliases must be recovered"
  );
  assert_eq!(
    slot_prop_alias_identifiers("{ value, meta }"),
    vec!["meta".to_owned(), "value".to_owned()],
    "slot prop destructuring must bind locals"
  );
  let shadowed = BTreeSet::from(["item".to_owned()]);
  assert_eq!(
    template_expression_identifiers_with_shadow("item + count", "interpolation", &shadowed),
    vec!["count".to_owned()],
    "template-local aliases must not appear as free reads"
  );
  assert!(
    template_expression_identifiers("{ value }", "slot").is_empty(),
    "slot prop patterns are bindings, not free reads"
  );
  assert!(
    template_expression_identifiers("??? not expression", "if").is_empty(),
    "parse failures stay quiet so callers can fall back"
  );
}

#[test]
fn supports_js_ts_jsx_and_tsx() {
  for language in ["js", "ts", "jsx", "tsx"] {
    let facts = analyze("const value = 1", language);
    assert_eq!(facts.language, language, "language selection must stay stable");
  }
}

#[test]
#[expect(clippy::panic, reason = "unexpected Oxc errors must fail adapter tests")]
fn lowers_vue_jsx_v_html_and_inner_html_to_template_facts() {
  let source = "export function Comp() { return <div v-html={html} innerHTML={raw} /> }";
  let analysis = match analyze_module_source(source, source, 0, "tsx", ScriptKind::Script) {
    Ok(analysis) => analysis,
    Err(error) => panic!("tsx analysis failed: {error}"),
  };
  assert!(
    analysis.template_facts.elements.iter().any(|element| {
      element.tag == "div"
        && element.directive("html").is_some()
        && element.directives.iter().filter(|directive| directive.name == "html").count() >= 2
    }),
    "v-html and innerHTML must lower to html directives; got {:?}",
    analysis.template_facts.elements
  );
}

#[test]
fn retains_block_kind_and_original_sfc_offsets() {
  let sfc = "<script>const value = run()</script>";
  let script = "const value = run()";
  let offset = sfc.find(script).unwrap_or_default();
  let facts = analyze_script(sfc, script, offset, "js", ScriptKind::Script);
  assert!(facts.is_ok(), "a normal script block must be analyzable");
  if let Ok(facts) = facts {
    assert_eq!(facts.kind, ScriptKind::Script, "the SFC block kind must be retained");
    assert_eq!(
      facts.calls.first().map(|call| call.span.offset),
      sfc.find("run()"),
      "Oxc spans must map back to the original SFC source"
    );
  }
}

#[test]
fn retains_side_effect_imports_for_project_edges() {
  let facts = analyze("import './setup'", "ts");
  assert_eq!(
    facts.imports.first().map(|import| import.source.as_str()),
    Some("./setup"),
    "side-effect imports must remain visible to the project graph"
  );
}
