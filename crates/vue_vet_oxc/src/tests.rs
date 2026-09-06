use std::collections::BTreeSet;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;

use super::*;
use crate::source_contracts::collect_source_contract_facts_with_stats;
use vue_vet_core::{ReactiveReadKind, ToRefIgnoredKeyReason};

#[expect(clippy::panic, reason = "unexpected Oxc errors must fail adapter tests")]
fn analyze(source: &str, language: &str) -> ScriptBlockFacts {
  match analyze_script(source, source, 0, language, ScriptKind::Setup) {
    Ok(facts) => facts,
    Err(error) => panic!("script analysis unexpectedly failed: {error}"),
  }
}

fn contract_stats(source: &str) -> (vue_vet_core::SourceContractFacts, u64) {
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
  assert!(parsed.diagnostics.is_empty(), "stats fixture failed to parse");
  let built = SemanticBuilder::new().with_build_nodes(true).build(&parsed.program);
  assert!(built.diagnostics.is_empty(), "stats fixture failed semantics");
  let line_index = vue_vet_core::LineIndex::new(source);
  collect_source_contract_facts_with_stats(
    &built.semantic,
    &line_index,
    source,
    0,
    ScriptKind::Setup,
  )
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
fn operand_binding_span_uses_resolved_symbol_not_name() {
  let facts = analyze(
    "import { ref, watch } from 'vue';\n\
     const count = ref(0);\n\
     watch(count, (count) => { if (!count) return; });\n\
     const ok = count > 0;",
    "ts",
  );
  let mut count_bindings: Vec<_> =
    facts.bindings.iter().filter(|binding| binding.name == "count").collect();
  count_bindings.sort_by_key(|binding| binding.span.offset);
  assert_eq!(
    count_bindings.len(),
    2,
    "outer ref and callback param must both bind; {count_bindings:?}"
  );
  let mut count_operands: Vec<_> =
    facts.operands.iter().filter(|operand| operand.name == "count").collect();
  count_operands.sort_by_key(|operand| operand.span.offset);
  assert_eq!(
    count_operands.len(),
    2,
    "callback `!count` and outer `count > 0`; {count_operands:?}"
  );
  assert_eq!(
    count_operands.first().and_then(|operand| operand.binding_span.map(|span| span.offset)),
    count_bindings.get(1).map(|binding| binding.span.offset),
    "callback operand must resolve to the parameter symbol"
  );
  assert_eq!(
    count_operands.get(1).and_then(|operand| operand.binding_span.map(|span| span.offset)),
    count_bindings.first().map(|binding| binding.span.offset),
    "outer operand must resolve to the ref symbol"
  );
}

#[test]
fn operand_binding_span_unicode_and_crlf() {
  let facts = analyze(
    "import { ref } from 'vue';\r\n\
     const \u{8ba1}\u{6570} = ref(0);\r\n\
     const ok = \u{8ba1}\u{6570} > 0;\r\n",
    "ts",
  );
  let binding = facts.bindings.iter().find(|binding| binding.name == "计数");
  let operand = facts.operands.iter().find(|operand| operand.name == "计数");
  assert!(
    binding.is_some_and(|binding| {
      operand.is_some_and(|operand| {
        operand.binding_span.is_some_and(|span| span.offset == binding.span.offset)
      })
    }),
    "CRLF unicode operand must resolve to the ref declaration; bindings={:?} operands={:?}",
    facts.bindings,
    facts.operands
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
    template_expression_identifiers("target = 0", "on"),
    vec!["target".to_owned()],
    "event assignment targets must remain identifier facts"
  );
  assert_eq!(
    template_expression_identifiers("'target'", "on"),
    Vec::<String>::new(),
    "a string containing a binding name is not an identifier fact"
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
    text.is_some_and(|binding| binding.plain_initializer && !binding.escaped),
    "literal local must be a proven plain initializer: {text:?}"
  );
  let form = facts.bindings.iter().find(|binding| binding.name == "form");
  assert!(
    form.is_some_and(|binding| !binding.plain_initializer),
    "unknown member provenance must not look plain: {form:?}"
  );
  let target = facts.bindings.iter().find(|binding| binding.name == "target");
  assert!(
    target.is_some_and(|binding| binding.escaped),
    "object/return uses must mark the binding escaped: {target:?}"
  );
}

#[test]
fn returned_inner_ref_is_escaped() {
  let facts = analyze("function firstWriter() { const result = ref(0); return result }", "ts");
  let result = facts.bindings.iter().find(|binding| binding.name == "result");
  assert!(
    result.is_some_and(|binding| binding.escaped),
    "returned locals must be escaped: {result:?}"
  );
}

#[test]
fn reassigned_binding_is_not_a_proven_plain_local() {
  let facts = analyze("let form;\nform = createForm();", "ts");
  let form = facts.bindings.iter().find(|binding| binding.name == "form");
  assert!(
    form.is_some_and(|binding| binding.plain_initializer && binding.writes >= 1),
    "declaration-time empty init plus later assignment must record writes: {form:?}"
  );
}

#[test]
fn global_undefined_is_plain_and_imported_undefined_is_not() {
  let global = analyze("let form = undefined;", "ts");
  let form = global.bindings.iter().find(|binding| binding.name == "form");
  assert!(
    form.is_some_and(|binding| binding.plain_initializer),
    "global undefined must remain a plain initializer: {form:?}"
  );
  let shadowed =
    analyze("import { undefined } from 'generic-library';\nlet form = undefined;", "ts");
  let form = shadowed.bindings.iter().find(|binding| binding.name == "form");
  assert!(
    form.is_some_and(|binding| !binding.plain_initializer),
    "imported undefined must not look plain: {form:?}"
  );
}

#[test]
fn type_only_imports_share_declaration_span() {
  let facts = analyze(
    "import type { Item, Other } from './types'\nimport { type Flag } from './flags'\n",
    "ts",
  );
  assert!(
    facts.imports.iter().all(|import| import.type_only),
    "import type and inline type specifiers must be type-only: {:?}",
    facts.imports
  );
  let first_span = facts.imports.first().map(|import| import.declaration_span);
  assert!(
    facts
      .imports
      .iter()
      .filter(|import| import.source == "./types")
      .all(
        |import| Some(import.declaration_span) == first_span && import.declaration_span.length > 0
      ),
    "named bindings of one declaration share a declaration span: {:?}",
    facts.imports
  );
}

#[test]
fn object_literal_key_is_proven_without_trailing_spread() {
  assert!(object_literal_has_own_key("{ key: item.id, item }", "key"));
  assert!(object_literal_has_own_key("{ ...extra, key: item.id }", "key"));
  assert!(object_literal_has_own_key("{ 'key': item.id }", "key"));
  assert!(object_literal_has_own_key("({ key: item.id })", "key"));
  assert!(object_literal_has_own_key("({ key: item.id } as const)", "key"));
  assert!(object_literal_has_own_key(
    "({ key: item.id }) satisfies Record<string, unknown>",
    "key"
  ));
  assert!(object_literal_has_own_key("{ [field]: undefined, key: item.id }", "key"));
  assert!(!object_literal_has_own_key("{ key: item.id, [field]: undefined }", "key"));
  assert!(!object_literal_has_own_key("{ key: item.id, ...extra }", "key"));
  assert!(!object_literal_has_own_key("extra", "key"));
  assert!(!object_literal_has_own_key("{ ...extra }", "key"));
  assert!(object_literal_has_own_key("{ key: item.id, 1: true }", "key"));
  assert!(object_literal_has_own_key("{ key: item.id, [2]: true }", "key"));
}

#[test]
fn mixed_type_and_value_import_specifiers_are_distinct() {
  let facts = analyze(
    "import { type Flag, value } from './mod'\nimport { runtime } from './runtime'\n",
    "ts",
  );
  let flag = facts.imports.iter().find(|import| import.local == "Flag");
  let value = facts.imports.iter().find(|import| import.local == "value");
  let runtime = facts.imports.iter().find(|import| import.local == "runtime");
  assert!(flag.is_some_and(|import| import.type_only), "inline type specifier: {flag:?}");
  assert!(
    value.is_some_and(|import| !import.type_only),
    "value specifier stays runtime: {value:?}"
  );
  assert!(
    runtime.is_some_and(|import| !import.type_only),
    "ordinary runtime import stays runtime: {runtime:?}"
  );
  assert_eq!(
    flag.map(|import| import.declaration_span),
    value.map(|import| import.declaration_span),
    "mixed specifiers share one declaration span"
  );
  assert_ne!(
    flag.map(|import| import.declaration_span),
    runtime.map(|import| import.declaration_span),
    "distinct declarations keep distinct spans"
  );
}

#[test]
fn nuxt_config_modules_policy_ignores_comments_and_unrelated_strings() {
  use super::{NuxtContentModulePolicy, parse_nuxt_config};
  assert_eq!(
    nuxt_config_content_modules(
      "nuxt.config.ts",
      "export default defineNuxtConfig({ modules: ['@nuxt/content'] })\n"
    ),
    NuxtContentModulePolicy::IncludesContent
  );
  assert_eq!(
    nuxt_config_content_modules(
      "nuxt.config.ts",
      "// modules: ['@nuxt/content']\nexport default defineNuxtConfig({ modules: [] })\n"
    ),
    NuxtContentModulePolicy::Empty
  );
  assert_eq!(
    nuxt_config_content_modules(
      "nuxt.config.ts",
      "export default defineNuxtConfig({ app: { title: '@nuxt/content' } })\n"
    ),
    NuxtContentModulePolicy::Absent
  );
  assert_eq!(
    nuxt_config_content_modules(
      "nuxt.config.ts",
      "export default defineNuxtConfig({ modules: extra })\n"
    ),
    NuxtContentModulePolicy::Unresolved
  );
  assert_eq!(
    nuxt_config_content_modules(
      "nuxt.config.ts",
      "const example = { modules: ['@nuxt/content'] }; export default defineNuxtConfig({ modules: [] })\n"
    ),
    NuxtContentModulePolicy::Empty
  );
  assert_eq!(
    nuxt_config_content_modules(
      "nuxt.config.ts",
      "const field = 'modules'; export default defineNuxtConfig({ modules: ['@nuxt/content'], [field]: [] })\n"
    ),
    NuxtContentModulePolicy::Unresolved
  );
  assert_eq!(
    nuxt_config_content_modules(
      "nuxt.config.ts",
      "const config = defineNuxtConfig({ modules: ['@nuxt/content'] }); export default config\n"
    ),
    NuxtContentModulePolicy::IncludesContent
  );
  assert_eq!(
    nuxt_config_content_modules("nuxt.config.ts", "export default defineNuxtConfig({ modules: ["),
    NuxtContentModulePolicy::Unresolved
  );
  let src = parse_nuxt_config(
    "nuxt.config.ts",
    "export default defineNuxtConfig({ srcDir: 'ui', modules: ['@nuxt/content'] })\n",
  );
  assert_eq!(src.src_dir.as_deref(), Some("ui"));
  let layers = parse_nuxt_config(
    "nuxt.config.ts",
    "export default defineNuxtConfig({ extends: ['theme-kit'] })\n",
  );
  assert_eq!(layers.extends, ["theme-kit"]);
  assert_eq!(
    nuxt_config_content_modules(
      "nuxt.config.ts",
      "let config = defineNuxtConfig({ modules: ['@nuxt/content'] }); config = defineNuxtConfig({ modules: [] }); export default config\n"
    ),
    NuxtContentModulePolicy::Absent
  );
  assert_eq!(
    nuxt_config_content_modules(
      "nuxt.config.ts",
      "const config = defineNuxtConfig({ modules: [] }); config.modules = ['@nuxt/content']; export default config\n"
    ),
    NuxtContentModulePolicy::Absent
  );
  assert_eq!(
    parse_nuxt_config(
      "nuxt.config.ts",
      "const src = 'ui'; const modules = ['@nuxt/content']; export default defineNuxtConfig({ srcDir: src, modules })\n"
    )
    .src_dir
    .as_deref(),
    Some("ui")
  );
  let computed = parse_nuxt_config(
    "nuxt.config.ts",
    "const field = 'srcDir'; export default defineNuxtConfig({ srcDir: 'ui', [field]: 'other', modules: ['@nuxt/content'] })\n",
  );
  assert_eq!(computed.src_dir, None);
  assert_eq!(computed.modules, NuxtContentModulePolicy::IncludesContent);
  let restored = parse_nuxt_config(
    "nuxt.config.ts",
    "const field = 'srcDir'; export default defineNuxtConfig({ srcDir: 'lost', [field]: 'other', srcDir: 'ui', modules: ['@nuxt/content'] })\n",
  );
  assert_eq!(restored.src_dir.as_deref(), Some("ui"));
  assert_eq!(restored.modules, NuxtContentModulePolicy::IncludesContent);
  assert_eq!(
    nuxt_config_content_modules(
      "nuxt.config.cjs",
      "module.exports = { modules: [] }; module.exports = { modules: ['@nuxt/content'] };\n"
    ),
    NuxtContentModulePolicy::Absent
  );
  assert_eq!(
    nuxt_config_content_modules(
      "nuxt.config.ts",
      "function defineNuxtConfig(value) { return value }\nexport default defineNuxtConfig({ modules: ['@nuxt/content'] })\n"
    ),
    NuxtContentModulePolicy::Unresolved
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
fn deferred_nested_raf_is_not_self_scheduling() {
  let facts = analyze(
    "function render() {\n\
       function later() { requestAnimationFrame(render) }\n\
     }\n\
     requestAnimationFrame(render)\n\
     const paint = () => {\n\
       const later = () => { requestAnimationFrame(paint) }\n\
     }\n\
     requestAnimationFrame(paint)\n",
    "ts",
  );
  assert!(
    facts
      .calls
      .iter()
      .filter(|call| call.callee == "requestAnimationFrame")
      .all(|call| { !call.callback_reschedules_self }),
    "an uncalled nested later() that schedules the outer callback is deferred, not a loop; got {:?}",
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

#[test]
fn extracts_returned_watcher_cleanup_and_skips_unknowns() {
  let facts = analyze(
    "import { watch, watchEffect } from 'vue';\
     const source = { value: 0 };\
     watchEffect(() => { return () => {}; });\
     watch(source, () => { const cleanup = () => {}; return cleanup; });\
     watchEffect(() => { const nested = () => { return () => {}; }; nested(); });\
     watch(() => { return () => source.value; }, () => {});",
    "ts",
  );
  assert_eq!(
    facts.lifetime.returned_watcher_cleanups.len(),
    2,
    "only proven callback returns; got {:?}",
    facts.lifetime.returned_watcher_cleanups
  );
}

#[test]
fn extracts_late_cleanup_await_and_keeps_oncleanup_quiet() {
  let facts = analyze(
    "import { onWatcherCleanup, watchEffect } from 'vue';\
     watchEffect(async () => { await Promise.resolve(); onWatcherCleanup(() => {}); });\
     watchEffect(async (onCleanup) => { await Promise.resolve(); onCleanup(() => {}); });",
    "ts",
  );
  assert_eq!(
    facts.lifetime.late_watcher_cleanups.len(),
    1,
    "callback-bound onCleanup after await must stay quiet; got {:?}",
    facts.lifetime.late_watcher_cleanups
  );
}

#[test]
fn extracts_orphaned_scope_watcher_and_late_dispose() {
  let facts = analyze(
    "import { effectScope, onScopeDispose, watchEffect } from 'vue';\
     const scope = effectScope();\
     scope.run(async () => { await Promise.resolve(); watchEffect(() => {}); onScopeDispose(() => {}); });\
     scope.run(async () => { await Promise.resolve(); onScopeDispose(() => {}, true); });\
     scope.run(async () => { await Promise.resolve(); scope.run(() => { watchEffect(() => {}); }); });",
    "ts",
  );
  assert_eq!(
    facts.lifetime.orphaned_scope_watchers.len(),
    1,
    "sync re-entry and failSilently must stay quiet; {:?}",
    facts.lifetime
  );
  assert_eq!(
    facts.lifetime.late_scope_disposes.len(),
    1,
    "{:?}",
    facts.lifetime.late_scope_disposes
  );
}

#[test]
fn lifetime_spans_cover_unicode_and_crlf() {
  let facts = analyze(
    "import { watchEffect } from 'vue';\r\n\
     watchEffect(() => {\r\n\
       const \u{6e05}\u{7406} = () => {};\r\n\
       return \u{6e05}\u{7406};\r\n\
     });\r\n",
    "ts",
  );
  let fact = facts.lifetime.returned_watcher_cleanups.first();
  assert!(fact.is_some(), "unicode identifier return must be extracted");
  let Some(fact) = fact else {
    return;
  };
  assert_eq!(fact.returned_span.length, "清理".len());
  assert!(fact.returned_span.line >= 3);
}

#[test]
fn alias_cycle_named_callback_stays_quiet() {
  let facts = analyze(
    "import { watchEffect } from 'vue';\
     const a = b; const b = a; watchEffect(a);",
    "ts",
  );
  assert!(facts.lifetime.is_empty(), "alias cycles must not invent facts: {:?}", facts.lifetime);
}

#[test]
fn late_cleanup_skips_explicit_owner_and_fail_silently() {
  let facts = analyze(
    "import { getCurrentWatcher, onWatcherCleanup, watchEffect } from 'vue';\
     watchEffect(async () => {\
       const owner = getCurrentWatcher();\
       await Promise.resolve();\
       onWatcherCleanup(() => {}, false, owner);\
       onWatcherCleanup(() => {}, true);\
     });",
    "ts",
  );
  assert!(
    facts.lifetime.late_watcher_cleanups.is_empty(),
    "explicit owner / failSilently must stay quiet: {:?}",
    facts.lifetime.late_watcher_cleanups
  );
}

#[test]
fn custom_then_and_timeout_data_slot_are_not_deferred() {
  let facts = analyze(
    "import { onWatcherCleanup, watchEffect } from 'vue';\
     const immediate = { then(fn) { fn(); } };\
     watchEffect(() => { immediate.then(() => { onWatcherCleanup(() => {}); }); });\
     watchEffect(() => { setTimeout(() => {}, 0, () => { onWatcherCleanup(() => {}); }); });",
    "ts",
  );
  assert!(
    facts.lifetime.late_watcher_cleanups.is_empty(),
    "unproven then / data-slot timeout must stay quiet: {:?}",
    facts.lifetime.late_watcher_cleanups
  );
}

#[test]
fn reassigned_and_generator_callbacks_stay_quiet() {
  let facts = analyze(
    "import { watchEffect } from 'vue';\
     function cb() { return () => {}; }\
     cb = () => {};\
     watchEffect(cb);\
     watchEffect(function*() { return () => {}; });",
    "ts",
  );
  assert!(
    facts.lifetime.returned_watcher_cleanups.is_empty(),
    "reassigned/generator callbacks must stay quiet: {:?}",
    facts.lifetime.returned_watcher_cleanups
  );
}

#[test]
fn scope_on_reentry_and_escaped_scope_stay_quiet() {
  let facts = analyze(
    "import { effectScope, onScopeDispose, watchEffect } from 'vue';\
     const scope = effectScope();\
     scope.run(async () => {\
       await Promise.resolve();\
       scope.on();\
       watchEffect(() => {});\
       onScopeDispose(() => {});\
       scope.off();\
     });\
     const other = effectScope();\
     hold(other);\
     other.run(async () => { await Promise.resolve(); watchEffect(() => {}); });",
    "ts",
  );
  assert!(
    facts.lifetime.orphaned_scope_watchers.is_empty()
      && facts.lifetime.late_scope_disposes.is_empty(),
    "reentry and escaped scopes must stay quiet: {:?}",
    facts.lifetime
  );
}

#[test]
fn named_effect_scope_run_callback_is_indexed() {
  let facts = analyze(
    "import { effectScope, watchEffect } from 'vue';\
     const scope = effectScope();\
     async function task() { await Promise.resolve(); watchEffect(() => {}); }\
     scope.run(task);",
    "ts",
  );
  assert_eq!(
    facts.lifetime.orphaned_scope_watchers.len(),
    1,
    "named run callbacks must be indexed: {:?}",
    facts.lifetime.orphaned_scope_watchers
  );
}

#[test]
fn lifetime_index_scales_linearly_with_watchers() {
  fn visits_for(source: &str) -> (usize, usize) {
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, source, oxc_span::SourceType::ts()).parse();
    let semantic =
      oxc_semantic::SemanticBuilder::new().with_build_nodes(true).build(&parsed.program).semantic;
    let line_index = vue_vet_core::LineIndex::new(source);
    let (facts, stats) = super::lifetime::collect_with_visits(&semantic, &line_index, source, 0);
    (facts.late_watcher_cleanups.len(), stats.total())
  }
  fn source(count: usize) -> String {
    let mut out = String::from("import { onWatcherCleanup, watchEffect } from 'vue';\n");
    for index in 0..count {
      out.push_str("watchEffect(async () => { await Promise.resolve(); onWatcherCleanup(() => { ");
      out.push_str(&index.to_string());
      out.push_str(" }); });\n");
    }
    out
  }
  let (facts_100, visits_100) = visits_for(&source(100));
  let (facts_200, visits_200) = visits_for(&source(200));
  let (facts_400, visits_400) = visits_for(&source(400));
  assert_eq!((facts_100, facts_200, facts_400), (100, 200, 400));
  assert!(
    visits_400 <= visits_100.saturating_mul(6),
    "node walks must stay near-linear: 100={visits_100} 200={visits_200} 400={visits_400}"
  );
  assert!(
    visits_200 <= visits_100.saturating_mul(3),
    "200 watchers should be about 2x 100: 100={visits_100} 200={visits_200}"
  );
}

#[test]
fn registered_and_returned_disposer_stays_quiet() {
  let facts = analyze(
    "import { onWatcherCleanup, watchEffect } from 'vue';\
     function attach(onCleanup) {\
       const dispose = () => {};\
       onCleanup(dispose);\
       return dispose;\
     }\
     watchEffect(attach);\
     watchEffect(() => { const dispose = () => {}; onWatcherCleanup(dispose); return dispose; });\
     watchEffect(() => { const dispose = () => {}; return dispose; });",
    "ts",
  );
  assert_eq!(
    facts.lifetime.returned_watcher_cleanups.len(),
    1,
    "only unregistered returns: {:?}",
    facts.lifetime.returned_watcher_cleanups
  );
}

#[test]
fn watch_spread_arguments_stay_quiet() {
  let facts = analyze(
    "import { watch } from 'vue';\
     watch(...[], () => () => {}, () => {}, { immediate: true });\
     const source = { value: 0 };\
     watch(source, () => () => {});",
    "ts",
  );
  assert_eq!(
    facts.lifetime.returned_watcher_cleanups.len(),
    1,
    "spread watch must stay quiet: {:?}",
    facts.lifetime.returned_watcher_cleanups
  );
}

#[test]
fn mutated_run_and_helper_escape_stay_quiet() {
  let facts = analyze(
    "import { effectScope, onScopeDispose, watchEffect } from 'vue';\
     const scope = effectScope();\
     scope['run'] = () => undefined;\
     scope.run(async () => { await Promise.resolve(); watchEffect(() => {}); onScopeDispose(() => {}); });\
     const other = effectScope();\
     const alias = other;\
     alias.run = () => undefined;\
     other.run(async () => { await Promise.resolve(); watchEffect(() => {}); });\
     const third = effectScope();\
     const helper = { run(owner) { owner.run = () => undefined; } };\
     helper.run(third);\
     third.run(async () => { await Promise.resolve(); watchEffect(() => {}); });",
    "ts",
  );
  assert!(
    facts.lifetime.orphaned_scope_watchers.is_empty()
      && facts.lifetime.late_scope_disposes.is_empty(),
    "mutated/escaped run must stay quiet: {:?}",
    facts.lifetime
  );
}

#[test]
fn named_run_keeps_independent_owner_when_sibling_escapes() {
  let facts = analyze(
    "import { effectScope, watchEffect } from 'vue';\
     const first = effectScope();\
     const second = effectScope();\
     async function task() { await Promise.resolve(); watchEffect(() => {}); }\
     first.run(task);\
     second.run(task);\
     hold(second);",
    "ts",
  );
  assert_eq!(
    facts.lifetime.orphaned_scope_watchers.len(),
    1,
    "proven first.run(task) must survive second escaping: {:?}",
    facts.lifetime.orphaned_scope_watchers
  );
}

#[test]
fn alias_depth_limit_does_not_poison_shorter_alias() {
  let facts = analyze(
    "import { watchEffect } from 'vue';\
     const f0 = () => () => {};\
     const f1 = f0; const f2 = f1; const f3 = f2; const f4 = f3;\
     const f5 = f4; const f6 = f5; const f7 = f6; const f8 = f7;\
     watchEffect(f8);\
     watchEffect(f1);",
    "ts",
  );
  assert!(
    facts.lifetime.returned_watcher_cleanups.iter().any(|fact| !fact.async_callback),
    "f1 must still resolve after a budget miss on f8: {:?}",
    facts.lifetime.returned_watcher_cleanups
  );
}

#[test]
fn after_await_callback_bound_and_alias_registration_stay_quiet() {
  let facts = analyze(
    "import { watch, watchEffect } from 'vue';\
     const source = { value: 0 };\
     watchEffect(async (onCleanup) => {\
       await Promise.resolve();\
       const dispose = () => {};\
       onCleanup(dispose);\
       return dispose;\
     });\
     watchEffect((onCleanup) => {\
       const dispose = () => {};\
       const alias = dispose;\
       onCleanup(alias);\
       return dispose;\
     });\
     watch(source, async (_v, _o, onCleanup) => {\
       await Promise.resolve();\
       const dispose = () => {};\
       onCleanup(dispose);\
       return dispose;\
     });",
    "ts",
  );
  assert!(
    facts.lifetime.returned_watcher_cleanups.is_empty(),
    "bound/aliased registrations must stay quiet: {:?}",
    facts.lifetime.returned_watcher_cleanups
  );
}

#[test]
fn destructured_watch_parameter_keeps_cleanup_slot() {
  let facts = analyze(
    "import { watch, watchEffect } from 'vue';\
     const source = { value: { x: 0 } };\
     watch(source, ({ x }, old, onCleanup) => {\
       const dispose = () => {};\
       onCleanup(dispose);\
       return dispose;\
     });\
     function shared(onCleanup) {\
       const dispose = () => {};\
       onCleanup(dispose);\
       return dispose;\
     }\
     watch(source, shared);\
     watchEffect(shared);",
    "ts",
  );
  assert_eq!(
    facts.lifetime.returned_watcher_cleanups.len(),
    1,
    "watch(shared) uses value/old/onCleanup slots; watchEffect(shared) stays quiet: {:?}",
    facts.lifetime.returned_watcher_cleanups
  );
  assert_eq!(
    facts.lifetime.returned_watcher_cleanups.first().map(|fact| fact.api),
    Some(vue_vet_core::WatcherApiKind::Watch)
  );
}

#[test]
fn assignment_pattern_and_conditional_alias_unprove_scope() {
  let facts = analyze(
    "import { effectScope, onScopeDispose, watchEffect } from 'vue';\
     const scope = effectScope();\
     ({ run: scope.run } = { run: () => undefined });\
     scope.run(async () => { await Promise.resolve(); watchEffect(() => {}); onScopeDispose(() => {}); });\
     const other = effectScope();\
     const alias = true ? other : effectScope();\
     alias.run = () => undefined;\
     other.run(async () => { await Promise.resolve(); watchEffect(() => {}); });\
     export { other };\
     const local = effectScope();\
     local.run(async () => { await Promise.resolve(); watchEffect(() => {}); });",
    "ts",
  );
  assert_eq!(
    facts.lifetime.orphaned_scope_watchers.len(),
    1,
    "only untouched local scope remains proven: {:?}",
    facts.lifetime.orphaned_scope_watchers
  );
}

#[test]
fn shared_run_callback_selection_scales() {
  fn source(registrations: usize, watchers: usize) -> String {
    let mut out = String::from(
      "import { effectScope, watchEffect } from 'vue';\nasync function task() { await Promise.resolve();\n",
    );
    for index in 0..watchers {
      out.push_str("watchEffect(() => { ");
      out.push_str(&index.to_string());
      out.push_str(" });\n");
    }
    out.push_str("}\n");
    for index in 0..registrations {
      out.push_str("const s");
      out.push_str(&index.to_string());
      out.push_str(" = effectScope(); s");
      out.push_str(&index.to_string());
      out.push_str(".run(task);\n");
    }
    out
  }
  fn visits_for(source: &str) -> usize {
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, source, oxc_span::SourceType::ts()).parse();
    let semantic =
      oxc_semantic::SemanticBuilder::new().with_build_nodes(true).build(&parsed.program).semantic;
    let line_index = vue_vet_core::LineIndex::new(source);
    super::lifetime::collect_with_visits(&semantic, &line_index, source, 0).1.total()
  }
  let visits_small = visits_for(&source(50, 4));
  let visits_large = visits_for(&source(200, 4));
  assert!(
    visits_large <= visits_small.saturating_mul(6),
    "shared-run selection must stay near-linear: 50x4={visits_small} 200x4={visits_large}"
  );
}

#[test]
fn explicit_owner_after_await_suppresses_returned_cleanup() {
  let facts = analyze(
    "import { getCurrentWatcher, onWatcherCleanup, watchEffect } from 'vue';\
     watchEffect(async () => {\
       const owner = getCurrentWatcher();\
       const dispose = () => {};\
       await Promise.resolve();\
       onWatcherCleanup(dispose, false, owner);\
       return dispose;\
     });",
    "ts",
  );
  assert!(
    facts.lifetime.returned_watcher_cleanups.is_empty(),
    "explicit owner must suppress returned-cleanup: {:?}",
    facts.lifetime.returned_watcher_cleanups
  );
}

#[test]
fn spread_cleanup_registration_stays_quiet_and_bare_return_still_reports() {
  let facts = analyze(
    "import { onWatcherCleanup, watchEffect } from 'vue';\
     watchEffect((onCleanup) => { const dispose = () => {}; onCleanup(...[dispose]); return dispose; });\
     watchEffect(() => { const dispose = () => {}; onWatcherCleanup(...[dispose]); return dispose; });\
     watchEffect((onCleanup) => { const dispose = () => {}; onCleanup(...unknown); return dispose; });\
     watchEffect(() => { const dispose = () => {}; return dispose; });",
    "ts",
  );
  assert_eq!(
    facts.lifetime.returned_watcher_cleanups.len(),
    1,
    "only the unregistered return remains: {:?}",
    facts.lifetime.returned_watcher_cleanups
  );
}

#[test]
fn exported_scope_declaration_is_unproven_local_stays_positive() {
  let facts = analyze(
    "import { effectScope, watchEffect } from 'vue';\
     export const scope = effectScope();\
     export function start() { scope.run(async () => { await Promise.resolve(); watchEffect(() => {}); }); }\
     const local = effectScope();\
     local.run(async () => { await Promise.resolve(); watchEffect(() => {}); });\
     const named = effectScope();\
     export { named };\
     named.run(async () => { await Promise.resolve(); watchEffect(() => {}); });\
     const def = effectScope();\
     export default def;\
     def.run(async () => { await Promise.resolve(); watchEffect(() => {}); });",
    "ts",
  );
  assert_eq!(
    facts.lifetime.orphaned_scope_watchers.len(),
    1,
    "only unexported local scope remains proven: {:?}",
    facts.lifetime.orphaned_scope_watchers
  );
}

#[test]
fn source_contracts_classify_vue_identity_and_provenance() {
  let trigger = analyze(
    "import { reactive, triggerRef } from 'vue'; const obj = reactive({ n: 1 }); triggerRef(obj);",
    "ts",
  );
  assert_eq!(
    trigger.source_contracts.trigger_ref_non_ref.len(),
    1,
    "{:?}",
    trigger.source_contracts
  );
  let torefs = analyze("import { toRefs } from 'vue'; toRefs({ a: 1 });", "ts");
  assert_eq!(torefs.source_contracts.torefs_non_proxy.len(), 1, "{:?}", torefs.source_contracts);
  let primitive = analyze("import { reactive } from 'vue'; reactive(0);", "ts");
  assert_eq!(
    primitive.source_contracts.primitive_reactive_target.len(),
    1,
    "{:?}",
    primitive.source_contracts
  );
  let unwrapped = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); watch((n.value) as number, () => {});",
    "ts",
  );
  assert_eq!(
    unwrapped.source_contracts.watch_unwrapped_source.len(),
    1,
    "{:?}",
    unwrapped.source_contracts
  );
  let replaced = analyze(
    "import { reactive, watch } from 'vue';\
     const obj = reactive({ nested: { x: 1 } });\
     watch(obj.nested, () => {});\
     obj.nested = { x: 2 };",
    "ts",
  );
  assert_eq!(
    replaced.source_contracts.watch_replaced_object_source.len(),
    1,
    "{:?}",
    replaced.source_contracts
  );
  let ignored =
    analyze("import { ref, toRef } from 'vue'; const n = ref(0); toRef(n as object, 'k');", "ts");
  assert_eq!(ignored.source_contracts.toref_ignored_key.len(), 1, "{:?}", ignored.source_contracts);
  assert_eq!(
    ignored.source_contracts.toref_ignored_key.first().map(|site| site.reason),
    Some(ToRefIgnoredKeyReason::Ref),
    "{:?}",
    ignored.source_contracts
  );
  let writeback =
    analyze("import { ref, toRef } from 'vue'; const n = ref(0); toRef(n, 'value');", "ts");
  assert!(
    writeback.source_contracts.toref_ignored_key.is_empty(),
    "{:?}",
    writeback.source_contracts
  );
  let cleared = analyze(
    "import { ref, toRef } from 'vue'; const state = ref({ count: 0 }); state.__v_isRef = false; toRef(state, 'count');",
    "ts",
  );
  assert!(
    cleared.source_contracts.toref_ignored_key.is_empty(),
    "cleared __v_isRef must abstain; {:?}",
    cleared.source_contracts
  );
  let deleted = analyze(
    "import { ref, toRef } from 'vue'; const object = ref(1); delete object.__v_isRef; toRef(object, 'future');",
    "ts",
  );
  assert!(
    deleted.source_contracts.toref_ignored_key.is_empty(),
    "deleted __v_isRef must abstain; {:?}",
    deleted.source_contracts
  );
  let tagged = analyze(
    "import { toRef } from 'vue'; const callable = () => 1; callable.__v_isRef = true; callable.value = 2; toRef(callable, 'value');",
    "ts",
  );
  assert!(
    tagged.source_contracts.toref_ignored_key.is_empty(),
    "tagged callable must abstain; {:?}",
    tagged.source_contracts
  );
  let receiver = analyze(
    "import { ref, toRef } from 'vue'; const n = ref(0); n.toString(); toRef(n, 'k');",
    "ts",
  );
  assert!(
    receiver.source_contracts.toref_ignored_key.is_empty(),
    "method receiver must abstain; {:?}",
    receiver.source_contracts
  );
  let helper = analyze(
    "import { ref, toRef } from 'vue'; const n = ref(0); opaque(n); toRef(n, 'k'); function opaque(_value: unknown) {}",
    "ts",
  );
  assert!(
    helper.source_contracts.toref_ignored_key.is_empty(),
    "helper argument must abstain; {:?}",
    helper.source_contracts
  );
  let scope =
    analyze("import { effectScope } from 'vue'; const run = () => {}; effectScope(run);", "ts");
  assert_eq!(scope.source_contracts.effect_scope_callback.len(), 1, "{:?}", scope.source_contracts);
  let detached = analyze("import { effectScope } from 'vue'; effectScope(true);", "ts");
  assert!(
    detached.source_contracts.effect_scope_callback.is_empty(),
    "{:?}",
    detached.source_contracts
  );
}

#[test]
fn source_contracts_toref_capability_escape_routes_stay_quiet() {
  let pattern = analyze(
    "import { ref, toRef } from 'vue';\
     const pattern = ref({ count: 1 });\
     ({ flag: pattern.__v_isRef } = { flag: false });\
     toRef(pattern, 'count');",
    "ts",
  );
  assert!(
    pattern.source_contracts.toref_ignored_key.is_empty(),
    "static pattern __v_isRef write must abstain; {:?}",
    pattern.source_contracts
  );
  let nested_default_rest = analyze(
    "import { ref, toRef } from 'vue';\
     const pattern = ref({ count: 1 });\
     ({ nested: { flag: pattern.__v_isRef = false } = {}, ..._rest } = { nested: {} });\
     toRef(pattern, 'count');",
    "ts",
  );
  assert!(
    nested_default_rest.source_contracts.toref_ignored_key.is_empty(),
    "nested default/rest __v_isRef pattern must abstain; {:?}",
    nested_default_rest.source_contracts
  );
  let ts_wrapper = analyze(
    "import { ref, toRef } from 'vue';\
     const pattern = ref({ count: 1 });\
     ({ flag: (pattern.__v_isRef as boolean) } = { flag: false });\
     toRef(pattern, 'count');",
    "ts",
  );
  assert!(
    ts_wrapper.source_contracts.toref_ignored_key.is_empty(),
    "TS-wrapped pattern __v_isRef write must abstain; {:?}",
    ts_wrapper.source_contracts
  );
  let computed = analyze(
    "import { ref, toRef } from 'vue';\
     const pattern = ref({ count: 1 });\
     const key = '__v_isRef';\
     ({ flag: pattern[key] } = { flag: false });\
     toRef(pattern, 'count');",
    "ts",
  );
  assert!(
    computed.source_contracts.toref_ignored_key.is_empty(),
    "computed pattern marker write must abstain; {:?}",
    computed.source_contracts
  );
  let constructor = analyze(
    "import { ref, toRef } from 'vue';\
     const created = ref({ count: 1 });\
     class ClearMarker { constructor(value: object) { delete (value as { __v_isRef?: boolean }).__v_isRef } }\
     new ClearMarker(created);\
     toRef(created, 'count');",
    "ts",
  );
  assert!(
    constructor.source_contracts.toref_ignored_key.is_empty(),
    "constructor argument must abstain; {:?}",
    constructor.source_contracts
  );
  let tagged = analyze(
    "import { ref, toRef } from 'vue';\
     const tagged = ref({ count: 1 });\
     tagged.clear = function () { delete this.__v_isRef };\
     tagged.clear``;\
     toRef(tagged, 'count');",
    "ts",
  );
  assert!(
    tagged.source_contracts.toref_ignored_key.is_empty(),
    "tagged-template receiver must abstain; {:?}",
    tagged.source_contracts
  );
  let positive = analyze(
    "import { ref, toRef } from 'vue'; const native = ref({ count: 1 }); toRef(native, 'count');",
    "ts",
  );
  assert_eq!(
    positive.source_contracts.toref_ignored_key.len(),
    1,
    "direct toRef(existingRef, staticKey) must stay positive; {:?}",
    positive.source_contracts
  );
  let value_write = analyze(
    "import { ref, toRef } from 'vue';\
     const native = ref({ count: 1 });\
     native.value = { count: 2 };\
     native.count = 3;\
     toRef(native, 'count');",
    "ts",
  );
  assert_eq!(
    value_write.source_contracts.toref_ignored_key.len(),
    1,
    "direct .value/.count writes must keep ignored-key; {:?}",
    value_write.source_contracts
  );
  let writeback =
    analyze("import { ref, toRef } from 'vue'; const n = ref(0); toRef(n, 'value');", "ts");
  assert!(
    writeback.source_contracts.toref_ignored_key.is_empty(),
    "value writeback must stay quiet; {:?}",
    writeback.source_contracts
  );
  let shadow = analyze(
    "function toRef(_source: unknown, _key: string) { return { value: 0 } }\
     const count = { value: 0 };\
     toRef(count, 'n');",
    "ts",
  );
  assert!(
    shadow.source_contracts.toref_ignored_key.is_empty(),
    "local toRef shadow must stay quiet; {:?}",
    shadow.source_contracts
  );
  let source5 =
    analyze("import { ref, watch } from 'vue'; const n = ref(0); watch(n.value, () => {});", "ts");
  assert_eq!(
    source5.source_contracts.watch_unwrapped_source.len(),
    1,
    "direct .value reads must keep source5 unwrapped-watch; {:?}",
    source5.source_contracts
  );
  let source5_data = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ nested: { x: 1 } });\
     watch(state.nested, () => {});\
     state.nested = { x: 2 };",
    "ts",
  );
  assert_eq!(
    source5_data.source_contracts.watch_replaced_object_source.len(),
    1,
    "direct data writes must keep source5 replaced-object; {:?}",
    source5_data.source_contracts
  );
}

