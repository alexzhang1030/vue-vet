//! Runtime oracle comparison: static under-approx vs committed Vue `onTrack` deps.
//!
//! Expected JSON is produced by `oracle/harness.mjs` (see `oracle/README.md`).
//! CI loads `oracle/expected/*.json` only — no Node required at test time.

#![expect(
  clippy::panic,
  reason = "oracle fixture IO/parse failures must fail the test suite loudly"
)]

use std::{collections::BTreeSet, fs, path::PathBuf};

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use serde::Deserialize;
use vue_vet_core::{ReactiveReadKind, ReactivityGraph, ScriptKind};

use crate::{DEEP_WATCH_PROPERTY, trace_reactivity};

#[derive(Debug, Deserialize)]
struct OracleCase {
  id: String,
  source: String,
  runtime_deps: Vec<RuntimeDep>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
struct RuntimeDep {
  binding: String,
  key: Option<String>,
}

fn expected_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("oracle/expected")
}

fn load_cases() -> Vec<OracleCase> {
  let dir = expected_dir();
  let mut paths = match fs::read_dir(&dir) {
    Ok(entries) => entries
      .filter_map(Result::ok)
      .map(|entry| entry.path())
      .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
      .collect::<Vec<_>>(),
    Err(error) => panic!("oracle expected dir missing at {}: {error}", dir.display()),
  };
  paths.sort();
  assert!(!paths.is_empty(), "oracle expected/*.json must be committed");
  paths
    .into_iter()
    .map(|path| {
      let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => panic!("read {}: {error}", path.display()),
      };
      match serde_json::from_str(&source) {
        Ok(case) => case,
        Err(error) => panic!("parse {}: {error}", path.display()),
      }
    })
    .collect()
}

fn graph(source: &str) -> ReactivityGraph {
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
  assert!(parsed.errors.is_empty(), "oracle source must parse: {source}");
  let built = SemanticBuilder::new().with_check_syntax_error(true).build(&parsed.program);
  assert!(built.errors.is_empty(), "oracle source must be semantically valid: {source}");
  trace_reactivity(&built.semantic, source, 0, ScriptKind::Setup)
}

/// Tracking deps only: unconditional/conditional reads (not after-await / outside).
fn tracking_dep_keys(graph: &ReactivityGraph) -> BTreeSet<(String, Option<String>)> {
  graph
    .scopes
    .iter()
    .flat_map(|scope| &scope.reads)
    .filter(|read| {
      matches!(read.kind, ReactiveReadKind::Unconditional | ReactiveReadKind::Conditional)
    })
    .map(|read| (read.binding.clone(), read.property.clone()))
    .collect()
}

fn runtime_keys(deps: &[RuntimeDep]) -> BTreeSet<(String, Option<String>)> {
  deps.iter().map(|dep| (dep.binding.clone(), dep.key.clone())).collect()
}

