use std::path::{Path, PathBuf};

use vue_vet_core::{Diagnostic, PRACTICE_CATEGORY, RuleEnvironment, Severity, VueVersion};
use vue_vet_vize::{AnalyzeError, analyze_sfc, analyze_sfc_with_environment};

fn normalize_path(path: &Path) -> String {
  path.to_string_lossy().replace('\\', "/")
}

#[expect(clippy::panic, reason = "fixture read or serialization errors must fail golden tests")]
fn diagnostics_snapshot(logical_path: &str, source: &str) -> String {
  let mut diagnostics = match analyze_sfc(Path::new(logical_path), source) {
    Ok(diagnostics) => diagnostics,
    Err(error) => panic!("fixture unexpectedly failed to parse: {error}"),
  };
  for diagnostic in &mut diagnostics {
    diagnostic.file = PathBuf::from(normalize_path(&diagnostic.file));
  }
  match serde_json::to_string_pretty(&diagnostics) {
    Ok(snapshot) => snapshot,
    Err(error) => panic!("failed to serialize diagnostic snapshot: {error}"),
  }
}

fn assert_diagnostics(logical_path: &str, source: &str, expected: &str) {
  assert_eq!(
    diagnostics_snapshot(logical_path, source),
    expected.trim_end(),
    "diagnostic snapshot changed for {logical_path}"
  );
}

#[expect(clippy::panic, reason = "fixture serialization errors must fail golden tests")]
fn assert_versioned_diagnostics(logical_path: &str, source: &str, minor: u64, expected: &str) {
  let mut diagnostics = analyze_versioned(logical_path, source, minor);
  for diagnostic in &mut diagnostics {
    diagnostic.file = PathBuf::from(normalize_path(Path::new(logical_path)));
  }
  let actual = match serde_json::to_string_pretty(&diagnostics) {
    Ok(snapshot) => snapshot,
    Err(error) => panic!("failed to serialize diagnostic snapshot: {error}"),
  };
  assert_eq!(actual, expected.trim_end(), "diagnostic snapshot changed for {logical_path}");
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
    Ok(analysis) => analysis.diagnostics,
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
  assert_eq!(ids.len(), 31, "every recommended rule needs a positive fixture");
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
  for path_and_source in [
    (
      "fixtures/rules/prefer-use-template-ref/valid/use-template-ref.vue",
      include_str!("../../../fixtures/rules/prefer-use-template-ref/valid/use-template-ref.vue"),
    ),
    (
      "fixtures/rules/prefer-use-template-ref/valid/nonmatching-ref.vue",
      include_str!("../../../fixtures/rules/prefer-use-template-ref/valid/nonmatching-ref.vue"),
    ),
  ] {
    assert!(
      analyze_versioned(path_and_source.0, path_and_source.1, 5).is_empty(),
      "safe Vue 3.5 template-ref patterns must stay quiet"
    );
  }
  let old_template_ref =
    include_str!("../../../fixtures/rules/prefer-use-template-ref/invalid/ref-null.vue");
  assert!(
    analyze_versioned(
      "fixtures/rules/prefer-use-template-ref/invalid/ref-null.vue",
      old_template_ref,
      4,
    )
    .is_empty(),
    "useTemplateRef must not be recommended before Vue 3.5"
  );
  let nested = include_str!(
    "../../../fixtures/rules/no-conditional-watch-effect-dependency/valid/nested-callback.vue"
  );
  let nested_diagnostics =
    analyze_sfc(Path::new("nested-callback.vue"), nested).unwrap_or_default();
  assert!(
    nested_diagnostics.is_empty(),
    "reactive reads inside a nested callback are not watchEffect dependencies"
  );
}