#[test]
fn source_contracts_toref_capability_roles_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { ref, toRef } from 'vue'; class Clear { constructor(value: object) { delete (value as { __v_isRef?: boolean }).__v_isRef } }",
    );
    for index in 0..size {
      source.push_str("const n");
      source.push_str(&index.to_string());
      source.push_str(" = ref({ count: 1 }); new Clear(n");
      source.push_str(&index.to_string());
      source.push_str("); toRef(n");
      source.push_str(&index.to_string());
      source.push_str(", 'count');");
    }
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.toref_ignored_key.is_empty(),
      "constructor-arg toRef sites must abstain; {contracts:?}"
    );
    assert!(work > 0, "capability-role walks must count work");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "toref capability-role work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_stay_quiet_for_shadowing_and_unknowns() {
  let facts = analyze(
    "function triggerRef(_value: unknown) {}\
     function toRefs(_value: object) { return {}; }\
     function reactive(_value: unknown) { return _value; }\
     function watch(_source: unknown, _cb: () => void) {}\
     triggerRef({ n: 1 });\
     toRefs({ a: 1 });\
     reactive(0);\
     watch(1, () => {});",
    "ts",
  );
  assert!(
    facts.source_contracts.is_empty(),
    "local shadows must not match Vue APIs; {:?}",
    facts.source_contracts
  );
}

