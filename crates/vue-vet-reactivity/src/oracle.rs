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

use crate::trace_reactivity;

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

    let invented = tracer.difference(&runtime).cloned().collect::<Vec<_>>();
    assert!(
      invented.is_empty(),
      "oracle {} invented deps not in runtime: {invented:?}\ntracer={tracer:?}\nruntime={runtime:?}",
      case.id
    );

    let hits = tracer.intersection(&runtime).count();
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
    "props-reactive-object",
    "sync-filter-hof",
    "runner-run-no-track",
    "watch-effect-await",
  ] {
    assert!(ids.contains(required), "missing oracle case {required}");
  }
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
fn sync_filter_callback_tracks_nested_reactive_reads() {
  let graph = graph(
    "import { ref, computed } from 'vue'\n\
     const list = ref(['ada', 'linus'])\n\
     const query = ref('a')\n\
     const filtered = computed(() => list.value.filter((item) => item.includes(query.value)))\n",
  );
  let computed =
    graph.scopes.iter().find(|scope| scope.kind == vue_vet_core::TrackingScopeKind::Computed);
  assert!(computed.is_some(), "computed scope missing");
  let reads = computed
    .into_iter()
    .flat_map(|scope| &scope.reads)
    .map(|read| (read.binding.as_str(), read.property.as_deref()))
    .collect::<BTreeSet<_>>();
  assert!(
    reads.contains(&("list", Some("value"))) && reads.contains(&("query", Some("value"))),
    "sync filter callback must track list and query; got {reads:?}"
  );
}
