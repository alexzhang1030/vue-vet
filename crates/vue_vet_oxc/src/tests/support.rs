pub(super) use std::collections::BTreeSet;
pub(super) use std::fmt::Write;

pub(super) use super::super::*;
pub(super) use crate::source_contracts::{
  ContractSink, SourceContractStats, collect_source_contract_facts_forced_full,
  collect_source_contract_facts_with_stats, contract_sink,
};
pub(super) use vue_vet_core::{ReactiveReadKind, ToRefIgnoredKeyReason};

#[expect(clippy::panic, reason = "unexpected Oxc errors must fail adapter tests")]
pub(super) fn analyze(source: &str, language: &str) -> ScriptBlockFacts {
  match analyze_script(source, source, 0, language, ScriptKind::Setup) {
    Ok(facts) => facts,
    Err(error) => panic!("script analysis unexpectedly failed: {error}"),
  }
}

pub(super) fn contract_stats(source: &str) -> (vue_vet_core::SourceContractFacts, u64) {
  let (facts, stats) = contract_collect(source, ScriptKind::Setup, false);
  (facts, stats.work())
}

pub(super) fn contract_full_stats(
  source: &str,
) -> (vue_vet_core::SourceContractFacts, SourceContractStats) {
  contract_collect(source, ScriptKind::Setup, false)
}