#[test]
fn source_contracts_ignore_type_only_default_and_unresolved_vue_spelling() {
  let type_only = analyze(
    "import { type triggerRef } from 'vue'; const triggerRef = (_value: unknown) => {}; triggerRef({ n: 1 });",
    "ts",
  );
  assert!(type_only.source_contracts.is_empty(), "{:?}", type_only.source_contracts);
  let default_ns =
    analyze("import Vue from 'vue'; const obj = { n: 1 }; Vue.triggerRef(obj);", "ts");
  assert!(default_ns.source_contracts.is_empty(), "{:?}", default_ns.source_contracts);
  let toolkit = analyze("import { triggerRef } from '@vue/toolkit'; triggerRef({ n: 1 });", "ts");
  assert!(toolkit.source_contracts.is_empty(), "{:?}", toolkit.source_contracts);
  let unresolved = analyze("triggerRef(1); reactive(0); watch(1, () => {});", "ts");
  assert!(unresolved.source_contracts.is_empty(), "{:?}", unresolved.source_contracts);
}

#[test]
fn source_contracts_abstain_on_unknown_wrapper_alias_and_unordered_writes() {
  let unknown_wrapper = analyze(
    "import { reactive, triggerRef } from 'vue'; declare const externalRef: { value: number }; triggerRef(reactive(externalRef));",
    "ts",
  );
  assert!(
    unknown_wrapper.source_contracts.trigger_ref_non_ref.is_empty(),
    "{:?}",
    unknown_wrapper.source_contracts
  );
  let alias_write = analyze(
    "import { reactive, ref, watch } from 'vue'; const r = ref(0); const a = r; a.value = reactive({ n: 1 }); watch(r.value, () => {});",
    "ts",
  );
  assert!(
    alias_write.source_contracts.watch_unwrapped_source.is_empty(),
    "{:?}",
    alias_write.source_contracts
  );
  let helper = analyze(
    "import { ref, watch } from 'vue'; const r = ref(0); function initialize(target: { value: unknown }) { target.value = {}; } initialize(r); watch(r.value, () => {});",
    "ts",
  );
  assert!(
    helper.source_contracts.watch_unwrapped_source.is_empty(),
    "{:?}",
    helper.source_contracts
  );
  let scan = analyze(
    "import { reactive, ref, watch } from 'vue'; const r = ref(0); function scan() { watch(r.value, () => {}); } r.value = reactive({ n: 1 }); scan();",
    "ts",
  );
  assert!(scan.source_contracts.watch_unwrapped_source.is_empty(), "{:?}", scan.source_contracts);
  let let_alias = analyze(
    "import { reactive, ref, watch } from 'vue'; const r = ref(0); let alias = r; alias.value = reactive({ n: 1 }); watch(r.value, () => {});",
    "ts",
  );
  assert!(
    let_alias.source_contracts.watch_unwrapped_source.is_empty(),
    "{:?}",
    let_alias.source_contracts
  );
  let boxed = analyze(
    "import { reactive, ref, watch } from 'vue'; const r = ref(0); const box = {}; box.ref = r; box.ref.value = reactive({ n: 1 }); watch(r.value, () => {});",
    "ts",
  );
  assert!(boxed.source_contracts.watch_unwrapped_source.is_empty(), "{:?}", boxed.source_contracts);
  let pattern = analyze(
    "import { reactive, ref, watch } from 'vue'; const r = ref(0); ({ x: r.value } = { x: reactive({ n: 1 }) }); watch(r.value, () => {});",
    "ts",
  );
  assert!(
    pattern.source_contracts.watch_unwrapped_source.is_empty(),
    "{:?}",
    pattern.source_contracts
  );
  let shadowed_map =
    analyze("import { reactive } from 'vue'; class Map {}; reactive(new Map());", "ts");
  assert!(
    shadowed_map.source_contracts.primitive_reactive_target.is_empty(),
    "{:?}",
    shadowed_map.source_contracts
  );
}

