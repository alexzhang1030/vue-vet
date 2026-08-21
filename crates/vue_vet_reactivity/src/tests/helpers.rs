use std::{collections::BTreeSet, path::Path, sync::Arc};

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;

pub(super) use crate::{
  ModuleLink, ModuleReactivity, ModuleSource, ModuleTraceState, NamedApiBag, TraceConfig,
  TraceModulesOptions, merge_declaration_implementation_summary, prepare_module_summary,
  prepare_standalone_module_source, trace_modules, trace_modules_incremental_with_options,
  trace_reactivity_with_config,
};
pub(super) use vue_vet_core::{
  ReactiveBindingKind, ReactiveDependencyKind, ReactiveGuardRole, ReactiveReadKind,
  ReactivityGraph, ScriptKind, SourceSpan, TemplateDirectiveFact, TemplateElementFact,
  TemplateExpressionFact, TemplateFacts, TrackingScopeKind,
};
/// Test fixture catalog mirroring production `vue_vet_plugins` defaults.
/// Engine unit tests must not depend on the plugins crate (avoids a dep cycle).
pub(super) fn fixture_named_api_bags() -> &'static [NamedApiBag] {
  fn async_data_field_kind(field: &str) -> Option<ReactiveBindingKind> {
    match field {
      "data" | "pending" | "error" | "status" => Some(ReactiveBindingKind::Ref),
      _ => None,
    }
  }
  fn i18n_field_kind(field: &str) -> Option<ReactiveBindingKind> {
    match field {
      "locale" | "fallbackLocale" | "locales" | "messages" | "availableLocales" => {
        Some(ReactiveBindingKind::Computed)
      }
      _ => None,
    }
  }
  static BAGS: &[NamedApiBag] = &[
    NamedApiBag {
      callee: "useAsyncData",
      field_kind: async_data_field_kind,
      ambient_methods: &[],
      ambient_fields: &[],
    },
    NamedApiBag {
      callee: "useFetch",
      field_kind: async_data_field_kind,
      ambient_methods: &[],
      ambient_fields: &[],
    },
    NamedApiBag {
      callee: "useI18n",
      field_kind: i18n_field_kind,
      ambient_methods: &["t", "d", "n", "rt", "te"],
      ambient_fields: &["locale", "fallbackLocale", "messages"],
    },
    NamedApiBag {
      callee: "useLazyAsyncData",
      field_kind: async_data_field_kind,
      ambient_methods: &[],
      ambient_fields: &[],
    },
    NamedApiBag {
      callee: "useLazyFetch",
      field_kind: async_data_field_kind,
      ambient_methods: &[],
      ambient_fields: &[],
    },
  ];
  BAGS
}

pub(super) fn default_trace_config() -> TraceConfig<'static> {
  TraceConfig { named_api_bags: fixture_named_api_bags() }
}

pub(super) fn default_trace_options() -> TraceModulesOptions {
  TraceModulesOptions { named_api_bags: fixture_named_api_bags().to_vec(), ..Default::default() }
}

pub(super) fn trace(
  sfc_source: &str,
  script_source: &str,
  script_offset: usize,
  kind: ScriptKind,
) -> ReactivityGraph {
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, script_source, SourceType::ts()).parse();
  assert!(
    parsed.diagnostics.is_empty(),
    "script parsing unexpectedly failed: {:?}",
    parsed.diagnostics
  );
  let built = SemanticBuilder::new()
    .with_build_nodes(true)
    .with_check_syntax_error(true)
    .build(&parsed.program);
  assert!(
    built.diagnostics.is_empty(),
    "semantic analysis unexpectedly failed: {:?}",
    built.diagnostics
  );
  trace_reactivity_with_config(
    &built.semantic,
    sfc_source,
    script_offset,
    kind,
    &default_trace_config(),
  )
}

pub(super) fn graph(source: &str) -> ReactivityGraph {
  trace(source, source, 0, ScriptKind::Setup)
}

pub(super) fn graph_tsx(source: &str) -> ReactivityGraph {
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, source, SourceType::tsx()).parse();
  assert!(
    parsed.diagnostics.is_empty(),
    "tsx parsing unexpectedly failed: {:?}",
    parsed.diagnostics
  );
  let built = SemanticBuilder::new()
    .with_build_nodes(true)
    .with_check_syntax_error(true)
    .build(&parsed.program);
  assert!(
    built.diagnostics.is_empty(),
    "tsx semantic analysis unexpectedly failed: {:?}",
    built.diagnostics
  );
  trace_reactivity_with_config(
    &built.semantic,
    source,
    0,
    ScriptKind::Script,
    &default_trace_config(),
  )
}

