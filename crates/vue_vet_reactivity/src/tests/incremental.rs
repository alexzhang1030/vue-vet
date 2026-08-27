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
fn independent_leaf_body_edit_reuses_other_graphs() {
  let modules = (0..8)
    .map(|index| {
      ModuleSource::standalone(
        format!("src/module-{index}.ts"),
        format!("import {{ ref }} from 'vue'; export const value{index} = ref({index});"),
        "ts",
        ScriptKind::Script,
      )
    })
    .collect::<Vec<_>>();
  let mut state = ModuleTraceState::default();
  let options =
    TraceModulesOptions { max_workers: 2, persist_linking_cache: true, ..default_trace_options() };
  let first = trace_modules_incremental_with_options(&modules, &[], &options, &mut state);
  assert!(first.issues.is_empty(), "warm setup must trace: {:?}", first.issues);

  let leaf_id = vue_vet_core::ModuleId::from("src/module-7.ts");
  let next_modules = modules
    .iter()
    .map(|module| {
      if module.id == leaf_id {
        return ModuleSource::standalone(
          leaf_id.clone(),
          "import { ref } from 'vue'; export const value7 = ref(70);",
          "ts",
          ScriptKind::Script,
        );
      }
      state.cached_source(&module.id).cloned().unwrap_or_else(|| module.clone())
    })
    .collect::<Vec<_>>();
  let second = trace_modules_incremental_with_options(&next_modules, &[], &options, &mut state);
  assert!(second.issues.is_empty(), "leaf edit must trace: {:?}", second.issues);
  assert_eq!(second.stats.reused_graphs, 7);
  assert_eq!(second.stats.seeded_reparses, 0);
  assert!(!second.stats.export_resolve_ran, "literal-only edit must skip export resolve");
  assert_eq!(second.stats.seed_plans_recomputed, 0);
}

fn live_ids(modules: &[ModuleSource]) -> BTreeSet<vue_vet_core::ModuleId> {
  modules.iter().map(|module| module.id.clone()).collect()
}

fn subset_options(live: BTreeSet<vue_vet_core::ModuleId>) -> TraceModulesOptions {
  TraceModulesOptions {
    max_workers: 2,
    persist_linking_cache: true,
    live_module_ids: Some(live),
    ..default_trace_options()
  }
}

fn retain_options() -> TraceModulesOptions {
  TraceModulesOptions {
    max_workers: 2,
    persist_linking_cache: true,
    retain_cached_modules: true,
    ..default_trace_options()
  }
}

fn graphs_from_state(
  state: &ModuleTraceState,
) -> std::collections::BTreeMap<vue_vet_core::ModuleId, &ReactivityGraph> {
  state.iter_cached_reactivity().map(|(id, module)| (id.clone(), module.graph.as_ref())).collect()
}

#[test]
fn subset_leaf_edit_matches_full_list_graphs() {
  let modules = (0..8)
    .map(|index| {
      ModuleSource::standalone(
        format!("src/module-{index}.ts"),
        format!("import {{ ref }} from 'vue'; export const value{index} = ref({index});"),
        "ts",
        ScriptKind::Script,
      )
    })
    .collect::<Vec<_>>();
  let leaf_id = vue_vet_core::ModuleId::from("src/module-7.ts");
  let leaf = ModuleSource::standalone(
    leaf_id.clone(),
    "import { ref } from 'vue'; export const value7 = ref(70);",
    "ts",
    ScriptKind::Script,
  );
  let full_next = modules
    .iter()
    .map(|module| if module.id == leaf_id { leaf.clone() } else { module.clone() })
    .collect::<Vec<_>>();

  let mut full_state = ModuleTraceState::default();
  let full_options =
    TraceModulesOptions { max_workers: 2, persist_linking_cache: true, ..default_trace_options() };
  let _warm = trace_modules_incremental_with_options(&modules, &[], &full_options, &mut full_state);
  let full =
    trace_modules_incremental_with_options(&full_next, &[], &full_options, &mut full_state);

  let mut subset_state = ModuleTraceState::default();
  let _warm =
    trace_modules_incremental_with_options(&modules, &[], &full_options, &mut subset_state);
  let subset = trace_modules_incremental_with_options(
    std::slice::from_ref(&leaf),
    &[],
    &retain_options(),
    &mut subset_state,
  );

  assert!(full.issues.is_empty() && subset.issues.is_empty());
  assert_eq!(subset.stats.phase_one_succeeded, 1);
  assert_eq!(subset.stats.reused_graphs, 7);
  assert!(!subset.stats.export_resolve_ran);
  assert_eq!(subset.stats.seed_plans_recomputed, 0);
  assert_eq!(
    subset.stats.cached_modules_merged, 0,
    "linking hit must not merge cached summaries: {:?}",
    subset.stats
  );
  assert_eq!(subset.modules.len(), 1);
  assert_eq!(graphs_from_state(&full_state), graphs_from_state(&subset_state));
}

