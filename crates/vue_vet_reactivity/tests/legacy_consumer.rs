//! Oxc-namespace + hidden root compat smoke.
//!
//! Oxc 0.142 requires `.with_build_nodes(true)` or `Semantic::nodes` is empty.
//! Asserts a real `count` read and equal graphs from `oxc::` vs hidden root aliases.

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use vue_vet_core::ScriptKind;
use vue_vet_reactivity::{ModuleSource, TraceConfig, oxc, trace_modules};

#[test]
#[expect(clippy::panic, reason = "Oxc compat smoke must fail closed on empty semantics")]
fn oxc_namespace_and_legacy_root_produce_equal_graphs_and_read_count() {
  let source = "import { ref, computed } from 'vue'; export const count=ref(1); const result=computed(()=>count.value)";
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
  assert!(parsed.diagnostics.is_empty(), "parse diagnostics: {:?}", parsed.diagnostics);
  let semantic = SemanticBuilder::new()
    .with_build_nodes(true)
    .with_check_syntax_error(true)
    .build(&parsed.program);
  assert!(semantic.diagnostics.is_empty(), "semantic diagnostics: {:?}", semantic.diagnostics);
  let semantic = &semantic.semantic;
  let graph = oxc::trace_reactivity(semantic, source, 0, ScriptKind::Script);
  assert!(
    graph.scopes.iter().any(|scope| scope.reads.iter().any(|read| read.binding == "count")),
    "oxc::trace_reactivity must record a count read: {:?}",
    graph.scopes
  );
  let legacy = vue_vet_reactivity::trace_reactivity(semantic, source, 0, ScriptKind::Script);
  assert_eq!(graph, legacy, "hidden root trace_reactivity must equal oxc::trace_reactivity");
  assert_eq!(
    graph,
    oxc::trace_reactivity_with_config(
      semantic,
      source,
      0,
      ScriptKind::Script,
      &TraceConfig::empty()
    ),
    "empty TraceConfig must match default oxc::trace_reactivity"
  );
  let summary = oxc::prepare_module_summary(semantic, source, 0, ScriptKind::Script, graph.clone());
  let legacy_summary =
    vue_vet_reactivity::prepare_module_trace(semantic, source, 0, ScriptKind::Script, graph);
  let module = ModuleSource::standalone("entry.ts", source, "ts", ScriptKind::Script);
  let preferred = match trace_modules(&[module.clone().with_module_summary(summary)], &[]) {
    Ok(graphs) => graphs,
    Err(error) => panic!("preferred summary: {error}"),
  };
  let legacy = match trace_modules(&[module.with_module_summary(legacy_summary)], &[]) {
    Ok(graphs) => graphs,
    Err(error) => panic!("legacy summary: {error}"),
  };
  assert_eq!(preferred, legacy, "prepare_module_summary and prepare_module_trace must agree");
}