#[test]
fn source_contracts_replacement_stays_quiet_for_control_flow_and_shallow() {
  let branched = analyze(
    "import { reactive, watch } from 'vue'; const state = reactive({ p: { x: 1 } }); const flag = true; if (flag) { watch(state.p, () => {}); } else { state.p = {}; }",
    "ts",
  );
  assert!(
    branched.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    branched.source_contracts
  );
  let same = analyze(
    "import { reactive, watch } from 'vue'; const state = reactive({ p: { x: 1 } }); watch(state.p, () => {}); state.p = state.p;",
    "ts",
  );
  assert!(
    same.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    same.source_contracts
  );
  let stopped = analyze(
    "import { reactive, watch } from 'vue'; const state = reactive({ p: { x: 1 } }); const stop = watch(state.p, () => {}); stop(); state.p = {};",
    "ts",
  );
  assert!(
    stopped.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    stopped.source_contracts
  );
  let readonly_wrap = analyze(
    "import { reactive, readonly, watch } from 'vue'; const state = readonly(reactive({ p: { x: 1 } })); watch(state.p, () => {}); state.p = { x: 2 };",
    "ts",
  );
  assert!(
    readonly_wrap.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    readonly_wrap.source_contracts
  );
  let shallow = analyze(
    "import { shallowReactive, watch } from 'vue'; const state = shallowReactive({ p: { x: 1 } }); watch(state.p, () => {}); state.p = { x: 2 };",
    "ts",
  );
  assert!(
    shallow.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    shallow.source_contracts
  );
  let spread = analyze(
    "import { reactive, watch } from 'vue'; declare const input: { p: { x: number } }; const state = reactive({ p: 1, ...input }); watch(state.p, () => {}); state.p = { x: 2 };",
    "ts",
  );
  assert!(
    spread.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    spread.source_contracts
  );
  let compound = analyze(
    "import { reactive, watch } from 'vue'; const state = reactive({ p: { x: 1 } }); watch(state.p, () => {}); state.p ||= {};",
    "ts",
  );
  assert!(
    compound.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    compound.source_contracts
  );
  let cached = analyze(
    "import { reactive, watch } from 'vue'; const obj = {}; const state = reactive({ p: reactive(obj) }); watch(state.p, () => {}); state.p = reactive(obj);",
    "ts",
  );
  assert!(
    cached.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    cached.source_contracts
  );
  let spread_args = analyze(
    "import { watch, ref } from 'vue'; const n = ref(0); const extra = []; watch(n.value, ...extra);",
    "ts",
  );
  assert!(
    spread_args.source_contracts.watch_unwrapped_source.is_empty(),
    "{:?}",
    spread_args.source_contracts
  );
}