#[test]
fn practice_prefer_to_value_fixtures_match_exact_diagnostics() {
  assert_versioned_diagnostics(
    "fixtures/rules/prefer-to-value/invalid/unref.vue",
    include_str!("../../../fixtures/rules/prefer-to-value/invalid/unref.vue"),
    3,
    include_str!("../../../fixtures/snapshots/prefer-to-value/unref.json"),
  );
  assert_versioned_diagnostics(
    "fixtures/rules/prefer-to-value/invalid/bare-unref.vue",
    include_str!("../../../fixtures/rules/prefer-to-value/invalid/bare-unref.vue"),
    3,
    include_str!("../../../fixtures/snapshots/prefer-to-value/bare-unref.json"),
  );
  assert_versioned_diagnostics(
    "fixtures/rules/prefer-to-value/invalid/imports-unref.vue",
    include_str!("../../../fixtures/rules/prefer-to-value/invalid/imports-unref.vue"),
    3,
    include_str!("../../../fixtures/snapshots/prefer-to-value/imports-unref.json"),
  );
  assert!(
    analyze_versioned(
      "fixtures/rules/prefer-to-value/invalid/unref.vue",
      include_str!("../../../fixtures/rules/prefer-to-value/invalid/unref.vue"),
      2,
    )
    .is_empty(),
    "toValue must not be recommended before Vue 3.3"
  );
  for (path, source) in [
    (
      "fixtures/rules/prefer-to-value/valid/to-value.vue",
      include_str!("../../../fixtures/rules/prefer-to-value/valid/to-value.vue"),
    ),
    (
      "fixtures/rules/prefer-to-value/valid/local-unref.vue",
      include_str!("../../../fixtures/rules/prefer-to-value/valid/local-unref.vue"),
    ),
  ] {
    assert!(
      analyze_versioned(path, source, 3).is_empty(),
      "safe toValue patterns must stay quiet on Vue 3.3+"
    );
  }
}

#[test]
fn practice_vueuse_fixtures_match_exact_diagnostics() {
  assert_diagnostics(
    "fixtures/rules/vueuse-use-debounce-fn/invalid/hand-rolled.vue",
    include_str!("../../../fixtures/rules/vueuse-use-debounce-fn/invalid/hand-rolled.vue"),
    include_str!("../../../fixtures/snapshots/vueuse-use-debounce-fn/hand-rolled.json"),
  );
  assert_diagnostics(
    "fixtures/rules/vueuse-use-event-listener/invalid/add-without-remove.vue",
    include_str!(
      "../../../fixtures/rules/vueuse-use-event-listener/invalid/add-without-remove.vue"
    ),
    include_str!("../../../fixtures/snapshots/vueuse-use-event-listener/add-without-remove.json"),
  );
  assert_diagnostics(
    "fixtures/rules/vueuse-use-interval-fn/invalid/set-without-clear.vue",
    include_str!("../../../fixtures/rules/vueuse-use-interval-fn/invalid/set-without-clear.vue"),
    include_str!("../../../fixtures/snapshots/vueuse-use-interval-fn/set-without-clear.json"),
  );
  assert_diagnostics(
    "fixtures/rules/vueuse-use-timeout-fn/invalid/set-without-clear.vue",
    include_str!("../../../fixtures/rules/vueuse-use-timeout-fn/invalid/set-without-clear.vue"),
    include_str!("../../../fixtures/snapshots/vueuse-use-timeout-fn/set-without-clear.json"),
  );
}