/// Under-approx: every tracking tracer dep must appear in runtime deps.
#[test]
fn oracle_tracer_is_under_approximation_of_runtime() {
  let cases = load_cases();
  let mut total_runtime = 0_usize;
  let mut total_hit = 0_usize;
  let mut report = Vec::new();

  for case in &cases {
    let graph = graph(&case.source);
    let tracer = tracking_dep_keys(&graph);
    let runtime = runtime_keys(&case.runtime_deps);

    let invented = tracer
      .iter()
      .filter(|dep| {
        // Deep-root sentinel `*` under-approximates runtime deep watches when the
        // binding appears in any runtime dep; it must not invent a concrete key.
        if dep.1.as_deref() == Some(DEEP_WATCH_PROPERTY) {
          return !runtime.iter().any(|(binding, _)| binding == &dep.0);
        }
        !runtime.contains(dep)
      })
      .cloned()
      .collect::<Vec<_>>();
    assert!(
      invented.is_empty(),
      "oracle {} invented deps not in runtime: {invented:?}\ntracer={tracer:?}\nruntime={runtime:?}",
      case.id
    );

    let concrete_hits = tracer.intersection(&runtime).count();
    // A deep-root sentinel covers every runtime dep key on that binding (iterate +
    // nested fields) without claiming those concrete keys in the tracer set.
    let deep_covered = tracer
      .iter()
      .filter(|dep| dep.1.as_deref() == Some(DEEP_WATCH_PROPERTY))
      .map(|dep| runtime.iter().filter(|(binding, _)| binding == &dep.0).count())
      .sum::<usize>();
    let hits = concrete_hits.saturating_add(deep_covered);
    total_runtime = total_runtime.saturating_add(runtime.len());
    total_hit = total_hit.saturating_add(hits);
    let recall = if runtime.is_empty() {
      1.0
    } else {
      #[expect(clippy::cast_precision_loss, reason = "recall reporting only")]
      let ratio = hits as f64 / runtime.len() as f64;
      ratio
    };
    report.push(format!(
      "{}: runtime={} tracer={} hits={} recall={:.0}%",
      case.id,
      runtime.len(),
      tracer.len(),
      hits,
      recall * 100.0
    ));
  }

  let overall = if total_runtime == 0 {
    1.0
  } else {
    #[expect(clippy::cast_precision_loss, reason = "recall reporting only")]
    let ratio = total_hit as f64 / total_runtime as f64;
    ratio
  };
  assert!(
    overall >= 0.99,
    "oracle recall regressed below 99%:\n{}\noverall={:.0}% ({total_hit}/{total_runtime})",
    report.join("\n"),
    overall * 100.0
  );
}

#[test]
fn oracle_cases_cover_known_hard_facts() {
  let ids = load_cases().into_iter().map(|case| case.id).collect::<BTreeSet<_>>();
  for required in [
    "baseline-ref-computed",
    "computed-object-get",
    "pause-tracking-window",
    "props-reactive-object",
    "reactive-member",
    "runner-run-no-track",
    "array-from-mapfn",
    "json-parse-reviver",
    "sort-hof",
    "string-replace-hof",
    "sync-every-hof",
    "sync-filter-hof",
    "sync-find-hof",
    "sync-flatMap-hof",
    "sync-forEach-hof",
    "sync-map-hof",
    "sync-reduce-hof",
    "sync-some-hof",
    "to-value-getter",
    "use-route-like",
    "watch-effect-await",
    "watch-effect-ref",
    "watch-source-array",
    "watch-source-array-getters",
    "watch-source-getter",
    "watch-source-ref",
  ] {
    assert!(ids.contains(required), "missing oracle case {required}");
  }
}

/// Exhaustive tracking-read set for a computed scope (not mere existence).
fn assert_computed_reads_exact(graph: &ReactivityGraph, expected: &[(&str, Option<&str>)]) {
  let computed =
    graph.scopes.iter().find(|scope| scope.kind == vue_vet_core::TrackingScopeKind::Computed);
  assert!(computed.is_some(), "computed scope missing; scopes={:?}", graph.scopes);
  let actual = computed
    .into_iter()
    .flat_map(|scope| &scope.reads)
    .filter(|read| {
      matches!(read.kind, ReactiveReadKind::Unconditional | ReactiveReadKind::Conditional)
    })
    .map(|read| (read.binding.as_str(), read.property.as_deref()))
    .collect::<BTreeSet<_>>();
  let expected = expected.iter().copied().collect::<BTreeSet<_>>();
  assert_eq!(
    actual, expected,
    "computed tracking reads must match exactly (no missing, no invented)"
  );
}

#[test]
fn define_props_is_modeled_as_reactive_binding() {
  let graph = graph(
    "import { computed } from 'vue'\n\
     const props = defineProps<{ count: number }>()\n\
     const doubled = computed(() => props.count * 2)\n",
  );
  assert!(
    graph.bindings.iter().any(|binding| {
      binding.name == "props" && binding.kind == vue_vet_core::ReactiveBindingKind::Reactive
    }),
    "defineProps assignment must create a reactive binding"
  );
  assert!(
    graph.scopes.iter().any(|scope| {
      scope.kind == vue_vet_core::TrackingScopeKind::Computed
        && scope
          .reads
          .iter()
          .any(|read| read.binding == "props" && read.property.as_deref() == Some("count"))
    }),
    "computed must read props.count; scopes={:?}",
    graph.scopes
  );
}