#[test]
fn subset_producer_export_pulls_consumer() {
  let producer_v1 = ModuleSource::standalone(
    "producer.ts",
    "import { ref } from 'vue'; export const count = ref(0);",
    "ts",
    ScriptKind::Script,
  );
  let consumer = ModuleSource::standalone(
    "consumer.ts",
    "import { watchEffect } from 'vue'; import { count } from './producer'; watchEffect(() => count.value);",
    "ts",
    ScriptKind::Script,
  );
  let unrelated = ModuleSource::standalone(
    "unrelated.ts",
    "import { ref } from 'vue'; export const other = ref(1);",
    "ts",
    ScriptKind::Script,
  );
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "./producer".into(),
    to: "producer.ts".into(),
  }];
  let modules_v1 = [producer_v1, consumer.clone(), unrelated.clone()];
  let producer_v2 = ModuleSource::standalone(
    "producer.ts",
    "import { ref } from 'vue'; export const count = ref(0); export const flag = ref(true);",
    "ts",
    ScriptKind::Script,
  );
  let full_v2 = [producer_v2.clone(), consumer, unrelated];

  let mut full_state = ModuleTraceState::default();
  let full_options =
    TraceModulesOptions { max_workers: 2, persist_linking_cache: true, ..default_trace_options() };
  let _warm =
    trace_modules_incremental_with_options(&modules_v1, &links, &full_options, &mut full_state);
  let full =
    trace_modules_incremental_with_options(&full_v2, &links, &full_options, &mut full_state);

  let mut subset_state = ModuleTraceState::default();
  let _warm =
    trace_modules_incremental_with_options(&modules_v1, &links, &full_options, &mut subset_state);
  let subset = trace_modules_incremental_with_options(
    std::slice::from_ref(&producer_v2),
    &links,
    &retain_options(),
    &mut subset_state,
  );

  assert!(full.issues.is_empty() && subset.issues.is_empty());
  assert_eq!(subset.stats.phase_one_succeeded, 1);
  assert!(subset.stats.export_resolve_ran);
  assert_eq!(subset.stats.seed_plans_recomputed, 2);
  assert_eq!(
    subset.stats.cached_modules_merged, 2,
    "linking miss must merge the other live modules: {:?}",
    subset.stats
  );
  assert_eq!(subset.seed_plan_dirty, BTreeSet::from(["producer.ts".into(), "consumer.ts".into()]),);
  assert_eq!(
    subset.modules.len(),
    1,
    "report is the retraced producer; consumer plan reuse stays in state"
  );
  let consumer = subset_state.cached_reactivity(&"consumer.ts".into());
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .effects
        .iter()
        .any(|effect| effect.reads.iter().any(|read| read.binding == "count"))
    }),
    "consumer must stay seeded when only the producer is in the input"
  );
  assert_eq!(graphs_from_state(&full_state), graphs_from_state(&subset_state));
}