pub(super) fn test_span(offset: usize) -> SourceSpan {
  SourceSpan { offset, length: 1, line: 1, column: offset.saturating_add(1) }
}

pub(super) fn helper_follow_scope(
  graph: &ReactivityGraph,
  kind: TrackingScopeKind,
) -> Option<&vue_vet_core::TrackingScopeFact> {
  graph.scopes.iter().find(|scope| scope.kind == kind)
}

pub(super) fn helper_follow_has_value_read(
  graph: &ReactivityGraph,
  kind: TrackingScopeKind,
  binding: &str,
) -> bool {
  helper_follow_scope(graph, kind).is_some_and(|scope| {
    scope
      .reads
      .iter()
      .any(|read| read.binding == binding && read.property.as_deref() == Some("value"))
  })
}

#[expect(clippy::panic, reason = "fixture setup failures must fail the unit test")]
pub(super) fn prepared_standalone(id: &str, source: &str, language: &str) -> ModuleSource {
  prepare_standalone_module_source(id, source, language)
    .unwrap_or_else(|error| panic!("prepare {id}: {error}"))
}

#[expect(clippy::panic, reason = "fixture setup failures must fail the unit test")]
pub(super) fn attached_summary(module: &ModuleSource) -> Arc<crate::ModuleSummary> {
  module.module_summary().unwrap_or_else(|| panic!("missing summary for {}", module.id))
}

/// One read in an exhaustive effect read-set assertion.
#[derive(serde::Deserialize)]
pub(super) struct LocalReadExpectation {
  pub(super) binding: String,
  pub(super) kind: ReactiveReadKind,
  #[serde(default)]
  pub(super) guards: Vec<String>,
}

#[derive(serde::Deserialize)]
pub(super) struct LocalExpectation {
  pub(super) effect: String,
  pub(super) binding: String,
  pub(super) kind: ReactiveReadKind,
  pub(super) guards: Vec<String>,
  /// When present, the effect's full read set must match exactly (no missing, no invented).
  #[serde(default)]
  pub(super) reads: Option<Vec<LocalReadExpectation>>,
}

#[derive(serde::Deserialize)]
pub(super) struct LocalFixture {
  pub(super) name: String,
  pub(super) source: String,
  pub(super) expected: LocalExpectation,
}

#[derive(serde::Deserialize)]
pub(super) struct ModuleExpectation {
  pub(super) module: String,
  pub(super) binding: String,
  pub(super) kind: ReactiveReadKind,
  pub(super) guards: Vec<String>,
  pub(super) trace: bool,
}

#[derive(serde::Deserialize)]
pub(super) struct ModuleFixture {
  pub(super) name: String,
  pub(super) modules: Vec<ModuleSource>,
  pub(super) links: Vec<ModuleLink>,
  pub(super) expected: ModuleExpectation,
}

#[derive(serde::Deserialize)]
pub(super) struct Provenance {
  pub(super) repository: String,
  pub(super) commit: String,
  pub(super) path: String,
  pub(super) adaptation: String,
}

#[derive(serde::Deserialize)]
pub(super) struct RealWorldFixture {
  pub(super) name: String,
  pub(super) provenance: Provenance,
  pub(super) modules: Vec<FixtureModule>,
  pub(super) links: Vec<ModuleLink>,
  pub(super) expected: ModuleExpectation,
}

#[derive(serde::Deserialize)]
pub(super) struct FixtureModule {
  pub(super) id: String,
  pub(super) file: String,
  pub(super) language: String,
  pub(super) kind: ScriptKind,
}

#[derive(serde::Deserialize)]
pub(super) struct RegressionManifest {
  pub(super) name: String,
  pub(super) expected: ModuleExpectation,
}

macro_rules! corpus_batches {
  ($($path:literal),+ $(,)?) => {
    [$(($path, include_str!(concat!("../../fixtures/corpus/", $path)))),+]
  };
}

