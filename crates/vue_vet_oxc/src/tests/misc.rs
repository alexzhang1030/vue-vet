use super::support::*;

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
    form.is_some_and(|binding| binding.plain_initializer && binding.mutable && binding.writes >= 1),
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
fn cleanup_identity_counters_are_counted_in_tests() {
  let (identity, stats) = super::lifetime::counter_layout();
  assert_eq!(
    (identity, stats),
    (64, 176),
    "test IdentityWork is 8 usizes and CollectStats is 22 (15 identity/ownership + 7 settlement)"
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
