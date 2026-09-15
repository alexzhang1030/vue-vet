use std::path::Path;

use vue_vet_core::{
  Diagnostic, FileId, PRACTICE_CATEGORY, RuleEnvironment, RuleRegistry, Severity, VueVersion,
};
use vue_vet_practice::practice_rules;
use vue_vet_rules::builtin_rules;
use vue_vet_vize::{AnalyzeError, analyze_sfc_with_facts};

fn analyze_sfc(path: &Path, source: &str) -> Result<Vec<Diagnostic>, AnalyzeError> {
  analyze_sfc_with_environment(path, source, RuleEnvironment::default())
}

fn analyze_sfc_with_environment(
  path: &Path,
  source: &str,
  environment: RuleEnvironment,
) -> Result<Vec<Diagnostic>, AnalyzeError> {
  let analysis = analyze_sfc_with_facts(path, source)?;
  let mut rules = builtin_rules();
  rules.extend(practice_rules());
  Ok(RuleRegistry::new(rules).run_with_environment(
    path,
    source,
    &analysis.facts.template,
    &analysis.facts.script,
    environment,
  ))
}

#[expect(clippy::panic, reason = "a missing parser error must fail the golden test")]
fn parser_error_snapshot(logical_path: &str, source: &str) -> String {
  match analyze_sfc(Path::new(logical_path), source) {
    Ok(diagnostics) => {
      panic!("malformed fixture unexpectedly produced diagnostics: {diagnostics:?}")
    }
    Err(AnalyzeError::Parse(message)) => AnalyzeError::Parse(message).to_string(),
    Err(AnalyzeError::Template(message)) => AnalyzeError::Template(message).to_string(),
    Err(AnalyzeError::Script(error)) => AnalyzeError::Script(error).to_string(),
  }
}

#[expect(clippy::panic, reason = "unexpected fixture analysis errors must fail golden tests")]
fn analyze_versioned(path: &str, source: &str, minor: u64) -> Vec<Diagnostic> {
  match analyze_sfc_with_environment(
    Path::new(path),
    source,
    RuleEnvironment {
      vue_version: Some(VueVersion { major: 3, minor, patch: 0 }),
      packages: Vec::new(),
    },
  ) {
    Ok(diagnostics) => diagnostics,
    Err(error) => panic!("versioned rule fixture unexpectedly failed: {error}"),
  }
}

#[test]
#[expect(clippy::panic, reason = "unexpected fixture analysis errors must fail golden tests")]
fn recommended_rule_pack_covers_all_rules_with_valid_spans() {
  let recommended = include_str!("../../../fixtures/rules/recommended/invalid.vue");
  let props =
    include_str!("../../../fixtures/rules/no-nonreactive-props-destructure/invalid/direct.vue");
  let template_ref =
    include_str!("../../../fixtures/rules/prefer-use-template-ref/invalid/ref-null.vue");
  let groups = [
    (
      recommended,
      match analyze_sfc(Path::new("fixtures/rules/recommended/invalid.vue"), recommended) {
        Ok(diagnostics) => diagnostics,
        Err(error) => panic!("recommended rule fixture unexpectedly failed: {error}"),
      },
    ),
    (
      props,
      analyze_versioned(
        "fixtures/rules/no-nonreactive-props-destructure/invalid/direct.vue",
        props,
        4,
      ),
    ),
    (
      template_ref,
      analyze_versioned(
        "fixtures/rules/prefer-use-template-ref/invalid/ref-null.vue",
        template_ref,
        5,
      ),
    ),
  ];
  let ids = groups
    .iter()
    .flat_map(|(_, diagnostics)| diagnostics)
    .map(|diagnostic| diagnostic.rule_id.as_str())
    .collect::<std::collections::BTreeSet<_>>();
  // Legacy recommended fixture pack covers the original Essential/a11y/reactivity
  // slice. Matrix/graph_extra/directive expansions have per-rule fixtures under
  // fixtures/rules/<name>/ and are not required to appear in recommended/invalid.vue.
  assert!(
    ids.len() >= 30,
    "recommended fixture pack should still exercise the original rule slice (got {})",
    ids.len()
  );
  assert!(
    groups
      .iter()
      .flat_map(|(_, diagnostics)| diagnostics)
      .filter(|diagnostic| diagnostic.rule_id == "vue-vet/reactivity/prefer-use-template-ref")
      .all(|diagnostic| {
        diagnostic.severity == Severity::Info && diagnostic.category == PRACTICE_CATEGORY
      }),
    "prefer-use-template-ref is a practice suggestion with recommendation payload"
  );
  assert!(
    groups
      .iter()
      .flat_map(|(_, diagnostics)| diagnostics)
      .filter(|diagnostic| diagnostic.rule_id == "vue-vet/reactivity/prefer-use-template-ref")
      .all(|diagnostic| diagnostic.recommendation.is_some()),
    "prefer-use-template-ref must attach a useTemplateRef recommendation"
  );
  for (source, diagnostics) in groups {
    for diagnostic in diagnostics {
      let end = diagnostic.span.offset.saturating_add(diagnostic.span.length);
      let snippet = source.get(diagnostic.span.offset..end);
      assert!(
        snippet.is_some_and(|snippet| !snippet.is_empty()),
        "{} must retain a non-empty original-source span",
        diagnostic.rule_id
      );
    }
  }
}

