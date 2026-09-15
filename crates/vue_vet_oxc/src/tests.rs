use std::collections::BTreeSet;
use std::fmt::Write;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;

use super::*;
use crate::source_contracts::{
  ContractSink, SourceContractStats, collect_source_contract_facts_forced_full,
  collect_source_contract_facts_with_stats, contract_sink,
};
use vue_vet_core::{ReactiveReadKind, ToRefIgnoredKeyReason};

#[expect(clippy::panic, reason = "unexpected Oxc errors must fail adapter tests")]
fn analyze(source: &str, language: &str) -> ScriptBlockFacts {
  match analyze_script(source, source, 0, language, ScriptKind::Setup) {
    Ok(facts) => facts,
    Err(error) => panic!("script analysis unexpectedly failed: {error}"),
  }
}

fn contract_stats(source: &str) -> (vue_vet_core::SourceContractFacts, u64) {
  let (facts, stats) = contract_collect(source, ScriptKind::Setup, false);
  (facts, stats.work())
}

fn contract_full_stats(source: &str) -> (vue_vet_core::SourceContractFacts, SourceContractStats) {
  contract_collect(source, ScriptKind::Setup, false)
}

fn contract_collect(
  source: &str,
  kind: ScriptKind,
  force_full: bool,
) -> (vue_vet_core::SourceContractFacts, SourceContractStats) {
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
  assert!(parsed.diagnostics.is_empty(), "stats fixture failed to parse");
  let built = SemanticBuilder::new().with_build_nodes(true).build(&parsed.program);
  assert!(built.diagnostics.is_empty(), "stats fixture failed semantics");
  let line_index = vue_vet_core::LineIndex::new(source);
  if force_full {
    collect_source_contract_facts_forced_full(&built.semantic, &line_index, source, 0, kind)
  } else {
    collect_source_contract_facts_with_stats(&built.semantic, &line_index, source, 0, kind)
  }
}

fn assert_gated_matches_forced(
  source: &str,
  kind: ScriptKind,
) -> vue_vet_core::SourceContractFacts {
  let (gated, gated_stats) = contract_collect(source, kind, false);
  let (forced, forced_stats) = contract_collect(source, kind, true);
  assert_eq!(gated, forced, "gated facts must match forced-full for {source}");
  assert!(
    forced_stats.owners > 0,
    "forced-full must still build owners for {source}: {forced_stats:?}"
  );
  assert!(
    !gated_stats.is_import_preflight_only(),
    "eligible source must not bypass indexes: {source} {gated_stats:?}"
  );
  gated
}

fn assert_bypass(source: &str, kind: ScriptKind) {
  let (facts, stats) = contract_collect(source, kind, false);
  assert!(facts.is_empty(), "expected bypass empty facts for {source}: {facts:?}");
  assert!(
    stats.is_import_preflight_only(),
    "bypass keeps source indexes empty for {source}: {stats:?}"
  );
  assert_eq!(stats.owners, 0, "bypass must leave owner indexes empty for {source}: {stats:?}");
  assert_eq!(
    stats.object_entries, 0,
    "bypass must leave object indexes empty for {source}: {stats:?}"
  );
  assert_eq!(stats.writes, 0, "bypass must leave write indexes empty for {source}: {stats:?}");
  assert!(stats.nodes > 0, "import preflight must visit semantic nodes for {source}");
  let (forced, forced_stats) = contract_collect(source, kind, true);
  assert!(forced.is_empty(), "forced-full must also stay empty for {source}: {forced:?}");
  assert!(
    forced_stats.owners > 0,
    "forced-full must still index owners on bypass fixtures: {source} {forced_stats:?}"
  );
}

fn semantic_node_count(source: &str) -> u64 {
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
  assert!(parsed.diagnostics.is_empty(), "node-count fixture failed to parse");
  let built = SemanticBuilder::new().with_build_nodes(true).build(&parsed.program);
  assert!(built.diagnostics.is_empty(), "node-count fixture failed semantics");
  u64::try_from(built.semantic.nodes().len()).unwrap_or(u64::MAX)
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
fn computed_retention_and_returned_watch_handle_are_owned() {
  let computed = analyze(
    "import { computed, ref, watch } from 'vue';\
     const outer = ref(0);\
     const inner = computed(() => 1);\
     const stop = watch(outer, () => { watch(inner, () => {}, { flush: 'sync' }); }, { flush: 'sync' });\
     outer.value++;\
     stop();",
    "ts",
  );
  assert_eq!(
    computed.lifetime.nested_watch_without_cleanups.len(),
    1,
    "constant computed still retains subscribers: {:?}",
    computed.lifetime
  );
  let returned = analyze(
    "import { ref, watch } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     watch(outer, () => { return watch(inner, () => {}, { flush: 'sync' }); }, { flush: 'sync' });",
    "ts",
  );
  assert_eq!(
    returned.lifetime.returned_watcher_cleanups.len(),
    1,
    "returned inner watch handle: {:?}",
    returned.lifetime
  );
  assert!(
    returned.lifetime.nested_watch_without_cleanups.is_empty(),
    "nested must stay quiet when the inner call is returned: {:?}",
    returned.lifetime
  );
}

#[test]
fn ownership_corpus_covers_repeat_stop_escape_and_reachability() {
  let quiet = [
    "import { ref, watch, watchEffect } from 'vue'; const inner = ref(0); watchEffect(() => { watch(inner, () => {}, { flush: 'sync' }); });",
    "import { ref, watch } from 'vue'; const inner = ref(0); watch(() => 1, () => { watch(inner, () => {}, { flush: 'sync' }); }, { immediate: true, flush: 'sync' });",
    "import { ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); { const stop = watch(outer, () => { watch(inner, () => {}, { flush: 'sync' }); }, { immediate: true, flush: 'sync' }); stop(); }",
    "import { ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); watch(outer, () => { watch(inner, () => {}, { immediate: true, once: true, flush: 'sync' }); }, { flush: 'sync' });",
    "import { ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); watch(outer, () => { watch(() => inner, () => {}, { flush: 'sync' }); }, { flush: 'sync' });",
    "import { ref, watch, watchEffect } from 'vue'; const outer = ref(0); const inner = ref(0); watch(outer, () => { watchEffect(() => { void inner; }); }, { flush: 'sync' });",
    "import { effectScope, ref, watch } from 'vue'; const owner = effectScope(); const outer = ref(0); const inner = ref(0); owner.run(() => { watch(outer, () => { owner.on(); watch(inner, () => {}, { flush: 'sync' }); owner.off(); }, { flush: 'sync' }); });",
    "import { ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); watch(outer, () => { return; watch(inner, () => {}, { flush: 'sync' }); }, { flush: 'sync' });",
    "import { ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); watch(outer, () => { inner.value = 1; watchEffect(() => { inner.value = 2; }); }, { flush: 'sync' });",
    "import { ref, watch, watchEffect } from 'vue'; const outer = ref(0); const inner = ref(0); watch(outer, () => { watchEffect(() => { if (outer.value) inner.value; }); }, { flush: 'sync' });",
    "import { ref, watch, watchEffect } from 'vue'; const outer = ref(0); const inner = ref(0); watch(outer, () => { watchEffect(async () => { await Promise.resolve(); inner.value; }); }, { flush: 'sync' });",
    "import { effectScope, ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); watch(outer, () => { const scope = effectScope(true); scope.run(() => { watch(inner, () => {}, { immediate: true, once: true, flush: 'sync' }); }); }, { flush: 'sync' });",
    "import { effectScope, ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); watch(outer, () => { const scope = effectScope(true); if (false) scope.run(() => { watch(inner, () => {}, { flush: 'sync' }); }); }, { flush: 'sync' });",
    "import { effectScope, ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); const method = 'run'; watch(outer, () => { const scope = effectScope(true); scope[method] = () => undefined; scope.run(() => { watch(inner, () => {}, { flush: 'sync' }); }); }, { flush: 'sync' });",
    "import { effectScope, ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); watch(outer, () => { const scope = effectScope(true); scope.run(() => { watch(inner, () => {}, { flush: 'sync' }); }); new (class { constructor(target: unknown) { void target; } })(scope); }, { flush: 'sync' });",
    "import { effectScope, ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); watch(outer, () => { const scope = effectScope(true); scope.run(() => { watch(inner, () => {}, { flush: 'sync' }); }); tag`${scope}`; }, { flush: 'sync' }); function tag(strings: TemplateStringsArray, value: unknown) { void strings; void value; }",
    "import { effectScope, ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); watch(outer, () => { const scope = effectScope(true); const key = 'run'; for (const method of [key]) scope[method] = () => undefined; scope.run(() => { watch(inner, () => {}, { flush: 'sync' }); }); }, { flush: 'sync' });",
    "import { effectScope, ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); watch(outer, () => { const scope = effectScope(true); delete (scope as { run?: unknown }).run; scope.run(() => { watch(inner, () => {}, { flush: 'sync' }); }); }, { flush: 'sync' });",
  ];
  for source in quiet {
    let facts = analyze(source, "ts");
    assert!(
      facts.lifetime.nested_watch_without_cleanups.is_empty()
        && facts.lifetime.detached_effect_scopes_without_stop.is_empty(),
      "quiet ownership case leaked: {source} {:?}",
      facts.lifetime
    );
  }
  let shared = analyze(
    "import { effectScope, ref, watch } from 'vue';\
     const owner = effectScope();\
     const outer = ref(0);\
     const inner = ref(0);\
     const create = () => { watch(inner, () => {}, { flush: 'sync' }); };\
     owner.run(create);\
     owner.run(() => { watch(outer, create, { flush: 'sync' }); });",
    "ts",
  );
  assert_eq!(
    shared.lifetime.nested_watch_without_cleanups.len(),
    1,
    "shared callback later invocation must leak: {:?}",
    shared.lifetime
  );
}

#[test]
fn second_review_outer_result_roles_scope_and_once_are_exact() {
  let quiet = [
    "import { ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); watch(() => { void outer.value; return 0; }, () => { watch(inner, () => {}, { flush: 'sync' }); }, { immediate: true, flush: 'sync' });",
    "import { computed, effectScope, ref, watch } from 'vue'; const outer = computed(() => 0); const inner = ref(0); const owner = effectScope(); owner.run(() => { watch(outer, () => { watch(inner, () => {}, { flush: 'sync' }); }, { immediate: true, flush: 'sync' }); });",
    "import { ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); watch(outer, () => { watch(() => { return 0; inner.value; }, () => {}, { flush: 'sync' }); }, { flush: 'sync' });",
    "import { ref, watch, watchEffect } from 'vue'; const outer = ref(0); const inner = ref(0); watch(outer, () => { watchEffect(() => { delete inner.value; }, { flush: 'sync' }); }, { flush: 'sync' });",
    "import { effectScope, getCurrentScope, ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); const scopes: Array<ReturnType<typeof effectScope>> = []; watch(outer, () => { const scope = effectScope(true); scope.run(() => { scopes.push(getCurrentScope()); watch(inner, () => {}, { flush: 'sync' }); }); }, { flush: 'sync' });",
    "import { effectScope, getCurrentScope as current, ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); const scopes: Array<ReturnType<typeof effectScope>> = []; watch(outer, () => { const scope = effectScope(true); scope.run(() => { scopes.push(current()); watch(inner, () => {}, { flush: 'sync' }); }); }, { flush: 'sync' });",
    "import { effectScope, ref, watch } from 'vue'; const outer = ref(0); const inner = ref(0); const owner = effectScope(); owner.run(() => { const stop = watch(outer, () => { watch(inner, () => {}, { flush: 'sync' }); }, { immediate: true, flush: 'sync' }); const snapshot = inner.value; stop(); void snapshot; });",
  ];
  for source in quiet {
    let facts = analyze(source, "ts");
    assert!(
      facts.lifetime.nested_watch_without_cleanups.is_empty()
        && facts.lifetime.detached_effect_scopes_without_stop.is_empty(),
      "second-review quiet leaked: {source} {:?}",
      facts.lifetime
    );
  }
  let assignment = analyze(
    "import { ref, watch, watchEffect } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     let snapshot = 0;\
     watch(outer, () => { watchEffect(() => { snapshot = inner.value; }, { flush: 'sync' }); }, { flush: 'sync' });",
    "ts",
  );
  assert_eq!(
    assignment.lifetime.nested_watch_without_cleanups.len(),
    1,
    "assignment RHS must subscribe: {:?}",
    assignment.lifetime
  );
  let effect_once = analyze(
    "import { ref, watch, watchEffect } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     watch(outer, () => { watchEffect(() => { void inner.value; }, { once: true, flush: 'sync' }); }, { flush: 'sync' });",
    "ts",
  );
  assert_eq!(
    effect_once.lifetime.nested_watch_without_cleanups.len(),
    1,
    "effect-family once is ignored: {:?}",
    effect_once.lifetime
  );
  let inherited = analyze(
    "import { ref, watch } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     watch(outer, () => { watch(inner, () => {}, { flush: 'sync' }); }, { __proto__: { once: true, immediate: true }, flush: 'sync' });",
    "ts",
  );
  assert_eq!(
    inherited.lifetime.nested_watch_without_cleanups.len(),
    1,
    "inherited option properties stay repeatable: {:?}",
    inherited.lifetime
  );
  let returned = analyze(
    "import { ref, watch, watchEffect } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     watch(outer, () => { watchEffect(() => inner.value, { flush: 'sync' }); }, { flush: 'sync' });",
    "ts",
  );
  assert_eq!(
    returned.lifetime.nested_watch_without_cleanups.len(),
    1,
    "returned tracked source stays positive: {:?}",
    returned.lifetime
  );
}

#[test]
fn third_review_sync_scope_purity_computed_wrapper_and_assignment_roles() {
  let getter_before = analyze(
    "import { ref, watch } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     const trigger = { get value() { outer.value++; outer.value++; return 0; } };\
     const stop = watch(outer, () => { watch(inner, () => {}, { flush: 'sync' }); }, { flush: 'sync' });\
     const snapshot = trigger.value;\
     stop();\
     void snapshot;",
    "ts",
  );
  assert_eq!(
    getter_before.lifetime.nested_watch_without_cleanups.len(),
    1,
    "arbitrary getter before stop must keep nested: {:?}",
    getter_before.lifetime
  );
  let default_read = analyze(
    "import { ref, watch, watchEffect } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     let snapshot = 0;\
     watch(outer, () => { watchEffect(() => { ({ value: snapshot = inner.value } = {}); }, { flush: 'sync' }); }, { flush: 'sync' });",
    "ts",
  );
  assert_eq!(
    default_read.lifetime.nested_watch_without_cleanups.len(),
    1,
    "assignment default must read: {:?}",
    default_read.lifetime
  );
  let computed_key = analyze(
    "import { ref, watch, watchEffect } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     let snapshot;\
     watch(outer, () => { watchEffect(() => { ({ [inner.value]: snapshot } = {}); }, { flush: 'sync' }); }, { flush: 'sync' });",
    "ts",
  );
  assert_eq!(
    computed_key.lifetime.nested_watch_without_cleanups.len(),
    1,
    "computed assignment key must read: {:?}",
    computed_key.lifetime
  );
  let wrapper = analyze(
    "import { computed, effectScope, ref, watch } from 'vue';\
     const source = computed(() => 0);\
     const inner = ref(0);\
     const owner = effectScope();\
     owner.run(() => { watch(() => source.value, () => { watch(inner, () => {}, { flush: 'sync' }); }, { immediate: true, flush: 'sync' }); });",
    "ts",
  );
  assert!(
    wrapper.lifetime.nested_watch_without_cleanups.is_empty(),
    "stable computed getter wrapper must stay quiet: {:?}",
    wrapper.lifetime
  );
  let late = analyze(
    "import { effectScope, getCurrentScope, ref, watchEffect } from 'vue';\
     const owner = effectScope();\
     const inner = ref(0);\
     owner.run(async () => { await Promise.resolve(); const current = getCurrentScope(); watchEffect(() => { void inner.value; }, { flush: 'sync' }); void current; });",
    "ts",
  );
  assert_eq!(
    late.lifetime.orphaned_scope_watchers.len(),
    1,
    "after-await getCurrentScope must keep orphan owner: {:?}",
    late.lifetime
  );
  assert!(
    late.lifetime.nested_watch_without_cleanups.is_empty()
      && late.lifetime.detached_effect_scopes_without_stop.is_empty(),
    "after-await current-scope must not steal orphan: {:?}",
    late.lifetime
  );
}

#[test]
fn fourth_review_cycle_subscribe_default_fresh_ref_and_may_escape() {
  let object_skip = analyze(
    "import { ref, watch, watchEffect } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     let snapshot;\
     watch(outer, () => { watchEffect(() => { ({ value: snapshot = inner.value } = { value: 42 }); }, { flush: 'sync' }); }, { flush: 'sync' });",
    "ts",
  );
  assert!(
    object_skip.lifetime.nested_watch_without_cleanups.is_empty(),
    "defined object default must skip: {:?}",
    object_skip.lifetime
  );
  let array_skip = analyze(
    "import { ref, watch, watchEffect } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     let snapshot;\
     watch(outer, () => { watchEffect(() => { [snapshot = inner.value] = [42]; }, { flush: 'sync' }); }, { flush: 'sync' });",
    "ts",
  );
  assert!(
    array_skip.lifetime.nested_watch_without_cleanups.is_empty(),
    "defined array default must skip: {:?}",
    array_skip.lifetime
  );
  let async_getter = analyze(
    "import { effectScope, ref, watch } from 'vue';\
     const source = ref(0);\
     const inner = ref(0);\
     const owner = effectScope();\
     owner.run(() => { watch(async () => { await Promise.resolve(); return source.value; }, () => { watch(inner, () => {}, { flush: 'sync' }); }, { immediate: true, flush: 'sync' }); });",
    "ts",
  );
  assert!(
    async_getter.lifetime.nested_watch_without_cleanups.is_empty(),
    "after-await getter result stays unknown: {:?}",
    async_getter.lifetime
  );
  let wrapped = analyze(
    "import { customRef, ref, watch } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     const trigger = ref(customRef(() => ({ get() { outer.value++; outer.value++; return 0; }, set() {} })));\
     const stop = watch(outer, () => { watch(inner, () => {}, { flush: 'sync' }); }, { flush: 'sync' });\
     const snapshot = trigger.value;\
     stop();\
     void snapshot;",
    "ts",
  );
  assert_eq!(
    wrapped.lifetime.nested_watch_without_cleanups.len(),
    1,
    "ref(customRef) preserves custom getter: {:?}",
    wrapped.lifetime
  );
  let fresh = analyze(
    "import { ref, watch } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     const trigger = ref(0);\
     const stop = watch(outer, () => { watch(inner, () => {}, { flush: 'sync' }); }, { flush: 'sync' });\
     const snapshot = trigger.value;\
     stop();\
     void snapshot;",
    "ts",
  );
  assert!(
    fresh.lifetime.nested_watch_without_cleanups.is_empty(),
    "fresh ref(0) snapshot stays pure: {:?}",
    fresh.lifetime
  );
  let conditional = analyze(
    "import { effectScope, getCurrentScope, ref, watch } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     const retained = [];\
     watch(outer, () => { const owner = effectScope(true); owner.run(() => { if (true) retained.push(getCurrentScope()); watch(inner, () => {}, { flush: 'sync' }); }); }, { flush: 'sync' });",
    "ts",
  );
  assert!(
    conditional.lifetime.detached_effect_scopes_without_stop.is_empty(),
    "sync conditional current-scope is MayEscape: {:?}",
    conditional.lifetime
  );
  let cycle = analyze(
    "import { computed, ref, watch } from 'vue';\
     const inner = ref(0);\
     const left = computed(() => right.value);\
     const right = computed(() => left.value);\
     watch(left, () => { watch(inner, () => {}, { flush: 'sync' }); }, { flush: 'sync' });",
    "ts",
  );
  assert!(
    cycle.lifetime.nested_watch_without_cleanups.is_empty(),
    "mutual computed cycle stays unknown: {:?}",
    cycle.lifetime
  );
  let self_cycle = analyze(
    "import { computed, ref, watch } from 'vue';\
     const inner = ref(0);\
     const looped = computed(() => looped.value);\
     watch(looped, () => { watch(inner, () => {}, { flush: 'sync' }); }, { flush: 'sync' });",
    "ts",
  );
  assert!(
    self_cycle.lifetime.nested_watch_without_cleanups.is_empty(),
    "self computed cycle stays unknown: {:?}",
    self_cycle.lifetime
  );
  let changing_chain = analyze(
    "import { computed, ref, watch } from 'vue';\
     const src = ref(0);\
     const inner = ref(0);\
     const c0 = computed(() => src.value);\
     const c1 = computed(() => c0.value);\
     const c2 = computed(() => c1.value);\
     const c3 = computed(() => c2.value);\
     watch(() => c3.value, () => { watch(inner, () => {}, { flush: 'sync' }); }, { immediate: true, flush: 'sync' });",
    "ts",
  );
  assert_eq!(
    changing_chain.lifetime.nested_watch_without_cleanups.len(),
    1,
    "acyclic changing computed chain stays nested: {:?}",
    changing_chain.lifetime
  );
  let stable_chain = analyze(
    "import { computed, ref, watch } from 'vue';\
     const inner = ref(0);\
     const c0 = computed(() => 0);\
     const c1 = computed(() => c0.value);\
     const c2 = computed(() => c1.value);\
     const c3 = computed(() => c2.value);\
     watch(() => c3.value, () => { watch(inner, () => {}, { flush: 'sync' }); }, { immediate: true, flush: 'sync' });",
    "ts",
  );
  assert!(
    stable_chain.lifetime.nested_watch_without_cleanups.is_empty(),
    "acyclic stable computed chain stays quiet: {:?}",
    stable_chain.lifetime
  );
}

#[test]
fn extracts_nested_watch_without_cleanup_and_keeps_controls_quiet() {
  let facts = analyze(
    "import { ref, watch } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     watch(outer, () => { watch(inner, () => {}, { flush: 'sync' }); }, { flush: 'sync' });\
     watch(outer, () => { const stop = watch(inner, () => {}, { flush: 'sync' }); stop(); }, { flush: 'sync' });\
     watch(outer, () => { watch(inner, () => {}, { flush: 'sync' }); }, { flush: 'sync', once: true });\
     watch(outer, () => { const local = ref(0); watch(local, () => {}, { flush: 'sync' }); }, { flush: 'sync' });\
     watch(outer, () => { if (true) watch(inner, () => {}, { flush: 'sync' }); }, { flush: 'sync' });",
    "ts",
  );
  assert_eq!(
    facts.lifetime.nested_watch_without_cleanups.len(),
    1,
    "only discarded repeating inner with external source: {:?}",
    facts.lifetime.nested_watch_without_cleanups
  );
}

#[test]
fn extracts_detached_scope_without_stop_and_suppresses_nested_child() {
  let facts = analyze(
    "import { effectScope, ref, watch } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     watch(outer, () => {\
       const scope = effectScope(true);\
       scope.run(() => { watch(inner, () => {}, { flush: 'sync' }); });\
     }, { flush: 'sync' });\
     watch(outer, () => {\
       const owned = effectScope(true);\
       owned.run(() => { watch(inner, () => {}, { flush: 'sync' }); });\
       return () => owned.stop();\
     }, { flush: 'sync' });\
     function factory() {\
       const scope = effectScope(true);\
       scope.run(() => { watch(inner, () => {}, { flush: 'sync' }); });\
     }\
     factory();",
    "ts",
  );
  assert_eq!(
    facts.lifetime.detached_effect_scopes_without_stop.len(),
    1,
    "function factories and returned disposers stay quiet: {:?}",
    facts.lifetime.detached_effect_scopes_without_stop
  );
  assert!(
    facts.lifetime.nested_watch_without_cleanups.is_empty(),
    "detached scope must suppress nested child: {:?}",
    facts.lifetime.nested_watch_without_cleanups
  );
}

#[test]
#[expect(clippy::panic, reason = "missing unicode/CRLF spans must fail the adapter test")]
fn nested_and_detached_ownership_spans_cover_unicode_and_crlf() {
  let source = "import { effectScope, ref, watch } from 'vue';\r\n\
     const \u{5916}\u{5c42} = ref(0);\r\n\
     const \u{5185}\u{5c42} = ref(0);\r\n\
     watch(\u{5916}\u{5c42}, () => {\r\n\
       watch(\u{5185}\u{5c42}, () => {}, { flush: 'sync' });\r\n\
       const \u{4f5c}\u{7528}\u{57df} = effectScope(true);\r\n\
       \u{4f5c}\u{7528}\u{57df}.run(() => { watch(\u{5185}\u{5c42}, () => {}, { flush: 'sync' }); });\r\n\
     }, { flush: 'sync' });\r\n";
  let facts = analyze(source, "ts");
  let Some(nested) = facts.lifetime.nested_watch_without_cleanups.first() else {
    panic!("unicode nested watch must be extracted: {:?}", facts.lifetime);
  };
  let Some(detached) = facts.lifetime.detached_effect_scopes_without_stop.first() else {
    panic!("unicode detached scope must be extracted: {:?}", facts.lifetime);
  };
  let line_index = vue_vet_core::LineIndex::new(source);
  let inner_needle = "watch(\u{5185}\u{5c42}, () => {}, { flush: 'sync' })";
  let Some(inner_offset) = source.find(inner_needle) else {
    panic!("missing inner watch call");
  };
  let (inner_line, inner_col) = line_index.byte_to_line_column(inner_offset);
  assert_eq!(nested.inner_span.offset, inner_offset);
  assert_eq!(nested.inner_span.length, inner_needle.len());
  assert_eq!(nested.inner_span.line, inner_line);
  assert_eq!(nested.inner_span.column, inner_col);
  let Some(source_offset) = source.find("\u{5185}\u{5c42}, () => {}, { flush: 'sync' }") else {
    panic!("missing inner source");
  };
  assert_eq!(nested.source_span.offset, source_offset);
  assert_eq!(nested.source_span.length, "内层".len());
  let Some(scope_offset) = source.find("effectScope(true)") else {
    panic!("missing detached scope");
  };
  assert_eq!(detached.scope_span.offset, scope_offset);
  assert_eq!(detached.scope_span.length, "effectScope(true)".len());
  let (scope_line, scope_col) = line_index.byte_to_line_column(scope_offset);
  assert_eq!(detached.scope_span.line, scope_line);
  assert_eq!(detached.scope_span.column, scope_col);
}

#[test]
#[expect(clippy::panic, reason = "missing unicode/CRLF wrapped-getter spans must fail")]
fn wrapped_custom_ref_getter_spans_cover_unicode_and_crlf() {
  let source = "import { customRef, ref, watch } from 'vue';\r\n\
     const \u{5916}\u{5c42} = ref(0);\r\n\
     const \u{5185}\u{5c42} = ref(0);\r\n\
     const \u{89e6}\u{53d1} = ref(customRef(() => ({ get() { \u{5916}\u{5c42}.value++; \u{5916}\u{5c42}.value++; return 0; }, set() {} })));\r\n\
     const stop = watch(\u{5916}\u{5c42}, () => {\r\n\
       watch(\u{5185}\u{5c42}, () => {}, { flush: 'sync' });\r\n\
     }, { flush: 'sync' });\r\n\
     const snapshot = \u{89e6}\u{53d1}.value;\r\n\
     stop();\r\n\
     void snapshot;\r\n";
  let facts = analyze(source, "ts");
  let Some(nested) = facts.lifetime.nested_watch_without_cleanups.first() else {
    panic!("unicode wrapped custom getter must keep nested: {:?}", facts.lifetime);
  };
  let line_index = vue_vet_core::LineIndex::new(source);
  let inner_needle = "watch(\u{5185}\u{5c42}, () => {}, { flush: 'sync' })";
  let Some(inner_offset) = source.find(inner_needle) else {
    panic!("missing inner watch call");
  };
  let (inner_line, inner_col) = line_index.byte_to_line_column(inner_offset);
  assert_eq!(nested.inner_span.offset, inner_offset);
  assert_eq!(nested.inner_span.length, inner_needle.len());
  assert_eq!(nested.inner_span.line, inner_line);
  assert_eq!(nested.inner_span.column, inner_col);
  assert!(source.contains('\r'), "fixture must keep physical CRLF");
}

#[test]
#[expect(
  clippy::print_stderr,
  clippy::too_many_lines,
  reason = "growth work counts and seven corpus shapes are captured for the repair report"
)]
fn nested_inner_work_scales_with_shared_source() {
  const OVERHEAD: usize = 256;
  fn visits_for(source: &str) -> (usize, usize, usize, super::lifetime::CollectStats) {
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, source, oxc_span::SourceType::ts()).parse();
    let semantic =
      oxc_semantic::SemanticBuilder::new().with_build_nodes(true).build(&parsed.program).semantic;
    let line_index = vue_vet_core::LineIndex::new(source);
    let (facts, stats) = super::lifetime::collect_with_visits(&semantic, &line_index, source, 0);
    (
      facts.nested_watch_without_cleanups.len(),
      facts.detached_effect_scopes_without_stop.len(),
      facts.returned_watcher_cleanups.len(),
      stats,
    )
  }
  fn assert_linear(label: &str, a: usize, b: usize) {
    assert!(a > 0 && b > 0, "{label} must count actual work: {a} -> {b}");
    assert!(
      b < a.saturating_mul(3).saturating_add(OVERHEAD),
      "{label} must stay linear: {a} -> {b}"
    );
  }
  fn shared(count: usize, active: bool) -> String {
    let reads = "void plain;\n".repeat(count);
    let inners = "watchEffect(getter, { flush: 'sync' });\n".repeat(count);
    let outers = "watch(outer, callback, { flush: 'sync' });\n".repeat(count);
    let result = if active { "inner.value" } else { "plain" };
    format!(
      "import {{ ref, watch, watchEffect }} from 'vue';\n\
const outer = ref(0);\n\
const inner = ref(0);\n\
const plain = 0;\n\
const getter = () => {{\n{reads}return {result};\n}};\n\
const callback = () => {{\n{inners}}};\n\
{outers}"
    )
  }
  fn watch_stop_pairs(count: usize) -> String {
    let mut body = String::new();
    for index in 0..count {
      body.push_str("const stop");
      body.push_str(&index.to_string());
      body.push_str(" = watch(outer, () => { watch(inner, () => {}, { flush: 'sync' }); }, { flush: 'sync' });\nstop");
      body.push_str(&index.to_string());
      body.push_str("();\n");
    }
    format!(
      "import {{ ref, watch }} from 'vue';\n\
const outer = ref(0);\n\
const inner = ref(0);\n\
function run() {{\n{body}}}\n\
run();"
    )
  }
  fn returned_handles(count: usize) -> String {
    let mut outers = String::new();
    for index in 0..count {
      outers.push_str("watch(outer, () => { return watch(inner, () => { ");
      outers.push_str(&index.to_string());
      outers.push_str(" }, { flush: 'sync' }); }, { flush: 'sync' });\n");
    }
    format!(
      "import {{ ref, watch }} from 'vue';\n\
const outer = ref(0);\n\
const inner = ref(0);\n\
{outers}"
    )
  }
  fn toggles_and_inners(count: usize) -> String {
    let mut body = String::new();
    for index in 0..count {
      body.push_str("owner.on();\nwatch(inner, () => { ");
      body.push_str(&index.to_string());
      body.push_str(" }, { flush: 'sync' });\nowner.off();\n");
    }
    format!(
      "import {{ effectScope, ref, watch }} from 'vue';\n\
const owner = effectScope();\n\
const outer = ref(0);\n\
const inner = ref(0);\n\
owner.run(() => {{\n\
  watch(outer, () => {{\n{body}}}, {{ flush: 'sync' }});\n\
}});"
    )
  }
  fn dense_negative_scopes(count: usize) -> String {
    let mut out = String::from(
      "import { effectScope, ref, watch } from 'vue';\n\
const outer = ref(0);\n\
const inner = ref(0);\n\
const leaked = [];\n",
    );
    for _ in 0..count {
      out.push_str(
        "watch(outer, () => { const scope = effectScope(true); scope.run(() => { watch(inner, () => {}, { flush: 'sync' }); }); leaked.push(scope); }, { flush: 'sync' });\n",
      );
    }
    out
  }
  fn repeated_stop(count: usize) -> String {
    let mut snapshots = String::new();
    for index in 0..count {
      snapshots.push_str("const snapshot");
      snapshots.push_str(&index.to_string());
      snapshots.push_str(" = inner.value;\n");
    }
    let stops = "stop();\n".repeat(count);
    format!(
      "import {{ ref, watch }} from 'vue';\n\
function install() {{\n\
const outer = ref(0);\n\
const inner = ref(0);\n\
const stop = watch(outer, () => {{ watch(inner, () => {{}}, {{ flush: 'sync' }}); }}, {{ flush: 'sync' }});\n\
{snapshots}{stops}}}\n"
    )
  }
  fn shared_current_scope(count: usize) -> String {
    let mut lookups = String::new();
    let mut runs = String::new();
    for index in 0..count {
      lookups.push_str("const current");
      lookups.push_str(&index.to_string());
      lookups.push_str(" = getCurrentScope();\n");
      runs.push_str("const owner");
      runs.push_str(&index.to_string());
      runs.push_str(" = effectScope(true); owner");
      runs.push_str(&index.to_string());
      runs.push_str(".run(install);\n");
    }
    format!(
      "import {{ effectScope, getCurrentScope, ref, watch }} from 'vue';\n\
const inner = ref(0);\n\
const install = () => {{\n{lookups}watch(inner, () => {{}}, {{ flush: 'sync' }});\n}};\n\
{runs}"
    )
  }
  fn shared_computed(count: usize) -> String {
    let mut wrappers = String::new();
    let mut watches = String::new();
    for index in 0..count {
      wrappers.push_str("const wrap");
      wrappers.push_str(&index.to_string());
      wrappers.push_str(" = computed(() => leaf.value);\n");
      watches.push_str("watch(() => wrap");
      watches.push_str(&index.to_string());
      watches.push_str(".value, callback, { immediate: true, flush: 'sync' });\n");
    }
    format!(
      "import {{ computed, ref, watch }} from 'vue';\n\
const leaf = computed(() => 0);\n\
const inner = ref(0);\n\
const callback = () => {{ watch(inner, () => {{}}, {{ flush: 'sync' }}); }};\n\
{wrappers}{watches}"
    )
  }
  let (facts_20, detached_20, _, shared_20) = visits_for(&shared(20, true));
  let (facts_40, detached_40, _, shared_40) = visits_for(&shared(40, true));
  let (facts_80, detached_80, _, shared_80) = visits_for(&shared(80, true));
  let (facts_160, detached_160, _, shared_160) = visits_for(&shared(160, true));
  let (quiet_80, quiet_detached, _, quiet) = visits_for(&shared(80, false));
  let (stop_20, _, _, stop_stats_20) = visits_for(&watch_stop_pairs(20));
  let (stop_40, _, _, stop_stats_40) = visits_for(&watch_stop_pairs(40));
  let (stop_80, _, _, stop_stats_80) = visits_for(&watch_stop_pairs(80));
  let (stop_160, _, _, stop_stats_160) = visits_for(&watch_stop_pairs(160));
  let (_, _, returned_20, ret_20) = visits_for(&returned_handles(20));
  let (_, _, returned_40, ret_40) = visits_for(&returned_handles(40));
  let (_, _, returned_80, ret_80) = visits_for(&returned_handles(80));
  let (_, _, returned_160, ret_160) = visits_for(&returned_handles(160));
  let (toggle_20, _, _, tog_20) = visits_for(&toggles_and_inners(20));
  let (toggle_40, _, _, tog_40) = visits_for(&toggles_and_inners(40));
  let (toggle_80, _, _, tog_80) = visits_for(&toggles_and_inners(80));
  let (toggle_160, _, _, tog_160) = visits_for(&toggles_and_inners(160));
  let (scope_20, scope_detached_20, _, scope_20_stats) = visits_for(&dense_negative_scopes(20));
  let (scope_40, scope_detached_40, _, scope_40_stats) = visits_for(&dense_negative_scopes(40));
  let (scope_80, scope_detached_80, _, scope_80_stats) = visits_for(&dense_negative_scopes(80));
  let (scope_160, scope_detached_160, _, scope_160_stats) = visits_for(&dense_negative_scopes(160));
  let (repeat_20, repeat_det_20, _, repeat_20_stats) = visits_for(&repeated_stop(20));
  let (repeat_40, repeat_det_40, _, repeat_40_stats) = visits_for(&repeated_stop(40));
  let (repeat_80, repeat_det_80, _, repeat_80_stats) = visits_for(&repeated_stop(80));
  let (repeat_160, repeat_det_160, _, repeat_160_stats) = visits_for(&repeated_stop(160));
  let (cur_20, cur_det_20, _, cur_20_stats) = visits_for(&shared_current_scope(20));
  let (cur_40, cur_det_40, _, cur_40_stats) = visits_for(&shared_current_scope(40));
  let (cur_80, cur_det_80, _, cur_80_stats) = visits_for(&shared_current_scope(80));
  let (cur_160, cur_det_160, _, cur_160_stats) = visits_for(&shared_current_scope(160));
  let (comp_20, comp_det_20, _, comp_20_stats) = visits_for(&shared_computed(20));
  let (comp_40, comp_det_40, _, comp_40_stats) = visits_for(&shared_computed(40));
  let (comp_80, comp_det_80, _, comp_80_stats) = visits_for(&shared_computed(80));
  let (comp_160, comp_det_160, _, comp_160_stats) = visits_for(&shared_computed(160));
  assert_eq!((facts_20, detached_20), (20, 0));
  assert_eq!((facts_40, detached_40), (40, 0));
  assert_eq!((facts_80, detached_80), (80, 0));
  assert_eq!((facts_160, detached_160), (160, 0));
  assert_eq!((quiet_80, quiet_detached), (0, 0));
  assert_eq!((stop_20, stop_40, stop_80, stop_160), (0, 0, 0, 0));
  assert_eq!((returned_20, returned_40, returned_80, returned_160), (20, 40, 80, 160));
  assert_eq!((toggle_20, toggle_40, toggle_80, toggle_160), (0, 0, 0, 0));
  assert_eq!((scope_20, scope_detached_20), (0, 0));
  assert_eq!((scope_40, scope_detached_40), (0, 0));
  assert_eq!((scope_80, scope_detached_80), (0, 0));
  assert_eq!((scope_160, scope_detached_160), (0, 0));
  assert_eq!((repeat_20, repeat_det_20, repeat_40, repeat_det_40), (0, 0, 0, 0));
  assert_eq!((repeat_80, repeat_det_80, repeat_160, repeat_det_160), (0, 0, 0, 0));
  assert_eq!((cur_20, cur_det_20, cur_40, cur_det_40), (0, 0, 0, 0));
  assert_eq!((cur_80, cur_det_80, cur_160, cur_det_160), (0, 0, 0, 0));
  assert_eq!((comp_20, comp_det_20, comp_40, comp_det_40), (0, 0, 0, 0));
  assert_eq!((comp_80, comp_det_80, comp_160, comp_det_160), (0, 0, 0, 0));
  assert!(
    comp_20_stats.computed_edges > 0
      && comp_40_stats.computed_edges > comp_20_stats.computed_edges
      && comp_80_stats.computed_edges > comp_40_stats.computed_edges
      && comp_160_stats.computed_edges > comp_80_stats.computed_edges,
    "shared computed chains must count actual edges: {} {} {} {}",
    comp_20_stats.computed_edges,
    comp_40_stats.computed_edges,
    comp_80_stats.computed_edges,
    comp_160_stats.computed_edges
  );
  eprintln!(
    "work shared20={} shared40={} shared80={} shared160={} quiet80={} stop20={} stop40={} stop80={} stop160={} ret20={} ret40={} ret80={} ret160={} tog20={} tog40={} tog80={} tog160={} scope20={} scope40={} scope80={} scope160={} stmts/refs/watchers/toggles shared80={}/{}/{}/{}",
    shared_20.work(),
    shared_40.work(),
    shared_80.work(),
    shared_160.work(),
    quiet.work(),
    stop_stats_20.work(),
    stop_stats_40.work(),
    stop_stats_80.work(),
    stop_stats_160.work(),
    ret_20.work(),
    ret_40.work(),
    ret_80.work(),
    ret_160.work(),
    tog_20.work(),
    tog_40.work(),
    tog_80.work(),
    tog_160.work(),
    scope_20_stats.work(),
    scope_40_stats.work(),
    scope_80_stats.work(),
    scope_160_stats.work(),
    shared_80.statements,
    shared_80.references,
    shared_80.watchers,
    shared_80.toggles,
  );
  assert_linear("shared 20->40", shared_20.work(), shared_40.work());
  assert_linear("shared 40->80", shared_40.work(), shared_80.work());
  assert_linear("shared 80->160", shared_80.work(), shared_160.work());
  assert_linear("stop 20->40", stop_stats_20.work(), stop_stats_40.work());
  assert_linear("stop 40->80", stop_stats_40.work(), stop_stats_80.work());
  assert_linear("stop 80->160", stop_stats_80.work(), stop_stats_160.work());
  assert_linear("returned 20->40", ret_20.work(), ret_40.work());
  assert_linear("returned 40->80", ret_40.work(), ret_80.work());
  assert_linear("returned 80->160", ret_80.work(), ret_160.work());
  assert_linear("toggles 20->40", tog_20.work(), tog_40.work());
  assert_linear("toggles 40->80", tog_40.work(), tog_80.work());
  assert_linear("toggles 80->160", tog_80.work(), tog_160.work());
  assert_linear("retained 20->40", scope_20_stats.work(), scope_40_stats.work());
  assert_linear("retained 40->80", scope_40_stats.work(), scope_80_stats.work());
  assert_linear("retained 80->160", scope_80_stats.work(), scope_160_stats.work());
  eprintln!(
    "work repeat20={} repeat40={} repeat80={} repeat160={} current20={} current40={} current80={} current160={} computed20={} computed40={} computed80={} computed160={} edges20={} edges40={} edges80={} edges160={}",
    repeat_20_stats.work(),
    repeat_40_stats.work(),
    repeat_80_stats.work(),
    repeat_160_stats.work(),
    cur_20_stats.work(),
    cur_40_stats.work(),
    cur_80_stats.work(),
    cur_160_stats.work(),
    comp_20_stats.work(),
    comp_40_stats.work(),
    comp_80_stats.work(),
    comp_160_stats.work(),
    comp_20_stats.computed_edges,
    comp_40_stats.computed_edges,
    comp_80_stats.computed_edges,
    comp_160_stats.computed_edges,
  );
  assert_linear("repeat-stop 20->40", repeat_20_stats.work(), repeat_40_stats.work());
  assert_linear("repeat-stop 40->80", repeat_40_stats.work(), repeat_80_stats.work());
  assert_linear("repeat-stop 80->160", repeat_80_stats.work(), repeat_160_stats.work());
  assert_linear("current-scope 20->40", cur_20_stats.work(), cur_40_stats.work());
  assert_linear("current-scope 40->80", cur_40_stats.work(), cur_80_stats.work());
  assert_linear("current-scope 80->160", cur_80_stats.work(), cur_160_stats.work());
  assert_linear("computed 20->40", comp_20_stats.work(), comp_40_stats.work());
  assert_linear("computed 40->80", comp_40_stats.work(), comp_80_stats.work());
  assert_linear("computed 80->160", comp_80_stats.work(), comp_160_stats.work());
  assert!(
    quiet.work() < shared_80.work().saturating_mul(3).saturating_add(OVERHEAD),
    "quiet shared getters must not explode work: active80={} quiet80={}",
    shared_80.work(),
    quiet.work()
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
  let instantiated_tagged = analyze(
    "import { ref, toRef } from 'vue';\
     const tagged = ref({ count: 1 });\
     tagged.clear = function () { delete this.__v_isRef };\
     (tagged.clear<number>)``;\
     toRef(tagged, 'count');",
    "ts",
  );
  assert!(
    instantiated_tagged.source_contracts.toref_ignored_key.is_empty(),
    "TS instantiation tagged-template receiver must abstain; {:?}",
    instantiated_tagged.source_contracts
  );
  let instantiated_call = analyze(
    "import { ref, toRef } from 'vue';\
     const tagged = ref({ count: 1 });\
     tagged.clear = function () { delete this.__v_isRef };\
     tagged.clear<number>();\
     toRef(tagged, 'count');",
    "ts",
  );
  assert!(
    instantiated_call.source_contracts.toref_ignored_key.is_empty(),
    "TS instantiation call receiver must abstain; {:?}",
    instantiated_call.source_contracts
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
  let mixed = analyze(
    "import { ref, toRef, watch } from 'vue';\
     const tagged = ref({ count: 1 });\
     tagged.clear = function () { delete this.__v_isRef };\
     (tagged.clear<number>)``;\
     toRef(tagged, 'count');\
     const n = ref(0);\
     watch(n.value, () => {});",
    "ts",
  );
  assert!(
    mixed.source_contracts.toref_ignored_key.is_empty(),
    "instantiated tagged receiver must quiet toRef identity; {:?}",
    mixed.source_contracts
  );
  assert_eq!(
    mixed.source_contracts.watch_unwrapped_source.len(),
    1,
    "direct .value source5 must stay positive beside instantiated receiver; {:?}",
    mixed.source_contracts
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
fn source_contracts_watch_api_option_and_signature_sites() {
  let equals = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n, (v) => v, { equals: (a, b) => a === b });",
    "ts",
  );
  assert_eq!(
    equals.source_contracts.watch_ignored_option.len(),
    1,
    "{:?}",
    equals.source_contracts
  );
  let method = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n, (v) => v, { equals(a, b) { return a === b; } });",
    "ts",
  );
  assert_eq!(
    method.source_contracts.watch_ignored_option.len(),
    1,
    "{:?}",
    method.source_contracts
  );
  let effect = analyze(
    "import { ref, watchEffect } from 'vue'; const n = ref(0); watchEffect(() => n.value, { once: true, immediate: true });",
    "ts",
  );
  assert_eq!(
    effect.source_contracts.watch_ignored_option.len(),
    2,
    "{:?}",
    effect.source_contracts
  );
  let trailing = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n, (v) => v, { equals: () => true }, 'extra');",
    "ts",
  );
  assert_eq!(
    trailing.source_contracts.watch_ignored_option.len(),
    1,
    "{:?}",
    trailing.source_contracts
  );
  let crlf_src = "import { ref, watch } from 'vue';\r\nconst \u{8ba1}\u{6570} = ref(0);\r\nwatch(\u{8ba1}\u{6570}, (v) => v, { equals: () => true });\r\n";
  let crlf = analyze(crlf_src, "ts");
  assert_eq!(crlf.source_contracts.watch_ignored_option.len(), 1, "{:?}", crlf.source_contracts);
  let ignored = crlf.source_contracts.watch_ignored_option.first();
  assert!(ignored.is_some_and(|site| site.span.line == 3 && site.span.length == 6), "{ignored:?}");
  if let Some(site) = ignored {
    let end = site.span.offset.saturating_add(site.span.length);
    assert_eq!(crlf_src.get(site.span.offset..end), Some("equals"));
  }
  let handler = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n, { handler() { return n.value; }, equals: () => true });",
    "ts",
  );
  assert_eq!(
    handler.source_contracts.watch_signature_mismatch.len(),
    1,
    "{:?}",
    handler.source_contracts
  );
  assert!(
    handler.source_contracts.watch_ignored_option.is_empty(),
    "signature must win over ignored-option; {:?}",
    handler.source_contracts
  );
  let array_cb = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n, [() => {}, () => {}]);",
    "ts",
  );
  assert!(
    array_cb.source_contracts.watch_signature_mismatch.is_empty(),
    "{:?}",
    array_cb.source_contracts
  );
  let zero = analyze("import { ref, watch } from 'vue'; const n = ref(0); watch(n, 0);", "ts");
  assert!(zero.source_contracts.watch_signature_mismatch.is_empty(), "{:?}", zero.source_contracts);
  for zero_bigint in ["0n", "0x0n", "0o0n", "0b0n", "0x0_0n"] {
    let source =
      format!("import {{ ref, watch }} from 'vue'; const n = ref(0); watch(n, {zero_bigint});");
    let facts = analyze(&source, "ts");
    assert!(
      facts.source_contracts.watch_signature_mismatch.is_empty(),
      "zero bigint {zero_bigint} must stay quiet; {:?}",
      facts.source_contracts
    );
  }
  let one_n = analyze("import { ref, watch } from 'vue'; const n = ref(0); watch(n, 1n);", "ts");
  assert_eq!(
    one_n.source_contracts.watch_signature_mismatch.len(),
    1,
    "{:?}",
    one_n.source_contracts
  );
  let computed_literal = analyze(
    "import { ref, watchEffect } from 'vue'; const n = ref(0); watchEffect(() => n.value, { ['once']: true });",
    "ts",
  );
  assert!(
    computed_literal.source_contracts.watch_ignored_option.is_empty(),
    "computed literal keys must stay quiet; {:?}",
    computed_literal.source_contracts
  );
  let imported_alias = analyze(
    "import { ref, watch as observe } from 'vue'; const n = ref(0); observe(n, { handler() { return n.value; } });",
    "ts",
  );
  assert_eq!(
    imported_alias.source_contracts.watch_signature_mismatch.len(),
    1,
    "{:?}",
    imported_alias.source_contracts
  );
  let shadow = analyze(
    "import { ref } from 'vue'; const n = ref(0); function watch(_a: unknown, _b: unknown) {} watch(n, { handler() {} });",
    "ts",
  );
  assert!(shadow.source_contracts.is_empty(), "{:?}", shadow.source_contracts);
  let two_fns = analyze(
    "import { ref, watchEffect } from 'vue'; const n = ref(0); watchEffect(() => n.value, (x) => x);",
    "ts",
  );
  assert_eq!(
    two_fns.source_contracts.watch_signature_mismatch.len(),
    1,
    "{:?}",
    two_fns.source_contracts
  );
  let named_opts = analyze(
    "import { ref, watchEffect } from 'vue'; const n = ref(0); function opts() {} watchEffect(() => n.value, opts);",
    "ts",
  );
  assert!(named_opts.source_contracts.is_empty(), "{:?}", named_opts.source_contracts);
  let aliased = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); const options = { equals: () => true }; watch(n, (v) => v, options);",
    "ts",
  );
  assert!(
    aliased.source_contracts.watch_ignored_option.is_empty(),
    "{:?}",
    aliased.source_contracts
  );
  let auto = analyze(
    "import { watchEffect } from '#imports'; const n = { value: 0 }; watchEffect(() => n.value, { once: true });",
    "ts",
  );
  assert_eq!(auto.source_contracts.watch_ignored_option.len(), 1, "{:?}", auto.source_contracts);
  let ns_auto =
    analyze("import * as Auto from '#imports'; Auto.watchEffect(() => 1, { once: true });", "ts");
  assert!(ns_auto.source_contracts.is_empty(), "{:?}", ns_auto.source_contracts);
  let type_only = analyze(
    "import { type watchEffect } from 'vue'; const watchEffect = (_a: unknown, _b: unknown) => {}; watchEffect(() => 1, () => 2);",
    "ts",
  );
  assert!(type_only.source_contracts.is_empty(), "{:?}", type_only.source_contracts);
}

#[test]
fn source_contracts_watch_option_sites_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [50_u64, 100, 200] {
    let mut source = String::from("import { ref, watch } from 'vue'; const n = ref(0);");
    for _ in 0..size {
      source.push_str("watch(n, (v) => v, { equals: () => true });");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.watch_ignored_option.len(), usize::try_from(size).unwrap_or(usize::MAX));
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "watch-option work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_watch_option_literal_width_stays_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [50_u64, 100, 200] {
    let mut keys = Vec::new();
    for index in 0..size {
      keys.push(format!("k{index}: 1"));
    }
    keys.push("equals: () => true".into());
    let source = format!(
      "import {{ ref, watch }} from 'vue'; const n = ref(0); watch(n, (v) => v, {{ {} }});",
      keys.join(", ")
    );
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.watch_ignored_option.len(),
      1,
      "one equals key among {size} fillers; {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "watch-option width work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
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

#[test]
fn source_contracts_gated_facts_match_forced_full_for_all_sinks() {
  let cases = [
    "import { reactive, triggerRef } from 'vue'; const obj = reactive({ n: 1 }); triggerRef(obj);",
    "import { toRefs } from 'vue'; toRefs({ a: 1 });",
    "import { reactive } from 'vue'; reactive(0);",
    "import { readonly } from 'vue'; readonly(0);",
    "import { shallowReactive } from 'vue'; shallowReactive(0);",
    "import { shallowReadonly } from 'vue'; shallowReadonly(0);",
    "import { ref, watch } from 'vue'; const n = ref(0); watch((n.value) as number, () => {});",
    "import { reactive, watch } from 'vue'; const obj = reactive({ nested: { x: 1 } }); watch(obj.nested, () => {}); obj.nested = { x: 2 };",
    "import { watchEffect } from 'vue'; watchEffect(() => {}, { once: true });",
    "import { watchPostEffect } from 'vue'; watchPostEffect(() => {}, { immediate: true });",
    "import { watchSyncEffect } from 'vue'; watchSyncEffect(() => {}, { flush: 'post', once: true });",
    "import * as Vue from 'vue'; Vue.watchEffect(() => {}, { deep: true });",
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n, (v) => v, { equals: () => true });",
    "import { ref, watch } from 'vue'; const n = ref(0); function accept(_value: unknown) {} watch(n, (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });",
    "import { reactive, watch } from 'vue'; const state = reactive({ n: 1 }); function accept(_value: unknown) {} watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "import { toRef } from 'vue'; toRef(1, 'k');",
    "import { effectScope } from 'vue'; effectScope(() => {});",
    "import { customRef } from 'vue'; const count = customRef(() => ({ set() {} })); void count.value;",
    "import { effectScope } from 'vue'; const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); void result.count;",
    "import { reactive, toRefs } from 'vue'; void toRefs(reactive({ count: 1 })).missing.value;",
    "import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }));",
    "import { customRef, watch } from 'vue'; const r = customRef((_t, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r, () => {}); r.value = 1;",
  ];
  for source in cases {
    let facts = assert_gated_matches_forced(source, ScriptKind::Setup);
    assert!(!facts.is_empty(), "sink fixture must emit a fact: {source}");
  }
  let custom_ref = assert_gated_matches_forced(
    "import { customRef } from 'vue'; const count = customRef(() => ({ set() {} })); void count.value;",
    ScriptKind::Setup,
  );
  assert_eq!(
    custom_ref.invalid_custom_ref_interface.len(),
    1,
    "lone named customRef missing-getter demand must populate its channel: {custom_ref:?}"
  );
  let stopped_scope = assert_gated_matches_forced(
    "import { effectScope } from 'vue'; const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); void result.count;",
    ScriptKind::Setup,
  );
  assert_eq!(
    stopped_scope.inactive_scope_result.len(),
    1,
    "lone named stopped effectScope result demand must populate its channel: {stopped_scope:?}"
  );
  let missing_key = assert_gated_matches_forced(
    "import { reactive, toRefs } from 'vue'; void toRefs(reactive({ count: 1 })).missing.value;",
    ScriptKind::Setup,
  );
  assert_eq!(
    missing_key.missing_torefs_key.len(),
    1,
    "missing toRefs key demand must populate its channel: {missing_key:?}"
  );
  let extracted = assert_gated_matches_forced(
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
    ScriptKind::Setup,
  );
  assert_eq!(
    extracted.extracted_reactive_collection_method.len(),
    1,
    "positive extracted-method channel must populate: {extracted:?}"
  );
  let native_clone = assert_gated_matches_forced(
    "import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }));",
    ScriptKind::Setup,
  );
  assert_eq!(
    native_clone.uncloneable_proxy_data.len(),
    1,
    "named clone positive must yield a clone fact; {native_clone:?}"
  );
  let trigger = assert_gated_matches_forced(
    "import { reactive, triggerRef } from 'vue'; const obj = reactive({ n: 1 }); triggerRef(obj);",
    ScriptKind::Setup,
  );
  assert_eq!(trigger.trigger_ref_non_ref.first().map(|site| site.api.as_str()), Some("triggerRef"));
  let primitive = assert_gated_matches_forced(
    "import { shallowReadonly } from 'vue'; shallowReadonly(null);",
    ScriptKind::Setup,
  );
  assert_eq!(
    primitive.primitive_reactive_target.first().map(|site| site.api.as_str()),
    Some("shallowReadonly")
  );
}

#[test]
fn source_contracts_identity_table_preserves_named_namespace_and_auto_import() {
  let runtime_sources =
    ["vue", "vue-demi", "@vue/runtime-core", "@vue/runtime-dom", "@vue/reactivity"];
  for source_mod in runtime_sources {
    let named = format!(
      "import {{ triggerRef, ref as makeRef }} from '{source_mod}'; triggerRef(makeRef(1)); triggerRef({{ n: 1 }});"
    );
    let facts = assert_gated_matches_forced(&named, ScriptKind::Setup);
    assert_eq!(facts.trigger_ref_non_ref.len(), 1, "{named}");
    let aliased = format!(
      "import {{ watch as observe, ref }} from '{source_mod}'; const n = ref(0); observe(n.value, () => {{}});"
    );
    let facts = assert_gated_matches_forced(&aliased, ScriptKind::Setup);
    assert_eq!(facts.watch_unwrapped_source.len(), 1, "{aliased}");
    let string_name = format!(
      "import {{ 'watch' as observe, ref }} from '{source_mod}'; const n = ref(0); observe(n.value, () => {{}});"
    );
    let facts = assert_gated_matches_forced(&string_name, ScriptKind::Setup);
    assert_eq!(facts.watch_unwrapped_source.len(), 1, "{string_name}");
    let namespace = format!("import * as Vue from '{source_mod}'; Vue.triggerRef({{ n: 1 }});");
    let facts = assert_gated_matches_forced(&namespace, ScriptKind::Setup);
    assert_eq!(facts.trigger_ref_non_ref.len(), 1, "{namespace}");
  }
  let named_auto = "import { triggerRef } from '#imports'; triggerRef({ n: 1 });";
  let facts = assert_gated_matches_forced(named_auto, ScriptKind::Setup);
  assert_eq!(facts.trigger_ref_non_ref.len(), 1);
  let mixed_type = "import { type ref, triggerRef } from 'vue'; triggerRef({ n: 1 });";
  let facts = assert_gated_matches_forced(mixed_type, ScriptKind::Setup);
  assert_eq!(facts.trigger_ref_non_ref.len(), 1);
}

#[test]
fn source_contracts_macros_need_a_sink_and_setup_kind() {
  let props_setup = "import { triggerRef } from 'vue'; triggerRef(defineProps());";
  let props_facts = assert_gated_matches_forced(props_setup, ScriptKind::Setup);
  assert_eq!(
    props_facts.trigger_ref_non_ref.len(),
    1,
    "defineProps is a proven proxy in setup: {props_facts:?}"
  );
  let model_setup = "import { triggerRef } from 'vue'; triggerRef(defineModel());";
  let model_facts = assert_gated_matches_forced(model_setup, ScriptKind::Setup);
  assert!(
    model_facts.trigger_ref_non_ref.is_empty(),
    "defineModel is a proven ref in setup: {model_facts:?}"
  );
  let props_script = assert_gated_matches_forced(props_setup, ScriptKind::Script);
  assert!(
    props_script.trigger_ref_non_ref.is_empty(),
    "ordinary script does not prove defineProps: {props_script:?}"
  );
  let model_script = assert_gated_matches_forced(model_setup, ScriptKind::Script);
  assert!(
    model_script.trigger_ref_non_ref.is_empty(),
    "ordinary script does not prove defineModel: {model_script:?}"
  );
}

fn custom_ref_source(body: &str) -> String {
  format!(
    "import {{ customRef, reactive, ref, triggerRef, watch, watchEffect, watchPostEffect }} from 'vue'; {body}"
  )
}

#[test]
fn custom_ref_lost_notification_emits_track_and_trigger_reasons() {
  let lost_track = analyze(
    &custom_ref_source(
      "const r = customRef((_track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r, () => {}); r.value = 1;",
    ),
    "ts",
  );
  assert_eq!(
    lost_track.source_contracts.custom_ref_lost_notification.len(),
    1,
    "{:?}",
    lost_track.source_contracts
  );
  assert_eq!(
    lost_track.source_contracts.custom_ref_lost_notification.first().map(|site| site.reason),
    Some(vue_vet_core::CustomRefLostNotificationReason::GetTracking)
  );
  let lost_trigger = analyze(
    &custom_ref_source(
      "const r = customRef((track, _trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    "ts",
  );
  assert_eq!(
    lost_trigger.source_contracts.custom_ref_lost_notification.len(),
    1,
    "{:?}",
    lost_trigger.source_contracts
  );
  assert_eq!(
    lost_trigger.source_contracts.custom_ref_lost_notification.first().map(|site| site.reason),
    Some(vue_vet_core::CustomRefLostNotificationReason::SetNotification)
  );
  let track_in_setter = analyze(
    &custom_ref_source(
      "const r = customRef((track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { track(); value = next; trigger(); } }; }); watch(r, () => {}); r.value = 1;",
    ),
    "ts",
  );
  assert_eq!(
    track_in_setter.source_contracts.custom_ref_lost_notification.len(),
    1,
    "{:?}",
    track_in_setter.source_contracts
  );
  assert_eq!(
    track_in_setter.source_contracts.custom_ref_lost_notification.first().map(|site| site.reason),
    Some(vue_vet_core::CustomRefLostNotificationReason::GetTracking)
  );
  let unused_branch = analyze(
    &custom_ref_source(
      "const r = customRef((_track, trigger) => { let value = 0; const initialized = true ? 0 : (value = 1); return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r, () => {}); r.value = 1;",
    ),
    "ts",
  );
  assert_eq!(
    unused_branch.source_contracts.custom_ref_lost_notification.len(),
    1,
    "unused false branch must not unprove storage; {:?}",
    unused_branch.source_contracts
  );
}

#[test]
fn custom_ref_lost_notification_stays_quiet_for_safe_and_unknown() {
  let cases = [
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r, () => {}); r.value = 1;",
    "const backing = ref(0); const r = customRef(() => ({ get() { return backing.value; }, set(next: number) { backing.value = next; } })); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1; triggerRef(r);",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((_track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; trigger(); } }; }); const stop = watch(r, () => {}); stop(); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; trigger(); } }; }); const handle = watchEffect(() => { void r.value; }); handle.pause(); r.value = 1;",
    "function delegate(track: () => void, trigger: () => void) { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; trigger(); } }; } const r = customRef((track, trigger) => delegate(track, trigger)); watch(r, () => {}); r.value = 1;",
    "const factory = (track: () => void, trigger: () => void) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; } }; }; const r = customRef(factory); watch(r, () => {}); r.value = 1;",
    "const r = customRef(() => ({ get: 1, set: 2 })); watch(r, () => {}); r.value = 1;",
    "const r = customRef((_track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); void r.value; r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchPostEffect(() => { void r.value; }); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(() => { void r.value; }, { flush: 'post' }); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); false && watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}, { immediate: true, once: true }); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(() => { return; void r.value; }); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const hooks = { track, trigger }; return { get() { hooks.track(); return value; }, set(next: number) { value = next; hooks.trigger(); } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { (() => track())(); return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); r.value = 1; watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(_next: number) { value = 0; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { if (value == next) return; value = next; } }; }); watch(r, () => {}); r.value = false as never;",
    "const backing = ref(0); const r = customRef((track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; } }; }); r._get = () => backing.value; r._set = (next: number) => { backing.value = next; }; watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value *= next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { next = 0; value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); value = 0; return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const initialized = (value = 1); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const hooks = { track, trigger }; const delegate = hooks; return { get() { delegate.track(); return value; }, set(next: number) { value = next; delegate.trigger(); } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(async () => { await Promise.resolve(); void r.value; }); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { const result = { value: (value = 0) }; track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { const result = { [next = 0]: true }; value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const initialized = { [value = 1]: true }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const initialized = true ? (value = 1) : (value = 0); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const initialized = true && (value = 1); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const initialized = false || (value = 1); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const initialized = Math.random() ? (value = 1) : (value = 0); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, *set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, async set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(async () => { const resolved = { [await Promise.resolve()]: true }; void r.value; }); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1) } = { supplied: true }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((track, trigger) => { let value = 0; const { [value = 0]: supplied } = (value = 1, {}); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1) } = { supplied: undefined }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1), [value = 0]: unused } = {}; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((track, trigger) => { let value = 0; const { missing: { leaf = (value = 1) } = { leaf: true } } = {}; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1) } = { __proto__: { supplied: true } }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((track, trigger) => { let value = 0; const { toString = (value = 1) } = {}; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((track, trigger) => { let value = 0; const undefined = 7; const initialized = undefined ?? (value = 1); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { const result = { Reset: class { static { value = 0 } } }; track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(() => { { return; } void r.value; }); r.value = 1;",
  ];
  for source in cases {
    let facts = analyze(&custom_ref_source(source), "ts");
    assert!(
      facts.source_contracts.custom_ref_lost_notification.is_empty(),
      "quiet control fired: {source} {:?}",
      facts.source_contracts.custom_ref_lost_notification
    );
  }
}

#[test]
fn custom_ref_lost_notification_many_roots_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { customRef, watch } from 'vue';");
    for index in 0..size {
      source.push_str("const r");
      source.push_str(&index.to_string());
      source.push_str(
        " = customRef((_track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r",
      );
      source.push_str(&index.to_string());
      source.push_str(", () => {}); r");
      source.push_str(&index.to_string());
      source.push_str(".value = 1;");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.custom_ref_lost_notification.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "{contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "many-root customRef work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn custom_ref_lost_notification_shared_root_and_dense_negatives_scale() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { customRef, watch } from 'vue'; const r = customRef((_track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; });",
    );
    for index in 0..size {
      source.push_str("const a");
      source.push_str(&index.to_string());
      source.push_str(" = r;");
    }
    source.push_str("watch(r, () => {}); r.value = 1;");
    for index in 0..size {
      source.push_str("const s");
      source.push_str(&index.to_string());
      source.push_str(
        " = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; trigger(); } }; }); watch(s",
      );
      source.push_str(&index.to_string());
      source.push_str(", () => {}); s");
      source.push_str(&index.to_string());
      source.push_str(".value = 1;");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.custom_ref_lost_notification.len(),
      1,
      "shared-root aliases plus dense standard factories must emit one defect; {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-root/dense-negative work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn custom_ref_lost_notification_repeated_writes_and_foreign_handle_calls_scale() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { customRef, watch } from 'vue'; const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); const handle = watch(r, () => {}); function later() {",
    );
    for _ in 0..size {
      source.push_str("handle();");
    }
    source.push('}');
    for _ in 0..size {
      source.push_str("r.value = 0;");
    }
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.custom_ref_lost_notification.is_empty(),
      "same-value writes plus foreign-block handle calls must stay quiet; {contracts:?}"
    );
    assert!(
      work > size.saturating_mul(4),
      "nested inactivity and write visits must be counted for size {size}: work={work}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "repeated-write/foreign-handle work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn custom_ref_lost_notification_many_roots_with_aliases_scale() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [64_u64, 128, 256] {
    let mut source = String::from("import { customRef, watch } from 'vue';");
    for index in 0..size {
      let n = index.to_string();
      source.push_str("const r");
      source.push_str(&n);
      source.push_str(
        " = customRef((_track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); const a",
      );
      source.push_str(&n);
      source.push_str(" = r");
      source.push_str(&n);
      source.push_str("; const b");
      source.push_str(&n);
      source.push_str(" = a");
      source.push_str(&n);
      source.push_str("; const c");
      source.push_str(&n);
      source.push_str(" = b");
      source.push_str(&n);
      source.push_str("; watch(c");
      source.push_str(&n);
      source.push_str(", () => {}); r");
      source.push_str(&n);
      source.push_str(".value = 1;");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.custom_ref_lost_notification.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "many roots with aliases must retain one finding per root; {contracts:?}"
    );
    assert!(
      work > size.saturating_mul(4),
      "root-member construction and lookups must be counted for size {size}: work={work}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "many-root-with-alias work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn custom_ref_lost_notification_nested_binding_and_branch_scale() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { customRef, watch } from 'vue';");
    for index in 0..size {
      let n = index.to_string();
      source.push_str("const r");
      source.push_str(&n);
      source.push_str(
        " = customRef((_track, trigger) => { let value = 0; const { a: { b: { c = (value = 1) } } } = { a: { b: { c: true } } }; const flag = true ? (true ? 0 : 1) : 1; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r",
      );
      source.push_str(&n);
      source.push_str(", () => {}); r");
      source.push_str(&n);
      source.push_str(".value = 1;");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.custom_ref_lost_notification.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "nested dormant defaults must keep lost-track findings; {contracts:?}"
    );
    assert!(
      work > size.saturating_mul(8),
      "nested binding and branch visits must be counted for size {size}: work={work}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "nested binding/branch work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

fn factory_width_source(size: u64) -> String {
  let mut pattern = String::new();
  let mut object = String::new();
  for index in 0..size {
    if index > 0 {
      pattern.push_str(", ");
      object.push_str(", ");
    }
    pattern.push('p');
    pattern.push_str(&index.to_string());
    pattern.push_str(" = (value = 2)");
    object.push('p');
    object.push_str(&index.to_string());
    object.push_str(": true");
  }
  let mut source = String::from(
    "import { customRef, watch } from 'vue'; const r = customRef((_track, trigger) => { let value = 0; const { ",
  );
  source.push_str(&pattern);
  source.push_str(" } = { ");
  source.push_str(&object);
  source.push_str(" }; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r, () => {}); r.value = 1;");
  source
}

#[test]
fn custom_ref_lost_notification_factory_binding_width_scales_near_linearly() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [64_u64, 128, 256] {
    let (contracts, work) = contract_stats(&factory_width_source(size));
    assert_eq!(
      contracts.custom_ref_lost_notification.len(),
      1,
      "width {size} must retain one lost-track finding; {contracts:?}"
    );
    assert!(
      work > size.saturating_mul(4),
      "width {size} must count summary and lookup visits: work={work}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(25),
        "factory-binding width work grew from {prev_work} to {work} on {prev_size}->{size} (must stay near-linear, <2.5x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn custom_ref_lost_notification_factory_bind_budget_exhausts_quietly() {
  let supported = analyze(&factory_width_source(256), "ts");
  assert_eq!(
    supported.source_contracts.custom_ref_lost_notification.len(),
    1,
    "supported width must keep the lost-track finding; {:?}",
    supported.source_contracts
  );
  let exhausted = analyze(&factory_width_source(800), "ts");
  assert!(
    exhausted.source_contracts.custom_ref_lost_notification.is_empty(),
    "exhausted bind budget must fail closed; {:?}",
    exhausted.source_contracts
  );
}

#[test]
fn custom_ref_lost_notification_closed_grammar_stays_quiet() {
  let cases = [
    (
      "getter object value write",
      "const r = customRef((track, trigger) => { let value = 0; return { get() { const result = { value: (value = 0) }; track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "setter computed-key param write",
      "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { const result = { [next = 0]: true }; value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "factory computed-key storage write",
      "const r = customRef((track, trigger) => { let value = 0; const initialized = { [value = 1]: true }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "factory proven conditional write",
      "const r = customRef((track, trigger) => { let value = 0; const initialized = true ? (value = 1) : (value = 0); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "factory proven logical write",
      "const r = customRef((track, trigger) => { let value = 0; const initialized = true && (value = 1); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "generator setter is dormant",
      "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, *set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "await in computed object key ends the prefix",
      "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(async () => { const resolved = { [await Promise.resolve()]: true }; void r.value; }); r.value = 1;",
    ),
    (
      "factory dormant default",
      "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1) } = { supplied: true }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
    (
      "factory binding key after initializer",
      "const r = customRef((track, trigger) => { let value = 0; const { [value = 0]: supplied } = (value = 1, {}); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
    (
      "shadowed undefined is not language undefined",
      "const r = customRef((track, trigger) => { let value = 0; const undefined = 7; const initialized = undefined ?? (value = 1); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
    (
      "object-valued class static block is unknown",
      "const r = customRef((track, trigger) => { let value = 0; return { get() { const result = { Reset: class { static { value = 0 } } }; track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "nested block return ends the prefix",
      "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(() => { { return; } void r.value; }); r.value = 1;",
    ),
    (
      "factory explicit undefined activates default",
      "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1) } = { supplied: undefined }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "factory interleaved computed key after default",
      "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1), [value = 0]: unused } = {}; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
    (
      "sequence shadowed undefined is not nullish",
      "const r = customRef((track, trigger) => { let value = 0; const undefined = 7; const initialized = (true, undefined) ?? (value = 1); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
    (
      "nested default object supplies inner properties",
      "const r = customRef((track, trigger) => { let value = 0; const { missing: { leaf = (value = 1) } = { leaf: true } } = {}; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
    (
      "object-literal proto setter supplies inherited own-looking keys",
      "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1) } = { __proto__: { supplied: true } }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
    (
      "standard prototype keys are not proven missing",
      "const r = customRef((track, trigger) => { let value = 0; const { toString = (value = 1) } = {}; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
  ];
  for (label, source) in cases {
    let facts = analyze(&custom_ref_source(source), "ts");
    assert!(
      facts.source_contracts.custom_ref_lost_notification.is_empty(),
      "{label} must stay quiet: {source} {:?}",
      facts.source_contracts.custom_ref_lost_notification
    );
  }
}

#[test]
fn custom_ref_lost_notification_pre_await_read_still_emits() {
  let facts = analyze(
    &custom_ref_source(
      "const r = customRef((track, _trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(async () => { void r.value; await Promise.resolve(); }); r.value = 1;",
    ),
    "ts",
  );
  assert_eq!(
    facts.source_contracts.custom_ref_lost_notification.len(),
    1,
    "synchronous prefix read before await remains a subscribed consumer; {:?}",
    facts.source_contracts
  );
  assert_eq!(
    facts.source_contracts.custom_ref_lost_notification.first().map(|site| site.reason),
    Some(vue_vet_core::CustomRefLostNotificationReason::SetNotification)
  );
}

#[test]
fn source_contracts_sink_inventory_is_the_eligibility_table() {
  assert_eq!(contract_sink("triggerRef"), Some(ContractSink::TriggerRef));
  assert_eq!(contract_sink("toRefs"), Some(ContractSink::ToRefs));
  for api in ["reactive", "readonly", "shallowReactive", "shallowReadonly"] {
    assert_eq!(contract_sink(api), Some(ContractSink::ProxyConstructor), "{api}");
  }
  assert_eq!(contract_sink("watch"), Some(ContractSink::Watch));
  for api in ["watchEffect", "watchPostEffect", "watchSyncEffect"] {
    assert_eq!(contract_sink(api), Some(ContractSink::WatchEffectFamily), "{api}");
  }
  assert_eq!(contract_sink("toRef"), Some(ContractSink::ToRef));
  assert_eq!(contract_sink("effectScope"), Some(ContractSink::EffectScope));
  assert_eq!(contract_sink("customRef"), Some(ContractSink::CustomRef));
  assert_eq!(contract_sink("computed"), Some(ContractSink::Computed));
  assert_eq!(contract_sink("syncRef"), Some(ContractSink::SyncRef));
  assert_eq!(contract_sink("computedAsync"), Some(ContractSink::ComputedAsync));
  assert_eq!(contract_sink("onMounted"), Some(ContractSink::OnMounted));
  for api in ["ref", "shallowRef", "toRaw", "nextTick"] {
    assert_eq!(contract_sink(api), None, "{api} must not gate collection");
  }
}

#[test]
fn source_contracts_effect_family_named_imports_alone_match_forced_full() {
  let watch_effect = assert_gated_matches_forced(
    "import { watchEffect } from 'vue'; watchEffect(() => {}, { once: true });",
    ScriptKind::Setup,
  );
  assert_eq!(watch_effect.watch_ignored_option.len(), 1, "{watch_effect:?}");
  let post = assert_gated_matches_forced(
    "import { watchPostEffect } from 'vue'; watchPostEffect(() => {}, { immediate: true });",
    ScriptKind::Setup,
  );
  assert_eq!(post.watch_ignored_option.len(), 1, "{post:?}");
  let sync = assert_gated_matches_forced(
    "import { watchSyncEffect } from 'vue'; watchSyncEffect(() => {}, { flush: 'post', once: true });",
    ScriptKind::Setup,
  );
  assert_eq!(
    sync.watch_ignored_option.len(),
    1,
    "named watchSyncEffect with flush:'post' must still report ignored once; {sync:?}"
  );
  assert_eq!(
    sync.watch_ignored_option.first().map(|site| site.api.as_str()),
    Some("watchSyncEffect")
  );
  let namespace = assert_gated_matches_forced(
    "import * as Vue from 'vue'; Vue.watchSyncEffect(() => {}, { once: true });",
    ScriptKind::Setup,
  );
  assert_eq!(namespace.watch_ignored_option.len(), 1, "{namespace:?}");
  let ordinary = assert_gated_matches_forced(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n, (v) => v, { equals: () => true });",
    ScriptKind::Setup,
  );
  assert_eq!(ordinary.watch_ignored_option.len(), 1, "{ordinary:?}");
  assert!(ordinary.watch_unwrapped_source.is_empty(), "{ordinary:?}");
}

#[test]
fn source_contracts_callback_named_watch_import_alone_match_forced_full() {
  let once = assert_gated_matches_forced(
    "import { ref, watch } from 'vue';     const n = ref(0);     function accept(_value: unknown) {}     watch(n, (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });",
    ScriptKind::Setup,
  );
  assert_eq!(
    once.watch_callback_contracts.len(),
    1,
    "named watch + ref (non-sink) must report once-immediate; {once:?}"
  );
  assert_eq!(
    once.watch_callback_contracts.first().map(|site| site.reason),
    Some(vue_vet_core::WatchCallbackContractReason::OnceImmediateUndefinedGuard)
  );
  let identity = assert_gated_matches_forced(
    "import { reactive, watch } from 'vue';     const state = reactive({ n: 1 });     function accept(_value: unknown) {}     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    ScriptKind::Setup,
  );
  assert_eq!(
    identity.watch_callback_contracts.len(),
    1,
    "named watch + reactive must report root identity; {identity:?}"
  );
  assert_eq!(
    identity.watch_callback_contracts.first().map(|site| site.reason),
    Some(vue_vet_core::WatchCallbackContractReason::ReactiveRootIdentityGuard)
  );
}

#[test]
fn source_contracts_normalization_named_imports_alone_match_forced_full() {
  let toref =
    assert_gated_matches_forced("import { toRef } from 'vue'; toRef(1, 'k');", ScriptKind::Setup);
  assert_eq!(
    toref.toref_ignored_key.len(),
    1,
    "named toRef alone must report ignored-key; {toref:?}"
  );
  assert_eq!(
    toref.toref_ignored_key.first().map(|site| site.reason),
    Some(ToRefIgnoredKeyReason::Primitive)
  );
  let scope = assert_gated_matches_forced(
    "import { effectScope } from 'vue'; effectScope(() => {});",
    ScriptKind::Setup,
  );
  assert_eq!(
    scope.effect_scope_callback.len(),
    1,
    "named effectScope alone must report callback argument; {scope:?}"
  );
}

#[test]
fn source_contracts_bypass_without_fact_producing_sinks() {
  assert_bypass("", ScriptKind::Setup);
  assert_bypass("const n = 1;", ScriptKind::Script);
  assert_bypass(
    "import { ref, toRaw } from 'vue'; const n = ref(0); const d = toRaw(n);",
    ScriptKind::Setup,
  );
  assert_bypass(
    "import { type triggerRef } from 'vue'; const triggerRef = (_value: unknown) => {}; triggerRef({ n: 1 });",
    ScriptKind::Setup,
  );
  assert_bypass("import Vue from 'vue'; Vue.triggerRef({ n: 1 });", ScriptKind::Setup);
  assert_bypass("import * as Auto from '#imports'; Auto.triggerRef({ n: 1 });", ScriptKind::Setup);
  assert_bypass(
    "import { triggerRef } from '@vue/toolkit'; triggerRef({ n: 1 });",
    ScriptKind::Setup,
  );
  assert_bypass("triggerRef(1); reactive(0); watch(1, () => {});", ScriptKind::Setup);
  assert_bypass("defineProps<{ title: string }>();", ScriptKind::Setup);
  assert_bypass(
    "import { ref } from 'vue'; const o = { a: { x: 1 }, b: [1] }; const r = ref(o); const alias = r; alias.value = { a: { x: 2 } };",
    ScriptKind::Setup,
  );
}

#[test]
fn source_contracts_once_immediate_and_root_identity_predicates() {
  let once = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function accept(_value: unknown) {}\
     watch(n, (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert_eq!(
    once.source_contracts.watch_callback_contracts.len(),
    1,
    "{:?}",
    once.source_contracts
  );
  assert_eq!(
    once.source_contracts.watch_callback_contracts.first().map(|site| site.reason),
    Some(vue_vet_core::WatchCallbackContractReason::OnceImmediateUndefinedGuard)
  );
  let ordered = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function accept(_value: unknown) {}\
     watch(n, (next, old) => { if (old === void 0) return; accept(next); }, { immediate: true, once: true });",
    "ts",
  );
  assert_eq!(ordered.source_contracts.watch_callback_contracts.len(), 1);
  let reactive_array = analyze(
    "import { reactive, watch } from 'vue';\
     const list = reactive([1]);\
     function accept(_value: unknown) {}\
     watch(list, (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert_eq!(reactive_array.source_contracts.watch_callback_contracts.len(), 1);
  let tuple = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function accept(_value: unknown) {}\
     watch([n], (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert!(
    tuple.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    tuple.source_contracts
  );
  let identity = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ n: 1 });\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert_eq!(
    identity.source_contracts.watch_callback_contracts.len(),
    1,
    "{:?}",
    identity.source_contracts
  );
  assert_eq!(
    identity.source_contracts.watch_callback_contracts.first().map(|site| site.reason),
    Some(vue_vet_core::WatchCallbackContractReason::ReactiveRootIdentityGuard)
  );
  let getter = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ n: 1 });\
     function accept(_value: unknown) {}\
     watch(() => state.n, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(getter.source_contracts.watch_callback_contracts.is_empty());
  let deep_ref = analyze(
    "import { ref, watch } from 'vue';\
     const source = ref({ n: 1 });\
     function accept(_value: unknown) {}\
     watch(source, (next, old) => { if (next === old) return; accept(next); }, { deep: true });",
    "ts",
  );
  assert!(deep_ref.source_contracts.watch_callback_contracts.is_empty());
  let local_undefined = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     const undefined = 1;\
     function accept(_value: unknown) {}\
     watch(n, (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert!(local_undefined.source_contracts.watch_callback_contracts.is_empty());
  let void_effect = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function accept(_value: unknown) {}\
     watch(n, (next, old) => { if (old === void accept(next)) return; accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert!(
    void_effect.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    void_effect.source_contracts
  );
  let after_return = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function accept(_value: unknown) {}\
     watch(n, (next, old) => { if (old === undefined) return; return; accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert!(
    after_return.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    after_return.source_contracts
  );
  let dead_branch = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function accept(_value: unknown) {}\
     watch(n, (next, old) => { if (old === undefined) return; if (false) accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert!(
    dead_branch.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    dead_branch.source_contracts
  );
  let tagged = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ __v_isRef: true, value: 0 });\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    tagged.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    tagged.source_contracts
  );
  let proxy_ref = analyze(
    "import { reactive, ref, watch } from 'vue';\
     const source = reactive(ref(0));\
     function accept(_value: unknown) {}\
     watch(source, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    proxy_ref.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    proxy_ref.source_contracts
  );
  let inherited = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ n: 1 });\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); }, { __proto__: { immediate: true } });",
    "ts",
  );
  assert_eq!(
    inherited.source_contracts.watch_callback_contracts.len(),
    1,
    "{:?}",
    inherited.source_contracts
  );
  let unicode = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function 接收(_value: unknown) {}\
     watch(n, (新, 旧) => { if (旧 === undefined) return; 接收(新); }, { once: true, immediate: true });",
    "ts",
  );
  assert_eq!(unicode.source_contracts.watch_callback_contracts.len(), 1);
  let crlf = analyze(
    "import { ref, watch } from 'vue';\r\nconst n = ref(0);\r\nfunction accept(_value: unknown) {}\r\nwatch(n, (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });\r\n",
    "ts",
  );
  #[expect(clippy::panic, reason = "missing CRLF span evidence must fail the regression")]
  let Some(site) = crlf.source_contracts.watch_callback_contracts.first() else {
    panic!("CRLF once-immediate fact missing");
  };
  assert!(site.guard_span.offset > 0 && site.watch_span.length > 0);
}

#[test]
fn source_contracts_second_review_false_positives_stay_quiet() {
  let nested_block = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function accept(_value: unknown) {}\
     watch(n, (next, old) => { if (old === undefined) return; { return } accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert!(
    nested_block.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    nested_block.source_contracts
  );
  let later_marker = analyze(
    "import { reactive, watch } from 'vue';\
     const taggedLater = reactive({ value: 0 });\
     taggedLater.__v_isRef = true;\
     function accept(_value: unknown) {}\
     watch(taggedLater, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    later_marker.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    later_marker.source_contracts
  );
  let frozen = analyze(
    "import { reactive, watch } from 'vue';\
     const frozenTarget = { n: 0 };\
     Object.freeze(frozenTarget);\
     const frozenState = reactive(frozenTarget);\
     function accept(_value: unknown) {}\
     watch(frozenState, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    frozen.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    frozen.source_contracts
  );
  let readonly_marker = analyze(
    "import { reactive, watch } from 'vue';\
     const readonlyMarker = reactive({ __v_isReadonly: true, n: 0 });\
     function accept(_value: unknown) {}\
     watch(readonlyMarker, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    readonly_marker.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    readonly_marker.source_contracts
  );
  let raw_marker = analyze(
    "import { reactive, watch } from 'vue';\
     const rawMarker = reactive({ __v_raw: { n: 0 }, n: 0 });\
     function accept(_value: unknown) {}\
     watch(rawMarker, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    raw_marker.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    raw_marker.source_contracts
  );
  let field_mutate = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ n: 1 });\
     state.n = 2;\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert_eq!(
    field_mutate.source_contracts.watch_callback_contracts.len(),
    1,
    "{:?}",
    field_mutate.source_contracts
  );
  let pattern_field = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ n: 1 });\
     ({ n: state.n } = { n: 2 });\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert_eq!(
    pattern_field.source_contracts.watch_callback_contracts.len(),
    1,
    "{:?}",
    pattern_field.source_contracts
  );
}

#[test]
fn source_contracts_third_review_capability_roles_stay_quiet() {
  let pattern_marker = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0 });\
     ({ x: tagged.__v_isRef } = { x: true });\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    pattern_marker.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    pattern_marker.source_contracts
  );
  let nested_default_rest = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0 });\
     ({ nested: { x: tagged.__v_isRef = true } = {}, ..._rest } = { nested: {} });\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    nested_default_rest.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    nested_default_rest.source_contracts
  );
  let ts_wrapper = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0 });\
     ({ x: (tagged.__v_isRef as boolean) } = { x: true });\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    ts_wrapper.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    ts_wrapper.source_contracts
  );
  let spread_freeze = analyze(
    "import { reactive, watch } from 'vue';\
     const frozen = { n: 0 };\
     Object.freeze(...[frozen]);\
     const state = reactive(frozen);\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    spread_freeze.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    spread_freeze.source_contracts
  );
  let indexed_freeze = analyze(
    "import { reactive, watch } from 'vue';\
     const published = { n: 0 };\
     const container = [published];\
     Object.freeze(container[0]);\
     const publishedState = reactive(published);\
     function accept(_value: unknown) {}\
     watch(publishedState, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    indexed_freeze.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    indexed_freeze.source_contracts
  );
  let sequence_freeze = analyze(
    "import { reactive, watch } from 'vue';\
     const frozen = { n: 0 };\
     Object.freeze((0, frozen));\
     const state = reactive(frozen);\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    sequence_freeze.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    sequence_freeze.source_contracts
  );
}

#[test]
fn source_contracts_fourth_review_pattern_values_and_receiver() {
  let same_object_pattern = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ child: { n: 0 } });\
     watch(state.child, () => {});\
     ({ child: state.child } = { child: state.child });",
    "ts",
  );
  assert!(
    same_object_pattern.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    same_object_pattern.source_contracts
  );
  let same_array_pattern = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ child: { n: 0 } });\
     watch(state.child, () => {});\
     [state.child] = [state.child];",
    "ts",
  );
  assert!(
    same_array_pattern.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    same_array_pattern.source_contracts
  );
  let direct_replace = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ child: { n: 0 } });\
     watch(state.child, () => {});\
     state.child = { n: 1 };",
    "ts",
  );
  assert_eq!(
    direct_replace.source_contracts.watch_replaced_object_source.len(),
    1,
    "{:?}",
    direct_replace.source_contracts
  );
  let receiver = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0, tag: function () { this.__v_isRef = true } });\
     tagged.tag();\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    receiver.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    receiver.source_contracts
  );
  let tagged_template = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0, tag: function () { this.__v_isRef = true } });\
     tagged.tag``;\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    tagged_template.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    tagged_template.source_contracts
  );
  let tagged_template_wrapper = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0, tag: function () { this.__v_isRef = true } });\
     ((tagged.tag as () => void)!)``;\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    tagged_template_wrapper.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    tagged_template_wrapper.source_contracts
  );
  let tagged_template_args = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0, tag: function () { this.__v_isRef = true } });\
     tagged.tag<string>``;\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    tagged_template_args.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    tagged_template_args.source_contracts
  );
  let instantiated_tagged = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0, tag: function () { this.__v_isRef = true } });\
     (tagged.tag<number>)``;\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    instantiated_tagged.source_contracts.watch_callback_contracts.is_empty(),
    "TS instantiation tagged-template receiver must abstain; {:?}",
    instantiated_tagged.source_contracts
  );
  let instantiated_call = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0, tag: function () { this.__v_isRef = true } });\
     tagged.tag<number>();\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    instantiated_call.source_contracts.watch_callback_contracts.is_empty(),
    "TS instantiation call receiver must abstain; {:?}",
    instantiated_call.source_contracts
  );
  let receiver_freeze = analyze(
    "import { reactive, watch } from 'vue';\
     const raw = { n: 0, lock: function () { Object.freeze(this) } };\
     raw.lock();\
     const state = reactive(raw);\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    receiver_freeze.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    receiver_freeze.source_contracts
  );
  let field_mutate = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ n: 1 });\
     state.n = 2;\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert_eq!(
    field_mutate.source_contracts.watch_callback_contracts.len(),
    1,
    "{:?}",
    field_mutate.source_contracts
  );
}

#[test]
fn source_parent_identifier_watch_use_drops_unwrapped_payload_proof() {
  let alone =
    analyze("import { ref, watch } from 'vue'; const n = ref(0); watch(n.value, () => {});", "ts");
  assert_eq!(
    alone.source_contracts.watch_unwrapped_source.len(),
    1,
    "parent unwrapped proof for watch(n.value) must stay; {:?}",
    alone.source_contracts
  );
  let combined = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n.value, () => {}); watch(n, () => {});",
    "ts",
  );
  assert!(
    combined.source_contracts.watch_unwrapped_source.is_empty(),
    "source-parent escape/uncertain indexing: a later identifier watch(n) currently drops watch(n.value); {:?}",
    combined.source_contracts
  );
  assert!(
    combined.source_contracts.watch_callback_contracts.is_empty(),
    "empty callback must not emit callback-contract facts; {:?}",
    combined.source_contracts
  );
}

#[test]
#[expect(clippy::panic, reason = "shared-literal fixture construction must fail the test")]
fn source_contracts_shared_wide_literal_watchers_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [32_u64, 64, 128] {
    let mut fields = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    for index in 0..size {
      fields.push(format!("f{index}: {index}"));
    }
    let mut source = format!(
      "import {{ reactive, watch }} from 'vue'; function accept(_value: unknown) {{}} const raw = {{ {} }};",
      fields.join(", ")
    );
    for index in 0..size {
      write!(
        source,
        "const s{index} = reactive(raw); watch(s{index}, (next, old) => {{ if (next === old) return; accept(next); }});"
      )
      .unwrap_or_else(|error| panic!("shared-literal fixture write: {error}"));
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.watch_callback_contracts.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "shared-literal identity facts at width {size}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-literal watcher work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_watch_callback_sites_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [50_u64, 100, 200] {
    let mut source = String::from(
      "import { ref, watch } from 'vue'; const n = ref(0); function accept(_value: unknown) {}",
    );
    for _ in 0..size {
      source.push_str(
        "watch(n, (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });",
      );
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.watch_callback_contracts.len(),
      usize::try_from(size).unwrap_or(usize::MAX)
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "callback-site work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
#[expect(clippy::panic, reason = "missing span evidence must fail the regression")]
fn source_contracts_structured_clone_reports_proven_proxy() {
  let direct =
    analyze("import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }));", "ts");
  assert_eq!(
    direct.source_contracts.uncloneable_proxy_data.len(),
    1,
    "{:?}",
    direct.source_contracts
  );
  let alias = analyze(
    "import { reactive } from 'vue'; const state = reactive({ count: 1 }); const alias = state; structuredClone(alias);",
    "ts",
  );
  assert_eq!(
    alias.source_contracts.uncloneable_proxy_data.len(),
    1,
    "{:?}",
    alias.source_contracts
  );
  let mutated = analyze(
    "import { reactive } from 'vue'; const state = reactive({ count: 1 }); state.count = 2; structuredClone(state);",
    "ts",
  );
  assert_eq!(
    mutated.source_contracts.uncloneable_proxy_data.len(),
    1,
    "{:?}",
    mutated.source_contracts
  );
  let asserted = analyze(
    "import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }) as { count: number });",
    "ts",
  );
  assert_eq!(
    asserted.source_contracts.uncloneable_proxy_data.len(),
    1,
    "{:?}",
    asserted.source_contracts
  );
  let Some(site) = asserted.source_contracts.uncloneable_proxy_data.first() else {
    panic!("assertion span missing");
  };
  let source =
    "import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }) as { count: number });";
  let Some(arg) = source.find("reactive({ count: 1 }) as { count: number }") else {
    panic!("assertion argument missing");
  };
  assert_eq!(site.span.offset, arg);
  assert_eq!(site.span.length, "reactive({ count: 1 }) as { count: number }".len());
}

#[test]
fn source_contracts_structured_clone_named_imports_alone_match_forced_full() {
  let facts = assert_gated_matches_forced(
    "import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }));",
    ScriptKind::Setup,
  );
  assert_eq!(
    facts.uncloneable_proxy_data.len(),
    1,
    "named clone positive must yield a clone fact; {facts:?}"
  );
}

#[test]
fn source_contracts_structured_clone_stays_quiet_for_controls() {
  let native_only = analyze("structuredClone({ count: 1 });", "ts");
  assert!(
    native_only.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    native_only.source_contracts
  );
  let raw = analyze(
    "import { markRaw, reactive } from 'vue'; const raw = { count: 1 }; markRaw(raw); const value = reactive(raw); structuredClone(value);",
    "ts",
  );
  assert!(raw.source_contracts.uncloneable_proxy_data.is_empty(), "{:?}", raw.source_contracts);
  let to_raw = analyze(
    "import { reactive, toRaw } from 'vue'; structuredClone(toRaw(reactive({ count: 1 })));",
    "ts",
  );
  assert!(
    to_raw.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    to_raw.source_contracts
  );
  let marker = analyze(
    "import { reactive } from 'vue'; structuredClone(reactive({ __v_skip: true, count: 1 }));",
    "ts",
  );
  assert!(
    marker.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    marker.source_contracts
  );
  let shadow = analyze(
    "import { reactive } from 'vue'; function structuredClone(_value: unknown) {} structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    shadow.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    shadow.source_contracts
  );
  let two =
    analyze("import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }), {});", "ts");
  assert!(two.source_contracts.uncloneable_proxy_data.is_empty(), "{:?}", two.source_contracts);
  let nested = analyze(
    "import { reactive } from 'vue'; const state = reactive({ count: 1 }); structuredClone({ state });",
    "ts",
  );
  assert!(
    nested.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    nested.source_contracts
  );
}

#[test]
#[expect(clippy::panic, reason = "missing span evidence must fail the regression")]
fn source_contracts_structured_clone_unicode_and_crlf_span_the_data_argument() {
  let unicode = "import { reactive } from 'vue'; structuredClone(reactive({ 计数: 1 }));";
  let facts = analyze(unicode, "ts");
  let needle = "reactive({ 计数: 1 })";
  let Some(arg) = unicode.find(needle) else {
    panic!("unicode argument missing");
  };
  let Some(site) = facts.source_contracts.uncloneable_proxy_data.first() else {
    panic!("unicode finding missing: {:?}", facts.source_contracts);
  };
  assert_eq!(site.span.offset, arg);
  assert_eq!(site.span.length, needle.len());

  let crlf = "import { reactive } from 'vue';\r\nstructuredClone(reactive({ count: 1 }));";
  let facts = analyze(crlf, "ts");
  let needle = "reactive({ count: 1 })";
  let Some(arg) = crlf.find(needle) else {
    panic!("crlf argument missing");
  };
  let Some(site) = facts.source_contracts.uncloneable_proxy_data.first() else {
    panic!("crlf finding missing: {:?}", facts.source_contracts);
  };
  assert_eq!(site.span.offset, arg);
  assert_eq!(site.span.length, needle.len());
  assert_eq!(site.span.line, 2);
}

#[test]
fn source_contracts_structured_clone_sites_scale_sublinear_per_doubling() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [32_u64, 64, 128] {
    let mut source = String::from("import { reactive } from 'vue';");
    for index in 0..size {
      source.push_str("const s");
      source.push_str(&index.to_string());
      source.push_str(" = reactive({ n: 1 }); structuredClone(s");
      source.push_str(&index.to_string());
      source.push_str(");");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.uncloneable_proxy_data.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "size {size} findings; {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "structured-clone work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_structured_clone_poisons_equivalent_native_writes() {
  let computed = analyze(
    "import { reactive } from 'vue'; globalThis['structuredClone'] = ((value: unknown) => value) as typeof structuredClone; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    computed.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    computed.source_contracts
  );
  let pattern_member = analyze(
    "import { reactive } from 'vue'; ({ clone: globalThis.structuredClone } = { clone: ((value: unknown) => value) as typeof structuredClone }); structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    pattern_member.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    pattern_member.source_contracts
  );
  let pattern_global = analyze(
    "import { reactive } from 'vue'; [structuredClone] = [((value: unknown) => value) as typeof structuredClone]; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    pattern_global.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    pattern_global.source_contracts
  );
  let asserted = analyze(
    "import { reactive } from 'vue'; (globalThis.structuredClone as typeof structuredClone) = ((value: unknown) => value) as typeof structuredClone; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    asserted.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    asserted.source_contracts
  );
  let deleted = analyze(
    "import { reactive } from 'vue'; delete globalThis.structuredClone; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    deleted.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    deleted.source_contracts
  );
  let rest = analyze(
    "import { reactive } from 'vue'; ({ ...structuredClone } = { structuredClone: ((value: unknown) => value) as typeof structuredClone }); structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(rest.source_contracts.uncloneable_proxy_data.is_empty(), "{:?}", rest.source_contracts);
  let defaulted = analyze(
    "import { reactive } from 'vue'; ({ structuredClone = ((value: unknown) => value) as typeof structuredClone } = {}); structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    defaulted.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    defaulted.source_contracts
  );
  let updated = analyze(
    "import { reactive } from 'vue'; structuredClone++; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    updated.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    updated.source_contracts
  );
  let dormant = analyze(
    "import { reactive } from 'vue'; function replace() { structuredClone = ((value: unknown) => value) as typeof structuredClone; } structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    dormant.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    dormant.source_contracts
  );
  let dynamic_ident = analyze(
    "import { reactive } from 'vue'; const key = 'structuredClone'; globalThis[key] = ((value: unknown) => value) as typeof structuredClone; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    dynamic_ident.source_contracts.uncloneable_proxy_data.is_empty(),
    "unresolved computed globalThis write must poison; {:?}",
    dynamic_ident.source_contracts
  );
}

#[test]
fn source_contracts_structured_clone_poisons_loop_assignment_targets() {
  const PREFIX: &str = "import { reactive } from 'vue'; ";
  const SUFFIX: &str = " structuredClone(reactive({ count: 1 }));";
  let identity = "((value: unknown) => value) as typeof structuredClone";
  let cases = [
    (
      "for-of dynamic",
      format!("const key = 'structuredClone'; for (globalThis[key] of [{identity}]) {{}}"),
    ),
    ("for-of static", format!("for (globalThis.structuredClone of [{identity}]) {{}}")),
    ("for-of ident", format!("for (structuredClone of [{identity}]) {{}}")),
    (
      "for-of member pattern",
      format!("for ({{ clone: globalThis.structuredClone }} of [{{ clone: {identity} }}]) {{}}"),
    ),
    ("for-of default", format!("for ({{ structuredClone = {identity} }} of [{{}}]) {{}}")),
    (
      "for-of rest",
      format!("for ({{ ...structuredClone }} of [{{ structuredClone: {identity} }}]) {{}}"),
    ),
    ("for-of array", format!("for ([structuredClone] of [[{identity}]]) {{}}")),
    (
      "for-of ts",
      format!("for ((globalThis.structuredClone as typeof structuredClone) of [{identity}]) {{}}"),
    ),
    (
      "for-in dynamic",
      "const key = 'structuredClone'; for (globalThis[key] in { x: 1 }) {}".to_string(),
    ),
    ("for-in static", "for (globalThis.structuredClone in { x: 1 }) {}".to_string()),
    (
      "for-await-of",
      format!(
        "async function replace() {{ for await (globalThis.structuredClone of [{identity}]) {{}} }}"
      ),
    ),
  ];
  for (label, head) in cases {
    let source = format!("{PREFIX}{head}{SUFFIX}");
    let facts = analyze(&source, "ts");
    assert!(
      facts.source_contracts.uncloneable_proxy_data.is_empty(),
      "{label} must poison native identity; {:?}",
      facts.source_contracts
    );
  }

  let unrelated = analyze(
    "import { reactive } from 'vue'; for (globalThis['fetch'] of [((value: unknown) => value) as typeof fetch]) {} structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert_eq!(
    unrelated.source_contracts.uncloneable_proxy_data.len(),
    1,
    "unrelated loop key must not poison; {:?}",
    unrelated.source_contracts
  );
  let shadowed = analyze(
    "import { reactive } from 'vue'; const globalThis = { structuredClone: ((value: unknown) => value) as typeof structuredClone }; for (globalThis.structuredClone of [((value: unknown) => value) as typeof structuredClone]) {} structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert_eq!(
    shadowed.source_contracts.uncloneable_proxy_data.len(),
    1,
    "shadowed globalThis loop write must not poison; {:?}",
    shadowed.source_contracts
  );
  let declared = analyze(
    "import { reactive } from 'vue'; for (const structuredClone of [((value: unknown) => value) as typeof structuredClone]) {} structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert_eq!(
    declared.source_contracts.uncloneable_proxy_data.len(),
    1,
    "declaration loop heads keep binding semantics; {:?}",
    declared.source_contracts
  );
}

#[test]
fn source_contracts_structured_clone_loop_assignment_targets_count_work() {
  let base = "import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }));";
  let (_, base_stats) = contract_full_stats(base);
  let mut loops = String::from("import { reactive } from 'vue';");
  for index in 0..8 {
    loops.push_str("for (globalThis['fetch'] of [0]) {} // ");
    loops.push_str(&index.to_string());
    loops.push('\n');
  }
  loops.push_str("structuredClone(reactive({ count: 1 }));");
  let (facts, stats) = contract_full_stats(&loops);
  assert_eq!(
    facts.uncloneable_proxy_data.len(),
    1,
    "unrelated loop keys must keep the clone positive; {facts:?}"
  );
  assert_eq!(
    stats.writes,
    base_stats.writes.saturating_add(8),
    "each assignment-form loop head charges one write visit; base={base_stats:?} loops={stats:?}"
  );
  assert_eq!(
    stats.import_source_steps, 2,
    "loop poison must not extra-walk import sources; {stats:?}"
  );
  let declared = "import { reactive } from 'vue'; for (const x of [0]) {} structuredClone(reactive({ count: 1 }));";
  let (_, declared_stats) = contract_full_stats(declared);
  assert_eq!(
    declared_stats.writes, base_stats.writes,
    "declaration loop heads must not charge assignment-target writes; {declared_stats:?}"
  );
}

#[test]
fn source_contracts_structured_clone_keeps_unrelated_static_globalthis_key_eligible() {
  let facts = analyze(
    "import { reactive } from 'vue'; globalThis['fetch'] = ((value: unknown) => value) as typeof fetch; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert_eq!(
    facts.source_contracts.uncloneable_proxy_data.len(),
    1,
    "known unrelated static key must not poison; {:?}",
    facts.source_contracts
  );
}

#[test]
fn source_contracts_structured_clone_requires_definite_call_key() {
  let dynamic_call = analyze(
    "import { reactive } from 'vue'; const key = 'structuredClone'; globalThis[key](reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    dynamic_call.source_contracts.uncloneable_proxy_data.is_empty(),
    "computed call is not a definite native intrinsic; {:?}",
    dynamic_call.source_contracts
  );
  let identity_call = analyze(
    "import { reactive } from 'vue'; const key = 'identity'; globalThis[key](reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    identity_call.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    identity_call.source_contracts
  );
}

#[test]
fn source_contracts_structured_clone_keeps_shadowed_global_this_eligible() {
  let facts = analyze(
    "import { reactive } from 'vue'; const globalThis = { structuredClone: ((value: unknown) => value) as typeof structuredClone }; globalThis.structuredClone = ((value: unknown) => value) as typeof structuredClone; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert_eq!(
    facts.source_contracts.uncloneable_proxy_data.len(),
    1,
    "{:?}",
    facts.source_contracts
  );
}

#[test]
fn source_contracts_structured_clone_excludes_optional_native_calls() {
  let facts =
    analyze("import { reactive } from 'vue'; structuredClone?.(reactive({ count: 1 }));", "ts");
  assert!(facts.source_contracts.uncloneable_proxy_data.is_empty(), "{:?}", facts.source_contracts);
}

#[test]
#[expect(clippy::panic, reason = "missing span evidence must fail the regression")]
fn source_contracts_structured_clone_budget_does_not_cache_exhaustion() {
  let deep_first = "import { reactive, readonly } from 'vue'; const state = reactive({ count: 1 }); function deep() { return structuredClone(readonly(readonly(readonly(readonly(readonly(readonly(readonly(state)))))))); } function direct() { return structuredClone(state); }";
  let facts = analyze(deep_first, "ts");
  let Some(direct_at) = deep_first.rfind("structuredClone(state)") else {
    panic!("direct call missing");
  };
  let arg = direct_at + "structuredClone(".len();
  assert!(
    facts.source_contracts.uncloneable_proxy_data.iter().any(|site| site.span.offset == arg),
    "direct state argument must report after a deeper exhausted query; {:?}",
    facts.source_contracts
  );

  let direct_first = "import { reactive, readonly } from 'vue'; const state = reactive({ count: 1 }); function direct() { return structuredClone(state); } function deep() { return structuredClone(readonly(readonly(readonly(readonly(readonly(readonly(readonly(state)))))))); }";
  let facts = analyze(direct_first, "ts");
  let Some(direct_at) = direct_first.find("structuredClone(state)") else {
    panic!("direct call missing");
  };
  let arg = direct_at + "structuredClone(".len();
  assert!(
    facts.source_contracts.uncloneable_proxy_data.iter().any(|site| site.span.offset == arg),
    "direct state argument must report when declared first; {:?}",
    facts.source_contracts
  );
}

#[test]
fn source_contracts_structured_clone_requires_vue3_proxy_origin() {
  let auto =
    analyze("import { reactive } from '#imports'; structuredClone(reactive({ count: 1 }));", "ts");
  assert!(auto.source_contracts.uncloneable_proxy_data.is_empty(), "{:?}", auto.source_contracts);
  let demi =
    analyze("import { reactive } from 'vue-demi'; structuredClone(reactive({ count: 1 }));", "ts");
  assert!(demi.source_contracts.uncloneable_proxy_data.is_empty(), "{:?}", demi.source_contracts);
  let namespace =
    analyze("import * as Vue from 'vue'; structuredClone(Vue.reactive({ count: 1 }));", "ts");
  assert_eq!(
    namespace.source_contracts.uncloneable_proxy_data.len(),
    1,
    "{:?}",
    namespace.source_contracts
  );
  let runtime_core = analyze(
    "import { reactive } from '@vue/runtime-core'; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert_eq!(
    runtime_core.source_contracts.uncloneable_proxy_data.len(),
    1,
    "{:?}",
    runtime_core.source_contracts
  );
  let primitive_auto = analyze("import { reactive } from '#imports'; void reactive(0);", "ts");
  assert_eq!(
    primitive_auto.source_contracts.primitive_reactive_target.len(),
    1,
    "named #imports must still feed the other source-contract rules; {:?}",
    primitive_auto.source_contracts
  );
}

#[test]
fn source_contracts_structured_clone_wide_negative_cache_and_property_width() {
  let mut previous: Option<(u64, u64)> = None;
  for width in [32_u64, 64, 128] {
    let mut source = String::from("import { reactive } from 'vue'; const state = reactive({");
    for index in 0..width {
      source.push_str(" p");
      source.push_str(&index.to_string());
      source.push_str(": 1,");
    }
    source.push_str(" __v_skip: true });");
    for index in 0..width {
      source.push_str(" structuredClone(state); // q");
      source.push_str(&index.to_string());
    }
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.uncloneable_proxy_data.is_empty(),
      "ineligible wide target must stay quiet at width {width}; {contracts:?}"
    );
    if let Some((prev_width, prev_work)) = previous {
      assert_eq!(width, prev_width * 2, "widths must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "wide negative clone work grew from {prev_work} to {work} on {prev_width}->{width} (must stay <3x per doubling)"
      );
    }
    previous = Some((width, work));
  }
}

#[test]
fn source_contracts_import_source_steps_bypass_nested_local_calls() {
  fn nest(depth: u32, with_positive: bool) -> String {
    let mut source = String::from("import { reactive } from 'vue';\n");
    for index in 0..depth {
      source.push_str("function local");
      source.push_str(&index.to_string());
      source.push_str("() {\n");
    }
    source.push_str("void 0;\n");
    for index in (0..depth).rev() {
      source.push_str("}\nlocal");
      source.push_str(&index.to_string());
      source.push_str("();\n");
    }
    if with_positive {
      source.push_str("structuredClone(reactive({ count: 1 }));\n");
    }
    source
  }

  let (quiet, quiet_stats) = contract_full_stats(&nest(64, false));
  assert!(quiet.uncloneable_proxy_data.is_empty(), "{quiet:?}");
  assert_eq!(
    quiet_stats.import_source_steps, 0,
    "nested local calls must not examine import sources; {quiet_stats:?}"
  );

  let mut previous: Option<(u32, u64, u64)> = None;
  for depth in [32_u32, 64, 128] {
    let (contracts, stats) = contract_full_stats(&nest(depth, true));
    assert_eq!(
      contracts.uncloneable_proxy_data.len(),
      1,
      "depth {depth} must keep the constructor/clone positive; {contracts:?}"
    );
    assert_eq!(
      stats.import_source_steps, 2,
      "only the nested Vue constructor (call node + argument record) may examine import source at depth {depth}; {stats:?}"
    );
    if let Some((prev_depth, prev_work, prev_steps)) = previous {
      assert_eq!(depth, prev_depth * 2, "depths must double");
      assert_eq!(stats.import_source_steps, prev_steps);
      assert!(
        stats.work().saturating_mul(10) < prev_work.saturating_mul(30),
        "nested-local clone work grew from {prev_work} to {} on {prev_depth}->{depth}",
        stats.work()
      );
    }
    previous = Some((depth, stats.work(), stats.import_source_steps));
  }
}

#[test]
fn value_contracts_emit_demanded_custom_ref_and_stay_quiet_without_demand() {
  let (missing, _) = contract_stats(
    "import { customRef } from 'vue'; const count = customRef(() => ({ set() {} })); void count.value;",
  );
  assert_eq!(missing.invalid_custom_ref_interface.len(), 1, "{missing:?}");
  let (unused, _) = contract_stats(
    "import { customRef } from 'vue'; const count = customRef(() => ({ set() {} })); void count;",
  );
  assert!(unused.invalid_custom_ref_interface.is_empty(), "{unused:?}");
  let (getter, _) = contract_stats(
    "import { customRef } from 'vue'; const count = customRef(() => ({ get() { return 1 } })); void count.value;",
  );
  assert!(getter.invalid_custom_ref_interface.is_empty(), "{getter:?}");
}

#[test]
fn value_contracts_stopped_scope_result_and_torefs_missing_key() {
  let (scope, _) = contract_stats(
    "import { effectScope } from 'vue'; const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); void result.count;",
  );
  assert_eq!(scope.inactive_scope_result.len(), 1, "{scope:?}");
  let (live, _) = contract_stats(
    "import { effectScope } from 'vue'; const scope = effectScope(); const result = scope.run(() => ({ count: 1 })); void result.count; scope.stop();",
  );
  assert!(live.inactive_scope_result.is_empty(), "{live:?}");
  let (missing, _) = contract_stats(
    "import { reactive, toRefs } from 'vue'; void toRefs(reactive({ count: 1 })).missing.value;",
  );
  assert_eq!(missing.missing_torefs_key.len(), 1, "{missing:?}");
  let (known, _) = contract_stats(
    "import { reactive, toRefs } from 'vue'; void toRefs(reactive({ count: 1 })).count.value;",
  );
  assert!(known.missing_torefs_key.is_empty(), "{known:?}");
}

#[test]
fn value_contracts_unicode_crlf_and_namespace() {
  let source = "import * as Vue from 'vue';\r\nconst 计数 = Vue.customRef(() => ({ set() {} }));\r\nvoid 计数.value;\r\n";
  let (facts, _) = contract_stats(source);
  assert_eq!(facts.invalid_custom_ref_interface.len(), 1, "{facts:?}");
  if let Some(site) = facts.invalid_custom_ref_interface.first() {
    assert!(site.demand_span.line >= 3, "{site:?}");
  }
}

#[test]
fn value_contracts_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source =
      String::from("import { customRef, effectScope, reactive, toRefs } from 'vue';");
    for index in 0..size {
      source.push_str("const r");
      source.push_str(&index.to_string());
      source.push_str(" = customRef(() => ({ set() {}, extra");
      source.push_str(&index.to_string());
      source.push_str(": 1 })); void r");
      source.push_str(&index.to_string());
      source.push_str(".value;");
      source.push_str("const s");
      source.push_str(&index.to_string());
      source.push_str(" = effectScope(); s");
      source.push_str(&index.to_string());
      source.push_str(".stop(); const out");
      source.push_str(&index.to_string());
      source.push_str(" = s");
      source.push_str(&index.to_string());
      source.push_str(".run(() => ({ count: 1 })); void out");
      source.push_str(&index.to_string());
      source.push_str(".count;");
      source.push_str("void toRefs(reactive({ count: 1, k");
      source.push_str(&index.to_string());
      source.push_str(": 1 })).missing.value;");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.invalid_custom_ref_interface.len(), expected, "{contracts:?}");
    assert_eq!(contracts.inactive_scope_result.len(), expected, "{contracts:?}");
    assert_eq!(contracts.missing_torefs_key.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "value-contract work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn value_contracts_review_safe_probes_stay_quiet() {
  for source in [
    "import { customRef } from 'vue'; function make(undefined: () => number) { const count = customRef(() => ({ get: undefined })); void count.value; } make(() => 7);",
    "import { customRef } from 'vue'; let out: unknown; out = customRef(() => ({ get() { return 7 } })).value;",
    "import { customRef } from 'vue'; const count = customRef(() => ({ set() {} })); delete count.value;",
    "import { customRef } from 'vue'; const count = customRef(() => ({ set() {} })); count._get = () => 7; void count.value;",
    "import { effectScope } from 'vue'; const scope = effectScope(); false && scope.stop(); const result = scope.run(() => ({ count: 1 })); void result.count;",
    "import { effectScope } from 'vue'; const scope = effectScope(); scope.stop = () => {}; scope.stop(); const result = scope.run(() => ({ count: 1 })); void result.count;",
    "import { effectScope } from 'vue'; const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); if (result) void result.count;",
    "import { effectScope } from 'vue'; const scope = effectScope(); scope.stop(); false && scope.run(() => ({ count: 1 })).count;",
    "import { reactive, ref, toRefs } from 'vue'; const refs = toRefs(reactive({ count: 1 })); refs.missing = ref(2); void refs.missing.value;",
    "import { reactive, ref, toRefs } from 'vue'; let { missing } = toRefs(reactive({ count: 1 })); missing = ref(2); void missing.value;",
    "import { reactive, toRefs } from 'vue'; void toRefs(reactive({ __proto__: { inherited: 1 }, count: 1 })).inherited.value;",
    "import { reactive, toRefs } from 'vue'; void toRefs(reactive({ count: 1 })).toString.value;",
    "import { reactive, toRefs } from 'vue'; const refs = toRefs(reactive({ count: 1 })); if (refs.missing) void refs.missing.value;",
    "import { customRef } from 'vue'; const count = customRef(() => ({ get() { this._set = (value: number) => { this.saved = value }; return 1 } })); void count.value; count.value = 7;",
    "import { customRef } from 'vue'; let saved = 0; const field = customRef(() => ({ get() { const marker = { [this._set = (value: number) => { saved = value }]: 1 }; return marker; } })); void field.value; field.value = 7;",
    "import { reactive, toRefs } from 'vue'; function install(target: object) { Object.assign(target, { added: 7 }); } const state = reactive({ initial: 1 }); install(state); const fields = toRefs(state); void fields.added.value;",
    "import { effectScope, reactive, toRefs } from 'vue'; function guardedScope() { const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); if (!result) return; void result.count; } function guardedBag() { const bag = toRefs(reactive({ count: 1 })); if (!bag.missing) return; void bag.missing.value; } guardedScope(); guardedBag();",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.invalid_custom_ref_interface.is_empty()
        && facts.inactive_scope_result.is_empty()
        && facts.missing_torefs_key.is_empty(),
      "safe probe must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn source5_logical_watch_stays_quiet() {
  let (facts, _) = contract_stats(
    "import { reactive, watch } from 'vue'; const state = reactive({ child: { count: 1 } }); false && watch(state.child, () => {}); state.child = { count: 2 };",
  );
  assert!(
    facts.watch_replaced_object_source.is_empty(),
    "short-circuit watch must not be a replacement site: {facts:?}"
  );
}

#[test]
fn value_contracts_order_picks_min_offset_consumer_deterministically() {
  let source = "import { effectScope } from 'vue'; const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ first: 1, second: 2 })); void result.first; void result.second;";
  let first = contract_stats(source);
  let second = contract_stats(source);
  assert_eq!(first.0.inactive_scope_result, second.0.inactive_scope_result);
  assert_eq!(first.0.inactive_scope_result.len(), 1, "{:?}", first.0);
  let Some(site) = first.0.inactive_scope_result.first() else {
    return;
  };
  assert_eq!(site.consumer_span.length, "result.first".len(), "{site:?}");
}

#[test]
fn value_contracts_mixed_root_growth_counts_per_root_queries() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source =
      String::from("import { customRef, effectScope, reactive, toRefs } from 'vue';");
    for index in 0..size {
      source.push_str("const bag");
      source.push_str(&index.to_string());
      source.push_str(" = toRefs(reactive({ count: 1 })); void bag");
      source.push_str(&index.to_string());
      source.push_str(".missing.value;");
      source.push_str("const keep");
      source.push_str(&index.to_string());
      source.push_str(" = toRefs(reactive({ missing: 1 })); void keep");
      source.push_str(&index.to_string());
      source.push_str(".missing.value;");
      source.push_str("const r");
      source.push_str(&index.to_string());
      source.push_str(" = customRef(() => ({ set() {} })); void r");
      source.push_str(&index.to_string());
      source.push_str(".value;");
      source.push_str("const s");
      source.push_str(&index.to_string());
      source.push_str(" = effectScope(); s");
      source.push_str(&index.to_string());
      source.push_str(".stop(); const out");
      source.push_str(&index.to_string());
      source.push_str(" = s");
      source.push_str(&index.to_string());
      source.push_str(".run(() => ({ count: 1 })); void out");
      source.push_str(&index.to_string());
      source.push_str(".count;");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.missing_torefs_key.len(), expected, "{contracts:?}");
    assert_eq!(contracts.invalid_custom_ref_interface.len(), expected, "{contracts:?}");
    assert_eq!(contracts.inactive_scope_result.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "mixed-root demand work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn value_contracts_prior_exit_and_uninvoked_nested_stay_quiet() {
  for source in [
    "import { effectScope } from 'vue'; function run() { const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); if (!result) return; void result.count; } run();",
    "import { effectScope } from 'vue'; function run() { const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); if (!result) throw new Error('empty'); void result.count; } run();",
    "import { effectScope } from 'vue'; function run() { const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); function inner() { void result.count; } } run();",
    "import { reactive, toRefs } from 'vue'; function run() { const bag = toRefs(reactive({ count: 1 })); if (!bag.missing) return; void bag.missing.value; } run();",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.inactive_scope_result.is_empty() && facts.missing_torefs_key.is_empty(),
      "prior-exit / uninvoked nested must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn value_contracts_same_region_demand_still_emits() {
  let (facts, _) = contract_stats(
    "import { effectScope, reactive, toRefs } from 'vue'; function run() { const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); void result.count; const bag = toRefs(reactive({ count: 1 })); void bag.missing.value; } run();",
  );
  assert_eq!(facts.inactive_scope_result.len(), 1, "{facts:?}");
  assert_eq!(facts.missing_torefs_key.len(), 1, "{facts:?}");
}

#[test]
fn value_contracts_receiver_mutation_suppresses_later_set() {
  let (facts, _) = contract_stats(
    "import { customRef } from 'vue'; const count = customRef(() => ({ get() { this._set = (value: number) => { this.saved = value }; return 1 } })); void count.value; count.value = 7;",
  );
  assert!(
    facts.invalid_custom_ref_interface.is_empty(),
    "getter receiver mutation must make later set demand unknown: {facts:?}"
  );
}

#[test]
fn value_contracts_closed_getter_then_missing_set_still_emits() {
  let (facts, _) = contract_stats(
    "import { customRef } from 'vue'; const count = customRef(() => ({ get() { return 1 } })); void count.value; count.value = 7;",
  );
  assert_eq!(facts.invalid_custom_ref_interface.len(), 1, "{facts:?}");
  let missing = facts.invalid_custom_ref_interface.first().map(|site| site.missing);
  assert_eq!(missing, Some(vue_vet_core::CustomRefCapability::Set), "{facts:?}");
}

#[test]
fn value_contracts_shared_scope_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source =
      String::from("import { effectScope } from 'vue'; const scope = effectScope(); scope.stop();");
    for index in 0..size {
      source.push_str("const out");
      source.push_str(&index.to_string());
      source.push_str(" = scope.run(() => ({ count: 1 })); void out");
      source.push_str(&index.to_string());
      source.push_str(".count;");
    }
    for index in 0..size {
      source.push_str("const live");
      source.push_str(&index.to_string());
      source.push_str(" = effectScope(); const keep");
      source.push_str(&index.to_string());
      source.push_str(" = live");
      source.push_str(&index.to_string());
      source.push_str(".run(() => ({ count: 1 })); void keep");
      source.push_str(&index.to_string());
      source.push_str(".count;");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.inactive_scope_result.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-scope demand work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn value_contracts_shared_bag_and_destructure_growth_stays_subquadratic() {
  let mut previous: Option<(u64, SourceContractStats)> = None;
  for size in [16_u64, 32, 64] {
    let mut keys = Vec::new();
    for index in 0..size {
      keys.push(format!("k{index}: 1"));
    }
    let mut source = format!(
      "import {{ reactive, toRefs }} from 'vue'; const state = reactive({{ count: 1, {} }});",
      keys.join(", ")
    );
    for index in 0..size {
      source.push_str("const bag");
      source.push_str(&index.to_string());
      source.push_str(" = toRefs(state); void bag");
      source.push_str(&index.to_string());
      source.push_str(".missing.value;");
      source.push_str("const keep");
      source.push_str(&index.to_string());
      source.push_str(" = toRefs(state); void keep");
      source.push_str(&index.to_string());
      source.push_str(".count.value;");
      source.push_str("const { missing: m");
      source.push_str(&index.to_string());
      source.push_str(" } = toRefs(reactive({ count: 1 })); void m");
      source.push_str(&index.to_string());
      source.push_str(".value;");
    }
    let (contracts, stats) = contract_full_stats(&source);
    let work = stats.work();
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.missing_torefs_key.len(), expected.saturating_mul(2), "{contracts:?}");
    let cloned_set_work = size.saturating_mul(2).saturating_mul(size.saturating_add(1));
    assert!(
      stats.key_copies < cloned_set_work,
      "key copies {copies} must stay below 2N(N+1)={cloned} whole-set clones for n={size}",
      copies = stats.key_copies,
      cloned = cloned_set_work,
    );
    if let Some((prev_size, prev)) = previous {
      assert_eq!(size, prev_size * 2);
      let prev_work = prev.work();
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-bag demand work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
      assert!(
        stats.key_copies.saturating_mul(10) < prev.key_copies.saturating_mul(30),
        "shared-bag key copies grew from {prev} to {now} on {prev_size}->{size}",
        prev = prev.key_copies,
        now = stats.key_copies,
      );
      assert!(
        stats.key_lookups.saturating_mul(10) < prev.key_lookups.saturating_mul(30),
        "shared-bag key lookups grew from {prev} to {now} on {prev_size}->{size}",
        prev = prev.key_lookups,
        now = stats.key_lookups,
      );
    }
    previous = Some((size, stats));
  }
}

#[test]
fn value_contracts_torefs_helper_escape_is_unknown() {
  for source in [
    "import { reactive, toRefs } from 'vue'; function install(target: object) { Object.assign(target, { added: 7 }); } const state = reactive({ initial: 1 }); install(state); const fields = toRefs(state); void fields.added.value;",
    "import { reactive, toRefs } from 'vue'; function install(target: object) { Object.assign(target, { added: 7 }); } const state = reactive({ initial: 1 }); install(state); void toRefs(state).added.value;",
    "import { reactive, toRefs } from 'vue'; function install(target: object) { Object.assign(target, { added: 7 }); } const state = reactive({ initial: 1 }); install(state); const { added } = toRefs(state); void added.value;",
    "import { reactive, toRefs } from 'vue'; function install(target: object) { Object.assign(target, { added: 7 }); } const state = reactive({ initial: 1 }); const alias = state; install(alias); void toRefs(state).added.value;",
    "import { reactive, toRefs as split } from 'vue'; function toRefs(target: object) { Object.assign(target, { added: 7 }); return {}; } const state = reactive({ initial: 1 }); toRefs(state); void split(state).added.value;",
    "import { reactive, toRefs } from 'vue'; const state = reactive({ count: 1 }); const box = { state }; void toRefs(state).missing.value;",
    "import { reactive, toRefs } from 'vue'; const state = reactive({ count: 1 }); new WeakSet([state]); void toRefs(state).missing.value;",
    "import { reactive, toRefs } from 'vue'; function tag(_strings: TemplateStringsArray, value: object) { return value; } const state = reactive({ count: 1 }); tag`${state}`; void toRefs(state).missing.value;",
    "import { reactive, toRefs } from 'vue'; const state = reactive({ count: 1 }); state.toJSON(); void toRefs(state).missing.value;",
    "import { reactive, toRefs } from 'vue'; function install<T>(this: { added?: number }) { this.added = 7 } const state = reactive({ initial: 1, install }); (state.install)``; const fields = toRefs(state); void fields.added.value;",
    "import { reactive, toRefs } from 'vue'; function install<T>(this: { added?: number }) { this.added = 7 } const state = reactive({ initial: 1, install }); (state.install<number>)``; const fields = toRefs(state); void fields.added.value;",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.missing_torefs_key.is_empty(),
      "helper/store/new/tag/member escape must keep keys unknown: {source} => {facts:?}"
    );
  }
}

#[test]
fn value_contracts_tagged_receiver_including_ts_instantiation_is_unknown() {
  for source in [
    "import { reactive, toRefs } from 'vue'; function install<T>(this: { added?: number }) { this.added = 7 } const state = reactive({ initial: 1, install }); (state.install)``; const fields = toRefs(state); void fields.added.value;",
    "import { reactive, toRefs } from 'vue'; function install<T>(this: { added?: number }) { this.added = 7 } const state = reactive({ initial: 1, install }); (state.install<number>)``; const fields = toRefs(state); void fields.added.value;",
  ] {
    let facts = assert_gated_matches_forced(source, ScriptKind::Setup);
    assert!(
      facts.missing_torefs_key.is_empty(),
      "tagged receiver must keep closed keys unknown: {source} => {facts:?}"
    );
  }
  let positive = assert_gated_matches_forced(
    "import { reactive, toRefs } from 'vue'; void toRefs(reactive({ count: 1 })).missing.value;",
    ScriptKind::Setup,
  );
  assert_eq!(
    positive.missing_torefs_key.len(),
    1,
    "ordinary missing-key demand must stay positive: {positive:?}"
  );
}

#[test]
fn value_contracts_torefs_known_borrows_still_report_missing_keys() {
  let (local, _) = contract_stats(
    "import { reactive, toRefs } from 'vue'; function install(target: object) { Object.assign(target, { added: 7 }); } const state = reactive({ initial: 1 }); const other = reactive({ initial: 1 }); install(other); void toRefs(state).missing.value;",
  );
  assert_eq!(local.missing_torefs_key.len(), 1, "{local:?}");
  let Some(site) = local.missing_torefs_key.first() else {
    return;
  };
  assert_eq!(site.key, "missing");
  assert_eq!(site.demand_span.length, "toRefs(state).missing.value".len(), "{site:?}");
  let (repeated, _) = contract_stats(
    "import { reactive, toRefs } from 'vue'; const state = reactive({ count: 1 }); void toRefs(state).missing.value; const bag = toRefs(state); void bag.missing.value;",
  );
  assert_eq!(repeated.missing_torefs_key.len(), 2, "{repeated:?}");
}

#[test]
fn value_contracts_source5_still_treats_torefs_arg_as_escape() {
  let (facts, _) = contract_stats(
    "import { reactive, toRefs, watch } from 'vue'; const state = reactive({ child: { count: 1 } }); toRefs(state); watch(state.child, () => {}); state.child = { count: 2 };",
  );
  assert!(
    facts.watch_replaced_object_source.is_empty(),
    "generic source5 escape of a toRefs argument must stay uncertain: {facts:?}"
  );
}

#[test]
fn value_contracts_computed_key_receiver_mutation_is_uncertain() {
  for source in [
    "import { customRef } from 'vue'; let saved = 0; const field = customRef(() => ({ get() { const marker = { [this._set = (value: number) => { saved = value }]: 1 }; return marker; } })); void field.value; field.value = 7;",
    "import { customRef } from 'vue'; let saved = 0; const field = customRef(() => ({ get() { const marker = { [(this._set = (value: number) => { saved = value })]: 1 }; return marker; } })); void field.value; field.value = 7;",
    "import { customRef } from 'vue'; let saved = 0; const field = customRef(() => ({ get() { const { [(this._set = (value: number) => { saved = value })]: marker } = { 1: 1 }; return marker; } })); void field.value; field.value = 7;",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.invalid_custom_ref_interface.is_empty(),
      "computed-key receiver mutation must make later set demand unknown: {source} => {facts:?}"
    );
  }
}

#[test]
fn value_contracts_literal_computed_key_and_unrun_uncertain_getter_still_report() {
  let (literal, _) = contract_stats(
    "import { customRef } from 'vue'; const field = customRef(() => ({ get() { const marker = { [1]: 1 }; return marker; } })); void field.value; field.value = 7;",
  );
  assert_eq!(literal.invalid_custom_ref_interface.len(), 1, "{literal:?}");
  let missing = literal.invalid_custom_ref_interface.first().map(|site| site.missing);
  assert_eq!(missing, Some(vue_vet_core::CustomRefCapability::Set), "{literal:?}");
  let Some(site) = literal.invalid_custom_ref_interface.first() else {
    return;
  };
  assert_eq!(site.demand_span.length, "field.value".len(), "{site:?}");
  let (unrun, _) = contract_stats(
    "import { customRef } from 'vue'; const field = customRef(() => ({ get() { const marker = { [this._set = (value: number) => { void value }]: 1 }; return 1; } })); field.value = 7;",
  );
  assert_eq!(unrun.invalid_custom_ref_interface.len(), 1, "{unrun:?}");
  let unrun_missing = unrun.invalid_custom_ref_interface.first().map(|site| site.missing);
  assert_eq!(unrun_missing, Some(vue_vet_core::CustomRefCapability::Set), "{unrun:?}");
}
fn extracted_methods(source: &str) -> vue_vet_core::SourceContractFacts {
  analyze(source, "ts").source_contracts
}

#[expect(clippy::panic, reason = "missing extracted-method fact must fail the regression")]
fn first_extracted(
  facts: &vue_vet_core::SourceContractFacts,
) -> &vue_vet_core::ExtractedReactiveCollectionMethodFact {
  facts
    .extracted_reactive_collection_method
    .first()
    .unwrap_or_else(|| panic!("expected extracted collection-method fact; {facts:?}"))
}

#[test]
fn extracted_collection_methods_report_bare_calls() {
  let map_get = extracted_methods(
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert_eq!(map_get.extracted_reactive_collection_method.len(), 1, "{map_get:?}");
  let site = first_extracted(&map_get);
  assert_eq!(site.method, "get");
  assert_eq!(site.collection, "Map");
  assert_eq!(site.api, "reactive");
  assert_eq!(site.object, "map");
  let map_set = extracted_methods(
    "import { reactive } from 'vue'; const map = reactive(new Map()); const set = map.set; set('a', 1);",
  );
  assert_eq!(first_extracted(&map_set).method, "set");
  let map_has = extracted_methods(
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { has } = map; has('a');",
  );
  assert_eq!(first_extracted(&map_has).method, "has");
  let set_add = extracted_methods(
    "import { reactive } from 'vue'; const items = reactive(new Set()); const { add } = items; add(1);",
  );
  assert_eq!(first_extracted(&set_add).collection, "Set");
  let set_has = extracted_methods(
    "import { reactive } from 'vue'; const items = reactive(new Set([1])); const has = items.has; has(1);",
  );
  assert_eq!(first_extracted(&set_has).method, "has");
  let array_map = extracted_methods(
    "import { reactive } from 'vue'; const items = reactive([1, 2]); const { map } = items; map((n) => n);",
  );
  assert_eq!(first_extracted(&array_map).collection, "Array");
  let array_includes = extracted_methods(
    "import { reactive } from 'vue'; const items = reactive([1, 2]); const { includes } = items; includes(1);",
  );
  assert_eq!(first_extracted(&array_includes).method, "includes");
  let array_push = extracted_methods(
    "import { reactive } from 'vue'; const items = reactive([1, 2]); const push = items.push; push(3);",
  );
  assert_eq!(first_extracted(&array_push).method, "push");
}

#[test]
fn extracted_collection_methods_follow_aliases_namespaces_and_ts_wrappers() {
  let aliased = extracted_methods(
    "import { reactive as rx } from 'vue'; const map = rx(new Map([['a', 1]])); const alias = map; const get = alias.get; get('a');",
  );
  assert_eq!(aliased.extracted_reactive_collection_method.len(), 1, "{aliased:?}");
  let namespace = extracted_methods(
    "import * as Vue from 'vue'; const map = Vue.reactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert_eq!(namespace.extracted_reactive_collection_method.len(), 1, "{namespace:?}");
  let wrapped = extracted_methods(
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]]) as Map<string, number>); const get = (map as Map<string, number>).get; get('a' as const);",
  );
  assert_eq!(wrapped.extracted_reactive_collection_method.len(), 1, "{wrapped:?}");
  let reactivity = extracted_methods(
    "import { reactive } from '@vue/reactivity'; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert_eq!(reactivity.extracted_reactive_collection_method.len(), 1, "{reactivity:?}");
  let runtime = extracted_methods(
    "import { shallowReactive } from '@vue/runtime-core'; const map = shallowReactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert_eq!(first_extracted(&runtime).api, "shallowReactive");
  let raw = extracted_methods(
    "import { reactive } from 'vue'; const raw = new Map([['a', 1]]); const map = reactive(raw); const { get } = map; get('a');",
  );
  assert_eq!(raw.extracted_reactive_collection_method.len(), 1, "{raw:?}");
}

#[test]
fn extracted_collection_methods_reuse_counted_actual_proxy_origin() {
  let (named, named_stats) = contract_full_stats(
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert_eq!(named.extracted_reactive_collection_method.len(), 1, "{named:?}");
  assert_eq!(
    named_stats.import_source_steps, 2,
    "named constructor origin is the counted VueImport lookup; {named_stats:?}"
  );
  let (namespace, namespace_stats) = contract_full_stats(
    "import * as Vue from 'vue'; const map = Vue.reactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert_eq!(namespace.extracted_reactive_collection_method.len(), 1, "{namespace:?}");
  assert_eq!(
    namespace_stats.import_source_steps, 2,
    "namespace constructor origin is the counted VueImport lookup; {namespace_stats:?}"
  );
  let (local, local_stats) = contract_full_stats(
    "function reactive<T>(value: T): T { return value; } const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert!(local.extracted_reactive_collection_method.is_empty(), "{local:?}");
  assert_eq!(
    local_stats.import_source_steps, 0,
    "local constructors must not examine VueImport; {local_stats:?}"
  );
  let (type_only, type_only_stats) = contract_full_stats(
    "import type { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert!(type_only.extracted_reactive_collection_method.is_empty(), "{type_only:?}");
  assert_eq!(
    type_only_stats.import_source_steps, 0,
    "type-only sources must not examine VueImport; {type_only_stats:?}"
  );
}

#[test]
fn extracted_collection_methods_stay_quiet_for_safe_and_unknown_controls() {
  let sources = [
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); map.get('a');",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; get.call(map, 'a');",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; get.apply(map, ['a']);",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; Reflect.apply(get, map, ['a']);",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const bound = map.get.bind(map); bound('a');",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; void get;",
    "import { reactive } from 'vue'; const object = reactive({ get: () => 1 }); const { get } = object; get();",
    "function reactive<T>(value: T): T { return value; } const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
    "import { reactive } from 'vue'; class Map { get(_key: string) { return 1; } } const map = reactive(new Map()); const { get } = map; get('a');",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); map.get = ((_key: string) => 1) as typeof map.get; const { get } = map; get('a');",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); (map as { __v_skip?: boolean }).__v_skip = true; const { get } = map; get('a');",
    "import { reactive } from 'vue'; function tag(value: object) { void value; } const map = reactive(new Map([['a', 1]])); tag(map); const { get } = map; get('a');",
    "import { reactive } from 'vue-demi'; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
    "import { reactive } from '#imports'; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
    "import { reactive } from 'vue'; function createMap() { return new Map([['a', 1]]); } const map = reactive(createMap()); const { get } = map; get('a');",
    "import { reactive } from 'vue'; Map.prototype.get = function get() { return 1; }; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
    "import { reactive } from 'vue'; const key = 'get'; const map = reactive(new Map([['a', 1]])); const get = map[key]; get('a');",
    "import { reactive } from 'vue'; let map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
    "import { readonly } from 'vue'; const map = readonly(new Map([['a', 1]])); const { get } = map; get('a');",
    "import { reactive } from 'vue'; class Box { constructor(public value: object) {} } const map = reactive(new Map([['a', 1]])); void new Box(map); const { get } = map; get('a');",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); let get: typeof map.get; ({ get } = map); get('a');",
    "import { reactive } from 'vue'; const map = reactive(new WeakMap()); const { get } = map as { get: (key: object) => unknown }; get({});",
    "import { reactive } from 'vue'; const items = reactive(new Array(2)); const { map } = items as { map: (fn: (n: unknown) => unknown) => unknown[] }; map((n) => n);",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); (map as { tag?: () => void }).tag = () => {}; (map as { tag: () => void }).tag(); const { get } = map; get('a');",
  ];
  for source in sources {
    let facts = extracted_methods(source);
    assert!(
      facts.extracted_reactive_collection_method.is_empty(),
      "must stay quiet: {source} -> {facts:?}"
    );
  }
}

#[test]
#[expect(clippy::panic, reason = "missing span evidence must fail the regression")]
fn extracted_collection_methods_unicode_and_crlf_use_exact_bytes() {
  let unicode = "import { reactive } from 'vue'; const 表 = reactive(new Map([['键', 1]])); const { get } = 表; get('键');";
  let facts = extracted_methods(unicode);
  assert_eq!(facts.extracted_reactive_collection_method.len(), 1, "{facts:?}");
  let site = first_extracted(&facts);
  let Some(call) = unicode.find("get('键')") else {
    panic!("unicode call");
  };
  assert_eq!(site.call_span.offset, call);
  assert_eq!(site.call_span.length, "get('键')".len());
  assert_eq!(unicode.as_bytes().get(call).copied(), Some(b'g'));
  let crlf = "import { reactive } from 'vue';\r\nconst map = reactive(new Map([['a', 1]]));\r\nconst { get } = map;\r\nget('a');\r\n";
  let crlf_facts = extracted_methods(crlf);
  assert_eq!(crlf_facts.extracted_reactive_collection_method.len(), 1, "{crlf_facts:?}");
  let crlf_site = first_extracted(&crlf_facts);
  let Some(crlf_call) = crlf.find("get('a')") else {
    panic!("crlf call");
  };
  assert_eq!(crlf_site.call_span.offset, crlf_call);
  assert_eq!(crlf_site.call_span.length, "get('a')".len());
  assert!(crlf.contains('\r'), "fixture must keep CR bytes");
}

#[test]
fn extracted_collection_methods_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { reactive } from 'vue';");
    for index in 0..size {
      source.push_str("const m");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(new Map([['a', 1]])); const { get: g");
      source.push_str(&index.to_string());
      source.push_str(" } = m");
      source.push_str(&index.to_string());
      source.push_str("; g");
      source.push_str(&index.to_string());
      source.push_str("('a');");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.extracted_reactive_collection_method.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "size={size} work={work}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "extracted-method work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn extracted_collection_methods_stay_quiet_for_review_capability_roles() {
  let sources = [
    "import { reactive } from 'vue'; const raw = [1]; ({ skip: raw.__v_skip, map: raw.map } = { skip: true, map: () => [7] }); const items = reactive(raw); const { map } = items; map();",
    "import { reactive } from 'vue'; const raw = [1]; const marker = '__v_skip'; raw[marker] = true; raw.map = () => [7]; const items = reactive(raw); const { map } = items; map();",
    "import { reactive } from 'vue'; function configure(target: { __v_skip?: boolean; map?: () => number[] }) { target.__v_skip = true; target.map = () => [7]; } const raw = [1]; configure(true && raw); const items = reactive(raw); const { map } = items; map();",
    "import { reactive } from 'vue'; const raw = [1] as { configure?: (value: TemplateStringsArray) => void; map?: () => number[]; __v_skip?: boolean }; raw.configure = function (this: typeof raw) { this.__v_skip = true; this.map = () => [7]; }; raw.configure`change`; const items = reactive(raw); const { map } = items; map();",
    "import { reactive } from 'vue'; const raw = [1] as { configure?: () => void; map?: () => number[]; __v_skip?: boolean }; raw.configure = function (this: typeof raw) { this.__v_skip = true; this.map = () => [7]; }; const method = 'configure'; raw[method](); const items = reactive(raw); const { map } = items; map();",
    "import { reactive } from 'vue'; const raw = [1] as { valueOf?: () => number[]; map?: () => number[]; __v_skip?: boolean }; raw.valueOf = function (this: typeof raw) { this.__v_skip = true; this.map = () => [7]; }; raw.valueOf(); const items = reactive(raw); const { map } = items; map();",
    "import { reactive } from 'vue'; const raw = [1] as { map?: () => number[]; __v_skip?: boolean }; function configure() { alias.__v_skip = true; alias.map = () => [7]; } const alias = raw; configure(); const items = reactive(raw); const { map } = items; map();",
    "import { reactive } from 'vue'; const original = Map; try { (globalThis as { Map: typeof Map }).Map = class { get() { return 7; } } as unknown as MapConstructor; const items = reactive(new Map()); const { get } = items as { get: () => number }; get(); } finally { (globalThis as { Map: typeof Map }).Map = original; }",
    "import { reactive } from 'vue'; const items = reactive(new Map()); const { get } = items; false && get('key');",
    "import { reactive } from 'vue'; function run() { const items = reactive(new Map()); const { get } = items; return 7; get('key'); } run();",
    "import { reactive } from 'vue'; const raw = [1]; delete raw.map; const items = reactive(raw); const { map } = items as { map: () => number[] }; map();",
    "import { reactive } from 'vue'; const raw = [1] as { map?: () => number[] }; for (raw.map of [() => [7]]) {} const items = reactive(raw); const { map } = items; map();",
  ];
  for source in sources {
    let facts = extracted_methods(source);
    assert!(
      facts.extracted_reactive_collection_method.is_empty(),
      "must stay quiet: {source} -> {facts:?}"
    );
  }
}

#[test]
fn source5_logical_watch_stays_quiet_after_collection_capability_repair() {
  let facts = analyze(
    "import { reactive, watch } from 'vue'; const state = reactive({ child: { count: 1 } }); false && watch(state.child, () => {}); state.child = { count: 2 };",
    "ts",
  );
  assert!(
    facts.source_contracts.watch_replaced_object_source.is_empty(),
    "source5 must keep statement eligibility; {:?}",
    facts.source_contracts
  );
}

#[test]
fn extracted_collection_methods_dense_negatives_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { reactive } from 'vue';");
    for index in 0..size {
      source.push_str("const r");
      source.push_str(&index.to_string());
      source.push_str(" = [1]; const k");
      source.push_str(&index.to_string());
      source.push_str(" = '__v_skip'; r");
      source.push_str(&index.to_string());
      source.push_str("[k");
      source.push_str(&index.to_string());
      source.push_str("] = true; const i");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(r");
      source.push_str(&index.to_string());
      source.push_str("); const { map: m");
      source.push_str(&index.to_string());
      source.push_str(" } = i");
      source.push_str(&index.to_string());
      source.push_str("; m");
      source.push_str(&index.to_string());
      source.push_str("();");
    }
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.extracted_reactive_collection_method.is_empty(),
      "dense computed-marker writes must stay quiet size={size} work={work} {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "dense-negative work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn extracted_collection_methods_shared_alias_negatives_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { reactive } from 'vue'; const raw = [1];");
    for index in 0..size {
      source.push_str("const a");
      source.push_str(&index.to_string());
      source.push_str(" = raw; a");
      source.push_str(&index.to_string());
      source.push_str(".__v_skip = true;");
    }
    source.push_str("const items = reactive(raw); const { map } = items; map();");
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.extracted_reactive_collection_method.is_empty(),
      "shared-alias writes must stay quiet size={size} work={work} {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-alias-negative work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

fn nest_true_and(inner: &str, depth: u32) -> String {
  let mut expr = inner.to_string();
  for _ in 0..depth {
    expr = format!("(true && {expr})");
  }
  expr
}

fn nest_ternary(inner: &str, depth: u32) -> String {
  let mut expr = inner.to_string();
  for _ in 0..depth {
    expr = format!("(true ? {expr} : 0)");
  }
  expr
}

fn nest_array(inner: &str, depth: u32) -> String {
  let mut expr = inner.to_string();
  for _ in 0..depth {
    expr = format!("[{expr}]");
  }
  expr
}

fn nest_sequence(inner: &str, depth: u32) -> String {
  let mut expr = inner.to_string();
  for _ in 0..depth {
    expr = format!("(0, {expr})");
  }
  expr
}

fn escape_helper_source(argument: &str) -> String {
  format!(
    "import {{ reactive }} from 'vue'; function configure(target: {{ __v_skip?: boolean; map?: () => number[] }}) {{ target.__v_skip = true; target.map = () => [7]; }} const raw = [1]; configure({argument}); const items = reactive(raw); const {{ map }} = items; map();"
  )
}

#[test]
fn extracted_collection_methods_stay_quiet_for_budget_boundary_escapes() {
  let sources = [
    escape_helper_source(&nest_true_and("raw", 8)),
    escape_helper_source(&nest_true_and("raw", 9)),
    escape_helper_source(&nest_true_and("raw", 16)),
    escape_helper_source(&nest_ternary("raw", 9)),
    escape_helper_source(&nest_sequence("raw", 9)),
    format!(
      "import {{ reactive }} from 'vue'; function configure(target: unknown) {{ const value = (target as {{ __v_skip?: boolean; map?: () => number[] }}[][][][][][][][][])[0][0][0][0][0][0][0][0][0]; value.__v_skip = true; value.map = () => [7]; }} const raw = [1] as {{ __v_skip?: boolean; map?: () => number[] }}; configure({}); const items = reactive(raw); const {{ map }} = items; map();",
      nest_array("raw", 9)
    ),
  ];
  for source in sources {
    let facts = extracted_methods(&source);
    assert!(
      facts.extracted_reactive_collection_method.is_empty(),
      "must stay quiet: {source} -> {facts:?}"
    );
  }
}

#[test]
fn extracted_collection_methods_keep_sibling_positive_across_unresolved_escape() {
  let source = format!(
    "{} const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
    escape_helper_source(&nest_true_and("raw", 9))
  );
  let facts = extracted_methods(&source);
  assert_eq!(facts.extracted_reactive_collection_method.len(), 1, "{facts:?}");
  assert_eq!(first_extracted(&facts).method, "get");
  assert_eq!(first_extracted(&facts).object, "kept");
}

#[test]
fn extracted_collection_methods_deeper_escape_negatives_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; function configure(target: { __v_skip?: boolean; map?: () => number[] } | unknown) { void target; }",
    );
    for index in 0..size {
      let name = format!("r{index}");
      let escaped = match index % 4 {
        0 => nest_true_and(&name, 9),
        1 => nest_ternary(&name, 9),
        2 => nest_array(&name, 9),
        _ => nest_sequence(&name, 9),
      };
      source.push_str("const ");
      source.push_str(&name);
      source.push_str(" = [1]; configure(");
      source.push_str(&escaped);
      source.push_str("); const i");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(");
      source.push_str(&name);
      source.push_str("); const { map: m");
      source.push_str(&index.to_string());
      source.push_str(" } = i");
      source.push_str(&index.to_string());
      source.push_str("; m");
      source.push_str(&index.to_string());
      source.push_str("();");
    }
    source.push_str("const deep = [1]; configure(");
    source.push_str(&nest_true_and("deep", u32::try_from(size).unwrap_or(u32::MAX)));
    source.push_str(
      "); const deepItems = reactive(deep); const { map: deepMap } = deepItems; deepMap();",
    );
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.extracted_reactive_collection_method.is_empty(),
      "deeper escape negatives must stay quiet size={size} work={work} {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "deeper-escape-negative work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

fn native_ctor_escape_source(argument: &str) -> String {
  format!(
    "import {{ reactive }} from 'vue'; function configure(ctor: ArrayConstructor) {{ const previous = ctor.prototype.map; ctor.prototype.__v_skip = true; ctor.prototype.map = () => [7]; return () => {{ delete ctor.prototype.__v_skip; ctor.prototype.map = previous; }}; }} const restore = configure({argument}); const items = reactive([1]); const {{ map }} = items; map(); restore();"
  )
}

fn native_prototype_escape_source(argument: &str) -> String {
  format!(
    "import {{ reactive }} from 'vue'; function configure(prototype: typeof Array.prototype) {{ const previous = prototype.map; prototype.__v_skip = true; prototype.map = () => [7]; return () => {{ delete prototype.__v_skip; prototype.map = previous; }}; }} const restore = configure({argument}); const items = reactive([1]); const {{ map }} = items; map(); restore();"
  )
}

fn global_alias_escape_source(argument: &str) -> String {
  format!(
    "import {{ reactive }} from 'vue'; function configure(world: typeof globalThis) {{ const previous = world.Map; world.Map = class {{ get() {{ return 7; }} }} as unknown as MapConstructor; return () => {{ world.Map = previous; }}; }} const world = globalThis; const restore = configure({argument}); const items = reactive(new Map()); const {{ get }} = items as {{ get: () => number }}; get(); restore();"
  )
}

#[test]
fn extracted_collection_methods_stay_quiet_for_native_global_escapes() {
  let sources = [
    native_ctor_escape_source(&nest_true_and("Array", 8)),
    native_ctor_escape_source(&nest_true_and("Array", 9)),
    native_ctor_escape_source(&nest_true_and("Array", 16)),
    native_prototype_escape_source(&nest_true_and("Array.prototype", 8)),
    native_prototype_escape_source(&nest_true_and("Array.prototype", 9)),
    native_prototype_escape_source(&nest_true_and("Array.prototype", 16)),
    global_alias_escape_source("world"),
    global_alias_escape_source(&nest_true_and("world", 8)),
    global_alias_escape_source(&nest_true_and("world", 9)),
    global_alias_escape_source(&nest_true_and("world", 16)),
  ];
  for source in sources {
    let facts = extracted_methods(&source);
    assert!(
      facts.extracted_reactive_collection_method.is_empty(),
      "must stay quiet: {source} -> {facts:?}"
    );
  }
}

#[test]
fn extracted_collection_methods_keep_map_positive_when_only_array_capability_is_unresolved() {
  let source = format!(
    "{} const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
    native_ctor_escape_source(&nest_true_and("Array", 9))
  );
  let facts = extracted_methods(&source);
  assert_eq!(facts.extracted_reactive_collection_method.len(), 1, "{facts:?}");
  assert_eq!(first_extracted(&facts).method, "get");
  assert_eq!(first_extracted(&facts).object, "kept");
}

#[test]
fn extracted_collection_methods_keep_positive_when_shadowed_globals_escape() {
  let shadowed_array = format!(
    "import {{ reactive }} from 'vue'; function configure(ctor: unknown) {{ void ctor; }} const Array = class {{}}; configure({}); const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
    nest_true_and("Array", 9)
  );
  let shadowed_global_this = format!(
    "import {{ reactive }} from 'vue'; function configure(world: unknown) {{ void world; }} const globalThis = {{ Map: class {{ get() {{ return 7; }} }} }}; configure({}); const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
    nest_true_and("globalThis", 9)
  );
  for source in [shadowed_array, shadowed_global_this] {
    let facts = extracted_methods(&source);
    assert_eq!(facts.extracted_reactive_collection_method.len(), 1, "{source} -> {facts:?}");
    assert_eq!(first_extracted(&facts).method, "get");
    assert_eq!(first_extracted(&facts).object, "kept");
  }
}

#[test]
fn extracted_collection_methods_unresolved_global_escape_work_scales_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; function configure(target: unknown) { void target; } const world = globalThis;",
    );
    for index in 0..size {
      let name = format!("r{index}");
      let escaped = match index % 4 {
        0 => nest_true_and("Array", 9),
        1 => nest_true_and("Array.prototype", 9),
        2 => nest_true_and("world", 9),
        _ => nest_true_and(&name, 9),
      };
      source.push_str("const ");
      source.push_str(&name);
      source.push_str(" = [1]; configure(");
      source.push_str(&escaped);
      source.push_str("); const i");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(");
      source.push_str(&name);
      source.push_str("); const { map: m");
      source.push_str(&index.to_string());
      source.push_str(" } = i");
      source.push_str(&index.to_string());
      source.push_str("; m");
      source.push_str(&index.to_string());
      source.push_str("();");
    }
    source.push_str("const deep = [1]; configure(");
    source.push_str(&nest_true_and("Array", u32::try_from(size).unwrap_or(u32::MAX)));
    source.push_str(
      "); const deepItems = reactive(deep); const { map: deepMap } = deepItems; deepMap();",
    );
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.extracted_reactive_collection_method.is_empty(),
      "unresolved global-escape work must stay quiet size={size} work={work} {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "unresolved-global-escape work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

fn native_ctor_alias_escape_source(init: &str, argument: &str) -> String {
  format!(
    "import {{ reactive }} from 'vue'; function configure(ctor: ArrayConstructor) {{ const previous = ctor.prototype.map; ctor.prototype.__v_skip = true; ctor.prototype.map = () => [7]; return () => {{ delete ctor.prototype.__v_skip; ctor.prototype.map = previous; }}; }} {init} const restore = configure({argument}); const items = reactive([1]); const {{ map }} = items; map(); restore();"
  )
}

fn native_prototype_alias_escape_source(init: &str, argument: &str) -> String {
  format!(
    "import {{ reactive }} from 'vue'; function configure(prototype: typeof Array.prototype) {{ const previous = prototype.map; prototype.__v_skip = true; prototype.map = () => [7]; return () => {{ delete prototype.__v_skip; prototype.map = previous; }}; }} {init} const restore = configure({argument}); const items = reactive([1]); const {{ map }} = items; map(); restore();"
  )
}

#[test]
fn extracted_collection_methods_stay_quiet_for_native_ctor_aliases() {
  let sources = [
    native_ctor_alias_escape_source("const capability = Array;", "capability"),
    native_ctor_alias_escape_source("const capability = Array;", &nest_true_and("capability", 8)),
    native_ctor_alias_escape_source("const capability = Array;", &nest_true_and("capability", 9)),
    native_ctor_alias_escape_source("const capability = Array;", &nest_true_and("capability", 16)),
    native_prototype_alias_escape_source("const capability = Array.prototype;", "capability"),
    native_prototype_alias_escape_source(
      "const capability = Array.prototype;",
      &nest_true_and("capability", 8),
    ),
    native_prototype_alias_escape_source(
      "const capability = Array.prototype;",
      &nest_true_and("capability", 9),
    ),
    native_prototype_alias_escape_source(
      "const capability = Array.prototype;",
      &nest_true_and("capability", 16),
    ),
    native_ctor_alias_escape_source("const ctor = Array; const capability = ctor;", "capability"),
    native_prototype_alias_escape_source(
      "const proto = Array.prototype; const capability = proto;",
      "capability",
    ),
    native_prototype_alias_escape_source(
      "const ctor = Array; const capability = ctor.prototype;",
      "capability",
    ),
  ];
  for source in sources {
    let facts = extracted_methods(&source);
    assert!(
      facts.extracted_reactive_collection_method.is_empty(),
      "must stay quiet: {source} -> {facts:?}"
    );
  }
}

#[test]
fn extracted_collection_methods_keep_map_positive_when_only_array_alias_escapes() {
  let sources = [
    format!(
      "{} const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
      native_ctor_alias_escape_source("const capability = Array;", "capability")
    ),
    format!(
      "{} const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
      native_ctor_alias_escape_source("const capability = Array;", &nest_true_and("capability", 9))
    ),
    format!(
      "{} const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
      native_prototype_alias_escape_source("const capability = Array.prototype;", "capability")
    ),
    format!(
      "{} const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
      native_prototype_alias_escape_source(
        "const capability = Array.prototype;",
        &nest_true_and("capability", 9)
      )
    ),
  ];
  for source in sources {
    let facts = extracted_methods(&source);
    assert_eq!(facts.extracted_reactive_collection_method.len(), 1, "{source} -> {facts:?}");
    assert_eq!(first_extracted(&facts).method, "get");
    assert_eq!(first_extracted(&facts).object, "kept");
  }
}

#[test]
fn extracted_collection_methods_keep_positive_when_shadowed_ctor_aliases_escape() {
  let shadowed_constructor = format!(
    "import {{ reactive }} from 'vue'; function configure(ctor: unknown) {{ void ctor; }} const Array = class {{}}; const capability = Array; configure({}); const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
    nest_true_and("capability", 9)
  );
  let shadowed_prototype = format!(
    "import {{ reactive }} from 'vue'; function configure(prototype: unknown) {{ void prototype; }} const Array = {{ prototype: {{}} }}; const capability = Array.prototype; configure({}); const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
    nest_true_and("capability", 9)
  );
  let shadowed_direct = "import { reactive } from 'vue'; function configure(ctor: unknown) { void ctor; } const Array = class {}; const capability = Array; configure(capability); const kept = reactive(new Map([['a', 1]])); const { get } = kept; get('a');";
  for source in [shadowed_constructor, shadowed_prototype, shadowed_direct.to_string()] {
    let facts = extracted_methods(&source);
    assert_eq!(facts.extracted_reactive_collection_method.len(), 1, "{source} -> {facts:?}");
    assert_eq!(first_extracted(&facts).method, "get");
    assert_eq!(first_extracted(&facts).object, "kept");
  }
}

#[test]
fn extracted_collection_methods_native_ctor_alias_work_scales_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; function configure(target: unknown) { void target; }",
    );
    for index in 0..size {
      let escaped = match index % 4 {
        0 => {
          source.push_str("const c");
          source.push_str(&index.to_string());
          source.push_str(" = Array; ");
          format!("c{index}")
        }
        1 => {
          source.push_str("const p");
          source.push_str(&index.to_string());
          source.push_str(" = Array.prototype; ");
          format!("p{index}")
        }
        2 => {
          source.push_str("const a");
          source.push_str(&index.to_string());
          source.push_str(" = Array; const b");
          source.push_str(&index.to_string());
          source.push_str(" = a");
          source.push_str(&index.to_string());
          source.push_str("; ");
          format!("b{index}")
        }
        _ => {
          source.push_str("const d");
          source.push_str(&index.to_string());
          source.push_str(" = Array; const e");
          source.push_str(&index.to_string());
          source.push_str(" = d");
          source.push_str(&index.to_string());
          source.push_str(".prototype; ");
          format!("e{index}")
        }
      };
      let nested = if index % 2 == 0 { nest_true_and(&escaped, 9) } else { escaped };
      source.push_str("configure(");
      source.push_str(&nested);
      source.push_str("); const i");
      source.push_str(&index.to_string());
      source.push_str(" = reactive([1]); const { map: m");
      source.push_str(&index.to_string());
      source.push_str(" } = i");
      source.push_str(&index.to_string());
      source.push_str("; m");
      source.push_str(&index.to_string());
      source.push_str("();");
    }
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.extracted_reactive_collection_method.is_empty(),
      "native ctor-alias work must stay quiet size={size} work={work} {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "native-ctor-alias work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

fn script_setup_from_sfc(sfc: &str) -> &str {
  const OPEN: &str = "<script setup lang=\"ts\">";
  let start = sfc.find(OPEN).map_or(0, |index| index.saturating_add(OPEN.len()));
  let rest = sfc.get(start..).unwrap_or("");
  let end = rest.find("</script>").map_or(sfc.len(), |index| start.saturating_add(index));
  sfc.get(start..end).unwrap_or(sfc).trim()
}

fn effect_only_source(api: &str, size: u64, second_arg: Option<&str>) -> String {
  let mut source = format!("import {{ {api} }} from 'vue';");
  for index in 0..size {
    source.push_str("const n");
    source.push_str(&index.to_string());
    source.push_str(" = { a: 1, b: 2, c: 3 }; ");
    source.push_str(api);
    source.push_str("(() => { n");
    source.push_str(&index.to_string());
    source.push_str(".a; }");
    if let Some(options) = second_arg {
      source.push_str(", ");
      source.push_str(options);
    }
    source.push_str(");");
  }
  source
}

fn effect_wrapper_source(depth: u64) -> String {
  let wrappers = " as any".repeat(usize::try_from(depth).unwrap_or(0));
  format!("import {{ watchEffect }} from 'vue'; (watchEffect{wrappers})(() => {{}});")
}

fn effect_import_width_source(width: usize) -> String {
  const NAMES: [&str; 8] = [
    "watchEffect",
    "ref",
    "effect",
    "shallowRef",
    "watchPostEffect",
    "defineProps",
    "useTemplateRef",
    "defineModel",
  ];
  assert!(width >= 1 && width <= NAMES.len(), "width={width}");
  let mut names = String::new();
  for (index, name) in NAMES.iter().enumerate() {
    if index >= width {
      break;
    }
    if index > 0 {
      names.push_str(", ");
    }
    names.push_str(name);
  }
  format!("import {{ {names} }} from 'vue'; watchEffect(() => {{}});")
}

#[test]
fn source_contracts_effect_family_one_arg_and_no_args_bypass_indexes() {
  for api in ["watchEffect", "watchPostEffect", "watchSyncEffect"] {
    assert_bypass(&format!("import {{ {api} }} from 'vue'; {api}(() => {{}});"), ScriptKind::Setup);
    assert_bypass(&format!("import {{ {api} }} from 'vue'; {api}();"), ScriptKind::Setup);
    assert_bypass(
      &format!("import {{ {api} as run }} from 'vue'; run(() => {{}});"),
      ScriptKind::Setup,
    );
    assert_bypass(
      &format!("import {{ {api} }} from 'vue'; {api}?.(() => {{}});"),
      ScriptKind::Setup,
    );
    assert_bypass(
      &format!("import {{ {api} }} from 'vue'; ({api} as any)(() => {{}});"),
      ScriptKind::Setup,
    );
    assert_bypass(
      &format!("import {{ {api} }} from 'vue'; {api}!(() => {{}});"),
      ScriptKind::Setup,
    );
    assert_bypass(
      &format!("import {{ {api} }} from '#imports'; {api}(() => {{}});"),
      ScriptKind::Setup,
    );
  }
}

#[test]
fn source_contracts_effect_family_second_arg_and_spreads_match_forced_full() {
  for api in ["watchEffect", "watchPostEffect", "watchSyncEffect"] {
    let ignored = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; {api}(() => {{}}, {{ once: true }});"),
      ScriptKind::Setup,
    );
    assert_eq!(ignored.watch_ignored_option.len(), 1, "{api} ignored once");
    let valid = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; {api}(() => {{}}, {{ flush: 'pre' }});"),
      ScriptKind::Setup,
    );
    assert!(valid.is_empty(), "{api} flush-only options must stay quiet: {valid:?}");
    let invalid_immediate = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; {api}(() => {{}}, {{ immediate: true }});"),
      ScriptKind::Setup,
    );
    assert_eq!(invalid_immediate.watch_ignored_option.len(), 1, "{api} ignored immediate");
    let signature = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; {api}(() => {{}}, () => {{}});"),
      ScriptKind::Setup,
    );
    assert_eq!(signature.watch_signature_mismatch.len(), 1, "{api} function-as-options");
    let spread = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; const args = [() => {{}}]; {api}(...args);"),
      ScriptKind::Setup,
    );
    assert!(spread.is_empty(), "{api} spread must stay equivalent: {spread:?}");
    let trailing_spread = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; {api}(() => {{}}, ...[{{ once: true }}]);"),
      ScriptKind::Setup,
    );
    assert!(
      trailing_spread.is_empty(),
      "{api} trailing spread must stay equivalent: {trailing_spread:?}"
    );
    let alias = assert_gated_matches_forced(
      &format!("import {{ {api} as run }} from 'vue'; run(() => {{}}, {{ once: true }});"),
      ScriptKind::Setup,
    );
    assert_eq!(alias.watch_ignored_option.len(), 1, "{api} alias");
    let optional = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; {api}?.(() => {{}}, {{ once: true }});"),
      ScriptKind::Setup,
    );
    assert_eq!(optional.watch_ignored_option.len(), 1, "{api} optional call");
    let wrapped = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; ({api} as any)(() => {{}}, {{ once: true }});"),
      ScriptKind::Setup,
    );
    assert_eq!(wrapped.watch_ignored_option.len(), 1, "{api} ts wrapper");
  }
}

#[test]
fn source_contracts_effect_family_namespace_aliases_and_unsupported_match_forced_full() {
  let namespace_one = assert_gated_matches_forced(
    "import * as Vue from 'vue'; Vue.watchEffect(() => {});",
    ScriptKind::Setup,
  );
  assert!(namespace_one.is_empty(), "{namespace_one:?}");
  let namespace_two = assert_gated_matches_forced(
    "import * as Vue from 'vue'; Vue.watchPostEffect(() => {}, { deep: true });",
    ScriptKind::Setup,
  );
  assert_eq!(namespace_two.watch_ignored_option.len(), 1, "{namespace_two:?}");
  let namespace_sync = assert_gated_matches_forced(
    "import * as Vue from 'vue'; Vue.watchSyncEffect(() => {}, { once: true });",
    ScriptKind::Setup,
  );
  assert_eq!(namespace_sync.watch_ignored_option.len(), 1, "{namespace_sync:?}");
  let binding = assert_gated_matches_forced(
    "import { watchEffect } from 'vue'; const fx = watchEffect; fx(() => {}, { once: true });",
    ScriptKind::Setup,
  );
  assert!(binding.is_empty(), "imported binding is not followed as a call: {binding:?}");
  assert_bypass(
    "import { watchEffect } from 'vue'; function inner() { const watchEffect = (_a: unknown, _b: unknown) => {}; watchEffect(() => {}, { once: true }); }",
    ScriptKind::Setup,
  );
  let dynamic = assert_gated_matches_forced(
    "import { watchEffect } from 'vue'; watchEffect.call(null, () => {}, { once: true });",
    ScriptKind::Setup,
  );
  assert!(dynamic.is_empty(), "dynamic .call stays unproven: {dynamic:?}");
  assert_bypass(
    "import { type watchEffect } from 'vue'; const watchEffect = (_a: unknown, _b: unknown) => {}; watchEffect(() => {}, { once: true });",
    ScriptKind::Setup,
  );
}

#[test]
fn source_contracts_mixed_and_ordinary_watch_keep_full_index() {
  let mixed = assert_gated_matches_forced(
    "import { ref, watch, watchEffect } from 'vue'; const n = ref(0); watch(n, (v) => v, { equals: () => true }); watchEffect(() => {});",
    ScriptKind::Setup,
  );
  assert_eq!(mixed.watch_ignored_option.len(), 1, "{mixed:?}");
  let ordinary = assert_gated_matches_forced(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n.value as number, () => {});",
    ScriptKind::Setup,
  );
  assert_eq!(ordinary.watch_unwrapped_source.len(), 1, "{ordinary:?}");
  let unused_watch = assert_gated_matches_forced(
    "import { watch, watchEffect } from 'vue'; watchEffect(() => {});",
    ScriptKind::Setup,
  );
  assert!(unused_watch.is_empty(), "{unused_watch:?}");
}

#[test]
fn source_contracts_recommended_invalid_fixture_bypasses_indexes() {
  let sfc = include_str!("../../../fixtures/rules/recommended/invalid.vue");
  let source = script_setup_from_sfc(sfc);
  assert!(
    source.contains("import { ref, watchEffect } from 'vue'"),
    "committed recommended-invalid fixture must import ref/watchEffect: {source}"
  );
  assert_eq!(
    source.matches("watchEffect(").count(),
    3,
    "committed fixture must keep three watchEffect calls: {source}"
  );
  assert_bypass(source, ScriptKind::Setup);
  let (facts, stats) = contract_collect(source, ScriptKind::Setup, false);
  let (forced, forced_stats) = contract_collect(source, ScriptKind::Setup, true);
  assert!(facts.is_empty(), "{facts:?}");
  assert_eq!(facts, forced);
  assert!(stats.is_import_preflight_only(), "gated={stats:?}");
  assert_eq!(stats.owners, 0, "{stats:?}");
  assert!(stats.queries >= 5, "two import-map entries plus three argument candidates: {stats:?}");
  assert!(
    stats.nodes > semantic_node_count(source),
    "ancestor hops must add nodes beyond the import pass: {stats:?}"
  );
  assert!(forced_stats.owners > 0, "{forced_stats:?}");
  assert!(
    stats.work() < forced_stats.work(),
    "bypass work {} must be below forced-full {}",
    stats.work(),
    forced_stats.work()
  );
}

#[test]
fn source_contracts_effect_only_one_arg_work_stays_preflight_while_forced_grows() {
  let mut previous: Option<(u64, u64, u64, u64)> = None;
  for size in [32_u64, 64, 128] {
    let source = effect_only_source("watchEffect", size, None);
    let (facts, stats) = contract_collect(&source, ScriptKind::Setup, false);
    let (forced, forced_stats) = contract_collect(&source, ScriptKind::Setup, true);
    assert!(facts.is_empty(), "{facts:?}");
    assert_eq!(facts, forced);
    assert!(stats.is_import_preflight_only(), "size={size} gated={stats:?}");
    assert_eq!(stats.owners, 0, "bypass must not build owners: {stats:?}");
    assert!(
      stats.queries > size,
      "one import-map entry plus {size} argument candidates: {stats:?}"
    );
    assert!(forced_stats.owners > 0, "size={size} forced={forced_stats:?}");
    assert!(
      stats.work() < forced_stats.work(),
      "size={size} gated {} vs forced {}",
      stats.work(),
      forced_stats.work()
    );
    if let Some((prev_size, prev_gated, prev_forced, prev_queries)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        forced_stats.work() > prev_forced,
        "forced-full work must grow from {prev_forced} to {} on {prev_size}->{size}",
        forced_stats.work()
      );
      assert!(
        stats.work() > prev_gated,
        "gated preflight work must grow from {prev_gated} to {} on {prev_size}->{size}",
        stats.work()
      );
      assert!(
        stats.queries > prev_queries,
        "argument-candidate queries must grow from {prev_queries} to {} on {prev_size}->{size}",
        stats.queries
      );
      assert!(
        stats.work().saturating_mul(10) < prev_gated.saturating_mul(30),
        "gated preflight work grew from {prev_gated} to {} on {prev_size}->{size}",
        stats.work()
      );
    }
    previous = Some((size, stats.work(), forced_stats.work(), stats.queries));
  }
  let emitting = effect_only_source("watchSyncEffect", 16, Some("{ once: true }"));
  let facts = assert_gated_matches_forced(&emitting, ScriptKind::Setup);
  assert_eq!(facts.watch_ignored_option.len(), 16, "{facts:?}");
}

#[test]
fn source_contracts_effect_wrapper_depth_counts_preflight_work() {
  let mut previous: Option<(u64, u64, u64)> = None;
  for depth in [0_u64, 8, 16, 32] {
    let source = effect_wrapper_source(depth);
    let (facts, stats) = contract_collect(&source, ScriptKind::Setup, false);
    let nodes = semantic_node_count(&source);
    assert!(facts.is_empty(), "depth={depth} {facts:?}");
    assert!(stats.is_import_preflight_only(), "depth={depth} {stats:?}");
    assert_eq!(stats.owners, 0, "depth={depth} {stats:?}");
    assert!(
      stats.work() > nodes,
      "depth={depth} counted work {} must include eligibility traversal beyond {nodes} import-pass nodes: {stats:?}",
      stats.work()
    );
    let extra = stats.work().saturating_sub(nodes);
    if let Some((prev_depth, prev_work, prev_extra)) = previous {
      assert!(
        stats.work() > prev_work,
        "wrapper-depth work must grow from {prev_work} to {} on {prev_depth}->{depth}: {stats:?}",
        stats.work()
      );
      assert!(
        extra > prev_extra,
        "wrapper ancestor/peel extra work must grow from {prev_extra} to {extra} on {prev_depth}->{depth}: {stats:?}"
      );
      if prev_depth > 0 {
        assert_eq!(depth, prev_depth * 2, "wrapper depths must double after the empty wrap");
        assert!(
          stats.work().saturating_mul(10) < prev_work.saturating_mul(30),
          "wrapper-depth work grew from {prev_work} to {} on {prev_depth}->{depth}",
          stats.work()
        );
      }
    }
    previous = Some((depth, stats.work(), extra));
  }
}

#[test]
fn source_contracts_effect_candidate_width_counts_preflight_work() {
  let mut previous_calls: Option<(u64, u64, u64)> = None;
  for width in [8_u64, 16, 32] {
    let source = effect_only_source("watchEffect", width, None);
    let (facts, stats) = contract_collect(&source, ScriptKind::Setup, false);
    assert!(facts.is_empty(), "calls={width} {facts:?}");
    assert!(stats.is_import_preflight_only(), "calls={width} {stats:?}");
    assert_eq!(stats.owners, 0, "calls={width} {stats:?}");
    assert!(
      stats.queries > width,
      "one import-map entry plus {width} remaining argument candidates: {stats:?}"
    );
    assert!(stats.references > width, "one specifier plus {width} resolved references: {stats:?}");
    if let Some((prev_width, prev_work, prev_queries)) = previous_calls {
      assert_eq!(width, prev_width * 2, "call widths must double");
      assert!(
        stats.work() > prev_work,
        "call-width work must grow from {prev_work} to {} on {prev_width}->{width}",
        stats.work()
      );
      assert!(
        stats.queries > prev_queries,
        "call-width queries must grow from {prev_queries} to {} on {prev_width}->{width}",
        stats.queries
      );
      assert!(
        stats.work().saturating_mul(10) < prev_work.saturating_mul(30),
        "call-width work grew from {prev_work} to {} on {prev_width}->{width}",
        stats.work()
      );
    }
    previous_calls = Some((width, stats.work(), stats.queries));
  }

  let mut previous_imports: Option<(usize, u64, u64)> = None;
  for width in [2_usize, 4, 8] {
    let source = effect_import_width_source(width);
    let (facts, stats) = contract_collect(&source, ScriptKind::Setup, false);
    assert!(facts.is_empty(), "imports={width} {facts:?}");
    assert!(stats.is_import_preflight_only(), "imports={width} {stats:?}");
    assert_eq!(stats.owners, 0, "imports={width} {stats:?}");
    let width_u64 = u64::try_from(width).unwrap_or(u64::MAX);
    assert!(
      stats.queries > width_u64,
      "{width} import-map entries plus one argument candidate: {stats:?}"
    );
    if let Some((prev_width, prev_work, prev_queries)) = previous_imports {
      assert_eq!(width, prev_width * 2, "import widths must double");
      assert!(
        stats.queries > prev_queries,
        "import-map queries must grow from {prev_queries} to {} on {prev_width}->{width}",
        stats.queries
      );
      assert!(
        stats.work() >= prev_work,
        "import-map work must not shrink from {prev_work} to {} on {prev_width}->{width}",
        stats.work()
      );
      assert!(
        stats.work().saturating_mul(10) < prev_work.saturating_mul(30).max(10),
        "import-map work grew from {prev_work} to {} on {prev_width}->{width}",
        stats.work()
      );
    }
    previous_imports = Some((width, stats.work(), stats.queries));
  }
}

#[test]
fn collection_lookup_emits_raw_proxy_map_key_and_keyed_foreach() {
  let (mismatch, _) = contract_stats(
    "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count;",
  );
  assert_eq!(mismatch.raw_proxy_map_key.len(), 1, "{mismatch:?}");
  let (same, _) = contract_stats(
    "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(raw).count;",
  );
  assert!(same.raw_proxy_map_key.is_empty(), "{same:?}");
  let (normalized, _) = contract_stats(
    "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = reactive(new Map([[raw, { count: 1 }]])); void map.get(proxy).count;",
  );
  assert!(normalized.raw_proxy_map_key.is_empty(), "{normalized:?}");
  let (keyed, _) = contract_stats(
    "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 1], ['other', 2]])); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;",
  );
  assert_eq!(keyed.keyed_map_dependency.len(), 1, "{keyed:?}");
  let (get, _) = contract_stats(
    "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 1]])); const selected = computed(() => keyed.get('selected')); void selected;",
  );
  assert!(get.keyed_map_dependency.is_empty(), "{get:?}");
}

#[test]
#[expect(clippy::panic, reason = "missing TypeScript demand span must fail the regression")]
fn collection_lookup_ts_unicode_crlf_and_unknown_controls() {
  let ts = include_str!("../../../fixtures/rules/no-raw-proxy-map-key/invalid/basic.ts");
  let (facts, _) = contract_stats(ts);
  assert_eq!(facts.raw_proxy_map_key.len(), 1, "{facts:?}");
  let Some(site) = facts.raw_proxy_map_key.first() else {
    panic!("ts demand");
  };
  let ts_demand = "map.get(proxy).count";
  let Some(ts_offset) = ts.find(ts_demand) else {
    panic!("ts demand text");
  };
  assert_eq!(site.demand_span.offset, ts_offset, "{site:?}");
  assert_eq!(site.demand_span.length, ts_demand.len(), "{site:?}");
  assert_eq!(ts.get(ts_offset..ts_offset + ts_demand.len()), Some(ts_demand));
  let crlf = "import { reactive } from 'vue';\r\nconst raw = {};\r\nconst proxy = reactive(raw);\r\nconst map = new Map([[raw, { count: 1 }]]);\r\nvoid map.get(proxy).count;\r\n";
  assert!(crlf.contains("\r\n"), "fixture must be true CRLF");
  let (crlf_facts, _) = contract_stats(crlf);
  assert_eq!(crlf_facts.raw_proxy_map_key.len(), 1, "{crlf_facts:?}");
  let Some(crlf_site) = crlf_facts.raw_proxy_map_key.first() else {
    panic!("crlf demand");
  };
  let crlf_demand = "map.get(proxy).count";
  let Some(crlf_offset) = crlf.find(crlf_demand) else {
    panic!("crlf demand text");
  };
  assert_eq!(crlf_site.demand_span.offset, crlf_offset, "{crlf_site:?}");
  assert_eq!(crlf_site.demand_span.length, crlf_demand.len(), "{crlf_site:?}");
  assert_eq!(crlf_site.demand_span.line, 5, "{crlf_site:?}");
  assert_eq!(
    crlf.as_bytes().get(crlf_offset..crlf_offset + crlf_demand.len()),
    Some(crlf_demand.as_bytes())
  );
  let crlf_bytes = include_bytes!("../../../fixtures/rules/no-raw-proxy-map-key/invalid/crlf.vue");
  assert!(
    crlf_bytes.windows(2).any(|window| window == b"\r\n"),
    "committed crlf.vue must keep CR bytes"
  );
  let unicode = "import { reactive } from 'vue'; const 原 = {}; const 代理 = reactive(原); const map = new Map([[原, { count: 1 }]]); void map.get(代理).count;";
  let (unicode_facts, _) = contract_stats(unicode);
  assert_eq!(unicode_facts.raw_proxy_map_key.len(), 1, "{unicode_facts:?}");
  let Some(unicode_site) = unicode_facts.raw_proxy_map_key.first() else {
    panic!("unicode demand");
  };
  let unicode_demand = "map.get(代理).count";
  let Some(unicode_offset) = unicode.find(unicode_demand) else {
    panic!("unicode demand text");
  };
  assert_eq!(unicode_site.demand_span.offset, unicode_offset, "{unicode_site:?}");
  assert_eq!(unicode_site.demand_span.length, unicode_demand.len(), "{unicode_site:?}");
  let demi = "import { reactive } from 'vue-demi'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count;";
  let (demi_facts, _) = contract_stats(demi);
  assert!(demi_facts.raw_proxy_map_key.is_empty(), "{demi_facts:?}");
  let auto = "import { reactive } from '#imports'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count;";
  let (auto_facts, _) = contract_stats(auto);
  assert!(auto_facts.raw_proxy_map_key.is_empty(), "{auto_facts:?}");
  let guarded = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy)?.count;";
  let (guarded_facts, _) = contract_stats(guarded);
  assert!(guarded_facts.raw_proxy_map_key.is_empty(), "{guarded_facts:?}");
}

#[test]
#[expect(clippy::panic, reason = "missing review-probe spans must fail the regression")]
fn collection_lookup_review_probes_stay_on_runtime_evidence() {
  let source5 = "import { reactive, watch } from 'vue'; const state = reactive({ child: 1 }); const helper = { get(target) { target.child = () => 7; } }; helper.get(state); watch(state.child, () => {});";
  let (source5_facts, _) = contract_stats(source5);
  assert!(
    source5_facts.watch_unwrapped_source.is_empty(),
    "helper.get first-arg spelling must not grant a source5 exemption: {source5_facts:?}"
  );
  let skip = "import { reactive } from 'vue'; const raw = { __v_skip: true }; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (skip_facts, _) = contract_stats(skip);
  assert!(skip_facts.raw_proxy_map_key.is_empty(), "{skip_facts:?}");
  let frozen = "import { reactive } from 'vue'; const raw = {}; Object.preventExtensions(raw); const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (frozen_facts, _) = contract_stats(frozen);
  assert!(frozen_facts.raw_proxy_map_key.is_empty(), "{frozen_facts:?}");
  let unknown = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const extra = ((value) => value)(proxy); const map = new Map([[raw, { count: 1 }], [extra, { count: 7 }]]); void map.get(proxy).count;";
  let (unknown_facts, _) = contract_stats(unknown);
  assert!(unknown_facts.raw_proxy_map_key.is_empty(), "{unknown_facts:?}");
  let dead = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); false && map.delete(proxy); void map.get(proxy).count;";
  let (dead_facts, _) = contract_stats(dead);
  assert!(dead_facts.raw_proxy_map_key.is_empty(), "{dead_facts:?}");
  let late = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); function read() { return map.get(proxy).count; } map.set(proxy, { count: 7 }); void read();";
  let (late_facts, _) = contract_stats(late);
  assert!(late_facts.raw_proxy_map_key.is_empty(), "{late_facts:?}");
  let property = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const answer = { undefined: 7 }; void answer[map.get(proxy)];";
  let (property_facts, _) = contract_stats(property);
  assert!(property_facts.raw_proxy_map_key.is_empty(), "{property_facts:?}");
  let proto = "import { reactive, computed } from 'vue'; Map.prototype.get = function get(key) { return key; }; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count; const keyed = reactive(new Map([['selected', 1]])); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (proto_facts, _) = contract_stats(proto);
  assert!(proto_facts.raw_proxy_map_key.is_empty(), "{proto_facts:?}");
  assert!(proto_facts.keyed_map_dependency.is_empty(), "{proto_facts:?}");
  let global = "import { computed, reactive } from 'vue'; globalThis.Map = class extends Map {}; const keyed = reactive(new Map([['selected', 1]])); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (global_facts, _) = contract_stats(global);
  assert!(global_facts.keyed_map_dependency.is_empty(), "{global_facts:?}");
  let generator = "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 7]])); const selected = computed(() => { let value; keyed.forEach(function* (entry, key) { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (generator_facts, _) = contract_stats(generator);
  assert!(generator_facts.keyed_map_dependency.is_empty(), "{generator_facts:?}");
  let deferred = "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 7]])); const selected = computed(() => { let value; queueMicrotask(() => keyed.forEach((entry, key) => { if (key === 'selected') value = entry })); return value; }); void selected;";
  let (deferred_facts, _) = contract_stats(deferred);
  assert!(deferred_facts.keyed_map_dependency.is_empty(), "{deferred_facts:?}");
  let wrappers = "import { reactive, shallowReactive } from 'vue'; const raw = {}; const first = reactive(raw); const second = shallowReactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(first).count;";
  let (wrapper_facts, _) = contract_stats(wrappers);
  assert_eq!(wrapper_facts.raw_proxy_map_key.len(), 1, "{wrapper_facts:?}");
  let Some(wrapper_site) = wrapper_facts.raw_proxy_map_key.first() else {
    return;
  };
  let first_wrapper = "reactive(raw)";
  let Some(wrapper_offset) = wrappers.find(first_wrapper) else {
    panic!("first wrapper");
  };
  assert_eq!(wrapper_site.wrapper_span.offset, wrapper_offset, "{wrapper_site:?}");
  assert_eq!(wrapper_site.wrapper_span.length, first_wrapper.len(), "{wrapper_site:?}");
}

#[test]
fn collection_lookup_growth_stays_subquadratic_and_wide_negatives_stay_quiet() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { computed, reactive } from 'vue';");
    for index in 0..size {
      source.push_str("const raw");
      source.push_str(&index.to_string());
      source.push_str(" = {}; const proxy");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(raw");
      source.push_str(&index.to_string());
      source.push_str("); const map");
      source.push_str(&index.to_string());
      source.push_str(" = new Map([[raw");
      source.push_str(&index.to_string());
      source.push_str(", { count: 1 }]]); void map");
      source.push_str(&index.to_string());
      source.push_str(".get(proxy");
      source.push_str(&index.to_string());
      source.push_str(").count;");
      source.push_str("const keyed");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(new Map([['selected', 1], ['other', ");
      source.push_str(&index.to_string());
      source.push_str("]])); const selected");
      source.push_str(&index.to_string());
      source.push_str(" = computed(() => { let value; keyed");
      source.push_str(&index.to_string());
      source.push_str(".forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected");
      source.push_str(&index.to_string());
      source.push(';');
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.raw_proxy_map_key.len(), expected, "{contracts:?}");
    assert_eq!(contracts.keyed_map_dependency.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "collection-lookup work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
  let mut negative = String::from("import { computed, reactive } from 'vue';");
  for index in 0..64 {
    negative.push_str("const n");
    negative.push_str(&index.to_string());
    negative.push_str(" = reactive(new Map([['k', ");
    negative.push_str(&index.to_string());
    negative.push_str("]])); void n");
    negative.push_str(&index.to_string());
    negative.push_str(".get('k'); const c");
    negative.push_str(&index.to_string());
    negative.push_str(" = computed(() => n");
    negative.push_str(&index.to_string());
    negative.push_str(".get('k')); void c");
    negative.push_str(&index.to_string());
    negative.push(';');
  }
  let (quiet, _) = contract_stats(&negative);
  assert!(quiet.raw_proxy_map_key.is_empty(), "{quiet:?}");
  assert!(quiet.keyed_map_dependency.is_empty(), "{quiet:?}");
  let nested = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(reactive(reactive(reactive(reactive(reactive(reactive(reactive(reactive(raw))))))))); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count;";
  let (exhausted, _) = contract_stats(nested);
  assert!(
    exhausted.raw_proxy_map_key.is_empty(),
    "MAX_DEPTH exhaustion must stay unknown: {exhausted:?}"
  );
}

#[test]
fn collection_lookup_growth_shared_root_wide_initializer_and_negatives() {
  let mut previous_shared: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]);",
    );
    for index in 0..size {
      source.push_str("void map.get(proxy).count; const _r");
      source.push_str(&index.to_string());
      source.push_str(" = 0;");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(
      contracts.raw_proxy_map_key.len(),
      expected,
      "shared-root reads {size}: {contracts:?}"
    );
    assert!(work > 0, "shared-root work must count inner visits: {work}");
    if let Some((prev_size, prev_work)) = previous_shared {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-root work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous_shared = Some((size, work));
  }
  let mut previous_wide: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([",
    );
    for index in 0..size {
      source.push_str("['k");
      source.push_str(&index.to_string());
      source.push_str("', ");
      source.push_str(&index.to_string());
      source.push_str("], ");
    }
    source.push_str("[raw, { count: 1 }]]); void map.get(proxy).count;");
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.raw_proxy_map_key.len(), 1, "wide initializer {size}: {contracts:?}");
    assert!(work > 0, "wide initializer work must count entries: {work}");
    if let Some((prev_size, prev_work)) = previous_wide {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "wide initializer work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous_wide = Some((size, work));
  }
  let mut negative = String::from("import { computed, reactive } from 'vue';");
  for index in 0..64 {
    negative.push_str("const raw");
    negative.push_str(&index.to_string());
    negative.push_str(" = { __v_skip: true }; const proxy");
    negative.push_str(&index.to_string());
    negative.push_str(" = reactive(raw");
    negative.push_str(&index.to_string());
    negative.push_str("); const map");
    negative.push_str(&index.to_string());
    negative.push_str(" = new Map([[raw");
    negative.push_str(&index.to_string());
    negative.push_str(", { count: 1 }]]); void map");
    negative.push_str(&index.to_string());
    negative.push_str(".get(proxy");
    negative.push_str(&index.to_string());
    negative.push_str(").count;");
  }
  let (quiet, work) = contract_stats(&negative);
  assert!(quiet.raw_proxy_map_key.is_empty(), "{quiet:?}");
  assert!(work > 0, "negative skip-marker work must still count: {work}");
}

#[test]
fn collection_lookup_second_review_boundaries_stay_quiet_or_report() {
  let helper = "import { reactive } from 'vue'; const helper = { get(target) { Object.preventExtensions(target) } }; const raw = {}; helper.get(raw); const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (helper_facts, _) = contract_stats(helper);
  assert!(helper_facts.raw_proxy_map_key.is_empty(), "{helper_facts:?}");
  let alias_helper = "import { reactive } from 'vue'; const helper = { get(target) { Object.preventExtensions(target) } }; const api = helper; const raw = {}; api.get(raw); const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (alias_helper_facts, _) = contract_stats(alias_helper);
  assert!(alias_helper_facts.raw_proxy_map_key.is_empty(), "{alias_helper_facts:?}");
  let native_key = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map(); map.set(raw, { count: 1 }); void map.get(proxy).count;";
  let (native_key_facts, _) = contract_stats(native_key);
  assert_eq!(native_key_facts.raw_proxy_map_key.len(), 1, "{native_key_facts:?}");
  let repeated = "import { reactive } from 'vue'; const raw = {}; const first = reactive(raw); const second = reactive(raw); const map = new Map([[raw, { count: 1 }], [first, { count: 7 }]]); void map.get(second).count;";
  let (repeated_facts, _) = contract_stats(repeated);
  assert!(repeated_facts.raw_proxy_map_key.is_empty(), "{repeated_facts:?}");
  let repeated_only = "import { reactive } from 'vue'; const raw = {}; const first = reactive(raw); const second = reactive(raw); const map = new Map([[first, { count: 7 }]]); void map.get(second).count;";
  let (repeated_only_facts, _) = contract_stats(repeated_only);
  assert!(repeated_only_facts.raw_proxy_map_key.is_empty(), "{repeated_only_facts:?}");
  let repeated_shallow = "import { shallowReactive } from 'vue'; const raw = {}; const first = shallowReactive(raw); const second = shallowReactive(raw); const map = new Map([[first, { count: 7 }]]); void map.get(second).count;";
  let (repeated_shallow_facts, _) = contract_stats(repeated_shallow);
  assert!(repeated_shallow_facts.raw_proxy_map_key.is_empty(), "{repeated_shallow_facts:?}");
  let alias_wrap = "import { reactive } from 'vue'; const raw = {}; const first = reactive(raw); const alias = first; const map = new Map([[raw, { count: 1 }], [first, { count: 7 }]]); void map.get(alias).count;";
  let (alias_wrap_facts, _) = contract_stats(alias_wrap);
  assert!(alias_wrap_facts.raw_proxy_map_key.is_empty(), "{alias_wrap_facts:?}");
  let inline_same = "import { reactive } from 'vue'; const raw = {}; const first = reactive(raw); const map = new Map([[first, { count: 7 }]]); void map.get(reactive(raw)).count;";
  let (inline_same_facts, _) = contract_stats(inline_same);
  assert!(inline_same_facts.raw_proxy_map_key.is_empty(), "{inline_same_facts:?}");
  let distinct = "import { reactive, shallowReactive } from 'vue'; const raw = {}; const deep = reactive(raw); const shallow = shallowReactive(raw); const map = new Map([[raw, { count: 1 }], [deep, { count: 7 }]]); void map.get(shallow).count;";
  let (distinct_facts, _) = contract_stats(distinct);
  assert_eq!(distinct_facts.raw_proxy_map_key.len(), 1, "{distinct_facts:?}");
  let wrappers = "import { reactive, shallowReactive } from 'vue'; const raw = {}; const first = reactive(raw); const second = shallowReactive(raw); const map = new Map([[raw, { count: 1 }]]); void second; void map.get(first).count;";
  let (wrapper_facts, _) = contract_stats(wrappers);
  assert_eq!(wrapper_facts.raw_proxy_map_key.len(), 1, "{wrapper_facts:?}");
  let zero_delete = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); 0 ?? map.delete(proxy); void map.get(proxy).count;";
  let (zero_delete_facts, _) = contract_stats(zero_delete);
  assert!(zero_delete_facts.raw_proxy_map_key.is_empty(), "{zero_delete_facts:?}");
  let false_delete = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); false ?? map.delete(proxy); void map.get(proxy).count;";
  let (false_delete_facts, _) = contract_stats(false_delete);
  assert!(false_delete_facts.raw_proxy_map_key.is_empty(), "{false_delete_facts:?}");
  let empty_delete = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); '' ?? map.delete(proxy); void map.get(proxy).count;";
  let (empty_delete_facts, _) = contract_stats(empty_delete);
  assert!(empty_delete_facts.raw_proxy_map_key.is_empty(), "{empty_delete_facts:?}");
  let null_delete = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); null ?? map.delete(proxy); void map.get(proxy).count;";
  let (null_delete_facts, _) = contract_stats(null_delete);
  assert_eq!(null_delete_facts.raw_proxy_map_key.len(), 1, "{null_delete_facts:?}");
  let undef_delete = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); undefined ?? map.delete(proxy); void map.get(proxy).count;";
  let (undef_delete_facts, _) = contract_stats(undef_delete);
  assert_eq!(undef_delete_facts.raw_proxy_map_key.len(), 1, "{undef_delete_facts:?}");
  let shadowed = "import { reactive } from 'vue'; const undefined = 0; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); undefined ?? map.delete(proxy); void map.get(proxy).count;";
  let (shadowed_facts, _) = contract_stats(shadowed);
  assert!(shadowed_facts.raw_proxy_map_key.is_empty(), "{shadowed_facts:?}");
  let unknown_delete = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); maybe ?? map.delete(proxy); void map.get(proxy).count;";
  let (unknown_delete_facts, _) = contract_stats(unknown_delete);
  assert!(unknown_delete_facts.raw_proxy_map_key.is_empty(), "{unknown_delete_facts:?}");
  let zero_get = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void (0 ?? map.get(proxy).count);";
  let (zero_get_facts, _) = contract_stats(zero_get);
  assert!(zero_get_facts.raw_proxy_map_key.is_empty(), "{zero_get_facts:?}");
  let false_and = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); false && map.delete(proxy); void map.get(proxy).count;";
  let (false_and_facts, _) = contract_stats(false_and);
  assert!(false_and_facts.raw_proxy_map_key.is_empty(), "{false_and_facts:?}");
  let own_get = "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 7]])); keyed.get = () => 9; const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (own_get_facts, _) = contract_stats(own_get);
  assert!(own_get_facts.keyed_map_dependency.is_empty(), "{own_get_facts:?}");
  let own_foreach = "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 7]])); keyed.forEach = () => {}; const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (own_foreach_facts, _) = contract_stats(own_foreach);
  assert!(own_foreach_facts.keyed_map_dependency.is_empty(), "{own_foreach_facts:?}");
  let alias_override = "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 7]])); const alias = keyed; alias.get = () => 9; const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (alias_override_facts, _) = contract_stats(alias_override);
  assert!(alias_override_facts.keyed_map_dependency.is_empty(), "{alias_override_facts:?}");
  let helper_escape = "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 7]])); const touch = (value) => value; touch(keyed); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (helper_escape_facts, _) = contract_stats(helper_escape);
  assert!(helper_escape_facts.keyed_map_dependency.is_empty(), "{helper_escape_facts:?}");
  let wrapped_ident = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (wrapped_ident_facts, _) = contract_stats(wrapped_ident);
  assert_eq!(wrapped_ident_facts.keyed_map_dependency.len(), 1, "{wrapped_ident_facts:?}");
  let ordinary_set = "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 1], ['other', 2]])); keyed.set('other', 3); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (ordinary_set_facts, _) = contract_stats(ordinary_set);
  assert_eq!(ordinary_set_facts.keyed_map_dependency.len(), 1, "{ordinary_set_facts:?}");
  let early_return = "import { reactive } from 'vue'; function read() { const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); return 7; void result.count; } read();";
  let (early_return_facts, _) = contract_stats(early_return);
  assert!(early_return_facts.raw_proxy_map_key.is_empty(), "{early_return_facts:?}");
  let early_throw = "import { reactive } from 'vue'; function read() { const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); throw 7; void result.count; }";
  let (early_throw_facts, _) = contract_stats(early_throw);
  assert!(early_throw_facts.raw_proxy_map_key.is_empty(), "{early_throw_facts:?}");
  let guarded_return = "import { reactive } from 'vue'; function read(flag: boolean) { const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); if (flag) return 7; void result.count; } read(true);";
  let (guarded_return_facts, _) = contract_stats(guarded_return);
  assert!(guarded_return_facts.raw_proxy_map_key.is_empty(), "{guarded_return_facts:?}");
  let chained_after = "import { reactive } from 'vue'; function read() { const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); return 7; void map.get(proxy).count; } read();";
  let (chained_after_facts, _) = contract_stats(chained_after);
  assert!(chained_after_facts.raw_proxy_map_key.is_empty(), "{chained_after_facts:?}");
  let await_demand = "import { reactive } from 'vue'; async function read() { const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); await Promise.resolve(); void result.count; }";
  let (await_demand_facts, _) = contract_stats(await_demand);
  assert!(await_demand_facts.raw_proxy_map_key.is_empty(), "{await_demand_facts:?}");
  let reachable = "import { reactive } from 'vue'; function read() { const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); void result.count; return 7; } read();";
  let (reachable_facts, _) = contract_stats(reachable);
  assert_eq!(reachable_facts.raw_proxy_map_key.len(), 1, "{reachable_facts:?}");
}

#[test]
fn collection_lookup_third_review_boundaries_stay_quiet() {
  let native_override = "import { reactive } from 'vue'; const helper = new Map(); helper.get = target => Object.preventExtensions(target); const raw = {}; helper.get(raw); const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (native_override_facts, _) = contract_stats(native_override);
  assert!(native_override_facts.raw_proxy_map_key.is_empty(), "{native_override_facts:?}");
  let later_override = "import { reactive } from 'vue'; const helper = new Map(); const raw = {}; helper.get(raw); helper.get = target => Object.preventExtensions(target); const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (later_override_facts, _) = contract_stats(later_override);
  assert!(later_override_facts.raw_proxy_map_key.is_empty(), "{later_override_facts:?}");
  let alias_override = "import { reactive } from 'vue'; const helper = new Map(); const api = helper; api.get = target => Object.preventExtensions(target); const raw = {}; helper.get(raw); const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (alias_override_facts, _) = contract_stats(alias_override);
  assert!(alias_override_facts.raw_proxy_map_key.is_empty(), "{alias_override_facts:?}");
  let wrapped_helper = "import { computed, reactive } from 'vue'; function alter(target) { target.get = () => 9 } const inner = new Map([['selected', 7]]); alter(inner); const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (wrapped_helper_facts, _) = contract_stats(wrapped_helper);
  assert!(wrapped_helper_facts.keyed_map_dependency.is_empty(), "{wrapped_helper_facts:?}");
  let wrapped_computed = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); inner['get'] = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (wrapped_computed_facts, _) = contract_stats(wrapped_computed);
  assert!(wrapped_computed_facts.keyed_map_dependency.is_empty(), "{wrapped_computed_facts:?}");
  let wrapped_ident = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (wrapped_ident_facts, _) = contract_stats(wrapped_ident);
  assert_eq!(wrapped_ident_facts.keyed_map_dependency.len(), 1, "{wrapped_ident_facts:?}");
  let return_before = "import { reactive } from 'vue'; function read() { return 7; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count; } read();";
  let (return_before_facts, _) = contract_stats(return_before);
  assert!(return_before_facts.raw_proxy_map_key.is_empty(), "{return_before_facts:?}");
  let throw_before = "import { reactive } from 'vue'; function read() { throw 7; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count; }";
  let (throw_before_facts, _) = contract_stats(throw_before);
  assert!(throw_before_facts.raw_proxy_map_key.is_empty(), "{throw_before_facts:?}");
  let guarded_before = "import { reactive } from 'vue'; function read(flag: boolean) { if (flag) return 7; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count; } read(true);";
  let (guarded_before_facts, _) = contract_stats(guarded_before);
  assert!(guarded_before_facts.raw_proxy_map_key.is_empty(), "{guarded_before_facts:?}");
  let skipped_and = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); false && result.count;";
  let (skipped_and_facts, _) = contract_stats(skipped_and);
  assert!(skipped_and_facts.raw_proxy_map_key.is_empty(), "{skipped_and_facts:?}");
  let skipped_or = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); true || result.count;";
  let (skipped_or_facts, _) = contract_stats(skipped_or);
  assert!(skipped_or_facts.raw_proxy_map_key.is_empty(), "{skipped_or_facts:?}");
  let skipped_nullish = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); 0 ?? result.count;";
  let (skipped_nullish_facts, _) = contract_stats(skipped_nullish);
  assert!(skipped_nullish_facts.raw_proxy_map_key.is_empty(), "{skipped_nullish_facts:?}");
  let tagged = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); tag`${map.get(proxy).count}`;";
  let (tagged_facts, _) = contract_stats(tagged);
  assert!(tagged_facts.raw_proxy_map_key.is_empty(), "{tagged_facts:?}");
  let native_set = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map(); map.set(raw, { count: 1 }); void map.get(proxy).count;";
  let (native_set_facts, _) = contract_stats(native_set);
  assert_eq!(native_set_facts.raw_proxy_map_key.len(), 1, "{native_set_facts:?}");
}

#[test]
fn collection_lookup_fourth_review_boundaries_stay_quiet_or_report() {
  let and_alias = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); let chosen = false; chosen &&= result.count;";
  let (and_alias_facts, _) = contract_stats(and_alias);
  assert!(and_alias_facts.raw_proxy_map_key.is_empty(), "{and_alias_facts:?}");
  let or_alias = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); let chosen = true; chosen ||= result.count;";
  let (or_alias_facts, _) = contract_stats(or_alias);
  assert!(or_alias_facts.raw_proxy_map_key.is_empty(), "{or_alias_facts:?}");
  let nullish_alias = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); let chosen = 0; chosen ??= result.count;";
  let (nullish_alias_facts, _) = contract_stats(nullish_alias);
  assert!(nullish_alias_facts.raw_proxy_map_key.is_empty(), "{nullish_alias_facts:?}");
  let and_direct = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); let chosen = false; chosen &&= map.get(proxy).count;";
  let (and_direct_facts, _) = contract_stats(and_direct);
  assert!(and_direct_facts.raw_proxy_map_key.is_empty(), "{and_direct_facts:?}");
  let skipped_mut = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); let chosen = false; chosen &&= map.delete(proxy); void map.get(proxy).count;";
  let (skipped_mut_facts, _) = contract_stats(skipped_mut);
  assert!(skipped_mut_facts.raw_proxy_map_key.is_empty(), "{skipped_mut_facts:?}");
  let opt_call_alias = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); const sink = null; sink?.(result.count);";
  let (opt_call_alias_facts, _) = contract_stats(opt_call_alias);
  assert!(opt_call_alias_facts.raw_proxy_map_key.is_empty(), "{opt_call_alias_facts:?}");
  let opt_call_direct = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const sink = null; sink?.(map.get(proxy).count);";
  let (opt_call_direct_facts, _) = contract_stats(opt_call_direct);
  assert!(opt_call_direct_facts.raw_proxy_map_key.is_empty(), "{opt_call_direct_facts:?}");
  let opt_computed_alias = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); const sink = null; void sink?.[result.count];";
  let (opt_computed_alias_facts, _) = contract_stats(opt_computed_alias);
  assert!(opt_computed_alias_facts.raw_proxy_map_key.is_empty(), "{opt_computed_alias_facts:?}");
  let opt_computed_direct = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const sink = null; void sink?.[map.get(proxy).count];";
  let (opt_computed_direct_facts, _) = contract_stats(opt_computed_direct);
  assert!(opt_computed_direct_facts.raw_proxy_map_key.is_empty(), "{opt_computed_direct_facts:?}");
  let holder = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); const holder = { inner }; holder.inner.get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (holder_facts, _) = contract_stats(holder);
  assert!(holder_facts.keyed_map_dependency.is_empty(), "{holder_facts:?}");
  let array = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); const holder = [inner]; holder[0].get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (array_facts, _) = contract_stats(array);
  assert!(array_facts.keyed_map_dependency.is_empty(), "{array_facts:?}");
  let returned = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); function expose() { return inner; } expose().get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (returned_facts, _) = contract_stats(returned);
  assert!(returned_facts.keyed_map_dependency.is_empty(), "{returned_facts:?}");
  let wrapped_ident = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (wrapped_ident_facts, _) = contract_stats(wrapped_ident);
  assert_eq!(wrapped_ident_facts.keyed_map_dependency.len(), 1, "{wrapped_ident_facts:?}");
  let taken = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); true && result.count;";
  let (taken_facts, _) = contract_stats(taken);
  assert_eq!(taken_facts.raw_proxy_map_key.len(), 1, "{taken_facts:?}");
  let ordinary = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (ordinary_facts, _) = contract_stats(ordinary);
  assert_eq!(ordinary_facts.raw_proxy_map_key.len(), 1, "{ordinary_facts:?}");
  let native_set = "import { reactive } from 'vue'; const helper = new Map(); const raw = {}; helper.set(raw, 1); const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (native_set_facts, _) = contract_stats(native_set);
  assert_eq!(native_set_facts.raw_proxy_map_key.len(), 1, "{native_set_facts:?}");
}

#[test]
fn collection_lookup_fifth_review_chain_and_wrapper_propagation() {
  let inherited_call = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); const sink = null; sink?.accept(result.count);";
  let (inherited_call_facts, _) = contract_stats(inherited_call);
  assert!(inherited_call_facts.raw_proxy_map_key.is_empty(), "{inherited_call_facts:?}");
  let inherited_computed = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); const sink = null; void sink?.field[result.count];";
  let (inherited_computed_facts, _) = contract_stats(inherited_computed);
  assert!(inherited_computed_facts.raw_proxy_map_key.is_empty(), "{inherited_computed_facts:?}");
  let grouped = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); const sink = {}; (sink?.accept)(result.count);";
  let (grouped_facts, _) = contract_stats(grouped);
  assert_eq!(grouped_facts.raw_proxy_map_key.len(), 1, "{grouped_facts:?}");
  let alias_storage = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); const alias = inner; const holder = { alias }; holder.alias.get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (alias_storage_facts, _) = contract_stats(alias_storage);
  assert!(alias_storage_facts.keyed_map_dependency.is_empty(), "{alias_storage_facts:?}");
  let secondary_write = "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]); const alternate = shallowReactive(inner); alternate.get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (secondary_write_facts, _) = contract_stats(secondary_write);
  assert!(secondary_write_facts.keyed_map_dependency.is_empty(), "{secondary_write_facts:?}");
  let paired_positive = "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]); const alternate = shallowReactive(inner); const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected; void alternate;";
  let (paired_positive_facts, _) = contract_stats(paired_positive);
  assert_eq!(paired_positive_facts.keyed_map_dependency.len(), 1, "{paired_positive_facts:?}");
  let middle_chain = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); const sink = null; void sink?.accept(result.count).next;";
  let (middle_chain_facts, _) = contract_stats(middle_chain);
  assert!(middle_chain_facts.raw_proxy_map_key.is_empty(), "{middle_chain_facts:?}");
  let nested_write = "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]); const middle = reactive(inner); const alternate = shallowReactive(middle); alternate.get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (nested_write_facts, _) = contract_stats(nested_write);
  assert!(nested_write_facts.keyed_map_dependency.is_empty(), "{nested_write_facts:?}");
  let inline_write = "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]); shallowReactive(inner).get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (inline_write_facts, _) = contract_stats(inline_write);
  assert!(inline_write_facts.keyed_map_dependency.is_empty(), "{inline_write_facts:?}");
  let nested_inline = "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]); shallowReactive(reactive(inner)).get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (nested_inline_facts, _) = contract_stats(nested_inline);
  assert!(nested_inline_facts.keyed_map_dependency.is_empty(), "{nested_inline_facts:?}");
  let mut hop = String::from(
    "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]); const wrapper0 = reactive(inner);",
  );
  for index in 1..=10 {
    hop.push_str("const wrapper");
    hop.push_str(&index.to_string());
    hop.push_str(" = shallowReactive(wrapper");
    hop.push_str(&(index - 1).to_string());
    hop.push_str(");");
  }
  hop.push_str("wrapper10.get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;");
  let (hop_facts, _) = contract_stats(&hop);
  assert!(hop_facts.keyed_map_dependency.is_empty(), "{hop_facts:?}");
  let grouped_suffix = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); const sink = null; (sink?.accept)(result.count)?.next;";
  let (grouped_suffix_facts, _) = contract_stats(grouped_suffix);
  assert_eq!(grouped_suffix_facts.raw_proxy_map_key.len(), 1, "{grouped_suffix_facts:?}");
}

#[test]
fn collection_lookup_paired_wrappers_and_nested_chains_grow_linearly() {
  let mut previous_wrappers: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [8_u64, 16, 32] {
    let mut source = String::from(
      "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]);",
    );
    for index in 0..size {
      source.push_str("const w");
      source.push_str(&index.to_string());
      source.push_str(" = shallowReactive(inner); void w");
      source.push_str(&index.to_string());
      source.push(';');
    }
    source.push_str("const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;");
    let (facts, stats) = contract_full_stats(&source);
    assert_eq!(facts.keyed_map_dependency.len(), 1, "paired wrappers {size}");
    if let Some((prev_size, prev)) = previous_wrappers {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.queries <= prev.queries.saturating_mul(2).saturating_add(prev_size.saturating_mul(8)),
        "paired-wrapper queries {prev:?} -> {stats:?}"
      );
      assert_eq!(stats.key_copies, 0, "paired-wrapper copies {stats:?}");
    }
    previous_wrappers = Some((size, stats));
  }
  let mut previous_chain: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [8_u64, 16, 32] {
    let mut source = String::from(
      "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const sink = null; void sink",
    );
    for index in 0..size {
      source.push_str("?.n");
      source.push_str(&index.to_string());
    }
    source.push_str("?.[map.get(proxy).count];");
    let (facts, stats) = contract_full_stats(&source);
    assert!(facts.raw_proxy_map_key.is_empty(), "nested chain {size}");
    if let Some((prev_size, prev)) = previous_chain {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.queries <= prev.queries.saturating_mul(2).saturating_add(prev_size.saturating_mul(8)),
        "nested-chain queries {prev:?} -> {stats:?}"
      );
    }
    previous_chain = Some((size, stats));
  }
}

#[test]
fn collection_lookup_shared_allocation_wc_grows_linearly() {
  let mut previous: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [8_u64, 16, 32] {
    let mut source = String::from(
      "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]);",
    );
    for index in 0..size {
      source.push_str("const w");
      source.push_str(&index.to_string());
      source.push_str(" = shallowReactive(inner); void w");
      source.push_str(&index.to_string());
      source.push(';');
    }
    source.push_str("const keyed = reactive(inner);");
    for index in 0..size {
      source.push_str("const selected");
      source.push_str(&index.to_string());
      source.push_str(" = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected");
      source.push_str(&index.to_string());
      source.push(';');
    }
    let (facts, stats) = contract_full_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(facts.keyed_map_dependency.len(), expected, "W=C facts {size}");
    assert_eq!(stats.key_copies, 0, "W=C copies {stats:?}");
    if let Some((prev_size, prev)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.queries
          <= prev.queries.saturating_mul(2).saturating_add(prev_size.saturating_mul(16)),
        "W=C queries {prev:?} -> {stats:?}"
      );
      assert!(
        stats.queries
          < prev.queries.saturating_mul(size / prev_size).saturating_mul(size / prev_size),
        "W=C must not be quadratic {prev:?} -> {stats:?}"
      );
    }
    previous = Some((size, stats));
  }
}

#[test]
fn collection_lookup_nested_origin_depth_grows_linearly() {
  let mut previous: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [8_u64, 16, 32] {
    let mut source = String::from(
      "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]); const w0 = reactive(inner);",
    );
    for index in 1..size {
      source.push_str("const w");
      source.push_str(&index.to_string());
      source.push_str(" = shallowReactive(w");
      source.push_str(&(index - 1).to_string());
      source.push_str(");");
    }
    source.push('w');
    source.push_str(&(size - 1).to_string());
    source.push_str(".get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;");
    let (facts, stats) = contract_full_stats(&source);
    assert!(facts.keyed_map_dependency.is_empty(), "nested origin {size}");
    assert_eq!(stats.key_copies, 0, "nested origin copies {stats:?}");
    if let Some((prev_size, prev)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.queries
          <= prev.queries.saturating_mul(2).saturating_add(prev_size.saturating_mul(16)),
        "nested-origin queries {prev:?} -> {stats:?}"
      );
    }
    previous = Some((size, stats));
  }
}

#[test]
fn collection_lookup_replay_counts_constructor_mutations_and_lookups_once() {
  let mut previous_shared: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]);",
    );
    for index in 0..size {
      source.push_str("void map.get(proxy).count; const _r");
      source.push_str(&index.to_string());
      source.push_str(" = 0;");
    }
    let (contracts, stats) = contract_full_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.raw_proxy_map_key.len(), expected, "shared-root reads {size}");
    assert_eq!(
      stats.key_copies, 1,
      "shared-root copies stay on the one closed {{ count }} object {size}: {stats:?}"
    );
    if let Some((prev_size, prev)) = previous_shared {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.key_lookups <= prev.key_lookups.saturating_mul(2).saturating_add(prev_size),
        "shared-root identity lookups {prev:?} -> {stats:?}"
      );
      assert!(
        stats.object_entries <= prev.object_entries.saturating_add(size),
        "shared-root constructor visits {prev:?} -> {stats:?}"
      );
    }
    previous_shared = Some((size, stats));
  }
  let mut previous_wide: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([",
    );
    for index in 0..size {
      source.push_str("['k");
      source.push_str(&index.to_string());
      source.push_str("', ");
      source.push_str(&index.to_string());
      source.push_str("], ");
    }
    source.push_str("[raw, { count: 1 }]]);");
    for index in 0..size {
      source.push_str("void map.get(proxy).count; const _w");
      source.push_str(&index.to_string());
      source.push_str(" = 0;");
    }
    let (contracts, stats) = contract_full_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.raw_proxy_map_key.len(), expected, "wide+reads {size}");
    assert_eq!(
      stats.key_copies, 1,
      "wide+reads copies stay on the one closed {{ count }} object {size}: {stats:?}"
    );
    if let Some((prev_size, prev)) = previous_wide {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.key_lookups <= prev.key_lookups.saturating_mul(2).saturating_add(size),
        "wide+reads identity lookups {prev:?} -> {stats:?}"
      );
      assert!(
        stats.object_entries <= prev.object_entries.saturating_mul(2).saturating_add(size),
        "wide+reads constructor visits {prev:?} -> {stats:?}"
      );
    }
    previous_wide = Some((size, stats));
  }
  let mut previous_raw: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { reactive } from 'vue';");
    for index in 0..size {
      source.push_str("const raw");
      source.push_str(&index.to_string());
      source.push_str(" = {}; const proxy");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(raw");
      source.push_str(&index.to_string());
      source.push_str(");");
    }
    source.push_str("const map = new Map([");
    for index in 0..size {
      source.push_str("[raw");
      source.push_str(&index.to_string());
      source.push_str(", { count: 1 }], ");
    }
    source.push_str("]);");
    for index in 0..size {
      source.push_str("void map.get(proxy");
      source.push_str(&index.to_string());
      source.push_str(").count;");
    }
    let (contracts, stats) = contract_full_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.raw_proxy_map_key.len(), expected, "distinct raw keys {size}");
    assert_eq!(
      stats.key_copies, size,
      "distinct raw copies follow closed {{ count }} objects {size}: {stats:?}"
    );
    if let Some((prev_size, prev)) = previous_raw {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.key_lookups <= prev.key_lookups.saturating_mul(2).saturating_add(size),
        "distinct raw identity lookups {prev:?} -> {stats:?}"
      );
    }
    previous_raw = Some((size, stats));
  }
  let mut previous_mut: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map();",
    );
    for index in 0..size {
      source.push_str("map.set(raw, { count: ");
      source.push_str(&index.to_string());
      source.push_str(" });");
    }
    for _ in 0..size {
      source.push_str("void map.get(proxy).count;");
    }
    let (contracts, stats) = contract_full_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.raw_proxy_map_key.len(), expected, "mutation-heavy {size}");
    assert_eq!(
      stats.key_copies, size,
      "mutation-heavy copies follow closed {{ count }} objects {size}: {stats:?}"
    );
    if let Some((prev_size, prev)) = previous_mut {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.writes <= prev.writes.saturating_mul(2).saturating_add(size),
        "mutation visits {prev:?} -> {stats:?}"
      );
    }
    previous_mut = Some((size, stats));
  }
  let mut unknown = String::from(
    "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]);",
  );
  for index in 0..32 {
    unknown.push_str("map.set(maybe");
    unknown.push_str(&index.to_string());
    unknown.push_str(", 1); void map.get(proxy).count;");
  }
  let (quiet, stats) = contract_full_stats(&unknown);
  assert!(quiet.raw_proxy_map_key.is_empty(), "{quiet:?}");
  assert_eq!(
    stats.key_copies, 1,
    "unknown-key copies stay on the one closed {{ count }} object: {stats:?}"
  );
  assert!(stats.key_lookups > 0 && stats.writes > 0, "unknown-key work: {stats:?}");
}

#[test]
fn collection_lookup_replay_pins_exact_work_for_size_16() {
  let mut shared = String::from(
    "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]);",
  );
  for index in 0..16 {
    shared.push_str("void map.get(proxy).count; const _r");
    shared.push_str(&index.to_string());
    shared.push_str(" = 0;");
  }
  let (shared_facts, shared_stats) = contract_full_stats(&shared);
  assert_eq!(shared_facts.raw_proxy_map_key.len(), 16, "{shared_facts:?}");
  let mut wide = String::from(
    "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([",
  );
  for index in 0..16 {
    wide.push_str("['k");
    wide.push_str(&index.to_string());
    wide.push_str("', ");
    wide.push_str(&index.to_string());
    wide.push_str("], ");
  }
  wide.push_str("[raw, { count: 1 }]]);");
  for index in 0..16 {
    wide.push_str("void map.get(proxy).count; const _w");
    wide.push_str(&index.to_string());
    wide.push_str(" = 0;");
  }
  let (wide_facts, wide_stats) = contract_full_stats(&wide);
  assert_eq!(wide_facts.raw_proxy_map_key.len(), 16, "{wide_facts:?}");
  let mut distinct = String::from("import { reactive } from 'vue';");
  for index in 0..16 {
    distinct.push_str("const raw");
    distinct.push_str(&index.to_string());
    distinct.push_str(" = {}; const proxy");
    distinct.push_str(&index.to_string());
    distinct.push_str(" = reactive(raw");
    distinct.push_str(&index.to_string());
    distinct.push_str(");");
  }
  distinct.push_str("const map = new Map([");
  for index in 0..16 {
    distinct.push_str("[raw");
    distinct.push_str(&index.to_string());
    distinct.push_str(", { count: 1 }], ");
  }
  distinct.push_str("]);");
  for index in 0..16 {
    distinct.push_str("void map.get(proxy");
    distinct.push_str(&index.to_string());
    distinct.push_str(").count;");
  }
  let (distinct_facts, distinct_stats) = contract_full_stats(&distinct);
  assert_eq!(distinct_facts.raw_proxy_map_key.len(), 16, "{distinct_facts:?}");
  let mut mutated = String::from(
    "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map();",
  );
  for index in 0..16 {
    mutated.push_str("map.set(raw, { count: ");
    mutated.push_str(&index.to_string());
    mutated.push_str(" });");
  }
  for _ in 0..16 {
    mutated.push_str("void map.get(proxy).count;");
  }
  let (mutated_facts, mutated_stats) = contract_full_stats(&mutated);
  assert_eq!(mutated_facts.raw_proxy_map_key.len(), 16, "{mutated_facts:?}");
  assert_eq!(shared_stats.key_lookups, 33, "{shared_stats:?}");
  assert_eq!(shared_stats.writes, 0, "{shared_stats:?}");
  assert_eq!(shared_stats.key_copies, 1, "{shared_stats:?}");
  assert_eq!(wide_stats.key_lookups, 49, "{wide_stats:?}");
  assert_eq!(wide_stats.writes, 0, "{wide_stats:?}");
  assert_eq!(wide_stats.key_copies, 1, "{wide_stats:?}");
  assert_eq!(distinct_stats.key_lookups, 48, "{distinct_stats:?}");
  assert_eq!(distinct_stats.key_copies, 16, "{distinct_stats:?}");
  assert_eq!(mutated_stats.writes, 16, "{mutated_stats:?}");
  assert_eq!(mutated_stats.key_lookups, 48, "{mutated_stats:?}");
  assert_eq!(mutated_stats.key_copies, 16, "{mutated_stats:?}");
}

fn cleanup_identity_source(body: &str) -> String {
  format!("import {{ nextTick, onWatcherCleanup, ref, shallowRef, watch }} from 'vue';\n{body}")
}

fn cleanup_identity_visits(source: &str) -> (usize, super::lifetime::CollectStats) {
  let allocator = oxc_allocator::Allocator::default();
  let parsed = oxc_parser::Parser::new(&allocator, source, oxc_span::SourceType::ts()).parse();
  let semantic =
    oxc_semantic::SemanticBuilder::new().with_build_nodes(true).build(&parsed.program).semantic;
  let line_index = vue_vet_core::LineIndex::new(source);
  let (facts, stats) = super::lifetime::collect_with_visits(&semantic, &line_index, source, 0);
  (facts.watch_cleanup_current_sources.len(), stats)
}

fn assert_cleanup_identity_quiet(body: &str, label: &str) {
  let facts = analyze(&cleanup_identity_source(body), "ts");
  assert!(
    facts.lifetime.watch_cleanup_current_sources.is_empty(),
    "{label} must stay quiet: {:?}",
    facts.lifetime.watch_cleanup_current_sources
  );
}

#[test]
fn watch_cleanup_current_source_emits_for_reread_event_target() {
  let facts = analyze(
    &cleanup_identity_source(
      "const handler = () => {};\
       const source = ref(new EventTarget());\
       watch(source, (target, _prev, onCleanup) => {\
         target.addEventListener('click', handler);\
         onCleanup(() => { source.value.removeEventListener('click', handler); });\
       }, { immediate: true, flush: 'sync' });\
       source.value = new EventTarget();",
    ),
    "ts",
  );
  assert_eq!(
    facts.lifetime.watch_cleanup_current_sources.len(),
    1,
    "reread source.value must emit: {:?}",
    facts.lifetime.watch_cleanup_current_sources
  );
}

#[test]
fn watch_cleanup_current_source_shares_flush_and_cleanup_apis() {
  let facts = analyze(
    &cleanup_identity_source(
      "const handler = () => {};\
       const source = ref(new EventTarget());\
       watch(source, (target, _prev, onCleanup) => {\
         target.addEventListener('click', handler);\
         onCleanup(() => { source.value.removeEventListener('click', handler); });\
       }, { flush: 'sync' });\
       source.value = new EventTarget();\
       source.value = new EventTarget();\
       const other = ref(new EventTarget());\
       watch(other, (target) => {\
         target.addEventListener('click', handler);\
         onWatcherCleanup(() => { other.value.removeEventListener('click', handler); });\
       }, { immediate: true, flush: 'sync' });\
       other.value = new EventTarget();",
    ),
    "ts",
  );
  assert_eq!(
    facts.lifetime.watch_cleanup_current_sources.len(),
    2,
    "sync flush and onWatcherCleanup share the same fact: {:?}",
    facts.lifetime.watch_cleanup_current_sources
  );
}

#[test]
fn watch_cleanup_current_source_stays_quiet_for_safe_and_unknown() {
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const captured = ref(new EventTarget());\
     watch(captured, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { target.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     captured.value = new EventTarget();",
    "captured target",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const logged = ref(new EventTarget());\
     watch(logged, (target, _prev, onCleanup) => {\
       void target;\
       onCleanup(() => { console.log(logged.value); });\
     }, { immediate: true, flush: 'sync' });\
     logged.value = new EventTarget();",
    "logging source",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const writeback = ref(new EventTarget());\
     watch(writeback, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { writeback.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     writeback.value = writeback.value;",
    "same-target writeback",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const once = ref(new EventTarget());\
     watch(once, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler, { once: true });\
       onCleanup(() => { once.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     once.value = new EventTarget();",
    "listener once option",
  );
  assert_cleanup_identity_quiet(
    "class EventTarget {}\
     const handler = () => {};\
     const shadowed = ref(new EventTarget());\
     watch(shadowed, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { shadowed.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     shadowed.value = new EventTarget();",
    "shadowed EventTarget",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     });\
     source.value = new EventTarget();\
     source.value = new EventTarget();",
    "default pre coalesced assignments",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync', once: true });\
     source.value = new EventTarget();\
     stop();",
    "watch once option",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     stop();\
     source.value = new EventTarget();",
    "stopped before replacement",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     function unusedReplacement() {\
       source.value = new EventTarget();\
       source.value = new EventTarget();\
     }",
    "uninvoked replacement",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = initial;\
     source.value = initial;",
    "same-target alias writeback",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     initial.addEventListener = () => {};\
     const source = ref(initial);\
     watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();",
    "payload method mutation",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     function disableListeners(value) { value.addEventListener = () => {}; }\
     const source = ref(new EventTarget());\
     watch(source, (target, _prev, onCleanup) => {\
       disableListeners(target);\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();",
    "helper method escape",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => {\
         target.removeEventListener('click', handler);\
         source.value.removeEventListener('click', handler);\
       });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();",
    "captured release discharge",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     const options = { immediate: true, flush: 'sync' };\
     watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, options);\
     source.value = new EventTarget();",
    "unknown options object",
  );
}

#[test]
fn watch_cleanup_current_source_stays_quiet_for_second_review_safes() {
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { flush: 'sync' });\
     source.value = initial;\
     source.value = new EventTarget();\
     stop();",
    "same-value write before first sync change",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true });\
     source.value = new EventTarget();\
     source.value = initial;\
     await nextTick();\
     stop();",
    "immediate pre round-trip before nextTick",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     if (false) {\
       watch(source, (target, _prev, onCleanup) => {\
         target.addEventListener('click', handler);\
         onCleanup(() => { source.value.removeEventListener('click', handler); });\
       }, { immediate: true, flush: 'sync' });\
     }\
     source.value = new EventTarget();",
    "inactive conditional watch",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     const cancel = stop;\
     cancel();\
     source.value = new EventTarget();",
    "const stop-handle alias",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     const firstAcquired = new EventTarget();\
     firstAcquired.addEventListener = () => {};\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { flush: 'sync' });\
     source.value = firstAcquired;\
     source.value = new EventTarget();\
     stop();",
    "later mutated acquisition payload",
  );
  assert_cleanup_identity_quiet(
    "const handler = null;\
     const source = ref(new EventTarget());\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "null listener argument",
  );
  assert_cleanup_identity_quiet(
    "const handler = { notAListener: true };\
     const source = ref(new EventTarget());\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "unknown object listener",
  );
}

#[test]
fn watch_cleanup_current_source_stays_quiet_for_third_review_safes() {
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let replacement = new EventTarget();\
     replacement = initial;\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = replacement;\
     stop();",
    "mutable payload alias rebound to the acquired allocation",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     if (true) stop();\
     source.value = new EventTarget();\
     stop();",
    "earlier conditional stop before a later definite stop",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     const handle = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     const cancel = handle;\
     if (true) cancel.stop();\
     source.value = new EventTarget();\
     handle.stop();",
    "earlier uncertain .stop() on a const handle alias",
  );
}

#[test]
fn watch_cleanup_current_source_stays_quiet_for_fourth_review_safes() {
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let replacement = initial;\
     if (false) replacement = new EventTarget();\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = replacement;\
     stop();",
    "conditional write on a payload alias is not executed replacement",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let replacement = initial;\
     function unusedReplacement() { replacement = new EventTarget(); }\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = replacement;\
     stop();",
    "uninvoked function write on a payload alias is not executed replacement",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let replacement = new EventTarget();\
     replacement = initial;\
     watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     replacement = new EventTarget();\
     source.value = replacement;",
    "executed mutable payload rebind stays Unknown",
  );
}

#[test]
fn watch_cleanup_current_source_stays_quiet_for_fifth_review_safes() {
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let replacement = new EventTarget();\
     replacement &&= initial;\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = replacement;\
     stop();",
    "logical assignment on a payload alias is Unknown",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let replacement = new EventTarget();\
     function run() {\
       const stop = watch(source, (target, _prev, onCleanup) => {\
         target.addEventListener('click', handler);\
         onCleanup(() => { source.value.removeEventListener('click', handler); });\
       }, { immediate: true, flush: 'sync' });\
       source.value = replacement;\
       stop();\
     }\
     replacement = initial;\
     run();",
    "later-owner write before call is Unknown",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let candidate = initial;\
     candidate = initial;\
     candidate.addEventListener = () => {};\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "method mutation through a written receiver alias is Unknown",
  );
}

#[test]
fn watch_cleanup_current_source_stays_quiet_for_sixth_review_safes() {
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let candidate = initial;\
     candidate = initial;\
     Reflect.set(candidate, 'addEventListener', () => {});\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "escape of a written payload alias is Unknown",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     const candidate = initial;\
     Reflect.set(candidate, 'addEventListener', () => {});\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "escape of a stable const payload alias stays quiet",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let replacement = new EventTarget();\
     [replacement] = [initial];\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = replacement;\
     stop();",
    "destructured payload write is Unknown",
  );
}

#[test]
fn watch_cleanup_current_source_stays_quiet_for_seventh_review_safes() {
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let candidate;\
     candidate = initial;\
     Reflect.set(candidate, 'addEventListener', () => {});\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "assignment of a native payload then generic escape is Unknown",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let candidate = initial;\
     candidate = initial;\
     let forwarded = candidate;\
     Reflect.set(forwarded, 'addEventListener', () => {});\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "copy of a written native payload then generic escape is Unknown",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     function disable() { Reflect.set(candidate, 'addEventListener', () => {}); }\
     const candidate = initial;\
     disable();\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "escape before a later declaration is resolved after the identity index",
  );
}

#[test]
fn watch_cleanup_current_source_const_chain_alias_still_emits() {
  let facts = analyze(
    &cleanup_identity_source(
      "const handler = () => {};\
       const initial = new EventTarget();\
       const source = ref(initial);\
       const fresh = new EventTarget();\
       const middle = fresh;\
       const replacement = middle;\
       watch(source, (target, _prev, onCleanup) => {\
         target.addEventListener('click', handler);\
         onCleanup(() => { source.value.removeEventListener('click', handler); });\
       }, { immediate: true, flush: 'sync' });\
       source.value = replacement;",
    ),
    "ts",
  );
  assert_eq!(
    facts.lifetime.watch_cleanup_current_sources.len(),
    1,
    "two-hop const payload alias must still emit: {:?}",
    facts.lifetime.watch_cleanup_current_sources
  );
}

#[test]
fn watch_cleanup_current_source_const_payload_alias_still_emits() {
  let facts = analyze(
    &cleanup_identity_source(
      "const handler = () => {};\
       const initial = new EventTarget();\
       const source = ref(initial);\
       const replacement = new EventTarget();\
       watch(source, (target, _prev, onCleanup) => {\
         target.addEventListener('click', handler);\
         onCleanup(() => { source.value.removeEventListener('click', handler); });\
       }, { immediate: true, flush: 'sync' });\
       source.value = replacement;",
    ),
    "ts",
  );
  assert_eq!(
    facts.lifetime.watch_cleanup_current_sources.len(),
    1,
    "stable const payload alias must still emit: {:?}",
    facts.lifetime.watch_cleanup_current_sources
  );
}

#[test]
fn cleanup_identity_counters_are_counted_in_tests() {
  let (identity, stats) = super::lifetime::counter_layout();
  assert_eq!(
    (identity, stats),
    (64, 176),
    "test IdentityWork is 8 usizes and CollectStats is 22 (15 identity/ownership + 7 settlement)"
  );
}

#[test]
fn watch_cleanup_current_source_handle_event_still_emits() {
  let facts = analyze(
    &cleanup_identity_source(
      "const handler = { handleEvent() {} };\
       const source = ref(new EventTarget());\
       watch(source, (target, _prev, onCleanup) => {\
         target.addEventListener('click', handler);\
         onCleanup(() => { source.value.removeEventListener('click', handler); });\
       }, { immediate: true, flush: 'sync' });\
       source.value = new EventTarget();",
    ),
    "ts",
  );
  assert_eq!(
    facts.lifetime.watch_cleanup_current_sources.len(),
    1,
    "proven handleEvent listener must still emit: {:?}",
    facts.lifetime.watch_cleanup_current_sources
  );
}

#[test]
fn watch_cleanup_current_source_spans_cover_unicode_and_crlf() {
  let facts = analyze(
    "import { ref, watch } from 'vue'\r\nconst 处理 = () => {}\r\nconst 源 = ref(new EventTarget())\r\nwatch(源, (目标, _prev, onCleanup) => {\r\n  目标.addEventListener('click', 处理)\r\n  onCleanup(() => { 源.value.removeEventListener('click', 处理) })\r\n}, { immediate: true, flush: 'sync' })\r\n源.value = new EventTarget()\r\n",
    "ts",
  );
  let fact = facts.lifetime.watch_cleanup_current_sources.first();
  assert!(fact.is_some(), "unicode/CRLF must emit: {:?}", facts.lifetime);
  let Some(fact) = fact else {
    return;
  };
  assert!(fact.release_span.length > 0);
  assert!(fact.acquisition_span.length > 0);
  assert!(fact.replacement_span.length > 0);
}

#[test]
fn watch_cleanup_current_source_index_scales_linearly() {
  fn independent(count: usize, aliases: usize) -> String {
    let mut out = String::from("import { ref, watch } from 'vue';\nconst handler = () => {};\n");
    for _ in 0..count {
      out.push_str("{\nconst source = ref(new EventTarget());\n");
      for alias in 0..aliases {
        out.push_str("const a");
        out.push_str(&alias.to_string());
        out.push_str(" = source;\n");
      }
      out.push_str(
        "watch(source, (target, _prev, onCleanup) => {\n  target.addEventListener('click', handler);\n  onCleanup(() => { source.value.removeEventListener('click', handler); });\n}, { immediate: true, flush: 'sync' });\nsource.value = new EventTarget();\n}\n",
      );
    }
    out
  }
  fn shared_source(watchers: usize, writes: usize) -> String {
    let mut out = String::from(
      "import { ref, watch } from 'vue';\nconst handler = () => {};\nconst source = ref(new EventTarget());\n",
    );
    for _ in 0..watchers {
      out.push_str(
        "watch(source, (target, _prev, onCleanup) => {\n  target.addEventListener('click', handler);\n  onCleanup(() => { source.value.removeEventListener('click', handler); });\n}, { immediate: true, flush: 'sync' });\n",
      );
    }
    for _ in 0..writes {
      out.push_str("source.value = new EventTarget();\n");
    }
    out
  }
  fn shared_callback(count: usize) -> String {
    let mut out = String::from(
      "import { ref, watch } from 'vue';\nconst handler = () => {};\nconst source = ref(new EventTarget());\nfunction cb(target, _prev, onCleanup) {\n  target.addEventListener('click', handler);\n  onCleanup(() => { source.value.removeEventListener('click', handler); });\n}\n",
    );
    for _ in 0..count {
      out.push_str("watch(source, cb, { immediate: true, flush: 'sync' });\n");
    }
    out.push_str("source.value = new EventTarget();\n");
    out
  }
  fn multiple_resources(count: usize) -> String {
    let mut out =
      String::from("import { ref, watch } from 'vue';\nconst source = ref(new EventTarget());\n");
    for index in 0..count {
      out.push_str("const h");
      out.push_str(&index.to_string());
      out.push_str(" = () => {};\n");
    }
    out.push_str("watch(source, (target, _prev, onCleanup) => {\n");
    for index in 0..count {
      out.push_str("  target.addEventListener('click', h");
      out.push_str(&index.to_string());
      out.push_str(");\n");
    }
    out.push_str("  onCleanup(() => {\n");
    for index in 0..count {
      out.push_str("    source.value.removeEventListener('click', h");
      out.push_str(&index.to_string());
      out.push_str(");\n");
    }
    out.push_str(
      "  });\n}, { immediate: true, flush: 'sync' });\nsource.value = new EventTarget();\n",
    );
    out
  }
  fn assert_linear(
    label: &str,
    facts20: usize,
    facts40: usize,
    facts80: usize,
    source20: &str,
    source40: &str,
    source80: &str,
  ) {
    let (got20, stats20) = cleanup_identity_visits(source20);
    let (got40, stats40) = cleanup_identity_visits(source40);
    let (got80, stats80) = cleanup_identity_visits(source80);
    assert_eq!((got20, got40, got80), (facts20, facts40, facts80), "{label} facts");
    assert!(
      stats20.identity_comparisons > 0
        && stats20.identity_registrations > 0
        && stats20.identity_alias_work > 0,
      "{label} must count comparisons/registrations/alias work: {stats20:?}"
    );
    assert!(
      stats80.total() <= stats20.total().saturating_mul(6),
      "{label} inner work must stay near-linear: 20={} 40={} 80={} comparisons {}/{}/{} registrations {}/{}/{} alias {}/{}/{}",
      stats20.total(),
      stats40.total(),
      stats80.total(),
      stats20.identity_comparisons,
      stats40.identity_comparisons,
      stats80.identity_comparisons,
      stats20.identity_registrations,
      stats40.identity_registrations,
      stats80.identity_registrations,
      stats20.identity_alias_work,
      stats40.identity_alias_work,
      stats80.identity_alias_work
    );
  }
  assert_linear(
    "independent sources",
    20,
    40,
    80,
    &independent(20, 8),
    &independent(40, 8),
    &independent(80, 8),
  );
  assert_linear(
    "shared source",
    20,
    40,
    80,
    &shared_source(20, 20),
    &shared_source(40, 40),
    &shared_source(80, 80),
  );
  assert_linear(
    "shared callback",
    20,
    40,
    80,
    &shared_callback(20),
    &shared_callback(40),
    &shared_callback(80),
  );
  assert_linear(
    "multiple resources",
    20,
    40,
    80,
    &multiple_resources(20),
    &multiple_resources(40),
    &multiple_resources(80),
  );
}

#[test]
#[expect(clippy::panic, reason = "missing memoize fact must fail the unit test")]
fn cached_result_memoize_and_controlled_emit_stale_demand() {
  let (memo, _) = contract_stats(
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve().toUpperCase();",
  );
  assert_eq!(memo.memoize_stale_result_demand.len(), 1, "{memo:?}");
  let Some(site) = memo.memoize_stale_result_demand.first() else {
    panic!("memoize stale demand missing");
  };
  assert_eq!(site.member, "toUpperCase", "{memo:?}");
  assert_eq!(site.cached_kind, vue_vet_core::PrimitiveValueKind::Number, "{memo:?}");
  assert_eq!(site.current_kind, vue_vet_core::PrimitiveValueKind::String, "{memo:?}");
  let (controlled, _) = contract_stats(
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; const revision = ref(0); const source = ref(1); const value = computedWithControl(revision, () => source.value); void value.value; source.value = 'text'; value.value.toUpperCase();",
  );
  assert_eq!(controlled.controlled_computed_stale_result_demand.len(), 1, "{controlled:?}");
  let (wrapped, _) = contract_stats(
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; (resolve() as string).toUpperCase();",
  );
  assert_eq!(wrapped.memoize_stale_result_demand.len(), 1, "{wrapped:?}");
  let (alias, _) = contract_stats(
    "import { ref } from 'vue'; import { controlledComputed } from '@vueuse/core'; const revision = ref(0); const source = ref(1); const value = controlledComputed(revision, () => source.value); void value.value; source.value = 'text'; value.value.toUpperCase();",
  );
  assert_eq!(alias.controlled_computed_stale_result_demand.len(), 1, "{alias:?}");
}

#[test]
fn cached_result_required_controls_stay_quiet() {
  for source in [
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); source.value = 'text'; resolve().toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve(1).toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 2; resolve().toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve().toString();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve()?.toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve.load(); resolve().toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve.delete(); resolve().toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve.clear(); resolve().toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value, { getKey: () => source.value }); resolve(); source.value = 'text'; resolve().toUpperCase();",
    "import { ref } from 'vue'; const useMemoize = (resolver: () => unknown) => resolver; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve().toUpperCase();",
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; const revision = ref(0); const source = ref(1); const value = computedWithControl(revision, () => source.value); source.value = 'text'; value.value.toUpperCase();",
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; const revision = ref(0); const source = ref(1); const value = computedWithControl([revision, source], () => source.value); void value.value; source.value = 'text'; value.value.toUpperCase();",
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; const revision = ref(0); const source = ref(1); const value = computedWithControl(revision, () => source.value); void value.value; source.value = 'text'; value.trigger(); value.value.toUpperCase();",
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; const revision = ref(0); const source = ref(1); const value = computedWithControl(revision, () => source.value); void value.value; source.value = 'text'; revision.value++; value.value.toUpperCase();",
    "import { computed, ref } from 'vue'; const source = ref(1); const value = computed(() => source.value); void value.value; source.value = 'text'; value.value.toUpperCase();",
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; const revision = ref(0); const source = ref(1); const value = computedWithControl(revision, () => source.value, { flush: 'sync' }); void value.value; source.value = 'text'; value.value.toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/shared'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve().toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref('initial'); const resolve = useMemoize(() => source.value); resolve(); source.value = 1; resolve(); source.value = 'latest'; resolve().toUpperCase();",
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; const revision = ref(0); const source = ref('initial'); const value = computedWithControl(revision, () => source.value); void value.value; source.value = 1; void value.value; source.value = 'latest'; value.value.toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; Number.prototype.toUpperCase = function () { return 'supported' }; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'latest'; resolve().toUpperCase();",
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; Number['prototype']['toUpperCase'] = function () { return 'supported' }; const revision = ref(0); const source = ref(1); const value = computedWithControl(revision, () => source.value); void value.value; source.value = 'latest'; value.value.toUpperCase();",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.memoize_stale_result_demand.is_empty()
        && facts.controlled_computed_stale_result_demand.is_empty(),
      "cached-result control must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn cached_result_independent_producers_grow_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [32_u64, 64, 128] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; import { computedWithControl } from '@vueuse/shared';",
    );
    for index in 0..size {
      source.push_str("const s");
      source.push_str(&index.to_string());
      source.push_str(" = ref(1); const r");
      source.push_str(&index.to_string());
      source.push_str(" = useMemoize(() => s");
      source.push_str(&index.to_string());
      source.push_str(".value); r");
      source.push_str(&index.to_string());
      source.push_str("(); s");
      source.push_str(&index.to_string());
      source.push_str(".value = 'text'; r");
      source.push_str(&index.to_string());
      source.push_str("().toUpperCase();");
      source.push_str("const rev");
      source.push_str(&index.to_string());
      source.push_str(" = ref(0); const cs");
      source.push_str(&index.to_string());
      source.push_str(" = ref(1); const v");
      source.push_str(&index.to_string());
      source.push_str(" = computedWithControl(rev");
      source.push_str(&index.to_string());
      source.push_str(", () => cs");
      source.push_str(&index.to_string());
      source.push_str(".value); void v");
      source.push_str(&index.to_string());
      source.push_str(".value; cs");
      source.push_str(&index.to_string());
      source.push_str(".value = 'text'; v");
      source.push_str(&index.to_string());
      source.push_str(".value.toUpperCase();");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.memoize_stale_result_demand.len(), expected, "{contracts:?}");
    assert_eq!(contracts.controlled_computed_stale_result_demand.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "independent cached-result work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn cached_result_shared_source_producers_grow_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [32_u64, 64, 128] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1);",
    );
    for index in 0..size {
      source.push_str("const r");
      source.push_str(&index.to_string());
      source.push_str(" = useMemoize(() => source.value); r");
      source.push_str(&index.to_string());
      source.push_str("();");
    }
    source.push_str("source.value = 'text';");
    for index in 0..size {
      source.push('r');
      source.push_str(&index.to_string());
      source.push_str("().toUpperCase();");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.memoize_stale_result_demand.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-source cached-result work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn cached_result_many_demands_on_one_cache_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [32_u64, 64, 128] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text';",
    );
    for _ in 0..size {
      source.push_str("resolve().toUpperCase();");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.memoize_stale_result_demand.len(), 1, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "many-demand cached-result work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn cached_result_deep_wrapper_chains_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text';",
    );
    source.push_str("const w0 = resolve;");
    for index in 1..size {
      source.push_str("const w");
      source.push_str(&index.to_string());
      source.push_str(" = w");
      source.push_str(&(index - 1).to_string());
      source.push(';');
    }
    source.push('w');
    source.push_str(&(size - 1).to_string());
    source.push_str("().toUpperCase();");
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.memoize_stale_result_demand.len(), 1, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "wrapper-chain cached-result work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn watch_cleanup_current_source_review_shapes_scale() {
  fn prefix_then_watchers(count: usize) -> String {
    let mut out = String::from(
      "import {ref,watch} from 'vue'; const handler=()=>{}; const initial=new EventTarget(); const source=ref(initial);\n",
    );
    out.push_str(&"source.value=new EventTarget();\n".repeat(count));
    out.push_str(
      &"watch(source,(target,prev,onCleanup)=>{target.addEventListener('click',handler);onCleanup(()=>{source.value.removeEventListener('click',handler)})},{immediate:true,flush:'sync'});\n".repeat(count),
    );
    out.push_str("source.value=new EventTarget();\n");
    out
  }
  fn watchers_then_same_writes(count: usize) -> String {
    let mut out = String::from(
      "import {ref,watch} from 'vue'; const handler=()=>{}; const initial=new EventTarget(); const source=ref(initial);\n",
    );
    out.push_str(
      &"watch(source,(target,prev,onCleanup)=>{target.addEventListener('click',handler);onCleanup(()=>{source.value.removeEventListener('click',handler)})},{immediate:true,flush:'sync'});\n".repeat(count),
    );
    out.push_str(&"source.value=initial;\n".repeat(count));
    out.push_str("source.value=new EventTarget();\n");
    out
  }
  fn separate_cleanups(count: usize) -> String {
    let mut out = String::from(
      "import {ref,watch} from 'vue'; const handler=()=>{}; const initial=new EventTarget(); const source=ref(initial);\n",
    );
    out.push_str("watch(source,(target,prev,onCleanup)=>{\n");
    for index in 0..count {
      out.push_str("target.addEventListener('event");
      out.push_str(&index.to_string());
      out.push_str("',handler); onCleanup(()=>{source.value.removeEventListener('event");
      out.push_str(&index.to_string());
      out.push_str("',handler)});\n");
    }
    out.push_str("},{immediate:true,flush:'sync'});\nsource.value=new EventTarget();\n");
    out
  }
  fn written_alias_chains(count: usize) -> String {
    let mut out = String::from(
      "import {ref,watch} from 'vue'; const handler=()=>{}; const initial=new EventTarget(); const source=ref(initial);\n",
    );
    out.push_str("let replacement=initial;watch(source,(target,prev,onCleanup)=>{target.addEventListener('click',handler);onCleanup(()=>{source.value.removeEventListener('click',handler)})},{immediate:true,flush:'sync'});\n");
    for _ in 0..count {
      out.push_str("replacement=new EventTarget();source.value=replacement;\n");
    }
    out
  }
  fn payload_flow_chain(count: usize) -> String {
    let mut out = String::from(
      "import {ref,watch} from 'vue'; const handler=()=>{}; const initial=new EventTarget(); const source=ref(initial);\nlet x0=initial;\n",
    );
    for index in 1..count {
      out.push_str("let x");
      out.push_str(&index.to_string());
      out.push_str("=x");
      out.push_str(&(index.saturating_sub(1)).to_string());
      out.push_str(";\n");
    }
    out.push_str("Reflect.set(x");
    out.push_str(&(count.saturating_sub(1)).to_string());
    out.push_str(",'addEventListener',()=>{});watch(source,(target,prev,onCleanup)=>{target.addEventListener('click',handler);onCleanup(()=>{source.value.removeEventListener('click',handler)})},{immediate:true,flush:'sync'});source.value=new EventTarget();\n");
    out
  }
  for (label, source_at, expected_facts) in [
    ("prefix", prefix_then_watchers as fn(usize) -> String, true),
    ("suffix", watchers_then_same_writes as fn(usize) -> String, true),
    ("cleanup", separate_cleanups as fn(usize) -> String, true),
    ("binding", written_alias_chains as fn(usize) -> String, false),
    ("flow", payload_flow_chain as fn(usize) -> String, false),
  ] {
    let mut totals = [0; 4];
    for (slot, count) in [20, 40, 80, 160].into_iter().enumerate() {
      let (facts, stats) = cleanup_identity_visits(&source_at(count));
      let want = if expected_facts { count } else { 0 };
      assert_eq!(facts, want, "{label} N={count} facts");
      assert!(
        stats.identity_construction > 0
          && stats.identity_queries > 0
          && stats.identity_comparisons > 0
          && stats.identity_reference_visits > 0,
        "{label} N={count} must count construction/query/comparison/reference visits: {stats:?}"
      );
      if label == "flow" {
        assert!(
          stats.identity_copies > 0 && stats.identity_alias_work > 0,
          "{label} N={count} must count seed/flow edge visits: {stats:?}"
        );
      }
      let expected_visits = match label {
        "prefix" | "suffix" => count.saturating_mul(3).saturating_add(1),
        "cleanup" | "binding" | "flow" => count.saturating_add(2),
        _ => 0,
      };
      assert!(
        stats.identity_reference_visits >= expected_visits,
        "{label} N={count} must pre-index at least each source reference once: {stats:?}"
      );
      if let Some(total) = totals.get_mut(slot) {
        *total = stats.total();
      }
    }
    let Some(&total20) = totals.first() else {
      continue;
    };
    let Some(&total80) = totals.get(2) else {
      continue;
    };
    assert!(
      total80 <= total20.saturating_mul(6),
      "{label} 80/20 must stay <= 6: 20={total20} 80={total80}"
    );
  }
}

#[test]
fn source_contracts_computed_only_imports_match_forced_full() {
  let source = "import { computed, ref } from 'vue';\
     const items = ref([1, 2]);\
     const doubled = computed(() => items.value.map((n: number) => n * 2));\
     const first = computed(() => doubled.value[0]);\
     void first.value;\
     items.value = [1, 2];\
     void first.value;";
  let facts = assert_gated_matches_forced(source, ScriptKind::Setup);
  assert_eq!(
    facts.stable_computed_identity.len(),
    1,
    "computed-only imports must still collect identity facts: {facts:?}"
  );
}

#[test]
fn source_contracts_stable_computed_identity_positive_and_controls() {
  let positive = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const items = ref([1, 2]);\
     const doubled = computed(() => items.value.map((n: number) => n * 2));\
     watch(doubled, (value) => { void value; });\
     items.value = [1, 2];",
    ScriptKind::Setup,
  );
  assert_eq!(positive.stable_computed_identity.len(), 1, "{positive:?}");

  let previous = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const items = ref([1, 2]);\
     const doubled = computed((previous: number[] | undefined) => {\
       const next = items.value.map((n: number) => n * 2);\
       if (previous) return previous;\
       return next;\
     });\
     watch(doubled, (value) => { void value; });\
     items.value = [1, 2];",
    ScriptKind::Setup,
  );
  assert!(previous.stable_computed_identity.is_empty(), "{previous:?}");

  let primitive = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const count = ref(2);\
     const doubled = computed(() => count.value * 2);\
     watch(doubled, (value) => { void value; });\
     count.value = 2;",
    ScriptKind::Setup,
  );
  assert!(primitive.stable_computed_identity.is_empty(), "{primitive:?}");

  let changed = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const items = ref([1, 2]);\
     const doubled = computed(() => items.value.map((n: number) => n * 2));\
     watch(doubled, (value) => { void value; });\
     items.value = [1, 3];",
    ScriptKind::Setup,
  );
  assert!(changed.stable_computed_identity.is_empty(), "{changed:?}");

  let lazy = assert_gated_matches_forced(
    "import { computed, ref } from 'vue';\
     const items = ref([1, 2]);\
     const doubled = computed(() => items.value.map((n: number) => n * 2));\
     const first = computed(() => doubled.value[0]);\
     void first.value;\
     items.value = [1, 2];",
    ScriptKind::Setup,
  );
  assert!(lazy.stable_computed_identity.is_empty(), "lazy child without later demand: {lazy:?}");

  let queued = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const items = ref([1, 2]);\
     const doubled = computed(() => items.value.map((n: number) => n * 2));\
     const stop = watch(doubled, (value) => { void value; }, { flush: 'pre' });\
     items.value = [1, 2];\
     stop();",
    ScriptKind::Setup,
  );
  assert!(
    queued.stable_computed_identity.is_empty(),
    "queued pre watch stopped after replace: {queued:?}"
  );

  let shadowed = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     function run(undefined: number) {\
       const items = ref([undefined]);\
       const copy = computed(() => items.value.map((n: number) => n));\
       watch(copy, (value) => { void value; }, { flush: 'sync' });\
       items.value = [void 0];\
     }\
     run(1);",
    ScriptKind::Setup,
  );
  assert!(shadowed.stable_computed_identity.is_empty(), "shadowed undefined: {shadowed:?}");

  let strings = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const items = ref(['a']);\
     const selected = computed(() => items.value.filter((n: string) => n < 'm'));\
     watch(selected, (value) => { void value; }, { flush: 'sync' });\
     items.value = ['b'];",
    ScriptKind::Setup,
  );
  assert!(strings.stable_computed_identity.is_empty(), "string relational filter: {strings:?}");

  let prior = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const items = ref([1]);\
     items.value = [2];\
     const doubled = computed(() => items.value.map((n: number) => n * 2));\
     void doubled.value;\
     watch(doubled, (value) => { void value; }, { flush: 'sync' });\
     items.value = [1];",
    ScriptKind::Setup,
  );
  assert!(prior.stable_computed_identity.is_empty(), "prior replacement baseline: {prior:?}");
}

#[test]
#[expect(clippy::panic, reason = "independent computed fixture construction must fail the test")]
fn source_contracts_many_independent_computeds_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [40_u64, 80, 160] {
    let mut source = String::from("import { computed, ref, watch } from 'vue';");
    for index in 0..size {
      write!(
        source,
        "const items{index} = ref([1, 2]);\
         const doubled{index} = computed(() => items{index}.value.map((n: number) => n * 2));\
         watch(doubled{index}, (value) => {{ void value; }});\
         items{index}.value = [1, 2];"
      )
      .unwrap_or_else(|error| panic!("independent computed fixture write: {error}"));
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.stable_computed_identity.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "size {size}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "independent computed work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
#[expect(clippy::panic, reason = "shared-source computed fixture construction must fail the test")]
fn source_contracts_shared_source_many_consumers_and_wide_projections_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [20_u64, 40, 80] {
    let mut source = String::from(
      "import { computed, ref, watch } from 'vue'; const items = ref([1, 2, 3, 4, 5, 6, 7, 8]);",
    );
    for index in 0..size {
      write!(
        source,
        "const doubled{index} = computed(() => items.value.map((n: number) => n * 2).filter((n: number) => n > 0).slice(0, 8).concat([9]));\
         watch(doubled{index}, (value) => {{ void value; }});"
      )
      .unwrap_or_else(|error| panic!("shared-source computed fixture write: {error}"));
    }
    source.push_str("items.value = [1, 2, 3, 4, 5, 6, 7, 8];");
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.stable_computed_identity.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "size {size} facts {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-source work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_projection_width_grows_inner_work() {
  let mut previous: Option<(usize, u64)> = None;
  for width in [8_usize, 16, 32] {
    let mut values = Vec::with_capacity(width);
    for index in 0..width {
      values.push(index.to_string());
    }
    let literal = values.join(", ");
    let source = format!(
      "import {{ computed, ref, watch }} from 'vue';\
       const items = ref([{literal}]);\
       const doubled = computed(() => items.value.map((n: number) => n * 2));\
       watch(doubled, (value) => {{ void value; }}, {{ flush: 'sync' }});\
       items.value = [{literal}];"
    );
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.stable_computed_identity.len(), 1, "width {width} {contracts:?}");
    if let Some((prev_width, prev_work)) = previous {
      assert_eq!(width, prev_width * 2, "widths must double");
      assert!(
        work > prev_work,
        "projection width work must grow: {prev_work} -> {work} on {prev_width}->{width}"
      );
    }
    previous = Some((width, work));
  }
}

#[test]
#[expect(clippy::panic, reason = "joint-shape fixture construction must fail the test")]
fn source_contracts_joint_producer_consumer_source_shapes_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [4_u64, 8, 16] {
    let mut source =
      String::from("import { computed, ref, watch } from 'vue'; const shared = ref([1, 2]);");
    for index in 0..size {
      write!(
        source,
        "const items{index} = ref([1, 2]);\
         const doubled{index} = computed(() => items{index}.value.map((n: number) => n * 2));\
         const shared{index} = computed(() => shared.value.map((n: number) => n * 2));\
         watch(doubled{index}, (value) => {{ void value; }}, {{ flush: 'sync' }});\
         watch(shared{index}, (value) => {{ void value; }}, {{ flush: 'sync' }});\
         items{index}.value = [1, 2];"
      )
      .unwrap_or_else(|error| panic!("joint shape write: {error}"));
    }
    source.push_str("shared.value = [1, 2];");
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size.saturating_mul(2)).unwrap_or(usize::MAX);
    assert_eq!(
      contracts.stable_computed_identity.len(),
      expected,
      "size {size} facts {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "joint shape work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn derivation_practice_sync_ref_one_way_collects_closed_sink() {
  let facts = assert_gated_matches_forced(
    "import { ref } from 'vue';\
     import { syncRef } from '@vueuse/core';\
     const left = ref(1);\
     const right = ref(0);\
     syncRef(left, right);\
     left.value = 5;\
     void right.value;",
    ScriptKind::Setup,
  );
  assert_eq!(facts.derivation_practice.sync_ref_one_way.len(), 1, "{facts:?}");
  let shared = assert_gated_matches_forced(
    "import { ref } from 'vue';\
     import { syncRef } from '@vueuse/shared';\
     const left = ref(1);\
     const right = ref(0);\
     syncRef(left, right, { direction: 'both', flush: 'sync', deep: false, immediate: true, transform: {} });\
     left.value = 6;\
     void right.value;",
    ScriptKind::Setup,
  );
  assert_eq!(shared.derivation_practice.sync_ref_one_way.len(), 1, "{shared:?}");
}

#[test]
fn derivation_practice_sync_ref_stays_quiet_for_controls() {
  for source in [
    "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref(1); const right = ref(0); syncRef(left, right, { direction: 'ltr' }); left.value = 5; void right.value;",
    "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref(1); const right = ref(0); syncRef(left, right); right.value = 9; left.value = 5; void right.value;",
    "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref({ n: 1 }); const right = ref({ n: 0 }); syncRef(left, right); left.value = { n: 2 }; void right.value;",
    "import { ref } from 'vue'; function syncRef(a, b) { void a; void b; } const left = ref(1); const right = ref(0); syncRef(left, right); left.value = 5; void right.value;",
    "import { ref } from 'vue'; import { syncRef } from '#imports'; const left = ref(1); const right = ref(0); syncRef(left, right); left.value = 5; void right.value;",
    "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref(1); const right = ref(0); syncRef(left, right, { transform: { ltr: (v) => v } }); left.value = 5; void right.value;",
    "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref(1); const right = ref(0); syncRef(left, right, { direction: 'r' + 'tl' }); left.value = 5; void right.value;",
    "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref(1); const right = ref(0); const stop = syncRef(left, right); stop(); left.value = 5; void right.value;",
    "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref(1); const right = ref(0); if (false) syncRef(left, right); left.value = 5; void right.value;",
  ] {
    let facts = contract_collect(source, ScriptKind::Setup, true).0;
    assert!(
      facts.derivation_practice.sync_ref_one_way.is_empty(),
      "must stay quiet: {source} {facts:?}"
    );
  }
}

#[test]
fn derivation_practice_conditional_watch_collects_idle_producer() {
  let facts = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const flag = ref(false);\
     const source = ref(2);\
     const sink = ref(5);\
     const heavy = computed(() => source.value);\
     watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; });\
     source.value = 3;",
    ScriptKind::Setup,
  );
  assert_eq!(facts.derivation_practice.conditional_watch_source.len(), 1, "{facts:?}");
}

#[test]
fn derivation_practice_conditional_watch_stays_quiet_for_controls() {
  for source in [
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; console.log(value); }); source.value = 3;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }); void heavy.value; source.value = 3;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(true); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }); source.value = 3;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); watch([flag, heavy], ([active, value], old) => { if (active) sink.value = value; void old; }); source.value = 3;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const events = []; const heavy = computed(() => source.value); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }, { flush: 'sync', onTrigger: event => events.push(event.type) }); source.value = 3;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const oneRun = true; const heavy = computed(() => source.value); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }, { flush: 'sync', once: oneRun }); source.value = 3; function enable(target: { value: boolean }) { target.value = true; } enable(flag); void sink.value;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const events = []; const heavy = computed(() => source.value, { onTrack: event => events.push(event.type) }); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }, { flush: 'sync' }); source.value = 3;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }); function unused() { source.value = 3; }",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); if (false) watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }); source.value = 3;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }); false && (source.value = 3);",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); function enable(target: { value: boolean }) { target.value = true; } enable(flag); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }, { flush: 'sync' }); source.value = 3;",
  ] {
    let facts = contract_collect(source, ScriptKind::Setup, true).0;
    assert!(
      facts.derivation_practice.conditional_watch_source.is_empty(),
      "must stay quiet: {source} {facts:?}"
    );
  }
}

#[test]
#[expect(clippy::panic, reason = "independent syncRef fixture construction must fail the test")]
fn derivation_practice_independent_sources_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [50_u64, 100, 200] {
    let mut source =
      String::from("import { ref } from 'vue'; import { syncRef } from '@vueuse/core';");
    for index in 0..size {
      write!(
        source,
        "const l{index} = ref(1); const r{index} = ref(0); syncRef(l{index}, r{index}); l{index}.value = 5; void r{index}.value;"
      )
      .unwrap_or_else(|error| panic!("independent syncRef fixture write: {error}"));
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.derivation_practice.sync_ref_one_way.len(),
      usize::try_from(size).unwrap_or(usize::MAX)
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "independent syncRef work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
#[expect(clippy::panic, reason = "alias fanout fixture construction must fail the test")]
fn derivation_practice_alias_fanout_and_deep_wrappers_stay_linear() {
  fn source(aliases: usize) -> String {
    let mut out = String::from(
      "import { computed, ref, watch } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref(1); const right = ref(0);",
    );
    let mut current = String::from("right");
    for index in 0..aliases {
      write!(out, "const a{index} = {current};")
        .unwrap_or_else(|error| panic!("alias fanout fixture write: {error}"));
      current = format!("a{index}");
    }
    out.push_str("syncRef(left, right); left.value = 5; void right.value;");
    out.push_str("const flag = ref(false); const src = ref(2); const sink = ref(5);");
    out.push_str("const heavy = computed(() => src.value);");
    out.push_str("watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }); src.value = 3;");
    out
  }
  let (small, small_work) = contract_stats(&source(20));
  let (large, large_work) = contract_stats(&source(80));
  assert_eq!(small.derivation_practice.sync_ref_one_way.len(), 1, "{small:?}");
  assert_eq!(large.derivation_practice.sync_ref_one_way.len(), 1, "{large:?}");
  assert!(
    large_work <= small_work.saturating_mul(8),
    "alias fanout work must stay near-linear: 20={small_work} 80={large_work}"
  );
}

#[test]
#[expect(clippy::panic, reason = "many-consumer fixture construction must fail the test")]
fn derivation_practice_one_source_many_consumers_stays_linear() {
  fn source(consumers: usize) -> String {
    let mut out = String::from(
      "import { computed, ref, watch } from 'vue'; const flag = ref(false); const src = ref(2); const heavy = computed(() => src.value);",
    );
    for index in 0..consumers {
      write!(
        out,
        "const sink{index} = ref(5); watch([flag, heavy], ([active, value]) => {{ if (active) sink{index}.value = value; }});"
      )
      .unwrap_or_else(|error| panic!("many-consumer fixture write: {error}"));
    }
    out.push_str("src.value = 3;");
    out
  }
  let (small, small_work) = contract_stats(&source(20));
  let (large, large_work) = contract_stats(&source(80));
  assert!(
    small.derivation_practice.conditional_watch_source.is_empty(),
    "extra consumers keep sole-demand unknown: {small:?}"
  );
  assert!(
    large.derivation_practice.conditional_watch_source.is_empty(),
    "extra consumers keep sole-demand unknown: {large:?}"
  );
  assert!(
    large_work <= small_work.saturating_mul(8),
    "one source many consumers must stay near-linear: 20={small_work} 80={large_work}"
  );
}

#[test]
#[expect(clippy::panic, reason = "joint alias/consumer fixture construction must fail the test")]
fn derivation_practice_joint_alias_and_consumer_fanout_stays_linear() {
  fn source(size: usize) -> String {
    let mut out = String::from(
      "import { computed, ref, watch } from 'vue'; const flag = ref(false); const src = ref(2); const heavy = computed(() => src.value);",
    );
    let mut current = String::from("heavy");
    for index in 0..size {
      write!(out, "const a{index} = {current};")
        .unwrap_or_else(|error| panic!("joint alias write: {error}"));
      current = format!("a{index}");
    }
    for index in 0..size {
      write!(
        out,
        "const sink{index} = ref(5); watch([flag, a{}], ([active, value]) => {{ if (active) sink{index}.value = value; }});",
        size.saturating_sub(1)
      )
      .unwrap_or_else(|error| panic!("joint consumer write: {error}"));
    }
    out.push_str("src.value = 3;");
    out
  }
  let mut previous: Option<(usize, u64)> = None;
  for size in [20_usize, 40, 80] {
    let (facts, work) = contract_stats(&source(size));
    assert!(
      facts.derivation_practice.conditional_watch_source.is_empty(),
      "joint fanout stays sole-demand unknown: {facts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "joint alias+consumer work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn derivation_practice_deep_wrappers_count_inner_traversal() {
  fn source(depth: usize) -> String {
    let mut getter = String::from("src.value");
    for _ in 0..depth {
      getter.insert(0, '(');
      getter.push(')');
    }
    format!(
      "import {{ computed, ref, watch }} from 'vue'; const flag = ref(false); const src = ref(2); const sink = ref(5); const heavy = computed(() => {getter}); watch([flag, heavy], ([active, value]) => {{ if (active) sink.value = value; }}); src.value = 3;"
    )
  }
  let (shallow, shallow_work) = contract_stats(&source(4));
  let (deep, deep_work) = contract_stats(&source(32));
  assert_eq!(shallow.derivation_practice.conditional_watch_source.len(), 1, "{shallow:?}");
  assert_eq!(deep.derivation_practice.conditional_watch_source.len(), 1, "{deep:?}");
  assert!(
    deep_work > shallow_work,
    "deeper wrappers must record extra peel work: 4={shallow_work} 32={deep_work}"
  );
  assert!(
    deep_work < shallow_work.saturating_mul(20),
    "wrapper peel must stay bounded: 4={shallow_work} 32={deep_work}"
  );
}

#[test]
fn derivation_practice_records_supported_alias_names() {
  let sync = assert_gated_matches_forced(
    "import { ref } from 'vue';\
     import { syncRef } from '@vueuse/core';\
     const left = ref(1);\
     const right = ref(0);\
     const alias = right;\
     syncRef(left, right);\
     left.value = 5;\
     void right.value;",
    ScriptKind::Setup,
  );
  let sink = sync.derivation_practice.sync_ref_one_way.first();
  assert!(
    sink.is_some_and(|site| site.sink_names.iter().any(|name| name == "alias")
      && site.sink_names.iter().any(|name| name == "right")),
    "sink aliases must be recorded: {sync:?}"
  );
  let watch = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const flag = ref(false);\
     const source = ref(2);\
     const sink = ref(5);\
     const heavy = computed(() => source.value);\
     const shown = heavy;\
     watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; });\
     source.value = 3;",
    ScriptKind::Setup,
  );
  let producer = watch.derivation_practice.conditional_watch_source.first();
  assert!(
    producer.is_some_and(|site| site.producer_names.iter().any(|name| name == "shown")
      && site.producer_names.iter().any(|name| name == "heavy")),
    "producer aliases must be recorded: {watch:?}"
  );
}

#[test]
#[expect(clippy::panic, reason = "shared-right fixture construction must fail the test")]
fn derivation_practice_shared_right_many_sync_calls_stays_linear() {
  fn source(size: usize) -> String {
    let mut out = String::from(
      "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const right = ref(0);",
    );
    for index in 0..size {
      write!(out, "const l{index} = ref(1); syncRef(l{index}, right); l{index}.value = 5;")
        .unwrap_or_else(|error| panic!("shared-right fixture write: {error}"));
    }
    out.push_str("void right.value;");
    out
  }
  let mut previous: Option<(usize, u64)> = None;
  for size in [20_usize, 40, 80] {
    let (facts, work) = contract_stats(&source(size));
    assert!(
      facts.derivation_practice.sync_ref_one_way.is_empty(),
      "shared right stays closed-sink unknown: {facts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-right many-sync-calls work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn scheduling_practice_queued_flush_collects_after_next_tick() {
  let facts = assert_gated_matches_forced(
    "import { nextTick, ref, watch } from 'vue';\
     const source = ref(0);\
     const sink = ref(0);\
     watch(source, (value) => { sink.value = value; }, { flush: 'sync' });\
     source.value = 1;\
     source.value = 2;\
     await nextTick();\
     void sink.value;",
    ScriptKind::Setup,
  );
  assert_eq!(facts.scheduling_practice.queued_watch_flush.len(), 1, "{facts:?}");
}

#[test]
fn scheduling_practice_queued_flush_stays_quiet_for_controls() {
  for source in [
    "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); watch(source, (value) => { sink.value = value; }); source.value = 1; source.value = 2; await nextTick(); void sink.value;",
    "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); source.value = 1; void sink.value; source.value = 2; await nextTick(); void sink.value;",
    "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); const stop = watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); source.value = 1; source.value = 2; stop(); await nextTick(); void sink.value;",
    "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); watch(source, (value) => { sink.value = value; }, { flush: 'sync', once: true }); source.value = 1; source.value = 2; await nextTick(); void sink.value;",
    "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(9); watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); source.value = 1; source.value = 0; await nextTick(); void sink.value;",
    "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); const observed = ref(0); watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); source.value = 1; source.value = 2; observed.value = sink.value; await nextTick(); void sink.value;",
    "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); source.value = 1; source.value = 2; if (false) { await nextTick(); } void sink.value;",
    "import { effectScope, nextTick, ref, watch } from 'vue'; const parent = effectScope(); parent.run(async () => { const source = ref(0); const sink = ref(0); watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); source.value = 1; source.value = 2; await nextTick(); void sink.value; }); parent.stop();",
  ] {
    let facts = contract_collect(source, ScriptKind::Setup, true).0;
    assert!(
      facts.scheduling_practice.queued_watch_flush.is_empty(),
      "must stay quiet: {source} {facts:?}"
    );
  }
}

#[test]
fn scheduling_practice_attached_scope_collects_paused_child() {
  let facts = assert_gated_matches_forced(
    "import { effectScope, onScopeDispose, ref, watch } from 'vue';\
     const source = ref(0);\
     const sink = ref(0);\
     const parent = effectScope();\
     parent.run(() => {\
       const child = effectScope(true);\
       child.run(() => { watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); });\
       onScopeDispose(() => child.stop());\
     });\
     parent.pause();\
     source.value = 2;\
     source.value = 3;\
     parent.resume();\
     void sink.value;",
    ScriptKind::Setup,
  );
  assert_eq!(facts.scheduling_practice.attached_effect_scope.len(), 1, "{facts:?}");
}

#[test]
fn scheduling_practice_attached_scope_stays_quiet_for_controls() {
  for source in [
    "import { effectScope, onScopeDispose, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); const parent = effectScope(); parent.run(() => { const child = effectScope(); child.run(() => { watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); }); onScopeDispose(() => child.stop()); }); parent.pause(); source.value = 2; source.value = 3; parent.resume(); void sink.value;",
    "import { effectScope, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); const parent = effectScope(); parent.run(() => { const child = effectScope(true); child.run(() => { watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); }); }); parent.pause(); source.value = 2; source.value = 3; parent.resume(); void sink.value;",
    "import { effectScope, onScopeDispose, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); const parent = effectScope(); parent.run(() => { const child = effectScope(true); child.run(() => { watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); }); onScopeDispose(() => child.stop()); child.pause(); }); parent.pause(); source.value = 2; source.value = 3; parent.resume(); void sink.value;",
    "import { effectScope, onScopeDispose, ref, watch } from 'vue'; const source = ref(0); const sink = ref(9); const parent = effectScope(); parent.run(() => { const child = effectScope(true); child.run(() => { watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); }); onScopeDispose(() => child.stop()); }); parent.pause(); source.value = 1; source.value = 0; parent.resume(); void sink.value;",
    "import { effectScope, onScopeDispose, ref, watch, watchSyncEffect } from 'vue'; const source = ref(0); const sink = ref(0); const parent = effectScope(); parent.run(() => { const child = effectScope(true); child.run(() => { watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); }); onScopeDispose(() => child.stop()); }); const seen = []; watchSyncEffect(() => { seen.push(sink.value); }); parent.pause(); source.value = 2; source.value = 3; parent.resume(); void sink.value;",
    "import { effectScope, onScopeDispose, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); const parent = effectScope(); parent.run(() => { const child = effectScope(true); child.run(() => { watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); }); onScopeDispose(() => child.stop()); }); if (false) { parent.pause(); } source.value = 2; source.value = 3; parent.resume(); void sink.value;",
  ] {
    let facts = contract_collect(source, ScriptKind::Setup, true).0;
    assert!(
      facts.scheduling_practice.attached_effect_scope.is_empty(),
      "must stay quiet: {source} {facts:?}"
    );
  }
}

#[test]
fn scheduling_practice_lazy_async_collects_eager_startup() {
  let facts = assert_gated_matches_forced(
    "import { ref, watch } from 'vue';\
     import { computedAsync } from '@vueuse/core';\
     const source = ref(1);\
     const sink = ref(0);\
     const value = computedAsync(async () => source.value * 10, -1);\
     source.value = 3;\
     watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    ScriptKind::Setup,
  );
  assert_eq!(facts.scheduling_practice.lazy_computed_async.len(), 1, "{facts:?}");
}

#[test]
fn scheduling_practice_lazy_async_stays_quiet_for_controls() {
  for source in [
    "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, -1, { lazy: true }); source.value = 3; watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); export const value = computedAsync(async () => source.value * 10, -1); source.value = 3; watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, -1); source.value = 3; value.value = 99; watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, -1); source.value = 3; void value.value; watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, -1, { lazy: !false }); source.value = 3; watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    "import { nextTick, ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, -1); source.value = 3; await nextTick(); await nextTick(); watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true, once: true });",
    "import { nextTick, ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, -1); source.value = 3; await nextTick(); await nextTick(); const stop = watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true }); stop();",
    "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); let currentInput = 1; Object.defineProperty(source, 'value', { get() { return currentInput; }, set(_value) {} }); const value = computedAsync(async () => source.value * 10, -1); source.value = 3; currentInput = 3; watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, -1); function neverCalled() { source.value = 3; } watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    "import { nextTick, ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, 'loading'); source.value = 3; await nextTick(); await nextTick(); watch(value, (current) => { if (current !== 'missing') sink.value = current; }, { immediate: true });",
  ] {
    let facts = contract_collect(source, ScriptKind::Setup, true).0;
    assert!(
      facts.scheduling_practice.lazy_computed_async.is_empty(),
      "must stay quiet: {source} {facts:?}"
    );
  }
}

#[test]
#[expect(clippy::format_push_string, reason = "geometric fixture assembly is clearer as push_str")]
fn scheduling_practice_independent_sources_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [50_u64, 100, 200] {
    let mut source = String::from("import { nextTick, ref, watch } from 'vue';");
    for index in 0..size {
      source.push_str(&format!(
        "const s{index} = ref(0); const k{index} = ref(0); watch(s{index}, (value) => {{ k{index}.value = value; }}, {{ flush: 'sync' }}); s{index}.value = 1; s{index}.value = 2;"
      ));
    }
    source.push_str("await nextTick();");
    for index in 0..size {
      source.push_str(&format!("void k{index}.value;"));
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.scheduling_practice.queued_watch_flush.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "{contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "independent queued-flush work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
#[expect(clippy::format_push_string, reason = "geometric fixture assembly is clearer as push_str")]
fn scheduling_practice_alias_fanout_and_wrappers_stay_linear() {
  fn source(aliases: usize) -> String {
    let mut out = String::from(
      "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0);",
    );
    let mut current = String::from("sink");
    for index in 0..aliases {
      out.push_str(&format!("const a{index} = {current};"));
      current = format!("a{index}");
    }
    out.push_str("watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); source.value = 1; source.value = 2; await nextTick(); void sink.value;");
    out
  }
  let (small, small_work) = contract_stats(&source(20));
  let (large, large_work) = contract_stats(&source(80));
  assert_eq!(small.scheduling_practice.queued_watch_flush.len(), 1, "{small:?}");
  assert_eq!(large.scheduling_practice.queued_watch_flush.len(), 1, "{large:?}");
  assert!(
    large_work <= small_work.saturating_mul(8),
    "alias fanout work must stay near-linear: 20={small_work} 80={large_work}"
  );
}

#[test]
#[expect(clippy::format_push_string, reason = "geometric fixture assembly is clearer as push_str")]
fn scheduling_practice_one_source_many_consumers_stays_linear() {
  fn source(consumers: usize) -> String {
    let mut out = String::from(
      "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const value = computedAsync(async () => source.value * 10, -1);",
    );
    for index in 0..consumers {
      out.push_str(&format!(
        "const sink{index} = ref(0); watch(value, (current) => {{ if (current !== -1) sink{index}.value = current; }}, {{ immediate: true }});"
      ));
    }
    out.push_str("source.value = 3;");
    out
  }
  let (small, small_work) = contract_stats(&source(20));
  let (large, large_work) = contract_stats(&source(80));
  assert!(
    small.scheduling_practice.lazy_computed_async.is_empty(),
    "extra consumers keep sole-demand unknown: {small:?}"
  );
  assert!(
    large.scheduling_practice.lazy_computed_async.is_empty(),
    "extra consumers keep sole-demand unknown: {large:?}"
  );
  assert!(
    large_work <= small_work.saturating_mul(8),
    "one source many consumers must stay near-linear: 20={small_work} 80={large_work}"
  );
}

#[test]
#[expect(clippy::format_push_string, reason = "geometric fixture assembly is clearer as push_str")]
fn scheduling_practice_joint_producer_consumer_owner_growth_stays_linear() {
  fn measure(kind: &str, size: u64) -> (usize, u64) {
    let n = usize::try_from(size).unwrap_or(usize::MAX);
    let mut source = String::from(
      "import { ref, watch, nextTick, effectScope, onScopeDispose } from 'vue'; import { computedAsync } from '@vueuse/core'; const shared = ref(1);",
    );
    if kind == "lazy-shared-source" {
      for index in 0..n {
        source.push_str(&format!(
          "const sink{index}=ref(0);const value{index}=computedAsync(async()=>shared.value*10,-1);"
        ));
      }
      source.push_str("shared.value=3;");
      for index in 0..n {
        source.push_str(&format!(
          "watch(value{index},(current)=>{{if(current!==-1)sink{index}.value=current}},{{immediate:true}});"
        ));
      }
    } else if kind == "attached-shared-source" {
      for index in 0..n {
        source.push_str(&format!(
          "const sink{index}=ref(0);const parent{index}=effectScope();parent{index}.run(()=>{{const child{index}=effectScope(true);child{index}.run(()=>{{watch(shared,(value)=>{{sink{index}.value=value}},{{flush:'sync'}})}});onScopeDispose(()=>child{index}.stop());}});"
        ));
      }
      for index in 0..n {
        source.push_str(&format!("parent{index}.pause();"));
      }
      source.push_str("shared.value=2;shared.value=3;");
      for index in 0..n {
        source.push_str(&format!("parent{index}.resume();void sink{index}.value;"));
      }
    } else {
      for index in 0..n {
        source.push_str(&format!(
          "const source{index}=ref(0);const sink{index}=ref(0);watch(source{index},(value)=>{{sink{index}.value=value}},{{flush:'sync'}});source{index}.value=1;source{index}.value=2;await nextTick();void sink{index}.value;"
        ));
      }
    }
    let (facts, work) = contract_stats(&source);
    let (forced, _) = contract_collect(&source, ScriptKind::Setup, true);
    assert_eq!(facts, forced, "eligible/forced fact parity for {kind} n={size}");
    let count = facts.scheduling_practice.queued_watch_flush.len()
      + facts.scheduling_practice.attached_effect_scope.len()
      + facts.scheduling_practice.lazy_computed_async.len();
    (count, work)
  }

  for kind in ["lazy-shared-source", "attached-shared-source", "queued-distinct-ticks"] {
    let mut previous: Option<(u64, u64)> = None;
    for size in [32_u64, 64, 128] {
      let (facts, work) = measure(kind, size);
      assert_eq!(facts, usize::try_from(size).unwrap_or(usize::MAX), "{kind} n={size} facts");
      if let Some((prev_size, prev_work)) = previous {
        assert_eq!(size, prev_size * 2, "fixture sizes must double");
        assert!(
          work.saturating_mul(10) < prev_work.saturating_mul(30),
          "{kind} work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
        );
      }
      previous = Some((size, work));
    }
  }
}

fn settlement_source(body: &str) -> String {
  format!(
    "import {{ ref, watch, watchEffect }} from 'vue';\
     const source = ref('one');\
     const result = ref(null);\
     {body}"
  )
}

fn settlement_facts(body: &str) -> vue_vet_core::ReactivityLifetimeFacts {
  analyze(&settlement_source(body), "ts").lifetime
}

fn settlement_work(source: &str) -> (usize, usize) {
  let allocator = oxc_allocator::Allocator::default();
  let parsed = oxc_parser::Parser::new(&allocator, source, oxc_span::SourceType::ts()).parse();
  let semantic =
    oxc_semantic::SemanticBuilder::new().with_build_nodes(true).build(&parsed.program).semantic;
  let line_index = vue_vet_core::LineIndex::new(source);
  let (facts, stats) = super::lifetime::collect_with_visits(&semantic, &line_index, source, 0);
  (facts.late_cancellation_guards.len(), stats.settlement_inner_work())
}

#[test]
fn late_cancellation_guard_fires_on_bound_oncleanup_after_await() {
  let facts = settlement_facts(
    "watch(source, async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
  );
  assert_eq!(
    facts.late_cancellation_guards.len(),
    1,
    "late bound onCleanup must emit: {:?}",
    facts.late_cancellation_guards
  );
}

#[test]
fn late_cancellation_guard_stays_quiet_when_registered_before_await() {
  let facts = settlement_facts(
    "watch(source, async (value, _previous, onCleanup) => {\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       const data = await Promise.resolve(value);\
       if (!cancelled) result.value = data;\
     });",
  );
  assert!(
    facts.late_cancellation_guards.is_empty(),
    "sync registration must stay quiet: {:?}",
    facts.late_cancellation_guards
  );
}

#[test]
fn late_cancellation_guard_stays_quiet_for_generation_and_equality() {
  let generation = settlement_facts(
    "let generation = 0;\
     watch(source, async (value) => {\
       const current = (generation += 1);\
       const data = await Promise.resolve(value);\
       if (current === generation) result.value = data;\
     });",
  );
  let equality = settlement_facts(
    "watch(source, async (value) => {\
       const data = await Promise.resolve(value);\
       if (source.value === value) result.value = data;\
     });",
  );
  assert!(
    generation.late_cancellation_guards.is_empty() && equality.late_cancellation_guards.is_empty(),
    "generation/equality must stay quiet: {generation:?} {equality:?}"
  );
}

#[test]
fn late_cancellation_guard_skips_onwatcher_cleanup_owner_loss() {
  let facts = analyze(
    "import { onWatcherCleanup, ref, watch } from 'vue';\
     const source = ref('one');\
     const result = ref(null);\
     watch(source, async (value) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onWatcherCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
    "ts",
  )
  .lifetime;
  assert!(
    facts.late_cancellation_guards.is_empty(),
    "onWatcherCleanup after await belongs to no-late-watcher-cleanup: {:?}",
    facts.late_cancellation_guards
  );
  assert_eq!(facts.late_watcher_cleanups.len(), 1);
}

#[test]
fn late_cancellation_guard_covers_alias_watch_effect_and_named_callback() {
  let alias = analyze(
    "import { ref, watch as vueWatch } from 'vue';\
     const source = ref('one');\
     const result = ref(null);\
     vueWatch(source, async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
    "ts",
  )
  .lifetime;
  let effect = settlement_facts(
    "watchEffect(async (onCleanup) => {\
       const data = await Promise.resolve(source.value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
  );
  let named = settlement_facts(
    "const load = async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     };\
     watch(source, load);",
  );
  assert_eq!(alias.late_cancellation_guards.len(), 1, "{alias:?}");
  assert_eq!(effect.late_cancellation_guards.len(), 1, "{effect:?}");
  assert_eq!(named.late_cancellation_guards.len(), 1, "{named:?}");
}

#[test]
fn late_cancellation_guard_unknown_shapes_stay_quiet() {
  for body in [
    "watch(source, async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       for (const _ of [0]) onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
    "watch(source, async (value, _previous, onCleanup) => {\
       await Promise.resolve();\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
    "watch(source, async (value, _previous, onCleanup) => {\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = value;\
     });",
    "function hold(flag) {}\
     watch(source, async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       hold(cancelled);\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
    "watch(source, async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     }, { once: true });",
    "const opts = { once: true };\
     watch(source, async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     }, opts);",
  ] {
    let facts = settlement_facts(body);
    assert!(
      facts.late_cancellation_guards.is_empty(),
      "unknown/safe shape must stay quiet: {body} {:?}",
      facts.late_cancellation_guards
    );
  }
}

#[test]
fn late_cancellation_guard_scales_near_linearly() {
  fn source(count: usize, kind: &str) -> String {
    let mut out = String::from("import { ref, watch } from 'vue';\nconst source = ref('one');\n");
    match kind {
      "independent" => {
        for index in 0..count {
          out.push_str("const r");
          out.push_str(&index.to_string());
          out.push_str(
            " = ref(null);\nwatch(source, async (value, _previous, onCleanup) => {\
             const data = await Promise.resolve(value);\
             let cancelled = false;\
             onCleanup(() => { cancelled = true; });\
             if (!cancelled) r",
          );
          out.push_str(&index.to_string());
          out.push_str(".value = data; });\n");
        }
      }
      "shared-source" => {
        out.push_str("const result = ref(null);\n");
        for index in 0..count {
          out.push_str(
            "watch(source, async (value, _previous, onCleanup) => {\
             const data = await Promise.resolve(value);\
             let cancelled = false;\
             onCleanup(() => { cancelled = true; });\
             if (!cancelled) result.value = data; void ",
          );
          out.push_str(&index.to_string());
          out.push_str("; });\n");
        }
      }
      "aliases" => {
        out.push_str("const result = ref(null);\nconst s0 = source;\n");
        for index in 1..=count {
          out.push_str("const s");
          out.push_str(&index.to_string());
          out.push_str(" = s");
          out.push_str(&(index - 1).to_string());
          out.push_str(";\n");
        }
        out.push_str(
          "watch(s0, async (value, _previous, onCleanup) => {\
           const data = await Promise.resolve(value);\
           let cancelled = false;\
           onCleanup(() => { cancelled = true; });\
           if (!cancelled) result.value = data; });\n",
        );
      }
      "wrappers" => {
        out.push_str(
          "const result = ref(null);\nwatch(source, async (value, _previous, onCleanup) => {\n",
        );
        out.push_str("  const data = await Promise.resolve(");
        for _ in 0..count {
          out.push_str("(((((");
        }
        out.push_str("value");
        for _ in 0..count {
          out.push_str(")))))");
        }
        out.push_str(
          ");\n  let cancelled = false;\n  onCleanup(() => { cancelled = true; });\n  if (!cancelled) result.value = data;\n});\n",
        );
      }
      _ => {
        out.push_str(
          "const result = ref(null);\nwatch(source, async (value, _previous, onCleanup) => {\n",
        );
        for index in 0..count {
          out.push_str("  void ");
          out.push_str(&index.to_string());
          out.push_str(";\n");
        }
        out.push_str(
          "  const data = await Promise.resolve(value);\n  let cancelled = false;\n  onCleanup(() => { cancelled = true; });\n  if (!cancelled) result.value = data;\n});\n",
        );
      }
    }
    out
  }
  for kind in ["independent", "shared-source", "aliases", "wrappers", "irrelevant"] {
    let (facts_20, work_20) = settlement_work(&source(20, kind));
    let (facts_40, work_40) = settlement_work(&source(40, kind));
    let (facts_80, work_80) = settlement_work(&source(80, kind));
    if kind == "independent" || kind == "shared-source" {
      assert_eq!((facts_20, facts_40, facts_80), (20, 40, 80), "{kind} facts");
    } else {
      assert_eq!((facts_20, facts_40, facts_80), (1, 1, 1), "{kind} facts");
    }
    assert!(
      work_80 <= work_20.saturating_mul(6).saturating_add(256),
      "{kind} settlement work must stay near-linear: 20={work_20} 40={work_40} 80={work_80}"
    );
  }
}

#[test]
fn late_cancellation_guard_stays_quiet_for_sync_onwatchercleanup_plus_late_bound() {
  let facts = analyze(
    "import { onWatcherCleanup, ref, watch } from 'vue';\
     const source = ref('one');\
     const result = ref(null);\
     watch(source, async (value, _previous, onCleanup) => {\
       let cancelled = false;\
       onWatcherCleanup(() => { cancelled = true; });\
       const data = await Promise.resolve(value);\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
    "ts",
  )
  .lifetime;
  assert!(
    facts.late_cancellation_guards.is_empty(),
    "sync onWatcherCleanup already guards the flag: {:?}",
    facts.late_cancellation_guards
  );
}

#[test]
fn late_cancellation_guard_still_fires_when_once_is_literally_false() {
  let facts = settlement_facts(
    "watch(source, async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     }, { once: false });",
  );
  assert_eq!(
    facts.late_cancellation_guards.len(),
    1,
    "closed once: false must not abstain: {:?}",
    facts.late_cancellation_guards
  );
}

#[test]
fn settlement_test_counter_has_storage() {
  // Production SettlementWork is a ZST via the `const _` assert in
  // lifetime/mod.rs under `cfg(not(test))`. Test builds keep saturating
  // fields so scaling tests observe real work — same contract as
  // source_contracts::stats::WorkCounter.
  let size = super::lifetime::settlement_counter_layout();
  assert!(size > 0, "test SettlementWork stores counters, got {size}");
}

#[test]
fn private_receiver_emits_method_and_getter_and_stays_quiet_without_demand() {
  let (method, _) = contract_stats(
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
  );
  assert_eq!(method.reactive_private_field_access.len(), 1, "{method:?}");
  let (getter, _) = contract_stats(
    "import { shallowReactive } from 'vue'; class Counter { #n = 1; get value() { return this.#n } } const proxy = shallowReactive(new Counter()); void proxy.value;",
  );
  assert_eq!(getter.reactive_private_field_access.len(), 1, "{getter:?}");
  assert!(
    getter.reactive_private_field_access.first().is_some_and(|site| site.getter),
    "{getter:?}"
  );
  let (unused, _) = contract_stats(
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } void reactive(new Counter());",
  );
  assert!(unused.reactive_private_field_access.is_empty(), "{unused:?}");
  let (class_only, _) = contract_stats("class Counter { #n = 1 }");
  assert!(class_only.reactive_private_field_access.is_empty(), "{class_only:?}");
}

#[test]
fn private_receiver_source_alias_and_chained_and_this_alias() {
  let (wrap, _) = contract_stats(
    "import { reactive } from 'vue'; const wrap = reactive; class Counter { #n = 1; read() { return this.#n } } const proxy = wrap(new Counter()); void proxy.read();",
  );
  assert_eq!(wrap.reactive_private_field_access.len(), 1, "{wrap:?}");
  let (chained, _) = contract_stats(
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } void reactive(new Counter()).read();",
  );
  assert_eq!(chained.reactive_private_field_access.len(), 1, "{chained:?}");
  let (alias, _) = contract_stats(
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { const self = this; return self.#n } } const proxy = reactive(new Counter()); void proxy.read();",
  );
  assert_eq!(alias.reactive_private_field_access.len(), 1, "{alias:?}");
}

#[test]
fn private_receiver_safe_controls_stay_quiet() {
  for source in [
    "import { reactive } from 'vue'; class Counter { #n = 1; read = () => this.#n } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; constructor() { this.read = this.read.bind(this) } read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { n = 1; read() { return this.n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { private n = 1; read() { return this.n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { static #n = 1; read() { return Counter.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; readOther(other: Counter) { return other.#n } } const raw = new Counter(); const proxy = reactive(new Counter()); void proxy.readOther(raw);",
    "import { reactive } from 'vue'; class Counter { #n = 1; guarded() { return #n in this ? this.#n : 0 } } const proxy = reactive(new Counter()); void proxy.guarded();",
    "import { reactive, toRaw } from 'vue'; class Counter { #n = 1; read() { return toRaw(this).#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { markRaw, reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = reactive(markRaw(new Counter())); void proxy.read();",
    "import { reactive } from 'vue'; class Replacement { #n = 3; constructor() { return { read: () => 4 } } read() { return this.#n } } const proxy = reactive(new Replacement()); void proxy.read();",
    "import { reactive } from 'vue'; class Base { #n = 1; read() { return this.#n } } class Child extends Base {} const proxy = reactive(new Child()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } Counter.prototype.read = function read() { return 0 }; const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } function retain(ctor: typeof Counter) { void ctor } retain(Counter); const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; function helper() {} class Counter { #n = 1; read() { helper(); return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = reactive(new Counter()); false && proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; constructor() { void new.target } read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = reactive(new Counter()); proxy.read = () => 0; void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = reactive(new Counter()); const fn = proxy.read; void fn();",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.reactive_private_field_access.is_empty(),
      "private-receiver safe probe must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn private_receiver_readonly_family_and_return_demand_report() {
  let (readonly_api, _) = contract_stats(
    "import { readonly } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = readonly(new Counter()); void proxy.read();",
  );
  assert_eq!(readonly_api.reactive_private_field_access.len(), 1, "{readonly_api:?}");
  assert_eq!(
    readonly_api.reactive_private_field_access.first().map(|site| site.api.as_str()),
    Some("readonly"),
    "{readonly_api:?}"
  );
  let (shallow_ro, _) = contract_stats(
    "import { shallowReadonly } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = shallowReadonly(new Counter()); void proxy.read();",
  );
  assert_eq!(shallow_ro.reactive_private_field_access.len(), 1, "{shallow_ro:?}");
  let (returned, _) = contract_stats(
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } function setup() { const proxy = reactive(new Counter()); return proxy.read() } setup();",
  );
  assert_eq!(returned.reactive_private_field_access.len(), 1, "{returned:?}");
}

#[test]
fn private_receiver_class_shape_repairs_stay_precise() {
  for source in [
    "import { reactive } from 'vue'; class Counter { #n = 1; __v_skip = true; read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; __v_raw = {}; read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; [Symbol.toStringTag] = 'Counter'; read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; get [Symbol.toStringTag]() { return 'Counter' } read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; patched = (this.read = () => 0); read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; async read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; const proxy = reactive(new Counter()); void proxy.read(); class Counter { #n = 1; read() { return this.#n } }",
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = reactive(new Counter()); proxy.other = 1; void proxy.read();",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.reactive_private_field_access.is_empty(),
      "private-receiver repair quiet probe must stay silent: {source} => {facts:?}"
    );
  }
  let shadowed = analyze(
    "import { reactive } from 'vue'; class Counter { #n = 1; read = function () { return 1 }; read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "js",
  );
  assert!(
    shadowed.source_contracts.reactive_private_field_access.is_empty(),
    "JS own-field shadow must stay silent: {shadowed:?}"
  );
  let (ctor, _) = contract_stats(
    "import { reactive } from 'vue'; class Counter { #n; constructor() { this.#n = 5; this.items = [] } read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
  );
  assert_eq!(ctor.reactive_private_field_access.len(), 1, "{ctor:?}");
  let (in_arg, _) = contract_stats(
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return String(this.#n) } } const proxy = reactive(new Counter()); void proxy.read();",
  );
  assert_eq!(in_arg.reactive_private_field_access.len(), 1, "{in_arg:?}");
}

#[test]
fn private_receiver_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { reactive } from 'vue';");
    for index in 0..size {
      source.push_str("class C");
      source.push_str(&index.to_string());
      source.push_str(" { #n = 1; read() { return this.#n } } const p");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(new C");
      source.push_str(&index.to_string());
      source.push_str("()); void p");
      source.push_str(&index.to_string());
      source.push_str(".read();");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.reactive_private_field_access.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "private-receiver work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn private_receiver_shared_class_and_alias_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; class Shared { #n = 1; read() { return this.#n } }",
    );
    for index in 0..size {
      source.push_str("const wrap");
      source.push_str(&index.to_string());
      source.push_str(" = reactive; const p");
      source.push_str(&index.to_string());
      source.push_str(" = wrap");
      source.push_str(&index.to_string());
      source.push_str("(new Shared()); const a");
      source.push_str(&index.to_string());
      source.push_str(" = p");
      source.push_str(&index.to_string());
      source.push_str("; void a");
      source.push_str(&index.to_string());
      source.push_str(".read();");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.reactive_private_field_access.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-class private-receiver work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn private_receiver_deeper_method_bodies_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { reactive } from 'vue'; class Deep { #n = 1; read() {");
    for index in 0..size {
      source.push_str("const v");
      source.push_str(&index.to_string());
      source.push_str(" = ");
      source.push_str(&index.to_string());
      source.push(';');
    }
    source.push_str(" return this.#n } } const proxy = reactive(new Deep()); void proxy.read();");
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.reactive_private_field_access.len(), 1, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "deep-method private-receiver work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn until_timeout_unmatched_demand_and_controls() {
  let (positive, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(0); const value = await until(source).toBe('ready', { timeout: 5 }); value.toUpperCase(); }",
  );
  assert_eq!(positive.until_timeout_unmatched_demand.len(), 1, "{positive:?}");
  let (shared, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/shared'; async function run() { const source = ref(0); const value = await until(source).toBe('ready', { timeout: 5 }); value.toUpperCase(); }",
  );
  assert_eq!(shared.until_timeout_unmatched_demand.len(), 1, "{shared:?}");
  let (number_expected, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(''); const value = await until(source).toBe(1, { timeout: 5 }); value.toFixed(1); }",
  );
  assert_eq!(number_expected.until_timeout_unmatched_demand.len(), 1, "{number_expected:?}");
  let (boolean_source, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(false); const value = await until(source).toBe('ready', { timeout: 5 }); value.trim(); }",
  );
  assert_eq!(boolean_source.until_timeout_unmatched_demand.len(), 1, "{boolean_source:?}");
  let (null_source, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(null); const value = await until(source).toBe('ready', { timeout: 5 }); value.toUpperCase(); }",
  );
  assert_eq!(null_source.until_timeout_unmatched_demand.len(), 1, "{null_source:?}");
  let (optional_primitive, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(0); const value = await until(source).toBe('ready', { timeout: 5 }); value?.toUpperCase(); }",
  );
  assert_eq!(optional_primitive.until_timeout_unmatched_demand.len(), 1, "{optional_primitive:?}");
  let (two_awaits, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(0); const first = await until(source).toBe('ready', { timeout: 5 }); source.value = 'ready'; const second = await until(source).toBe('ready', { timeout: 5 }); second.toUpperCase(); first.toUpperCase(); }",
  );
  assert_eq!(two_awaits.until_timeout_unmatched_demand.len(), 1, "{two_awaits:?}");
  let (zero_timeout, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(0); const value = await until(source).toBe('ready', { timeout: 0 }); value.toUpperCase(); }",
  );
  assert_eq!(zero_timeout.until_timeout_unmatched_demand.len(), 1, "{zero_timeout:?}");
  let (negative_timeout, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(0); const value = await until(source).toBe('ready', { timeout: -5 }); value.toUpperCase(); }",
  );
  assert_eq!(negative_timeout.until_timeout_unmatched_demand.len(), 1, "{negative_timeout:?}");
  let (matched, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref('ready'); const value = await until(source).toBe('ready', { timeout: 5 }); value.toUpperCase(); }",
  );
  assert!(matched.until_timeout_unmatched_demand.is_empty(), "{matched:?}");
  let (current, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(0); const value = await until(source).toBe('ready', { timeout: 5 }); value.toFixed(1); }",
  );
  assert!(current.until_timeout_unmatched_demand.is_empty(), "{current:?}");
  let (throws, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(0); const value = await until(source).toBe('ready', { timeout: 5, throwOnTimeout: true }); value.toUpperCase(); }",
  );
  assert!(throws.until_timeout_unmatched_demand.is_empty(), "{throws:?}");
  let (intervening, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(0); const pending = until(source).toBe('ready', { timeout: 5 }); source.value = 'ready'; const value = await pending; value.toUpperCase(); }",
  );
  assert!(intervening.until_timeout_unmatched_demand.is_empty(), "{intervening:?}");
  let (escape, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; function write(target: { value: unknown }) { target.value = 'ready' } async function run() { const source = ref(0); write(source); const value = await until(source).toBe('ready', { timeout: 5 }); value.toUpperCase(); }",
  );
  assert!(escape.until_timeout_unmatched_demand.is_empty(), "{escape:?}");
  let (conditional, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(0); const flag = false; if (flag) { source.value = 'other' } const value = await until(source).toBe(1, { timeout: 5 }); value.toFixed(1); }",
  );
  assert!(conditional.until_timeout_unmatched_demand.is_empty(), "{conditional:?}");
  let (looped, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(0); for (const _ of []) { source.value = 'other' } const value = await until(source).toBe(1, { timeout: 5 }); value.toFixed(1); }",
  );
  assert!(looped.until_timeout_unmatched_demand.is_empty(), "{looped:?}");
  let (or_assign, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(0); const pending = until(source).toBe('ready', { timeout: 5 }); source.value ||= 'ready'; const value = await pending; value.toUpperCase(); }",
  );
  assert!(or_assign.until_timeout_unmatched_demand.is_empty(), "{or_assign:?}");
  let (plus_assign, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(0); const pending = until(source).toBe('0ready', { timeout: 5 }); source.value += 'ready'; const value = await pending; value.toUpperCase(); }",
  );
  assert!(plus_assign.until_timeout_unmatched_demand.is_empty(), "{plus_assign:?}");
  let (updated, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(true); const pending = until(source).toBe(1, { timeout: 5 }); source.value++; const value = await pending; value.toFixed(1); }",
  );
  assert!(updated.until_timeout_unmatched_demand.is_empty(), "{updated:?}");
  let (destructure, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(0); const pending = until(source).toBe('ready', { timeout: 5 }); [source.value] = ['ready']; const value = await pending; value.toUpperCase(); }",
  );
  assert!(destructure.until_timeout_unmatched_demand.is_empty(), "{destructure:?}");
  let (reassigned, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(0); let value = await until(source).toBe('ready', { timeout: 5 }); value = String(value); value.toUpperCase(); }",
  );
  assert!(reassigned.until_timeout_unmatched_demand.is_empty(), "{reassigned:?}");
  let (optional_nullish, _) = contract_stats(
    "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(null); const value = await until(source).toBe('ready', { timeout: 5 }); value?.toUpperCase(); }",
  );
  assert!(optional_nullish.until_timeout_unmatched_demand.is_empty(), "{optional_nullish:?}");
}

#[test]
fn until_timeout_shared_consumers_scale_linearly() {
  let mut previous: Option<(u64, u64)> = None;
  let mut at_64: Option<u64> = None;
  for size in [64_u64, 128, 256] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { until } from '@vueuse/core'; async function run() { const source = ref(0);",
    );
    for index in 0..size {
      source.push_str("const v");
      source.push_str(&index.to_string());
      source.push_str(" = await until(source).toBe('ready', { timeout: 5 }); v");
      source.push_str(&index.to_string());
      source.push_str(".toUpperCase();");
    }
    source.push('}');
    let (contracts, stats) = contract_full_stats(&source);
    let queries = stats.queries;
    assert_eq!(
      contracts.until_timeout_unmatched_demand.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "{contracts:?}"
    );
    if size == 64 {
      at_64 = Some(queries);
    }
    if let Some((prev_size, prev_queries)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        queries.saturating_mul(10) < prev_queries.saturating_mul(25),
        "shared until await-lookup queries grew from {prev_queries} to {queries} on {prev_size}->{size}"
      );
    }
    if size == 256
      && let Some(baseline) = at_64
    {
      assert!(
        queries.saturating_mul(10) < baseline.saturating_mul(63),
        "shared until N=256 queries {queries} vs N=64 {baseline} must stay within 2.5x per doubling"
      );
    }
    previous = Some((size, queries));
  }
}

const INJECT_BASIC: &str = "import { inject, provide } from 'vue'; const key = Symbol('count'); provide(key, 7); const count = inject(key, 'missing'); count.toFixed(2);";

#[test]
fn injection_demand_reports_same_instance_symbol_fallback() {
  let (facts, _) = contract_stats(INJECT_BASIC);
  assert_eq!(facts.inject_same_instance_provide.len(), 1, "{facts:?}");
  let Some(site) = facts.inject_same_instance_provide.first() else {
    return;
  };
  assert_eq!(site.member, "toFixed");
  assert_eq!(site.fallback_kind, vue_vet_core::PrimitiveValueKind::String);
  assert_eq!(site.provided_kind, vue_vet_core::PrimitiveValueKind::Number);
  assert_eq!(site.demand_span.length, "count.toFixed(2)".len(), "{site:?}");
}

#[test]
fn injection_demand_survives_later_await() {
  let (facts, _) = contract_stats(
    "import { inject, provide } from 'vue'; const key = Symbol('count'); provide(key, 7); const count = inject(key, 'missing'); await Promise.resolve(); count.toFixed(2);",
  );
  assert_eq!(facts.inject_same_instance_provide.len(), 1, "{facts:?}");
}

#[test]
fn injection_demand_chained_and_factory_and_absent_default() {
  let (chained, _) = contract_stats(
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); inject(key, 'missing').toFixed(2);",
  );
  assert_eq!(chained.inject_same_instance_provide.len(), 1, "{chained:?}");
  let (factory, _) = contract_stats(
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, () => 'missing', true); count.toFixed(2);",
  );
  assert_eq!(factory.inject_same_instance_provide.len(), 1, "{factory:?}");
  let (absent, _) = contract_stats(
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key); count.toFixed(2);",
  );
  assert_eq!(absent.inject_same_instance_provide.len(), 1, "{absent:?}");
  let Some(site) = absent.inject_same_instance_provide.first() else {
    return;
  };
  assert_eq!(site.fallback_kind, vue_vet_core::PrimitiveValueKind::Nullish);
  assert!(site.default_absent, "{site:?}");
}

#[test]
fn injection_demand_typescript_wrappers_unary_computed_and_optional() {
  for (source, demand) in [
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); count!.toFixed(2);",
      "count!.toFixed(2)",
    ),
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); (count as number).toFixed(2);",
      "(count as number).toFixed(2)",
    ),
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); (count satisfies unknown as number).toFixed(2);",
      "(count satisfies unknown as number).toFixed(2)",
    ),
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key); count!.toFixed(2);",
      "count!.toFixed(2)",
    ),
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject<number>(key); count!.toFixed(2);",
      "count!.toFixed(2)",
    ),
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 'text'); const label = inject(key, -1); label.toUpperCase();",
      "label.toUpperCase()",
    ),
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); count['toFixed'](2);",
      "count['toFixed'](2)",
    ),
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); count?.toFixed(2);",
      "count?.toFixed(2)",
    ),
  ] {
    let (facts, _) = contract_stats(source);
    assert_eq!(facts.inject_same_instance_provide.len(), 1, "{source} => {facts:?}");
    let Some(site) = facts.inject_same_instance_provide.first() else {
      continue;
    };
    let Some(offset) = source.find(demand) else {
      continue;
    };
    assert_eq!(site.demand_span.offset, offset, "{demand} {site:?}");
    assert_eq!(site.demand_span.length, demand.len(), "{demand} {site:?}");
  }
}

#[test]
fn injection_demand_unicode_crlf_and_namespace() {
  let source = "import * as Vue from 'vue';\r\nconst 键 = Symbol('count');\r\nVue.provide(键, 7);\r\nconst 计数 = Vue.inject(键, 'missing');\r\n计数.toFixed(2);\r\n";
  let (facts, _) = contract_stats(source);
  assert_eq!(facts.inject_same_instance_provide.len(), 1, "{facts:?}");
  let Some(site) = facts.inject_same_instance_provide.first() else {
    return;
  };
  let demand = "计数.toFixed(2)";
  let offset = source.find(demand).unwrap_or(usize::MAX);
  let (line, column) = vue_vet_core::LineIndex::new(source).byte_to_line_column(offset);
  assert_eq!(site.demand_span.offset, offset, "{site:?}");
  assert_eq!(site.demand_span.length, demand.len(), "{site:?}");
  assert_eq!(site.demand_span.line, line, "{site:?}");
  assert_eq!(site.demand_span.column, column, "{site:?}");
  assert_eq!(site.demand_span.line, 5, "{site:?}");
  assert_eq!(site.demand_span.column, 1, "{site:?}");
}

#[test]
fn injection_demand_safe_probes_stay_quiet() {
  for source in [
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 0); count.toFixed(2);",
    "import { inject, provide } from 'vue'; provide('count', 7); const count = inject('count', 'missing'); count.toFixed(2);",
    "import { inject, provide } from 'vue'; const key = Symbol.for('count'); provide(key, 7); const count = inject(key, 'missing'); count.toFixed(2);",
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key); count?.toFixed(2);",
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); count?.toFixed?.(2);",
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); if (typeof count === 'number') count.toFixed(2);",
    "import { inject, provide } from 'vue'; function inner() { const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); count.toFixed(2); } inner();",
    "import { inject, provide } from 'vue'; function leak(key: symbol) { void key } const key = Symbol(); leak(key); provide(key, 7); const count = inject(key, 'missing'); count.toFixed(2);",
    "import { inject, provide } from 'vue'; Number.prototype.toFixed = String.prototype.toUpperCase; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); count.toFixed(2);",
    "import { inject, provide } from 'vue'; const Symbol = () => 'x'; const key = Symbol(); provide(key as never, 7); const count = inject(key as never, 'missing'); count.toFixed(2);",
    "import { inject, provide } from 'vue'; const flag = true; const key = Symbol(); provide(key, 7); const count = inject(key, () => 'missing', flag); count.toFixed(2);",
    "import { inject, provide } from 'vue'; let key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); count.toFixed(2);",
    "import { inject } from 'vue'; const key = Symbol(); const count = inject(key, 'missing'); count.toFixed(2);",
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); provide(key, 'text'); const count = inject(key, 'missing'); count.toFixed(2);",
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 'text'); const count = inject(key, 'missing'); count.toUpperCase();",
    "import Vue from 'vue'; const key = Symbol(); Vue.provide(key, 7); const count = Vue.inject(key, 'missing'); count.toFixed(2);",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.inject_same_instance_provide.is_empty(),
      "safe probe must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn injection_demand_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [20_u64, 80] {
    let mut source = String::from("import { inject, provide } from 'vue';");
    for index in 0..size {
      source.push_str("const k");
      source.push_str(&index.to_string());
      source.push_str(" = Symbol(); provide(k");
      source.push_str(&index.to_string());
      source.push_str(", 7); const c");
      source.push_str(&index.to_string());
      source.push_str(" = inject(k");
      source.push_str(&index.to_string());
      source.push_str(", 'missing'); c");
      source.push_str(&index.to_string());
      source.push_str(".toFixed(2);");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.inject_same_instance_provide.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 4);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(50),
        "injection work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn injection_demand_many_ops_under_one_setup_and_alias_chain() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [20_u64, 80] {
    let mut source =
      String::from("import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7);");
    for index in 0..size {
      source.push_str("const c");
      source.push_str(&index.to_string());
      source.push_str(" = inject(key, 'missing'); c");
      source.push_str(&index.to_string());
      source.push_str(".toFixed(2);");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.inject_same_instance_provide.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 4);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(50),
        "shared-key injection work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
  let mut source = String::from("import { inject, provide } from 'vue'; const k0 = Symbol();");
  for index in 1..16 {
    source.push_str("const k");
    source.push_str(&index.to_string());
    source.push_str(" = k");
    source.push_str(&(index - 1).to_string());
    source.push(';');
  }
  source.push_str("provide(k15, 7); const count = inject(k0, 'missing'); count.toFixed(2);");
  let (facts, _) = contract_stats(&source);
  assert_eq!(facts.inject_same_instance_provide.len(), 1, "{facts:?}");
}

#[test]
fn injection_demand_order_is_deterministic() {
  let first = contract_stats(INJECT_BASIC);
  let second = contract_stats(INJECT_BASIC);
  assert_eq!(first.0.inject_same_instance_provide, second.0.inject_same_instance_provide);
}

const IGNORABLE_POSITIVE: &str = "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core';\
const source = ref(0); const seen: number[] = [];\
const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' });\
void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });";

const SHARED_POSITIVE: &str = "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core';\
const useValue = createSharedComposable((value: string | number) => ref(value));\
const first = useValue(1); const second = useValue('text');\
void second.value.toUpperCase();";

#[test]
fn vueuse_ignorable_async_write_emits_and_controls_stay_quiet() {
  let (positive, _) = contract_stats(IGNORABLE_POSITIVE);
  assert_eq!(positive.ignorable_async_ignore_window.len(), 1, "{positive:?}");
  let (nested, _) = contract_stats(
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core';\
     const source = ref(0); const seen: number[] = [];\
     const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' });\
     void ignoreUpdates(async () => { await Promise.resolve(); ignoreUpdates(() => { source.value = 2; }); });",
  );
  assert!(nested.ignorable_async_ignore_window.is_empty(), "{nested:?}");
  let (same, _) = contract_stats(
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core';\
     const source = ref(0); const seen: number[] = [];\
     const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' });\
     void ignoreUpdates(async () => { await Promise.resolve(); source.value = 0; });",
  );
  assert!(same.ignorable_async_ignore_window.is_empty(), "{same:?}");
  let (immediate, _) = contract_stats(
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core';\
     const source = ref(0); const seen: number[] = [];\
     const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync', immediate: true });\
     void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
  );
  assert_eq!(immediate.ignorable_async_ignore_window.len(), 1, "{immediate:?}");
  let (once, _) = contract_stats(
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core';\
     const source = ref(0); const seen: number[] = [];\
     const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync', once: true });\
     void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
  );
  assert_eq!(once.ignorable_async_ignore_window.len(), 1, "{once:?}");
}

#[test]
fn vueuse_shared_incompatible_demand_emits_and_numeric_seeds_stay_quiet() {
  let (positive, _) = contract_stats(SHARED_POSITIVE);
  assert_eq!(positive.shared_composable_first_instance_args.len(), 1, "{positive:?}");
  let (numbers, _) = contract_stats(
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core';\
     const useValue = createSharedComposable((value: number) => ref(value));\
     const first = useValue(1); const second = useValue(2); first.value = 3;\
     void second.value.toFixed(0);",
  );
  assert!(numbers.shared_composable_first_instance_args.is_empty(), "{numbers:?}");
  let (global, _) = contract_stats(
    "import { ref } from 'vue'; import { createGlobalState } from '@vueuse/core';\
     const useValue = createGlobalState((value: string | number) => ref(value));\
     const first = useValue(1); const second = useValue('text');\
     void second.value.toUpperCase();",
  );
  assert_eq!(global.shared_composable_first_instance_args.len(), 1, "{global:?}");
  let (nullish, _) = contract_stats(
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core';\
     const useValue = createSharedComposable((value: string | null) => ref(value));\
     const first = useValue(null); const second = useValue('text');\
     void second.value.toUpperCase();",
  );
  assert_eq!(nullish.shared_composable_first_instance_args.len(), 1, "{nullish:?}");
}

#[test]
fn vueuse_aliases_namespace_and_shared_package() {
  let (alias, _) = contract_stats(
    "import { ref } from 'vue'; import { watchIgnorable as ignorable } from '@vueuse/core';\
     const source = ref(0); const seen: number[] = [];\
     const { ignoreUpdates } = ignorable(source, (value) => { seen.push(value); }, { flush: 'sync' });\
     void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
  );
  assert_eq!(alias.ignorable_async_ignore_window.len(), 1, "{alias:?}");
  let (shared_pkg, _) = contract_stats(
    "import { ref } from 'vue'; import { ignorableWatch } from '@vueuse/shared';\
     const source = ref(0); const seen: number[] = [];\
     const { ignoreUpdates } = ignorableWatch(source, (value) => { seen.push(value); }, { flush: 'sync' });\
     void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
  );
  assert_eq!(shared_pkg.ignorable_async_ignore_window.len(), 1, "{shared_pkg:?}");
  let (namespace, _) = contract_stats(
    "import { ref } from 'vue'; import * as VueUse from '@vueuse/core';\
     const useValue = VueUse.createSharedComposable((value: string | number) => ref(value));\
     const first = useValue(1); const second = useValue('text');\
     void second.value.toUpperCase();",
  );
  assert_eq!(namespace.shared_composable_first_instance_args.len(), 1, "{namespace:?}");
}

#[test]
fn vueuse_review_safe_probes_stay_quiet() {
  for source in [
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const other = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); void ignoreUpdates(async () => { await Promise.resolve(); other.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'pre' }); void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }); void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync', eventFilter: (invoke) => invoke() }); void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); void ignoreUpdates(async () => { await Promise.resolve(); const later = () => { source.value = 2; }; void later; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates, stop } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); stop(); void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const first = useValue(1); const second = useValue('text'); if (typeof second.value === 'string') void second.value.toUpperCase();",
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const first = useValue(1); const second = useValue('text'); void second.value?.toUpperCase();",
    "import { effectScope, ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const firstOwner = effectScope(); firstOwner.run(() => useValue(1)); firstOwner.stop(); const secondOwner = effectScope(); const second = secondOwner.run(() => useValue('text')); void second?.value.toUpperCase();",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); source.value = 2; void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); source.value += 2; void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); void ignoreUpdates(async () => { source.value = Number('2'); await Promise.resolve(); source.value = 2; });",
    "import { ref, watch } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const trigger = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); watch(trigger, () => { source.value = 2; }, { flush: 'sync' }); trigger.value = 1; void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync', immediate: true, once: true }); void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates, stop } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); void ignoreUpdates(async () => { await Promise.resolve(); stop(); source.value = 2; });",
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const seed = String(Date.now()); const zero = useValue(seed); const first = useValue(1); const second = useValue('text'); void second.value.toUpperCase();",
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const args: [string] = ['seed']; const zero = useValue(...args); const first = useValue(1); const second = useValue('text'); void second.value.toUpperCase();",
    "import { effectScope, ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const seeded = useValue('seed'); const owner = effectScope(); owner.run(() => { const first = useValue(1); const second = useValue('text'); void second.value.toUpperCase(); });",
    "import { ref } from 'vue'; import { createGlobalState } from '@vueuse/core'; const useValue = createGlobalState((value: string | number) => ref(value)); const seeded = useValue('seed'); function load() { const first = useValue(1); const second = useValue('text'); void second.value.toUpperCase(); } load();",
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const first = useValue('text'); const second = useValue(1); const third = useValue('text'); void third.value.toUpperCase();",
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const first = useValue(1); const second = useValue('text'); const third = useValue(2); third.value = 'fixed'; void second.value.toUpperCase();",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.ignorable_async_ignore_window.is_empty()
        && facts.shared_composable_first_instance_args.is_empty(),
      "safe probe must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn vueuse_demand_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { watchIgnorable, createSharedComposable } from '@vueuse/core';",
    );
    source.push_str("const source = ref(0); const seen: number[] = [];");
    for index in 0..size {
      source.push_str("const { ignoreUpdates: skip");
      source.push_str(&index.to_string());
      source.push_str(
        " } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' });",
      );
      source.push_str("void skip");
      source.push_str(&index.to_string());
      source.push_str("(async () => { await Promise.resolve(); source.value = ");
      source.push_str(&(index + 1).to_string());
      source.push_str("; });");
      source.push_str("const use");
      source.push_str(&index.to_string());
      source.push_str(" = createSharedComposable((value: string | number) => ref(value)); const a");
      source.push_str(&index.to_string());
      source.push_str(" = use");
      source.push_str(&index.to_string());
      source.push_str("(1); const b");
      source.push_str(&index.to_string());
      source.push_str(" = use");
      source.push_str(&index.to_string());
      source.push_str("('text'); void b");
      source.push_str(&index.to_string());
      source.push_str(".value.toUpperCase();");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.ignorable_async_ignore_window.len(), expected, "{contracts:?}");
    assert_eq!(contracts.shared_composable_first_instance_args.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "vueuse demand work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn vueuse_demand_one_wrapper_growth_stays_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [20_u64, 80] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core';\
       const useValue = createSharedComposable((value: string | number) => ref(value));",
    );
    for index in 0..size {
      source.push_str("const item");
      source.push_str(&index.to_string());
      if index % 2 == 0 {
        source.push_str(" = useValue(1); void item");
        source.push_str(&index.to_string());
        source.push_str(".value.toFixed(0);");
      } else {
        source.push_str(" = useValue('text'); void item");
        source.push_str(&index.to_string());
        source.push_str(".value.toUpperCase();");
      }
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.shared_composable_first_instance_args.len(), 1, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 4);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(90),
        "one-wrapper vueuse demand work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn filter_settlement_emits_cancelled_demand_and_stays_quiet_without_await() {
  let (positive, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(positive.cancelled_filter_promise_demand.len(), 1, "{positive:?}");
  let Some(site) = positive.cancelled_filter_promise_demand.first() else {
    return;
  };
  assert_eq!(site.member, "slice", "{site:?}");
  assert_eq!(site.api, "useDebounceFn", "{site:?}");
  let (quiet, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); void first;",
  );
  assert!(quiet.cancelled_filter_promise_demand.is_empty(), "{quiet:?}");
}

#[test]
fn filter_settlement_shared_alias_namespace_and_awaited_binding() {
  let (shared, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/shared'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(shared.cancelled_filter_promise_demand.len(), 1, "{shared:?}");
  let (alias, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const alias = run; const first = alias('aa'); alias('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(alias.cancelled_filter_promise_demand.len(), 1, "{alias:?}");
  let (namespace, _) = contract_stats(
    "import * as VueUse from '@vueuse/core'; const run = VueUse.useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(namespace.cancelled_filter_promise_demand.len(), 1, "{namespace:?}");
  let (bound, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); const cancelled = await first; cancelled.slice(0, 1);",
  );
  assert_eq!(bound.cancelled_filter_promise_demand.len(), 1, "{bound:?}");
  let (literal_arg, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const query = 'aa'; const first = run(query); run('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(literal_arg.cancelled_filter_promise_demand.len(), 1, "{literal_arg:?}");
  let (siblings, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const a = run('aa'); const b = run('bb'); run('cc'); void (await a).slice(0, 1); void (await b).slice(0, 1);",
  );
  assert_eq!(siblings.cancelled_filter_promise_demand.len(), 2, "{siblings:?}");
}

#[test]
#[expect(clippy::panic, reason = "unicode CRLF fixture must contain the demand")]
fn filter_settlement_unicode_crlf_and_options() {
  let source = "import { useDebounceFn } from '@vueuse/core';\r\nconst 运行 = useDebounceFn((value: string) => value.toUpperCase(), 50);\r\nconst 先前 = 运行('aa');\r\n运行('bb');\r\nconst 结果 = (await 先前).slice(0, 1);\r\n";
  let demand = "(await 先前).slice(0, 1)";
  let Some(offset) = source.find(demand) else {
    panic!("unicode CRLF fixture must contain the demand");
  };
  let (line, column) = vue_vet_core::LineIndex::new(source).byte_to_line_column(offset);
  let (facts, _) = contract_stats(source);
  assert_eq!(facts.cancelled_filter_promise_demand.len(), 1, "{facts:?}");
  if let Some(site) = facts.cancelled_filter_promise_demand.first() {
    assert_eq!(site.demand_span.offset, offset, "{site:?}");
    assert_eq!(site.demand_span.length, demand.len(), "{site:?}");
    assert_eq!(site.demand_span.line, line, "{site:?}");
    assert_eq!(site.demand_span.column, column, "{site:?}");
  }
  let (empty, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, {}); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(empty.cancelled_filter_promise_demand.len(), 1, "{empty:?}");
  let (reject_false, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, { rejectOnCancel: false }); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(reject_false.cancelled_filter_promise_demand.len(), 1, "{reject_false:?}");
  let (duplicate, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, { rejectOnCancel: true, rejectOnCancel: false }); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(duplicate.cancelled_filter_promise_demand.len(), 1, "{duplicate:?}");
}

#[test]
fn filter_settlement_safe_controls_stay_quiet() {
  for source in [
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); const cancelled = await first; if (cancelled) cancelled.slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); void (await first)?.slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, { rejectOnCancel: true }); const first = run('aa'); run('bb'); void (await first);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 0); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = await run('aa'); const latest = await run('bb'); first.slice(0, 1); latest.slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); const latest = run('bb'); void (await first); void (await latest).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; function hold(fn: (value: string) => Promise<string>) { void fn; } const run = useDebounceFn((value: string) => value.toUpperCase(), 50); hold(run); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; function hold(value: Promise<string>) { void value; } const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); hold(first); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const options = { rejectOnCancel: false }; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, options); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; String.prototype.slice = function slice() { return ''; }; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); async function later() { const first = run('aa'); run('bb'); void (await first).slice(0, 1); } void later;",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, { maxWait: 0 }); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, { maxWait: 5 }); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useThrottleFn } from '@vueuse/core'; const run = useThrottleFn((value: string) => value.toUpperCase(), 20, true, true); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); first.then((value: string) => value.slice(0, 1));",
    "import { ref } from 'vue'; import { useDebounceFn } from '@vueuse/core'; const delay = ref(50); const run = useDebounceFn((value: string) => value.toUpperCase(), delay); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn(async (value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const extra = { maxWait: 0 }; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, { ...extra }); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 20); const first = run('aa'); await new Promise((resolve) => setTimeout(resolve, 100)); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 20); const slow = useDebounceFn((value: string) => value, 80); const first = run('aa'); await slow('x'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 20); const first = run('aa'); await first; run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 20); const first = run('aa'); void (await first).slice(0, 1); run('bb'); void (await first).slice(1, 2);",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.cancelled_filter_promise_demand.is_empty(),
      "safe probe must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn filter_settlement_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { useDebounceFn } from '@vueuse/core';");
    for index in 0..size {
      source.push_str("const r");
      source.push_str(&index.to_string());
      source.push_str(" = useDebounceFn((value: string) => value.toUpperCase(), 50); const f");
      source.push_str(&index.to_string());
      source.push_str(" = r");
      source.push_str(&index.to_string());
      source.push_str("('aa'); r");
      source.push_str(&index.to_string());
      source.push_str("('bb'); void (await f");
      source.push_str(&index.to_string());
      source.push_str(").slice(0, 1);");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.cancelled_filter_promise_demand.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "filter-settlement work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn filter_settlement_shared_wrapper_and_alias_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const a0 = run;",
    );
    for index in 1..size {
      source.push_str("const a");
      source.push_str(&index.to_string());
      source.push_str(" = a");
      source.push_str(&(index - 1).to_string());
      source.push(';');
    }
    source.push_str("const first = a");
    source.push_str(&(size - 1).to_string());
    source.push_str("('aa'); a");
    source.push_str(&(size - 1).to_string());
    source.push_str("('bb'); void (await first).slice(0, 1);");
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.cancelled_filter_promise_demand.len(), 1, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-wrapper alias work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn filter_settlement_shared_calls_and_fanout_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [20_u64, 40, 80] {
    let mut source = String::from(
      "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50);",
    );
    for index in 0..size {
      source.push_str("const f");
      source.push_str(&index.to_string());
      source.push_str(" = run('a");
      source.push_str(&index.to_string());
      source.push_str("');");
    }
    for index in 0..size {
      source.push_str("void (await f");
      source.push_str(&index.to_string());
      source.push_str(").slice(0, 1);");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size.saturating_sub(1)).unwrap_or(usize::MAX);
    assert_eq!(contracts.cancelled_filter_promise_demand.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-calls work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
  previous = None;
  for size in [20_u64, 40, 80] {
    let mut source = String::from(
      "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb');",
    );
    for _ in 0..size {
      source.push_str("void (await first).slice(0, 1);");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.cancelled_filter_promise_demand.len(), 1, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "fanout await/demand work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn snapshot_contracts_json_clone_date_demand_and_string_consumer() {
  let positive = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when.getTime();",
    ScriptKind::Setup,
  );
  assert_eq!(positive.json_clone_lossy_type.len(), 1, "{positive:?}");
  let Some(site) = positive.json_clone_lossy_type.first() else {
    return;
  };
  assert_eq!(site.path, "when");
  assert_eq!(site.method, "getTime");
  assert_eq!(site.output_kind, "string");
  let year = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when.getUTCFullYear();",
    ScriptKind::Setup,
  );
  assert_eq!(
    year.json_clone_lossy_type.first().map(|site| site.method.as_str()),
    Some("getUTCFullYear")
  );
  let quiet = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when.slice(0, 4);",
    ScriptKind::Setup,
  );
  assert!(quiet.json_clone_lossy_type.is_empty(), "{quiet:?}");
}

#[test]
fn snapshot_contracts_json_clone_unknown_and_safe_controls() {
  for source in [
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') }), { clone: (value: object) => value }); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') }), { immediate: false }); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const extra = { deep: true }; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') }), { ...extra }); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const make = () => ({ when: new Date('2020-01-01') }); const { cloned } = useCloned(ref(make())); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value?.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); if (cloned.value.when) cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value = { when: new Date('2021-01-01') }; cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; class Date { getTime() { return 1 } } const { cloned } = useCloned(ref({ when: new Date() })); cloned.value.when.getTime();",
    "import { useCloned } from '@vueuse/shared'; import { ref } from 'vue'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when.getTime();",
    "import type { useCloned } from '@vueuse/core'; import { ref } from 'vue'; const useCloned = (value: unknown) => ({ cloned: value }); const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })) as { cloned: { value: { when: Date } } }; cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when = new Date('2021-01-01'); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const bag = useCloned(ref({ when: new Date('2020-01-01') })); bag.cloned.value = { when: new Date('2021-01-01') }; bag.cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); const payload = cloned.value; payload.when = new Date('2021-01-01'); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; String.prototype.getTime = () => 42; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; String.prototype['getTime'] = () => 42; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; JSON.parse = () => ({ when: new Date('2021-01-01') }); const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') }), { onTrack: () => { throw new Error('stopped'); } }); cloned.value.when.getTime();",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.json_clone_lossy_type.is_empty(),
      "json clone control must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn snapshot_contracts_history_alias_and_safe_clone() {
  let positive = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source); commit(); source.value.n = 2; undo(); void source.value.n;",
    ScriptKind::Setup,
  );
  assert_eq!(positive.ref_history_snapshot_alias.len(), 1, "{positive:?}");
  let Some(site) = positive.ref_history_snapshot_alias.first() else {
    return;
  };
  assert_eq!(site.property, "n");
  assert_eq!(site.demand_kind, "undo");
  let reset = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { reset } = useManualRefHistory(source); source.value.n = 2; reset(); void source.value.n;",
    ScriptKind::Setup,
  );
  assert_eq!(
    reset.ref_history_snapshot_alias.first().map(|site| site.demand_kind.as_str()),
    Some("reset")
  );
  let snapshot_read = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { history } = useManualRefHistory(source); source.value.n = 2; void history.value[0].snapshot.n;",
    ScriptKind::Setup,
  );
  assert_eq!(
    snapshot_read.ref_history_snapshot_alias.first().map(|site| site.demand_kind.as_str()),
    Some("history")
  );
  let clone_true = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source, { clone: true }); commit(); source.value.n = 2; undo(); void source.value.n;",
    ScriptKind::Setup,
  );
  assert!(clone_true.ref_history_snapshot_alias.is_empty(), "{clone_true:?}");
  let replaced = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source); commit(); source.value = { n: 2 }; undo(); void source.value.n;",
    ScriptKind::Setup,
  );
  assert!(replaced.ref_history_snapshot_alias.is_empty(), "{replaced:?}");
}

#[test]
fn snapshot_contracts_history_unknown_controls() {
  for source in [
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { undo } = useManualRefHistory(source); source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo, clear } = useManualRefHistory(source); commit(); clear(); source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source, { dump: (value: unknown) => value }); commit(); source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref(1); const { commit, undo } = useManualRefHistory(source); commit(); source.value = 2; undo(); void source.value;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source); commit(); source.value.n = 1; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { history } = useRefHistory(source); source.value.n = 2; void history.value;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source, { clone: !false }); commit(); source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source, { clone: (value: { n: number }) => ({ n: value.n }) }); commit(); source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source); commit(); source.value = { n: 3 }; source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); source.value.n = 2; const { commit, undo } = useManualRefHistory(source); commit(); source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source); commit(); source.value.n = 2; source.value.n = 1; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source); commit(); undo(); source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source); commit(); source.value.n = 2; undo(); const result = 42; void result;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const bag = useManualRefHistory(source); source.value.n = 2; const result = typeof bag.history; void result;",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.ref_history_snapshot_alias.is_empty(),
      "history control must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn snapshot_contracts_namespace_alias_unicode_and_import_preflight() {
  let json = assert_gated_matches_forced(
    "import { ref } from 'vue';\r\nimport * as VueUse from '@vueuse/core';\r\nconst { cloned } = VueUse.useCloned(ref({ when: new Date('2020-01-01') }));\r\ncloned.value.when.getTime();\r\n",
    ScriptKind::Setup,
  );
  assert_eq!(json.json_clone_lossy_type.len(), 1, "{json:?}");
  let history = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useManualRefHistory as historyOf } from '@vueuse/core'; const 源 = ref({ n: 1 }); const { commit, undo } = historyOf(源); commit(); 源.value.n = 2; undo(); void 源.value.n;",
    ScriptKind::Setup,
  );
  assert_eq!(history.ref_history_snapshot_alias.len(), 1, "{history:?}");
  let json_demand = "cloned.value.when.getTime()";
  let json_source = "import { ref } from 'vue';\r\nimport * as VueUse from '@vueuse/core';\r\nconst { cloned } = VueUse.useCloned(ref({ when: new Date('2020-01-01') }));\r\ncloned.value.when.getTime();\r\n";
  let Some(json_site) = json.json_clone_lossy_type.first() else {
    return;
  };
  let Some(json_offset) = json_source.find(json_demand) else {
    return;
  };
  assert_eq!(json_site.demand_span.offset, json_offset, "{json_site:?}");
  assert_eq!(json_site.demand_span.length, json_demand.len(), "{json_site:?}");
  assert_eq!(json_site.demand_span.line, 4, "{json_site:?}");
  assert_eq!(json_site.demand_span.column, 1, "{json_site:?}");
  let history_write = "源.value.n";
  let history_source = "import { ref } from 'vue'; import { useManualRefHistory as historyOf } from '@vueuse/core'; const 源 = ref({ n: 1 }); const { commit, undo } = historyOf(源); commit(); 源.value.n = 2; undo(); void 源.value.n;";
  let Some(history_site) = history.ref_history_snapshot_alias.first() else {
    return;
  };
  let Some(history_offset) = history_source.find(history_write) else {
    return;
  };
  assert_eq!(history_site.write_span.offset, history_offset, "{history_site:?}");
  assert_eq!(history_site.write_span.length, history_write.len(), "{history_site:?}");
  assert_eq!(history_site.write_span.line, 1, "{history_site:?}");
  assert_eq!(history_site.write_span.column, history_offset + 1, "{history_site:?}");
  assert_bypass("const n = 1; console.log(n);", ScriptKind::Setup);
  assert_bypass(
    "import { useTimeoutFn } from '@vueuse/core'; useTimeoutFn(() => {}, 0);",
    ScriptKind::Setup,
  );
}

#[test]
fn snapshot_contracts_independent_producers_grow_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { useCloned, useManualRefHistory } from '@vueuse/core';",
    );
    for index in 0..size {
      source.push_str("const { cloned: cloned");
      source.push_str(&index.to_string());
      source.push_str(" } = useCloned(ref({ when: new Date('2020-01-01') })); cloned");
      source.push_str(&index.to_string());
      source.push_str(".value.when.getTime();");
      source.push_str("const src");
      source.push_str(&index.to_string());
      source.push_str(" = ref({ n: 1 }); const { commit: c");
      source.push_str(&index.to_string());
      source.push_str(", undo: u");
      source.push_str(&index.to_string());
      source.push_str(" } = useManualRefHistory(src");
      source.push_str(&index.to_string());
      source.push_str("); c");
      source.push_str(&index.to_string());
      source.push_str("(); src");
      source.push_str(&index.to_string());
      source.push_str(".value.n = 2; u");
      source.push_str(&index.to_string());
      source.push_str("(); void src");
      source.push_str(&index.to_string());
      source.push_str(".value.n;");
    }
    let (facts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(facts.json_clone_lossy_type.len(), expected, "{facts:?}");
    assert_eq!(facts.ref_history_snapshot_alias.len(), expected, "{facts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "snapshot-contract work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn snapshot_contracts_joint_history_ops_grow_linearly() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [20_u64, 80] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source);",
    );
    for _ in 0..size {
      source.push_str("commit();");
    }
    for _ in 0..size {
      source.push_str("source.value.n = 2;");
    }
    for _ in 0..size {
      source.push_str("undo();");
    }
    source.push_str("void source.value.n;");
    let (facts, work) = contract_stats(&source);
    assert_eq!(facts.ref_history_snapshot_alias.len(), 1, "{facts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 4);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(90),
        "joint history work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <9x for 4x size)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn snapshot_contracts_keep_source5_and_value3_outputs() {
  let facts = analyze(
    "import { reactive, triggerRef, toRefs, ref, watch, customRef, effectScope } from 'vue';\
     triggerRef(reactive({ n: 1 }));\
     toRefs({ a: 1 });\
     reactive(0);\
     const n = ref(0); watch(n.value, () => {});\
     const bad = customRef(() => ({ set() {} })); void bad.value;\
     const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); void result.count;\
     void toRefs(reactive({ count: 1 })).missing.value;",
    "ts",
  );
  assert_eq!(facts.source_contracts.trigger_ref_non_ref.len(), 1);
  assert_eq!(facts.source_contracts.torefs_non_proxy.len(), 1);
  assert_eq!(facts.source_contracts.primitive_reactive_target.len(), 1);
  assert_eq!(facts.source_contracts.watch_unwrapped_source.len(), 1);
  assert_eq!(facts.source_contracts.invalid_custom_ref_interface.len(), 1);
  assert_eq!(facts.source_contracts.inactive_scope_result.len(), 1);
  assert_eq!(facts.source_contracts.missing_torefs_key.len(), 1);
  assert!(facts.source_contracts.json_clone_lossy_type.is_empty());
  assert!(facts.source_contracts.ref_history_snapshot_alias.is_empty());
}

#[test]
#[expect(clippy::panic, reason = "missing span evidence must fail the regression")]
fn source_contracts_define_model_literal_default_and_undefined_ref() {
  let facts = analyze(
    "import { ref, onMounted } from 'vue'\nconst model = defineModel({ default: 1 })\nconst value = ref()\nonMounted(() => { value.value.toFixed(2) })\ndefineExpose({ model })\n",
    "ts",
  );
  assert_eq!(facts.source_contracts.model_defaults.len(), 1, "{:?}", facts.source_contracts);
  let Some(default) = facts.source_contracts.model_defaults.first() else {
    panic!("default missing: {:?}", facts.source_contracts);
  };
  assert_eq!(default.model_name, "modelValue");
  assert_eq!(default.origin, vue_vet_core::ModelDefaultOrigin::LiteralPrimitive);
  assert_eq!(default.primitive, Some(vue_vet_core::ModelPrimitiveKind::Number));
  assert!(
    facts
      .source_contracts
      .ordinary_ref_inits
      .iter()
      .any(|init| { init.binding == "value" && init.kind == vue_vet_core::RefInitKind::Undefined }),
    "{:?}",
    facts.source_contracts.ordinary_ref_inits
  );
  assert_eq!(
    facts.source_contracts.mounted_member_demands.len(),
    1,
    "{:?}",
    facts.source_contracts.mounted_member_demands
  );
  assert_eq!(
    facts.source_contracts.mounted_member_demands.first().map(|demand| demand.member.as_str()),
    Some("toFixed")
  );
  assert!(
    facts
      .source_contracts
      .define_expose
      .iter()
      .any(|expose| expose.names.contains(&"model".into()))
  );
}

#[test]
fn source_contracts_shared_factory_and_fresh_factory_and_instance_chains() {
  let shared = analyze(
    "const shared = { n: 1 }\nconst model = defineModel({ default: () => shared })\ndefineExpose({ model })\n",
    "ts",
  );
  assert!(
    shared.source_contracts.model_defaults.iter().any(|model| {
      model.origin == vue_vet_core::ModelDefaultOrigin::SharedObjectFactory
        && model.shared_binding.as_deref() == Some("shared")
    }),
    "{:?}",
    shared.source_contracts.model_defaults
  );
  assert!(
    shared.source_contracts.shared_object_bindings.iter().any(|binding| {
      binding.binding == "shared"
        && binding
          .own_paths
          .iter()
          .any(|(path, kind)| path == "n" && *kind == vue_vet_core::ModelPrimitiveKind::Number)
    }),
    "{:?}",
    shared.source_contracts.shared_object_bindings
  );
  let fresh = analyze("const model = defineModel({ default: () => ({ n: 1 }) })\n", "ts");
  assert!(
    fresh
      .source_contracts
      .model_defaults
      .iter()
      .any(|model| { model.origin == vue_vet_core::ModelDefaultOrigin::FreshObjectFactory }),
    "{:?}",
    fresh.source_contracts.model_defaults
  );
  let literal = analyze("const model = defineModel({ default: { n: 1 } })\n", "ts");
  assert!(
    literal.source_contracts.model_defaults.iter().any(|model| {
      model.origin == vue_vet_core::ModelDefaultOrigin::SharedObjectLiteral
        && model
          .own_paths
          .iter()
          .any(|(path, kind)| path == "n" && *kind == vue_vet_core::ModelPrimitiveKind::Number)
    }),
    "{:?}",
    literal.source_contracts.model_defaults
  );
  let array = analyze("const model = defineModel({ default: [] })\n", "ts");
  assert!(
    array
      .source_contracts
      .model_defaults
      .iter()
      .any(|model| model.origin == vue_vet_core::ModelDefaultOrigin::SharedObjectLiteral),
    "{:?}",
    array.source_contracts.model_defaults
  );
  let chain = analyze(
    "import { ref, onMounted } from 'vue'\nconst left = ref(null)\nconst right = ref(null)\nonMounted(() => { left.value.model.n = 'text'; right.value.model.n.toFixed(2) })\n",
    "ts",
  );
  assert!(
    chain.source_contracts.instance_path_writes.iter().any(|write| {
      write.instance == "left"
        && write.path == ["model", "n"]
        && write.rhs_kind == vue_vet_core::RefInitKind::String
    }),
    "{:?}",
    chain.source_contracts.instance_path_writes
  );
  assert!(
    chain.source_contracts.instance_member_demands.iter().any(|demand| {
      demand.instance == "right"
        && demand.path == ["model", "n"]
        && demand.member == "toFixed"
        && !demand.optional
        && !demand.guarded
    }),
    "{:?}",
    chain.source_contracts.instance_member_demands
  );
}

#[test]
#[expect(clippy::panic, reason = "missing span evidence must fail the regression")]
fn source_contracts_model_demand_unicode_and_crlf_span_the_call() {
  let unicode = "import { ref, onMounted } from 'vue'\nconst 值 = ref()\nonMounted(() => { 值.value.toFixed(2) })\n";
  let facts = analyze(unicode, "ts");
  let needle = "值.value.toFixed(2)";
  let Some(offset) = unicode.find(needle) else {
    panic!("unicode demand missing");
  };
  let Some(demand) = facts.source_contracts.mounted_member_demands.first() else {
    panic!("unicode demand fact missing: {:?}", facts.source_contracts);
  };
  assert_eq!(demand.span.offset, offset);
  assert_eq!(demand.span.length, needle.len());

  let crlf = "import { ref, onMounted } from 'vue';\r\nconst value = ref();\r\nonMounted(() => { value.value.toFixed(2) })\r\n";
  let facts = analyze(crlf, "ts");
  let needle = "value.value.toFixed(2)";
  let Some(offset) = crlf.find(needle) else {
    panic!("crlf demand missing");
  };
  let Some(demand) = facts.source_contracts.mounted_member_demands.first() else {
    panic!("crlf demand fact missing: {:?}", facts.source_contracts);
  };
  assert_eq!(demand.span.offset, offset);
  assert_eq!(demand.span.length, needle.len());
}

#[test]
fn source_contracts_model_facts_scale_with_combined_fanout() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [8_u64, 16, 32] {
    let mut source = String::from("import { ref, onMounted } from 'vue'\n");
    for index in 0..size {
      source.push_str("const s");
      source.push_str(&index.to_string());
      source.push_str(" = { n: 1 }\nconst m");
      source.push_str(&index.to_string());
      source.push_str(" = defineModel({ default: () => s");
      source.push_str(&index.to_string());
      source.push_str(" })\nconst r");
      source.push_str(&index.to_string());
      source.push_str(" = ref()\n");
    }
    source.push_str("onMounted(() => {\n");
    for index in 0..size {
      source.push('r');
      source.push_str(&index.to_string());
      source.push_str(".value.toFixed(2)");
      source.push('\n');
    }
    source.push_str("})\n");
    let (contracts, work) = contract_stats(&source);
    let count = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.model_defaults.len(), count, "size {size} defaults; {contracts:?}");
    assert_eq!(contracts.mounted_member_demands.len(), count, "size {size} demands; {contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(work > 0, "model-fact collection must count work");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "model-fact work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_model_write_compares_literal_value_on_model_binding() {
  let same = analyze(
    "const model = defineModel({ default: 1 })\nconst label = ref('a')\nmodel.value = 1\nlabel.value = 'b'\n",
    "ts",
  );
  assert!(
    same.source_contracts.model_value_writes.iter().any(|write| {
      write.binding == "model" && write.unchanged_default && write.rhs_text.as_deref() == Some("1")
    }),
    "{:?}",
    same.source_contracts.model_value_writes
  );
  assert!(
    same.source_contracts.model_value_writes.iter().all(|write| write.binding != "label"),
    "unrelated ref writes must not be model writes: {:?}",
    same.source_contracts.model_value_writes
  );
  let changed = analyze("const model = defineModel({ default: 1 })\nmodel.value = 2\n", "ts");
  assert!(
    changed
      .source_contracts
      .model_value_writes
      .iter()
      .any(|write| write.binding == "model" && !write.unchanged_default),
    "{:?}",
    changed.source_contracts.model_value_writes
  );
}

#[test]
fn source_contracts_early_return_marks_mounted_demand_guarded() {
  let facts = analyze(
    "import { ref, onMounted } from 'vue'\nconst value = ref()\nonMounted(() => { if (value.value === undefined) return; value.value.toFixed(2) })\n",
    "ts",
  );
  assert!(
    facts.source_contracts.mounted_member_demands.iter().any(|demand| demand.guarded),
    "{:?}",
    facts.source_contracts.mounted_member_demands
  );
}

#[test]
fn source_contracts_ordinary_script_records_shared_object_without_model_surface() {
  let (facts, _) =
    contract_collect("const shared = { n: 1 }\n", vue_vet_core::ScriptKind::Script, true);
  assert!(
    facts.shared_object_bindings.iter().any(|binding| {
      binding.binding == "shared"
        && binding
          .own_paths
          .iter()
          .any(|(path, kind)| path == "n" && *kind == vue_vet_core::ModelPrimitiveKind::Number)
    }),
    "{:?}",
    facts.shared_object_bindings
  );
}

#[test]
fn source_contracts_model_preflight_equals_forced_full_without_surface() {
  let source = "const n = 1\n";
  let (relative, _) = contract_collect(source, vue_vet_core::ScriptKind::Setup, false);
  let (full, _) = contract_collect(source, vue_vet_core::ScriptKind::Setup, true);
  assert_eq!(relative.model_defaults, full.model_defaults);
  assert_eq!(relative.mounted_member_demands, full.mounted_member_demands);
}
