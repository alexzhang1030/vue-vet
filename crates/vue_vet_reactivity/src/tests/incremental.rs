use std::collections::BTreeSet;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;

use super::helpers::*;

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
    &TraceModulesOptions { max_workers: 2, ..default_trace_options() },
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
    &TraceModulesOptions { max_workers: 2, ..default_trace_options() },
    &mut state,
  );
  assert!(first.issues.is_empty());
  assert_eq!(first.stats.seeded_reparses, 1);
  assert!(first.stats.export_resolve_ran);
  assert_eq!(first.stats.seed_plans_recomputed, 2);
  let second = trace_modules_incremental_with_options(
    &modules,
    &links,
    &TraceModulesOptions { max_workers: 2, ..default_trace_options() },
    &mut state,
  );
  assert!(second.issues.is_empty());
  assert_eq!(second.stats.seeded_reparses, 0);
  assert_eq!(second.stats.reused_graphs, 2);
  assert!(!second.stats.export_resolve_ran);
  assert_eq!(second.stats.seed_plans_recomputed, 0);
  assert!(second.seed_plan_dirty.is_empty());
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
    &TraceModulesOptions { max_workers: 2, ..default_trace_options() },
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
    &TraceModulesOptions { max_workers: 2, ..default_trace_options() },
    &mut state,
  );
  assert!(second.issues.is_empty());
  assert!(second.stats.export_resolve_ran);
  assert_eq!(
    second.stats.seed_plans_recomputed, 2,
    "producer surface + consumer importer; unrelated must keep prior seed plan"
  );
  assert_eq!(
    second.seed_plan_dirty,
    BTreeSet::from(["producer.ts".into(), "consumer.ts".into()]),
    "export_closure is the seed-dirty set, not every workspace module"
  );
}

#[test]
fn incremental_linking_skips_export_resolve_when_only_local_graph_changes() {
  use std::sync::Arc;

  use crate::{TraceSeeds, prepare_module_summary_with_config, trace_reactivity_seeded};

  fn summary_for(source: &str) -> Arc<crate::ModuleSummary> {
    let allocator = oxc_allocator::Allocator::default();
    let source_type = oxc_span::SourceType::default().with_module(true).with_typescript(true);
    let parsed = oxc_parser::Parser::new(&allocator, source, source_type).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let semantic =
      oxc_semantic::SemanticBuilder::new().with_build_nodes(true).build(&parsed.program).semantic;
    let config = default_trace_config();
    let graph = Arc::new(trace_reactivity_seeded(
      &semantic,
      source,
      0,
      ScriptKind::Script,
      &TraceSeeds::default(),
      &config,
    ));
    Arc::new(prepare_module_summary_with_config(
      &semantic,
      source,
      0,
      ScriptKind::Script,
      graph,
      &config,
    ))
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
    &TraceModulesOptions { max_workers: 2, ..default_trace_options() },
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
    &TraceModulesOptions { max_workers: 2, ..default_trace_options() },
    &mut state,
  );
  assert!(second.issues.is_empty());
  assert!(!second.stats.export_resolve_ran, "linking surface unchanged → skip export fixed point");
  assert_eq!(second.stats.seed_plans_recomputed, 0);
}

#[test]
fn prepared_phase_one_facts_avoid_an_unseeded_second_parse() {
  let source = "import { ref } from 'vue'; export const count = ref(0);";
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
  assert!(parsed.diagnostics.is_empty());
  let built = SemanticBuilder::new()
    .with_build_nodes(true)
    .with_check_syntax_error(true)
    .build(&parsed.program);
  assert!(built.diagnostics.is_empty());
  let local_graph = trace_reactivity_with_config(
    &built.semantic,
    source,
    0,
    ScriptKind::Script,
    &default_trace_config(),
  );
  let summary = prepare_module_summary(&built.semantic, source, 0, ScriptKind::Script, local_graph);
  let mut module = ModuleSource::standalone("count.ts", source, "ts", ScriptKind::Script)
    .with_module_summary(summary);

  // If phase one parsed again this deliberate mutation would fail. No seeds
  // means the retained local graph is sufficient.
  module.source = "const = ;".into();
  let traced = trace_modules(&[module], &[]);
  assert!(traced.is_ok(), "prepared phase-one facts should bypass a second parse");
}