#[test]
fn source_contracts_many_watch_sites_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [50_u64, 100, 200] {
    let mut source = String::from("import { ref, watch } from 'vue'; const n = ref(0);");
    for _ in 0..size {
      source.push_str("watch(n.value, () => {});");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.watch_unwrapped_source.len(), usize::try_from(size).unwrap_or(usize::MAX));
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "watch-site work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_assignment_patterns_parse_and_stay_quiet() {
  let facts = analyze(
    "import { reactive, ref, watch } from 'vue';\
     const r = ref(0);\
     const source = { x: 1, y: 2, z: 3 };\
     ({ x: r.value = 0, y: r.value, ...rest } = source);\
     const arr = [1, 2, 3];\
     ([r.value = 1, ...tail] = arr);\
     ({ [String('k')]: r.value } = { k: 9 });\
     watch(r.value, () => {}); void rest; void tail;",
    "ts",
  );
  assert!(facts.source_contracts.watch_unwrapped_source.is_empty(), "{:?}", facts.source_contracts);
}

#[test]
fn source_contracts_shadowed_map_is_not_fresh_allocation() {
  let facts = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ p: { x: 1 } });\
     watch(state.p, () => {});\
     function Map() { return state.p; }\
     state.p = new Map();",
    "ts",
  );
  assert!(
    facts.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    facts.source_contracts
  );
}