#[test]
fn practice_vueuse_safe_fixtures_produce_no_diagnostics() {
  let empty = "[]";
  for (path, source) in [
    (
      "fixtures/rules/vueuse-use-debounce-fn/valid/use-debounce-fn.vue",
      include_str!("../../../fixtures/rules/vueuse-use-debounce-fn/valid/use-debounce-fn.vue"),
    ),
    (
      "fixtures/rules/vueuse-use-debounce-fn/valid/plain-timeout.vue",
      include_str!("../../../fixtures/rules/vueuse-use-debounce-fn/valid/plain-timeout.vue"),
    ),
    (
      "fixtures/rules/vueuse-use-debounce-fn/valid/unrelated-clear.vue",
      include_str!("../../../fixtures/rules/vueuse-use-debounce-fn/valid/unrelated-clear.vue"),
    ),
    (
      "fixtures/rules/vueuse-use-event-listener/valid/use-event-listener.vue",
      include_str!(
        "../../../fixtures/rules/vueuse-use-event-listener/valid/use-event-listener.vue"
      ),
    ),
    (
      "fixtures/rules/vueuse-use-event-listener/valid/with-remove.vue",
      include_str!("../../../fixtures/rules/vueuse-use-event-listener/valid/with-remove.vue"),
    ),
    (
      "fixtures/rules/vueuse-use-event-listener/valid/bare-add.vue",
      include_str!("../../../fixtures/rules/vueuse-use-event-listener/valid/bare-add.vue"),
    ),
    (
      "fixtures/rules/vueuse-use-interval-fn/valid/use-interval-fn.vue",
      include_str!("../../../fixtures/rules/vueuse-use-interval-fn/valid/use-interval-fn.vue"),
    ),
    (
      "fixtures/rules/vueuse-use-interval-fn/valid/with-clear.vue",
      include_str!("../../../fixtures/rules/vueuse-use-interval-fn/valid/with-clear.vue"),
    ),
    (
      "fixtures/rules/vueuse-use-interval-fn/valid/bare-interval.vue",
      include_str!("../../../fixtures/rules/vueuse-use-interval-fn/valid/bare-interval.vue"),
    ),
    (
      "fixtures/rules/vueuse-use-timeout-fn/valid/use-timeout-fn.vue",
      include_str!("../../../fixtures/rules/vueuse-use-timeout-fn/valid/use-timeout-fn.vue"),
    ),
    (
      "fixtures/rules/vueuse-use-timeout-fn/valid/with-clear.vue",
      include_str!("../../../fixtures/rules/vueuse-use-timeout-fn/valid/with-clear.vue"),
    ),
    (
      "fixtures/rules/vueuse-use-timeout-fn/valid/bare-timeout.vue",
      include_str!("../../../fixtures/rules/vueuse-use-timeout-fn/valid/bare-timeout.vue"),
    ),
  ] {
    assert_diagnostics(path, source, empty);
  }
}

#[test]
fn no_v_html_invalid_fixtures_match_exact_diagnostics() {
  assert_diagnostics(
    "fixtures/rules/no-v-html/invalid/basic.vue",
    include_str!("../../../fixtures/rules/no-v-html/invalid/basic.vue"),
    include_str!("../../../fixtures/snapshots/no-v-html/basic.json"),
  );
  assert_diagnostics(
    "fixtures/rules/no-v-html/invalid/multiline.vue",
    include_str!("../../../fixtures/rules/no-v-html/invalid/multiline.vue"),
    include_str!("../../../fixtures/snapshots/no-v-html/multiline.json"),
  );
  assert_diagnostics(
    "fixtures/rules/no-v-html/invalid/multiple.vue",
    include_str!("../../../fixtures/rules/no-v-html/invalid/multiple.vue"),
    include_str!("../../../fixtures/snapshots/no-v-html/multiple.json"),
  );
  assert_diagnostics(
    "fixtures/rules/no-v-html/invalid/unicode.vue",
    include_str!("../../../fixtures/rules/no-v-html/invalid/unicode.vue"),
    include_str!("../../../fixtures/snapshots/no-v-html/unicode.json"),
  );
}

#[test]
fn no_v_html_safe_fixtures_produce_no_diagnostics() {
  let empty = include_str!("../../../fixtures/snapshots/no-v-html/empty.json");
  assert_diagnostics(
    "fixtures/rules/no-v-html/valid/comments-and-text.vue",
    include_str!("../../../fixtures/rules/no-v-html/valid/comments-and-text.vue"),
    empty,
  );
  assert_diagnostics(
    "fixtures/rules/no-v-html/valid/script-string.vue",
    include_str!("../../../fixtures/rules/no-v-html/valid/script-string.vue"),
    empty,
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
    normalize_path(Path::new(r"fixtures\rules\no-v-html\invalid\basic.vue")),
    "fixtures/rules/no-v-html/invalid/basic.vue",
    "Windows separators must normalize to the persisted form"
  );
}