pub(super) fn contract_collect(
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

pub(super) fn assert_gated_matches_forced(
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

pub(super) fn assert_bypass(source: &str, kind: ScriptKind) {
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

pub(super) fn semantic_node_count(source: &str) -> u64 {
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
  assert!(parsed.diagnostics.is_empty(), "node-count fixture failed to parse");
  let built = SemanticBuilder::new().with_build_nodes(true).build(&parsed.program);
  assert!(built.diagnostics.is_empty(), "node-count fixture failed semantics");
  u64::try_from(built.semantic.nodes().len()).unwrap_or(u64::MAX)
}

pub(super) fn custom_ref_source(body: &str) -> String {
  format!(
    "import {{ customRef, reactive, ref, triggerRef, watch, watchEffect, watchPostEffect }} from 'vue'; {body}"
  )
}

pub(super) fn factory_width_source(size: u64) -> String {
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

pub(super) fn extracted_methods(source: &str) -> vue_vet_core::SourceContractFacts {
  analyze(source, "ts").source_contracts
}

#[expect(clippy::panic, reason = "missing extracted-method fact must fail the regression")]
pub(super) fn first_extracted(
  facts: &vue_vet_core::SourceContractFacts,
) -> &vue_vet_core::ExtractedReactiveCollectionMethodFact {
  facts
    .extracted_reactive_collection_method
    .first()
    .unwrap_or_else(|| panic!("expected extracted collection-method fact; {facts:?}"))
}

pub(super) fn nest_true_and(inner: &str, depth: u32) -> String {
  let mut expr = inner.to_string();
  for _ in 0..depth {
    expr = format!("(true && {expr})");
  }
  expr
}

pub(super) fn nest_ternary(inner: &str, depth: u32) -> String {
  let mut expr = inner.to_string();
  for _ in 0..depth {
    expr = format!("(true ? {expr} : 0)");
  }
  expr
}

pub(super) fn nest_array(inner: &str, depth: u32) -> String {
  let mut expr = inner.to_string();
  for _ in 0..depth {
    expr = format!("[{expr}]");
  }
  expr
}

pub(super) fn nest_sequence(inner: &str, depth: u32) -> String {
  let mut expr = inner.to_string();
  for _ in 0..depth {
    expr = format!("(0, {expr})");
  }
  expr
}

pub(super) fn escape_helper_source(argument: &str) -> String {
  format!(
    "import {{ reactive }} from 'vue'; function configure(target: {{ __v_skip?: boolean; map?: () => number[] }}) {{ target.__v_skip = true; target.map = () => [7]; }} const raw = [1]; configure({argument}); const items = reactive(raw); const {{ map }} = items; map();"
  )
}

pub(super) fn native_ctor_escape_source(argument: &str) -> String {
  format!(
    "import {{ reactive }} from 'vue'; function configure(ctor: ArrayConstructor) {{ const previous = ctor.prototype.map; ctor.prototype.__v_skip = true; ctor.prototype.map = () => [7]; return () => {{ delete ctor.prototype.__v_skip; ctor.prototype.map = previous; }}; }} const restore = configure({argument}); const items = reactive([1]); const {{ map }} = items; map(); restore();"
  )
}

pub(super) fn native_prototype_escape_source(argument: &str) -> String {
  format!(
    "import {{ reactive }} from 'vue'; function configure(prototype: typeof Array.prototype) {{ const previous = prototype.map; prototype.__v_skip = true; prototype.map = () => [7]; return () => {{ delete prototype.__v_skip; prototype.map = previous; }}; }} const restore = configure({argument}); const items = reactive([1]); const {{ map }} = items; map(); restore();"
  )
}

pub(super) fn global_alias_escape_source(argument: &str) -> String {
  format!(
    "import {{ reactive }} from 'vue'; function configure(world: typeof globalThis) {{ const previous = world.Map; world.Map = class {{ get() {{ return 7; }} }} as unknown as MapConstructor; return () => {{ world.Map = previous; }}; }} const world = globalThis; const restore = configure({argument}); const items = reactive(new Map()); const {{ get }} = items as {{ get: () => number }}; get(); restore();"
  )
}

pub(super) fn native_ctor_alias_escape_source(init: &str, argument: &str) -> String {
  format!(
    "import {{ reactive }} from 'vue'; function configure(ctor: ArrayConstructor) {{ const previous = ctor.prototype.map; ctor.prototype.__v_skip = true; ctor.prototype.map = () => [7]; return () => {{ delete ctor.prototype.__v_skip; ctor.prototype.map = previous; }}; }} {init} const restore = configure({argument}); const items = reactive([1]); const {{ map }} = items; map(); restore();"
  )
}

pub(super) fn native_prototype_alias_escape_source(init: &str, argument: &str) -> String {
  format!(
    "import {{ reactive }} from 'vue'; function configure(prototype: typeof Array.prototype) {{ const previous = prototype.map; prototype.__v_skip = true; prototype.map = () => [7]; return () => {{ delete prototype.__v_skip; prototype.map = previous; }}; }} {init} const restore = configure({argument}); const items = reactive([1]); const {{ map }} = items; map(); restore();"
  )
}

pub(super) fn script_setup_from_sfc(sfc: &str) -> &str {
  const OPEN: &str = "<script setup lang=\"ts\">";
  let start = sfc.find(OPEN).map_or(0, |index| index.saturating_add(OPEN.len()));
  let rest = sfc.get(start..).unwrap_or("");
  let end = rest.find("</script>").map_or(sfc.len(), |index| start.saturating_add(index));
  sfc.get(start..end).unwrap_or(sfc).trim()
}

pub(super) fn effect_only_source(api: &str, size: u64, second_arg: Option<&str>) -> String {
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

pub(super) fn effect_wrapper_source(depth: u64) -> String {
  let wrappers = " as any".repeat(usize::try_from(depth).unwrap_or(0));
  format!("import {{ watchEffect }} from 'vue'; (watchEffect{wrappers})(() => {{}});")
}

pub(super) fn effect_import_width_source(width: usize) -> String {
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

pub(super) fn cleanup_identity_source(body: &str) -> String {
  format!("import {{ nextTick, onWatcherCleanup, ref, shallowRef, watch }} from 'vue';\n{body}")
}

pub(super) fn cleanup_identity_visits(source: &str) -> (usize, super::lifetime::CollectStats) {
  let allocator = oxc_allocator::Allocator::default();
  let parsed = oxc_parser::Parser::new(&allocator, source, oxc_span::SourceType::ts()).parse();
  let semantic =
    oxc_semantic::SemanticBuilder::new().with_build_nodes(true).build(&parsed.program).semantic;
  let line_index = vue_vet_core::LineIndex::new(source);
  let (facts, stats) = super::lifetime::collect_with_visits(&semantic, &line_index, source, 0);
  (facts.watch_cleanup_current_sources.len(), stats)
}

pub(super) fn assert_cleanup_identity_quiet(body: &str, label: &str) {
  let facts = analyze(&cleanup_identity_source(body), "ts");
  assert!(
    facts.lifetime.watch_cleanup_current_sources.is_empty(),
    "{label} must stay quiet: {:?}",
    facts.lifetime.watch_cleanup_current_sources
  );
}

fn settlement_source(body: &str) -> String {
  format!(
    "import {{ ref, watch, watchEffect }} from 'vue';\
     const source = ref('one');\
     const result = ref(null);\
     {body}"
  )
}

pub(super) fn settlement_facts(body: &str) -> vue_vet_core::ReactivityLifetimeFacts {
  analyze(&settlement_source(body), "ts").lifetime
}

pub(super) fn settlement_work(source: &str) -> (usize, usize) {
  let allocator = oxc_allocator::Allocator::default();
  let parsed = oxc_parser::Parser::new(&allocator, source, oxc_span::SourceType::ts()).parse();
  let semantic =
    oxc_semantic::SemanticBuilder::new().with_build_nodes(true).build(&parsed.program).semantic;
  let line_index = vue_vet_core::LineIndex::new(source);
  let (facts, stats) = super::lifetime::collect_with_visits(&semantic, &line_index, source, 0);
  (facts.late_cancellation_guards.len(), stats.settlement_inner_work())
}

fn template_demand_span(offset: usize) -> vue_vet_core::SourceSpan {
  vue_vet_core::SourceSpan { offset, length: 4, line: 1, column: offset.saturating_add(1) }
}

pub(super) fn native_alloc(
  name: &str,
  condition: &str,
  offset: usize,
) -> vue_vet_core::TemplateAllocationFact {
  vue_vet_core::TemplateAllocationFact {
    element_span: template_demand_span(offset),
    tag: "span".into(),
    is_component: false,
    static_ref: Some(name.into()),
    ref_span: Some(template_demand_span(offset.saturating_add(8))),
    parent_span: None,
    condition: Some(vue_vet_core::TemplateConditionRelation {
      expression: condition.into(),
      span: template_demand_span(offset.saturating_add(2)),
      identifiers: Some(vec![condition.into()]),
      simple_identifier: Some(condition.into()),
      on_self: true,
    }),
    memo: None,
    v_show: false,
    v_for: false,
    slot: false,
    transition: false,
    nested_memo: false,
    callback_ref: false,
    condition_inside_memo: false,
  }
}

pub(super) fn demand_stats(
  source: &str,
  allocations: Vec<vue_vet_core::TemplateAllocationFact>,
  force_full: bool,
) -> (vue_vet_core::TemplateRefDemandFacts, crate::TemplateDemandStats) {
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
  assert!(parsed.diagnostics.is_empty(), "demand fixture failed to parse");
  let built = SemanticBuilder::new().with_build_nodes(true).build(&parsed.program);
  assert!(built.diagnostics.is_empty(), "demand fixture failed semantics");
  let line_index = vue_vet_core::LineIndex::new(source);
  let template =
    vue_vet_core::TemplateFacts { allocations, ..vue_vet_core::TemplateFacts::default() };
  if force_full {
    crate::collect_template_demand_forced_full(
      &built.semantic,
      &line_index,
      source,
      0,
      ScriptKind::Setup,
      Some(&template),
    )
  } else {
    crate::collect_template_demand_stats(
      &built.semantic,
      &line_index,
      source,
      0,
      ScriptKind::Setup,
      Some(&template),
      false,
    )
  }
}

pub(super) fn shared_source_fanout(
  width: u32,
) -> (String, Vec<vue_vet_core::TemplateAllocationFact>) {
  let mut source =
    String::from("import { onMounted, ref, watch } from 'vue'\nconst visible = ref(false)\n");
  let mut allocations = Vec::new();
  for index in 0..width {
    source.push_str("const node");
    source.push_str(&index.to_string());
    source.push_str(" = ref(null)\n");
    allocations.push(native_alloc(&format!("node{index}"), "visible", 200 + index as usize * 10));
  }
  source.push_str("watch(visible, () => {\n");
  for index in 0..width {
    source.push_str("  node");
    source.push_str(&index.to_string());
    source.push_str(".value.textContent\n");
  }
  source.push_str("}, { flush: 'pre' })\nonMounted(() => { visible.value = true })\n");
  (source, allocations)
}

pub(super) fn watches_times_demands(
  width: u32,
) -> (String, Vec<vue_vet_core::TemplateAllocationFact>) {
  let mut source = String::from(
    "import { onMounted, ref, watch } from 'vue'\nconst visible = ref(false)\nconst node = ref(null)\n",
  );
  for _ in 0..width {
    source.push_str("watch(visible, () => {\n  node.value.textContent\n}, { flush: 'pre' })\n");
  }
  source.push_str("onMounted(() => { visible.value = true })\n");
  (source, vec![native_alloc("node", "visible", 200)])
}