#[test]
#[expect(clippy::panic, reason = "missing span evidence must fail the regression")]
fn source_contracts_nested_wrapper_budget_does_not_poison_direct_use() {
  let source = "import { reactive, triggerRef } from 'vue';\
     const shared = { n: 1 };\
     triggerRef(reactive(reactive(reactive(reactive(reactive(reactive(reactive(reactive(reactive(shared))))))))));\
     triggerRef(shared);";
  let facts = analyze(source, "ts");
  let needle = "triggerRef(shared)";
  let Some(call) = source.rfind(needle) else {
    panic!("direct triggerRef(shared) missing");
  };
  let arg = call + "triggerRef(".len();
  assert_eq!(
    facts.source_contracts.trigger_ref_non_ref.len(),
    1,
    "only the direct shared root must report; {:?}",
    facts.source_contracts
  );
  let Some(site) = facts.source_contracts.trigger_ref_non_ref.first() else {
    panic!("direct-use finding missing");
  };
  assert_eq!(site.span.offset, arg, "{site:?} source={source}");
  assert_eq!(site.span.length, "shared".len());
}

#[test]
fn source_contracts_vue_identity_sources() {
  let runtime =
    analyze("import { triggerRef } from '@vue/runtime-core'; triggerRef({ n: 1 });", "ts");
  assert_eq!(
    runtime.source_contracts.trigger_ref_non_ref.len(),
    1,
    "{:?}",
    runtime.source_contracts
  );
  let named_auto = analyze("import { triggerRef } from '#imports'; triggerRef({ n: 1 });", "ts");
  assert_eq!(
    named_auto.source_contracts.trigger_ref_non_ref.len(),
    1,
    "{:?}",
    named_auto.source_contracts
  );
  let ns_auto = analyze("import * as Auto from '#imports'; Auto.triggerRef({ n: 1 });", "ts");
  assert!(ns_auto.source_contracts.is_empty(), "{:?}", ns_auto.source_contracts);
  let custom = analyze("import { useMagic } from '#imports'; useMagic();", "ts");
  assert!(custom.source_contracts.is_empty(), "{:?}", custom.source_contracts);
}