#[test]
fn empty_subset_keeps_cached_universe() {
  let modules = [
    ModuleSource::standalone(
      "a.ts",
      "import { ref } from 'vue'; export const a = ref(1);",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "b.ts",
      "import { ref } from 'vue'; export const b = ref(2);",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let mut state = ModuleTraceState::default();
  let options = retain_options();
  let first = trace_modules_incremental_with_options(&modules, &[], &options, &mut state);
  assert!(first.issues.is_empty());
  let second = trace_modules_incremental_with_options(&[], &[], &options, &mut state);
  assert!(second.issues.is_empty());
  assert!(second.modules.is_empty(), "empty subset does not clone cached graphs into the report");
  assert_eq!(second.stats.reused_graphs, 2);
  assert_eq!(second.stats.phase_one_succeeded, 0);
  assert!(!second.stats.export_resolve_ran);
  assert!(state.cached_source(&"a.ts".into()).is_some());
  assert!(state.cached_source(&"b.ts".into()).is_some());
}

#[test]
fn subset_live_ids_drop_deleted_modules() {
  let modules = [
    ModuleSource::standalone(
      "keep.ts",
      "import { ref } from 'vue'; export const keep = ref(1);",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "gone.ts",
      "import { ref } from 'vue'; export const gone = ref(2);",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let mut state = ModuleTraceState::default();
  let options = subset_options(live_ids(&modules));
  let first = trace_modules_incremental_with_options(&modules, &[], &options, &mut state);
  assert!(first.issues.is_empty());
  assert!(state.cached_source(&"gone.ts".into()).is_some());

  let drop = BTreeSet::from([vue_vet_core::ModuleId::from("gone.ts")]);
  let second = trace_modules_incremental_with_options(
    &[],
    &[],
    &TraceModulesOptions {
      max_workers: 2,
      persist_linking_cache: true,
      retain_cached_modules: true,
      drop_module_ids: drop,
      ..default_trace_options()
    },
    &mut state,
  );
  assert!(second.issues.is_empty());
  assert!(second.modules.is_empty(), "unchanged keep is not cloned into the report");
  assert_eq!(second.stats.reused_graphs, 1);
  assert!(state.cached_source(&"keep.ts".into()).is_some());
  assert!(state.cached_source(&"gone.ts".into()).is_none());
}

#[test]
fn explicit_live_ids_empty_subset_keeps_cache() {
  let modules = [
    ModuleSource::standalone(
      "a.ts",
      "import { ref } from 'vue'; export const a = ref(1);",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "b.ts",
      "import { ref } from 'vue'; export const b = ref(2);",
      "ts",
      ScriptKind::Script,
    ),
  ];
  let mut state = ModuleTraceState::default();
  let options = subset_options(live_ids(&modules));
  let first = trace_modules_incremental_with_options(&modules, &[], &options, &mut state);
  assert!(first.issues.is_empty());
  let second = trace_modules_incremental_with_options(&[], &[], &options, &mut state);
  assert!(second.issues.is_empty());
  assert!(second.modules.is_empty());
  assert_eq!(second.stats.reused_graphs, 2);
  assert!(state.cached_source(&"a.ts".into()).is_some());
  assert!(state.cached_source(&"b.ts".into()).is_some());
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

#[test]
fn unused_factory_import_skips_seeded_reparse() {
  let producer = ModuleSource::standalone(
    "producer.ts",
    "import { ref } from 'vue'; export function useFlag() { const flag = ref(false); return flag; }",
    "ts",
    ScriptKind::Script,
  );
  let unused = ModuleSource::standalone(
    "consumer.ts",
    "import { useFlag } from './producer';",
    "ts",
    ScriptKind::Script,
  );
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "./producer".into(),
    to: "producer.ts".into(),
  }];
  let options =
    TraceModulesOptions { max_workers: 2, persist_linking_cache: true, ..default_trace_options() };
  let mut state = ModuleTraceState::default();
  let first = trace_modules_incremental_with_options(
    &[producer.clone(), unused],
    &links,
    &options,
    &mut state,
  );
  assert!(first.issues.is_empty(), "unused factory setup must trace: {:?}", first.issues);
  assert_eq!(
    first.stats.seeded_reparses, 0,
    "unused Factory import must reuse local_graph: {:?}",
    first.stats
  );
  let unused_graph = state.cached_reactivity(&"consumer.ts".into());
  assert!(
    unused_graph.is_some_and(|module| {
      !module.graph.bindings.iter().any(|binding| binding.name == "isCoarse")
    }),
    "unused factory must not invent a seeded binding: {:?}",
    unused_graph.map(|module| &module.graph.bindings)
  );

  let used = ModuleSource::standalone(
    "consumer.ts",
    "import { computed } from 'vue'; import { useFlag } from './producer'; const isCoarse = useFlag(); const hint = computed(() => isCoarse.value ? 'a' : 'b');",
    "ts",
    ScriptKind::Script,
  );
  let second = trace_modules_incremental_with_options(
    &[producer.clone(), used.clone()],
    &links,
    &options,
    &mut state,
  );
  assert!(second.issues.is_empty(), "adding a factory call must trace: {:?}", second.issues);
  assert_eq!(
    second.stats.seeded_reparses, 1,
    "calling the imported factory must reparse: {:?}",
    second.stats
  );

  let oneshot = traced_modules(&[producer, used], &links);
  let incremental = state.cached_reactivity(&"consumer.ts".into());
  let fresh = oneshot.iter().find(|module| module.id == "consumer.ts");
  assert!(
    incremental.is_some_and(|module| {
      fresh.is_some_and(|oneshot| module.graph.as_ref() == oneshot.graph.as_ref())
    }),
    "skipped-then-called graph must match a cold seeded trace"
  );
}

#[test]
fn unused_factory_statement_call_keeps_linking_cache() {
  let producer = ModuleSource::standalone(
    "producer.ts",
    "import { ref } from 'vue'; export function useFlag() { const flag = ref(false); return flag; }",
    "ts",
    ScriptKind::Script,
  );
  let unused = ModuleSource::standalone(
    "consumer.ts",
    "import { useFlag } from './producer';",
    "ts",
    ScriptKind::Script,
  );
  let called = ModuleSource::standalone(
    "consumer.ts",
    "import { useFlag } from './producer'; useFlag();",
    "ts",
    ScriptKind::Script,
  );
  let links = [ModuleLink {
    from: "consumer.ts".into(),
    specifier: "./producer".into(),
    to: "producer.ts".into(),
  }];
  let options =
    TraceModulesOptions { max_workers: 2, persist_linking_cache: true, ..default_trace_options() };
  let mut state = ModuleTraceState::default();
  let first = trace_modules_incremental_with_options(
    &[producer.clone(), unused],
    &links,
    &options,
    &mut state,
  );
  assert!(first.issues.is_empty(), "unused factory setup must trace: {:?}", first.issues);
  let second =
    trace_modules_incremental_with_options(&[producer, called], &links, &options, &mut state);
  assert!(second.issues.is_empty(), "statement call must trace: {:?}", second.issues);
  assert!(
    !second.stats.export_resolve_ran,
    "called_locals is not a linking key: {:?}",
    second.stats
  );
  assert_eq!(second.stats.seed_plans_recomputed, 0);
  assert_eq!(
    second.stats.seeded_reparses, 1,
    "a statement call still forces a conservative reparse: {:?}",
    second.stats
  );
}

#[test]
fn unused_known_import_still_reparses() {
  let modules = [
    ModuleSource::standalone(
      "producer.ts",
      "import { ref } from 'vue'; export const count = ref(0);",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { count } from './producer';",
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
  let report = trace_modules_incremental_with_options(
    &modules,
    &links,
    &TraceModulesOptions { max_workers: 2, persist_linking_cache: true, ..default_trace_options() },
    &mut state,
  );
  assert!(report.issues.is_empty(), "unused Known setup must trace: {:?}", report.issues);
  assert_eq!(
    report.stats.seeded_reparses, 1,
    "Known imports materialize from the import span even when unused: {:?}",
    report.stats
  );
  let consumer = state.cached_reactivity(&"consumer.ts".into());
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .bindings
        .iter()
        .any(|binding| binding.name == "count" && binding.kind == ReactiveBindingKind::Ref)
    }),
    "unused Known import must still seed the binding: {:?}",
    consumer.map(|module| &module.graph.bindings)
  );
}

#[test]
fn unused_factory_does_not_skip_when_known_import_is_present() {
  let modules = [
    ModuleSource::standalone(
      "producer.ts",
      "import { ref } from 'vue'; export const count = ref(0); export function useFlag() { const flag = ref(false); return flag; }",
      "ts",
      ScriptKind::Script,
    ),
    ModuleSource::standalone(
      "consumer.ts",
      "import { count, useFlag } from './producer';",
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
  let report = trace_modules_incremental_with_options(
    &modules,
    &links,
    &TraceModulesOptions { max_workers: 2, persist_linking_cache: true, ..default_trace_options() },
    &mut state,
  );
  assert!(report.issues.is_empty(), "mixed unused seeds must trace: {:?}", report.issues);
  assert_eq!(
    report.stats.seeded_reparses, 1,
    "a Known import in the same plan must still force a reparse: {:?}",
    report.stats
  );
}