pub(super) const SYSTEMATIC_FIXTURES: [(&str, &str); 10] = corpus_batches!(
  "systematic/batch-01.json",
  "systematic/batch-02.json",
  "systematic/batch-03.json",
  "systematic/batch-04.json",
  "systematic/batch-05.json",
  "systematic/batch-06.json",
  "systematic/batch-07.json",
  "systematic/batch-08.json",
  "systematic/batch-09.json",
  "systematic/batch-10.json",
);

pub(super) const COMPLEX_FIXTURES: [(&str, &str); 10] = corpus_batches!(
  "complex/01-sequential-early-returns.json",
  "complex/02-nested-if.json",
  "complex/03-if-logical.json",
  "complex/04-logical-chain.json",
  "complex/05-nested-ternary.json",
  "complex/06-early-return-then-if.json",
  "complex/07-else-if.json",
  "complex/08-try-finally-in-branch.json",
  "complex/09-switch-in-branch.json",
  "complex/10-loop-in-branch.json",
);

pub(super) const MODULE_FIXTURES: [(&str, &str); 8] = corpus_batches!(
  "modules/01-direct-named.json",
  "modules/02-composable-alias.json",
  "modules/03-default-export.json",
  "modules/04-star-barrel.json",
  "modules/05-named-multihop.json",
  "modules/06-cycle.json",
  "modules/07-unresolved.json",
  "modules/08-conflicting-star.json",
);

pub(super) const REAL_WORLD_FIXTURES: [(&str, &str); 5] = [
  ("nuxt-async-data", include_str!("../../fixtures/real-world/nuxt-async-data/case.json")),
  (
    "vueuse-computed-async",
    include_str!("../../fixtures/real-world/vueuse-computed-async/case.json"),
  ),
  (
    "vueuse-computed-eager",
    include_str!("../../fixtures/real-world/vueuse-computed-eager/case.json"),
  ),
  (
    "vue-router-current-route",
    include_str!("../../fixtures/real-world/vue-router-current-route/case.json"),
  ),
  ("pinia-store-to-refs", include_str!("../../fixtures/real-world/pinia-store-to-refs/case.json")),
];

#[expect(clippy::panic, reason = "malformed committed fixtures must fail corpus tests")]
pub(super) fn parse_fixture_batch<T: serde::de::DeserializeOwned>(
  path: &str,
  source: &str,
) -> Vec<T> {
  match serde_json::from_str(source) {
    Ok(fixtures) => fixtures,
    Err(error) => panic!("could not parse fixture batch {path}: {error}"),
  }
}

#[expect(clippy::panic, reason = "malformed committed fixtures must fail corpus tests")]
pub(super) fn parse_fixture<T: serde::de::DeserializeOwned>(path: &str, source: &str) -> T {
  match serde_json::from_str(source) {
    Ok(fixture) => fixture,
    Err(error) => panic!("could not parse fixture {path}: {error}"),
  }
}

pub(super) fn load_fixture_batches<T: serde::de::DeserializeOwned>(
  batches: &[(&str, &str)],
) -> Vec<T> {
  let mut fixtures = Vec::new();
  for (path, source) in batches {
    fixtures.extend(parse_fixture_batch(path, source));
  }
  fixtures
}

pub(super) fn assert_local_fixture(fixture: &LocalFixture) {
  let graph = graph(&fixture.source);
  let effect = graph.effects.iter().find(|effect| effect.callee == fixture.expected.effect);
  assert!(effect.is_some(), "expected effect must be resolved in {}", fixture.name);
  let payload = effect
    .into_iter()
    .flat_map(|effect| &effect.reads)
    .find(|read| read.binding == fixture.expected.binding);
  assert_eq!(
    payload.map(|read| read.kind),
    Some(fixture.expected.kind),
    "unexpected read classification in {}",
    fixture.name
  );
  assert!(
    payload.is_some_and(|read| {
      fixture
        .expected
        .guards
        .iter()
        .all(|expected| read.guards.iter().any(|guard| guard.binding == *expected))
    }),
    "expected guard evidence must survive in {}",
    fixture.name
  );
  assert!(
    fixture.expected.reads.is_some(),
    "local fixture {} must pin exhaustive expected.reads (regenerate from tracer if adding cases)",
    fixture.name
  );
  if let (Some(effect), Some(expected_reads)) = (effect, fixture.expected.reads.as_ref()) {
    assert_effect_reads_exact(effect, expected_reads, &fixture.name);
  }
}