#[test]
fn source_contracts_destructured_binding_reassignment_is_quiet() {
  let facts = analyze(
    "import { reactive, ref, toRefs, triggerRef, watch } from 'vue';\
     let target = 0;\
     ({ target } = { target: ref(1) });\
     triggerRef(target);\
     let data = 0;\
     [data] = [reactive({})];\
     reactive(data);\
     let plain = {};\
     ({ plain } = { plain: reactive({ count: 1 }) });\
     toRefs(plain);\
     let source = 0;\
     [source] = [ref(1)];\
     watch(source, () => {});",
    "ts",
  );
  assert!(
    facts.source_contracts.trigger_ref_non_ref.is_empty(),
    "shorthand destructure to Ref must not keep primitive triggerRef; {:?}",
    facts.source_contracts
  );
  assert!(
    facts.source_contracts.primitive_reactive_target.is_empty(),
    "array destructure to reactive must not keep primitive reactive(); {:?}",
    facts.source_contracts
  );
  assert!(
    facts.source_contracts.torefs_non_proxy.is_empty(),
    "shorthand destructure to reactive must not keep plain toRefs; {:?}",
    facts.source_contracts
  );
  assert!(
    facts.source_contracts.watch_unwrapped_source.is_empty(),
    "array destructure to ref must not keep primitive watch source; {:?}",
    facts.source_contracts
  );
}