#[test]
#[expect(clippy::panic, reason = "unexpected fixture analysis errors must fail golden tests")]
fn recommended_rule_pack_safe_patterns_are_quiet() {
  let source = include_str!("../../../fixtures/rules/recommended/valid.vue");
  let diagnostics = match analyze_sfc(Path::new("fixtures/rules/recommended/valid.vue"), source) {
    Ok(diagnostics) => diagnostics,
    Err(error) => panic!("recommended safe fixture unexpectedly failed: {error}"),
  };
  assert!(diagnostics.is_empty(), "safe patterns must not produce recommended findings");

  let props =
    include_str!("../../../fixtures/rules/no-nonreactive-props-destructure/invalid/direct.vue");
  assert!(
    analyze_versioned(
      "fixtures/rules/no-nonreactive-props-destructure/invalid/direct.vue",
      props,
      5,
    )
    .is_empty(),
    "Vue 3.5 compiler-reactive props destructuring must stay quiet"
  );
  let to_refs =
    include_str!("../../../fixtures/rules/no-nonreactive-props-destructure/valid/to-refs.vue");
  assert!(
    analyze_versioned(
      "fixtures/rules/no-nonreactive-props-destructure/valid/to-refs.vue",
      to_refs,
      4,
    )
    .is_empty(),
    "toRefs must preserve props reactivity before Vue 3.5"
  );
  assert!(
    analyze_versioned(
      "fixtures/rules/prefer-use-template-ref/invalid/ref-null.vue",
      include_str!("../../../fixtures/rules/prefer-use-template-ref/invalid/ref-null.vue"),
      4,
    )
    .is_empty(),
    "useTemplateRef must not be recommended before Vue 3.5"
  );
}

#[test]
fn practice_prefer_to_value_is_version_gated() {
  let unref = include_str!("../../../fixtures/rules/prefer-to-value/invalid/unref.vue");
  assert!(
    analyze_versioned("fixtures/rules/prefer-to-value/invalid/unref.vue", unref, 3)
      .iter()
      .any(|diagnostic| diagnostic.rule_id.ends_with("prefer-to-value")),
    "toValue must be recommended on Vue 3.3+"
  );
  assert!(
    analyze_versioned("fixtures/rules/prefer-to-value/invalid/unref.vue", unref, 2).is_empty(),
    "toValue must not be recommended before Vue 3.3"
  );
}

#[test]
fn practice_stable_computed_identity_is_version_gated() {
  let source = include_str!(
    "../../../fixtures/rules/prefer-stable-computed-identity/invalid/object-fields.vue"
  );
  let path = "fixtures/rules/prefer-stable-computed-identity/invalid/object-fields.vue";
  assert!(
    analyze_versioned(path, source, 4)
      .iter()
      .any(|diagnostic| diagnostic.rule_id.ends_with("prefer-stable-computed-identity")),
    "stable computed identity must report on Vue 3.4+"
  );
  assert!(
    analyze_versioned(path, source, 3)
      .iter()
      .all(|diagnostic| !diagnostic.rule_id.ends_with("prefer-stable-computed-identity")),
    "stable computed identity must stay quiet before Vue 3.4"
  );
}

#[test]
fn malformed_parser_fixture_matches_the_error_snapshot() {
  let actual = parser_error_snapshot(
    "fixtures/parser/malformed/unclosed-template.vue",
    include_str!("../../../fixtures/parser/malformed/unclosed-template.vue"),
  );
  assert_eq!(
    actual,
    include_str!("../../../fixtures/snapshots/parser/unclosed-template.txt").trim_end(),
    "parser failure snapshot changed"
  );
}