#[test]
fn with_defaults_define_props_is_reactive_binding() {
  let graph = graph(
    "import { computed, withDefaults } from 'vue'\n\
     const props = withDefaults(defineProps<{ n: number }>(), { n: 0 })\n\
     const doubled = computed(() => props.n * 2)\n",
  );
  assert!(
    graph.bindings.iter().any(|binding| {
      binding.name == "props" && binding.kind == vue_vet_core::ReactiveBindingKind::Reactive
    }),
    "withDefaults(defineProps()) must seed a reactive props binding; bindings={:?}",
    graph.bindings
  );
  assert_computed_reads_exact(&graph, &[("props", Some("n"))]);
}

#[test]
fn computed_object_get_set_tracks_getter_reads() {
  let graph = graph(
    "import { ref, computed } from 'vue'\n\
     const count = ref(1)\n\
     const doubled = computed({\n\
       get() { return count.value * 2 },\n\
       set(v) { count.value = v },\n\
     })\n",
  );
  assert_computed_reads_exact(&graph, &[("count", Some("value"))]);
}

#[test]
fn watch_source_array_of_getters_tracks_each_body() {
  let graph = graph(
    "import { ref, watch } from 'vue'\n\
     const a = ref(1)\n\
     const b = ref(2)\n\
     watch([() => a.value, () => b.value], () => {})\n",
  );
  let Some(sources) =
    graph.scopes.iter().find(|scope| scope.kind == vue_vet_core::TrackingScopeKind::WatchSources)
  else {
    panic!("watch sources scope missing; scopes={:?}", graph.scopes);
  };
  let keys = sources
    .reads
    .iter()
    .map(|read| (read.binding.as_str(), read.property.as_deref()))
    .collect::<BTreeSet<_>>();
  assert_eq!(
    keys,
    BTreeSet::from([("a", Some("value")), ("b", Some("value"))]),
    "array of getters must contribute both source reads"
  );
}

#[test]
fn watch_source_mixed_ref_and_getter() {
  let graph = graph(
    "import { ref, watch } from 'vue'\n\
     const a = ref(1)\n\
     const b = ref(2)\n\
     watch([a, () => b.value], () => {})\n",
  );
  let Some(sources) =
    graph.scopes.iter().find(|scope| scope.kind == vue_vet_core::TrackingScopeKind::WatchSources)
  else {
    panic!("watch sources scope missing; scopes={:?}", graph.scopes);
  };
  let keys = sources
    .reads
    .iter()
    .map(|read| (read.binding.as_str(), read.property.as_deref()))
    .collect::<BTreeSet<_>>();
  assert_eq!(
    keys,
    BTreeSet::from([("a", Some("value")), ("b", Some("value"))]),
    "mixed ref + getter sources must both track"
  );
}

#[test]
fn sort_comparator_tracks_nested_reactive_reads() {
  let graph = graph(
    "import { ref, watchEffect } from 'vue'\n\
     const list = ref([3, 1, 2])\n\
     const key = ref(0)\n\
     watchEffect(() => { list.value.slice().sort((a, b) => a - b + key.value) })\n",
  );
  let Some(effect) = graph.scopes.iter().find(|scope| scope.kind.is_effect_family()) else {
    panic!("watchEffect scope missing; scopes={:?}", graph.scopes);
  };
  let keys = effect
    .reads
    .iter()
    .filter(|read| {
      matches!(read.kind, ReactiveReadKind::Unconditional | ReactiveReadKind::Conditional)
    })
    .map(|read| (read.binding.as_str(), read.property.as_deref()))
    .collect::<BTreeSet<_>>();
  assert!(
    keys.contains(&("list", Some("value"))) && keys.contains(&("key", Some("value"))),
    "sort comparator must track list.value and key.value; got {keys:?}"
  );
}