/// Exact effect read-set: every (binding, kind, guard-names) pair must match.
pub(super) fn assert_effect_reads_exact(
  effect: &vue_vet_core::ReactivityEffectFact,
  expected: &[LocalReadExpectation],
  name: &str,
) {
  let actual = effect
    .reads
    .iter()
    .map(|read| {
      let guards = read.guards.iter().map(|guard| guard.binding.as_str()).collect::<BTreeSet<_>>();
      (read.binding.as_str(), read.kind, guards)
    })
    .collect::<BTreeSet<_>>();
  let expected = expected
    .iter()
    .map(|read| {
      let guards = read.guards.iter().map(String::as_str).collect::<BTreeSet<_>>();
      (read.binding.as_str(), read.kind, guards)
    })
    .collect::<BTreeSet<_>>();
  assert_eq!(
    actual, expected,
    "effect read set must match exactly in {name} (no missing, no invented)"
  );
}

pub(super) fn module_fixture_signature(modules: &[ModuleSource], links: &[ModuleLink]) -> String {
  let module_sources = modules
    .iter()
    .map(|module| format!("{}\n{}", module.id, module.source))
    .collect::<Vec<_>>()
    .join("\n---module---\n");
  let resolved_links = links
    .iter()
    .map(|link| format!("{}:{}:{}", link.from, link.specifier, link.to))
    .collect::<Vec<_>>()
    .join("\n");
  format!("{module_sources}\n---links---\n{resolved_links}")
}

pub(super) fn assert_module_case(
  name: &str,
  modules: &[ModuleSource],
  links: &[ModuleLink],
  expected: &ModuleExpectation,
) {
  assert!(modules.len() >= 2, "cross-module fixture must contain separate files: {name}");
  let traced = traced_modules(modules, links);
  let consumer = traced.iter().find(|module| module.id == expected.module);
  let payload = consumer
    .into_iter()
    .flat_map(|module| &module.graph.effects)
    .flat_map(|effect| &effect.reads)
    .find(|read| read.binding == expected.binding);
  if expected.trace {
    assert_eq!(
      payload.map(|read| read.kind),
      Some(expected.kind),
      "linked payload has the wrong classification in {name}"
    );
    assert!(
      payload.is_some_and(|read| {
        expected
          .guards
          .iter()
          .all(|expected| read.guards.iter().any(|guard| guard.binding == *expected))
      }),
      "linked payload must retain local guard evidence in {name}"
    );
  } else {
    assert!(payload.is_none(), "unsupported or shadowed module shapes must stay quiet in {name}");
  }
}

#[expect(clippy::panic, reason = "module tracing errors must fail corpus tests")]
pub(super) fn traced_modules(
  modules: &[ModuleSource],
  links: &[ModuleLink],
) -> Vec<ModuleReactivity> {
  match trace_modules(modules, links) {
    Ok(traced) => traced,
    Err(error) => panic!("cross-module tracing unexpectedly failed: {error}"),
  }
}

pub(super) fn module_source(id: &str, source: &str) -> ModuleSource {
  ModuleSource::standalone(id, source, "ts", ScriptKind::Script)
}

#[expect(clippy::panic, reason = "missing committed source files must fail corpus tests")]
pub(super) fn load_real_world_modules(
  case_dir: &str,
  files: &[FixtureModule],
) -> Vec<ModuleSource> {
  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/real-world").join(case_dir);
  files
    .iter()
    .map(|file| {
      let path = root.join(&file.file);
      let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => panic!("could not read real-world fixture {}: {error}", path.display()),
      };
      ModuleSource::standalone(file.id.clone(), source, file.language.clone(), file.kind)
    })
    .collect()
}

pub(super) fn regression_case(
  manifest_path: &str,
  manifest_source: &str,
  producer_source: &str,
  consumer_source: &str,
) {
  let manifest = parse_fixture::<RegressionManifest>(manifest_path, manifest_source);
  let modules = vec![
    module_source("producer.ts", producer_source),
    module_source("consumer.ts", consumer_source),
  ];
  let links = vec![ModuleLink {
    from: "consumer.ts".into(),
    specifier: "./producer".into(),
    to: "producer.ts".into(),
  }];
  assert_module_case(&manifest.name, &modules, &links, &manifest.expected);
}