#[test]
fn path_normalization_is_platform_independent() {
  assert_eq!(
    FileId::from(r"fixtures\rules\no-v-html\invalid\basic.vue").as_str(),
    "fixtures/rules/no-v-html/invalid/basic.vue",
    "Windows separators must normalize to the persisted form"
  );
}

#[test]
#[expect(clippy::panic, reason = "fixture analysis errors must fail the producer test")]
fn returned_watch_handle_producer_owns_both_rule_configurations() {
  let path = "fixtures/rules/no-returned-watcher-cleanup/invalid/returned-watch-handle.vue";
  let source = include_str!(
    "../../../fixtures/rules/no-returned-watcher-cleanup/invalid/returned-watch-handle.vue"
  );
  let both = analyze_sfc(Path::new(path), source).unwrap_or_else(|error| panic!("{error}"));
  assert_eq!(
    both.iter().filter(|row| row.rule_id.ends_with("no-returned-watcher-cleanup")).count(),
    1,
    "{both:?}"
  );
  assert!(
    both.iter().all(|row| !row.rule_id.ends_with("no-nested-watch-without-cleanup")),
    "nested must stay quiet when the inner call is returned: {both:?}"
  );
  assert_eq!(both.first().map(|row| row.span.line), Some(6));
  let analysis =
    analyze_sfc_with_facts(Path::new(path), source).unwrap_or_else(|error| panic!("{error}"));
  let nested_only = RuleRegistry::new(
    builtin_rules()
      .into_iter()
      .filter(|rule| rule.meta().id.ends_with("no-nested-watch-without-cleanup"))
      .collect(),
  )
  .run(Path::new(path), source, &analysis.facts.template, &analysis.facts.script);
  assert!(
    nested_only.iter().all(|row| !row.rule_id.ends_with("no-nested-watch-without-cleanup")),
    "nested-only config must stay quiet: {nested_only:?}"
  );
  let returned_only = RuleRegistry::new(
    builtin_rules()
      .into_iter()
      .filter(|rule| rule.meta().id.ends_with("no-returned-watcher-cleanup"))
      .collect(),
  )
  .run(Path::new(path), source, &analysis.facts.template, &analysis.facts.script);
  assert_eq!(
    returned_only.iter().filter(|row| row.rule_id.ends_with("no-returned-watcher-cleanup")).count(),
    1,
    "returned-only config must keep the handle finding: {returned_only:?}"
  );
}

/// Vue tracks dynamic dependencies and coalesces watch*Effect self-writes.
/// These fixtures stay as semantic regressions after the retired IDs were removed.
#[test]
fn reactivity_semantics_keep_dynamic_deps_and_self_writes_quiet() {
  for (path, source) in [
    (
      "fixtures/reactivity-semantics/dynamic-deps/nested-callback.vue",
      include_str!("../../../fixtures/reactivity-semantics/dynamic-deps/nested-callback.vue"),
    ),
    (
      "fixtures/reactivity-semantics/dynamic-deps/guarded-watch-effect.vue",
      include_str!("../../../fixtures/reactivity-semantics/dynamic-deps/guarded-watch-effect.vue"),
    ),
    (
      "fixtures/reactivity-semantics/dynamic-deps/computed-ternary.vue",
      include_str!("../../../fixtures/reactivity-semantics/dynamic-deps/computed-ternary.vue"),
    ),
    (
      "fixtures/reactivity-semantics/effect-self-write/watch-effect.vue",
      include_str!("../../../fixtures/reactivity-semantics/effect-self-write/watch-effect.vue"),
    ),
    (
      "fixtures/reactivity-semantics/setup-after-await/on-mounted.vue",
      include_str!("../../../fixtures/reactivity-semantics/setup-after-await/on-mounted.vue"),
    ),
    (
      "fixtures/reactivity-semantics/setup-after-await/define-props.vue",
      include_str!("../../../fixtures/reactivity-semantics/setup-after-await/define-props.vue"),
    ),
  ] {
    let diagnostics = analyze_sfc(Path::new(path), source).unwrap_or_default();
    assert!(diagnostics.is_empty(), "{path} must stay quiet; {diagnostics:?}");
  }
}