#[test]
fn source_contracts_rest_and_default_binding_reassignment_is_quiet() {
  let facts = analyze(
    "import { reactive, ref, triggerRef } from 'vue';\
     let leftover = 0;\
     [...leftover] = [reactive({})];\
     reactive(leftover);\
     let boxed = 0;\
     ({ boxed = ref(1) } = {});\
     triggerRef(boxed);",
    "ts",
  );
  assert!(
    facts.source_contracts.primitive_reactive_target.is_empty(),
    "rest-to-array must invalidate primitive reactive(); {:?}",
    facts.source_contracts
  );
  assert!(
    facts.source_contracts.trigger_ref_non_ref.is_empty(),
    "default-to-Ref must invalidate primitive triggerRef; {:?}",
    facts.source_contracts
  );
}

#[test]
fn source_contracts_watch_then_writes_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64, 128] {
    let mut source = String::from("import { ref, watch } from 'vue'; const n = ref(0);");
    for _ in 0..size {
      source.push_str("watch(n.value, () => {});");
    }
    for _ in 0..size {
      source.push_str("n.value = 1;");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.watch_unwrapped_source.len(), usize::try_from(size).unwrap_or(usize::MAX));
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "watch-then-write work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_many_properties_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut keys = Vec::new();
    for index in 0..size {
      keys.push(format!("p{index}: {{}}"));
    }
    let mut source = format!(
      "import {{ reactive, watch }} from 'vue'; const state = reactive({{ {} }});",
      keys.join(", ")
    );
    for index in 0..size {
      source.push_str("watch(state.p");
      source.push_str(&index.to_string());
      source.push_str(", () => {}); state.p");
      source.push_str(&index.to_string());
      source.push_str(" = {};");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.watch_replaced_object_source.len(),
      usize::try_from(size).unwrap_or(usize::MAX)
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "property-object work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_early_spreads_then_explicit_props_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { toRefs } from 'vue';");
    for index in 0..size {
      source.push_str("const s");
      source.push_str(&index.to_string());
      source.push_str(" = {};");
    }
    source.push_str("toRefs({");
    for index in 0..size {
      source.push_str("...s");
      source.push_str(&index.to_string());
      source.push(',');
    }
    for index in 0..size {
      source.push('p');
      source.push_str(&index.to_string());
      source.push_str(": {},");
    }
    source.push_str("});");
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.torefs_non_proxy.len(),
      1,
      "plain object after early spreads must emit toRefs-on-non-proxy; {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "early-spread then explicit-prop work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}