#[test]
fn string_replace_callback_tracks_nested_reactive_reads() {
  let graph = graph(
    "import { ref, computed } from 'vue'\n\
     const text = ref('ab')\n\
     const flag = ref(true)\n\
     const d = computed(() => text.value.replace(/./g, c => flag.value ? c : ''))\n\
     void d.value\n",
  );
  assert_computed_reads_exact(&graph, &[("flag", Some("value")), ("text", Some("value"))]);
}

#[test]
fn array_from_mapfn_tracks_nested_reactive_reads() {
  let graph = graph(
    "import { ref, computed } from 'vue'\n\
     const list = ref([1, 2])\n\
     const factor = ref(2)\n\
     const d = computed(() => Array.from(list.value, x => x * factor.value))\n\
     void d.value\n",
  );
  assert_computed_reads_exact(&graph, &[("factor", Some("value")), ("list", Some("value"))]);
}

#[test]
fn json_parse_reviver_tracks_nested_reactive_reads() {
  let graph = graph(
    "import { ref, computed } from 'vue'\n\
     const raw = ref('{\"a\":1}')\n\
     const flag = ref(true)\n\
     const d = computed(() => JSON.parse(raw.value, (k, v) => flag.value ? v : v))\n\
     void d.value\n",
  );
  assert_computed_reads_exact(&graph, &[("flag", Some("value")), ("raw", Some("value"))]);
}

#[test]
fn sync_filter_callback_tracks_nested_reactive_reads() {
  let graph = graph(
    "import { ref, computed } from 'vue'\n\
     const list = ref(['ada', 'linus'])\n\
     const query = ref('a')\n\
     const filtered = computed(() => list.value.filter((item) => item.includes(query.value)))\n",
  );
  assert_computed_reads_exact(&graph, &[("list", Some("value")), ("query", Some("value"))]);
}

#[test]
fn store_to_refs_destructure_fields_are_ref_like() {
  let graph = graph(
    "import { storeToRefs } from 'pinia'\n\
     import { computed } from 'vue'\n\
     const store = useCounterStore()\n\
     const { count, label } = storeToRefs(store)\n\
     const text = computed(() => count.value + label.value)\n",
  );
  assert!(
    graph.bindings.iter().any(|binding| {
      binding.name == "count" && binding.kind == vue_vet_core::ReactiveBindingKind::ToRef
    }) && graph.bindings.iter().any(|binding| {
      binding.name == "label" && binding.kind == vue_vet_core::ReactiveBindingKind::ToRef
    }),
    "storeToRefs destructure must seed ToRef locals; bindings={:?}",
    graph.bindings
  );
  assert_computed_reads_exact(&graph, &[("count", Some("value")), ("label", Some("value"))]);
}

#[test]
fn use_route_is_reactive_object_source() {
  let graph = graph(
    "import { useRoute } from 'vue-router'\n\
     import { computed } from 'vue'\n\
     const route = useRoute()\n\
     const title = computed(() => route.path)\n",
  );
  assert!(
    graph.bindings.iter().any(|binding| {
      binding.name == "route" && binding.kind == vue_vet_core::ReactiveBindingKind::Reactive
    }),
    "useRoute() must create a reactive binding"
  );
  assert_computed_reads_exact(&graph, &[("route", Some("path"))]);
}

#[test]
fn dependency_edges_qualify_effect_and_template_from_nodes() {
  let graph = graph(
    "import { ref, computed, watchEffect } from 'vue'\n\
     const source = ref(1)\n\
     const doubled = computed(() => source.value * 2)\n\
     watchEffect(() => { void source.value })\n",
  );
  assert!(
    graph.edges.iter().any(|edge| {
      edge.kind == vue_vet_core::ReactiveDependencyKind::Computed
        && edge.from == "doubled"
        && edge.to == "source"
    }),
    "computed edges keep binding-name from"
  );
  assert!(
    graph.edges.iter().any(|edge| {
      edge.kind == vue_vet_core::ReactiveDependencyKind::Effect
        && edge.from.starts_with("effect:watchEffect@")
        && edge.to == "source"
    }),
    "effect edges use kind:callee@offset from ids; edges={:?}",
    graph.edges
  );
}
