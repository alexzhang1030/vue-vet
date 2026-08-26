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

#[expect(clippy::panic, reason = "fixture read or serialization errors must fail golden tests")]
fn diagnostics_snapshot(logical_path: &str, source: &str) -> String {
  let diagnostics = match analyze_sfc(Path::new(logical_path), source) {
    Ok(diagnostics) => diagnostics,
    Err(error) => panic!("fixture unexpectedly failed to parse: {error}"),
  };
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
  let diagnostics = analyze_versioned(logical_path, source, minor);
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
  for (path, source, expected) in [
    (
      "fixtures/rules/vueuse-use-debounce-fn/invalid/hand-rolled.vue",
      include_str!("../../../fixtures/rules/vueuse-use-debounce-fn/invalid/hand-rolled.vue"),
      include_str!("../../../fixtures/snapshots/vueuse-use-debounce-fn/hand-rolled.json"),
    ),
    (
      "fixtures/rules/vueuse-use-event-listener/invalid/add-without-remove.vue",
      include_str!(
        "../../../fixtures/rules/vueuse-use-event-listener/invalid/add-without-remove.vue"
      ),
      include_str!("../../../fixtures/snapshots/vueuse-use-event-listener/add-without-remove.json"),
    ),
    (
      "fixtures/rules/vueuse-use-interval-fn/invalid/set-without-clear.vue",
      include_str!("../../../fixtures/rules/vueuse-use-interval-fn/invalid/set-without-clear.vue"),
      include_str!("../../../fixtures/snapshots/vueuse-use-interval-fn/set-without-clear.json"),
    ),
    (
      "fixtures/rules/vueuse-use-timeout-fn/invalid/set-without-clear.vue",
      include_str!("../../../fixtures/rules/vueuse-use-timeout-fn/invalid/set-without-clear.vue"),
      include_str!("../../../fixtures/snapshots/vueuse-use-timeout-fn/set-without-clear.json"),
    ),
    (
      "fixtures/rules/vueuse-use-raf-fn/invalid/raf-without-cancel.vue",
      include_str!("../../../fixtures/rules/vueuse-use-raf-fn/invalid/raf-without-cancel.vue"),
      include_str!("../../../fixtures/snapshots/vueuse-use-raf-fn/raf-without-cancel.json"),
    ),
    (
      "fixtures/rules/vueuse-use-intersection-observer/invalid/new-without-disconnect.vue",
      include_str!(
        "../../../fixtures/rules/vueuse-use-intersection-observer/invalid/new-without-disconnect.vue"
      ),
      include_str!(
        "../../../fixtures/snapshots/vueuse-use-intersection-observer/new-without-disconnect.json"
      ),
    ),
    (
      "fixtures/rules/vueuse-use-resize-observer/invalid/new-without-disconnect.vue",
      include_str!(
        "../../../fixtures/rules/vueuse-use-resize-observer/invalid/new-without-disconnect.vue"
      ),
      include_str!(
        "../../../fixtures/snapshots/vueuse-use-resize-observer/new-without-disconnect.json"
      ),
    ),
    (
      "fixtures/rules/vueuse-use-mutation-observer/invalid/new-without-disconnect.vue",
      include_str!(
        "../../../fixtures/rules/vueuse-use-mutation-observer/invalid/new-without-disconnect.vue"
      ),
      include_str!(
        "../../../fixtures/snapshots/vueuse-use-mutation-observer/new-without-disconnect.json"
      ),
    ),
  ] {
    assert_diagnostics(path, source, expected);
  }
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
    (
      "fixtures/rules/vueuse-use-raf-fn/valid/use-raf-fn.vue",
      include_str!("../../../fixtures/rules/vueuse-use-raf-fn/valid/use-raf-fn.vue"),
    ),
    (
      "fixtures/rules/vueuse-use-raf-fn/valid/with-cancel.vue",
      include_str!("../../../fixtures/rules/vueuse-use-raf-fn/valid/with-cancel.vue"),
    ),
    (
      "fixtures/rules/vueuse-use-raf-fn/valid/bare-raf.vue",
      include_str!("../../../fixtures/rules/vueuse-use-raf-fn/valid/bare-raf.vue"),
    ),
    (
      "fixtures/rules/vueuse-use-intersection-observer/valid/use-intersection-observer.vue",
      include_str!(
        "../../../fixtures/rules/vueuse-use-intersection-observer/valid/use-intersection-observer.vue"
      ),
    ),
    (
      "fixtures/rules/vueuse-use-intersection-observer/valid/with-disconnect.vue",
      include_str!(
        "../../../fixtures/rules/vueuse-use-intersection-observer/valid/with-disconnect.vue"
      ),
    ),
    (
      "fixtures/rules/vueuse-use-intersection-observer/valid/bare-observer.vue",
      include_str!(
        "../../../fixtures/rules/vueuse-use-intersection-observer/valid/bare-observer.vue"
      ),
    ),
    (
      "fixtures/rules/vueuse-use-resize-observer/valid/use-resize-observer.vue",
      include_str!(
        "../../../fixtures/rules/vueuse-use-resize-observer/valid/use-resize-observer.vue"
      ),
    ),
    (
      "fixtures/rules/vueuse-use-resize-observer/valid/with-disconnect.vue",
      include_str!("../../../fixtures/rules/vueuse-use-resize-observer/valid/with-disconnect.vue"),
    ),
    (
      "fixtures/rules/vueuse-use-resize-observer/valid/bare-observer.vue",
      include_str!("../../../fixtures/rules/vueuse-use-resize-observer/valid/bare-observer.vue"),
    ),
    (
      "fixtures/rules/vueuse-use-mutation-observer/valid/use-mutation-observer.vue",
      include_str!(
        "../../../fixtures/rules/vueuse-use-mutation-observer/valid/use-mutation-observer.vue"
      ),
    ),
    (
      "fixtures/rules/vueuse-use-mutation-observer/valid/with-disconnect.vue",
      include_str!(
        "../../../fixtures/rules/vueuse-use-mutation-observer/valid/with-disconnect.vue"
      ),
    ),
    (
      "fixtures/rules/vueuse-use-mutation-observer/valid/bare-observer.vue",
      include_str!("../../../fixtures/rules/vueuse-use-mutation-observer/valid/bare-observer.vue"),
    ),
  ] {
    assert_diagnostics(path, source, empty);
  }
}

#[test]
fn anchor_has_content_fixtures_match_exact_diagnostics() {
  assert_diagnostics(
    "fixtures/rules/anchor-has-content/invalid/icon-only.vue",
    include_str!("../../../fixtures/rules/anchor-has-content/invalid/icon-only.vue"),
    include_str!("../../../fixtures/snapshots/anchor-has-content/icon-only.json"),
  );
  assert_diagnostics(
    "fixtures/rules/anchor-has-content/invalid/empty.vue",
    include_str!("../../../fixtures/rules/anchor-has-content/invalid/empty.vue"),
    include_str!("../../../fixtures/snapshots/anchor-has-content/empty.json"),
  );
  assert_diagnostics(
    "fixtures/rules/anchor-has-content/invalid/aria-hidden-child.vue",
    include_str!("../../../fixtures/rules/anchor-has-content/invalid/aria-hidden-child.vue"),
    include_str!("../../../fixtures/snapshots/anchor-has-content/aria-hidden-child.json"),
  );
  assert_diagnostics(
    "fixtures/rules/anchor-has-content/invalid/router-link.vue",
    include_str!("../../../fixtures/rules/anchor-has-content/invalid/router-link.vue"),
    include_str!("../../../fixtures/snapshots/anchor-has-content/router-link.json"),
  );
  let empty = "[]";
  for (path, source) in [
    (
      "fixtures/rules/anchor-has-content/valid/text.vue",
      include_str!("../../../fixtures/rules/anchor-has-content/valid/text.vue"),
    ),
    (
      "fixtures/rules/anchor-has-content/valid/aria-label.vue",
      include_str!("../../../fixtures/rules/anchor-has-content/valid/aria-label.vue"),
    ),
    (
      "fixtures/rules/anchor-has-content/valid/img-alt.vue",
      include_str!("../../../fixtures/rules/anchor-has-content/valid/img-alt.vue"),
    ),
    (
      "fixtures/rules/anchor-has-content/valid/interpolation.vue",
      include_str!("../../../fixtures/rules/anchor-has-content/valid/interpolation.vue"),
    ),
  ] {
    assert_diagnostics(path, source, empty);
  }
}

#[test]
fn button_has_content_fixtures_match_exact_diagnostics() {
  assert_diagnostics(
    "fixtures/rules/button-has-content/invalid/icon-only.vue",
    include_str!("../../../fixtures/rules/button-has-content/invalid/icon-only.vue"),
    include_str!("../../../fixtures/snapshots/button-has-content/icon-only.json"),
  );
  assert_diagnostics(
    "fixtures/rules/button-has-content/valid/aria-label.vue",
    include_str!("../../../fixtures/rules/button-has-content/valid/aria-label.vue"),
    "[]",
  );
}

#[test]
fn no_aria_hidden_on_focusable_fixtures_match_exact_diagnostics() {
  assert_diagnostics(
    "fixtures/rules/no-aria-hidden-on-focusable/invalid/button.vue",
    include_str!("../../../fixtures/rules/no-aria-hidden-on-focusable/invalid/button.vue"),
    include_str!("../../../fixtures/snapshots/no-aria-hidden-on-focusable/button.json"),
  );
  assert_diagnostics(
    "fixtures/rules/no-aria-hidden-on-focusable/invalid/bound-true.vue",
    include_str!("../../../fixtures/rules/no-aria-hidden-on-focusable/invalid/bound-true.vue"),
    include_str!("../../../fixtures/snapshots/no-aria-hidden-on-focusable/bound-true.json"),
  );
  assert_diagnostics(
    "fixtures/rules/no-aria-hidden-on-focusable/invalid/tabindex.vue",
    include_str!("../../../fixtures/rules/no-aria-hidden-on-focusable/invalid/tabindex.vue"),
    include_str!("../../../fixtures/snapshots/no-aria-hidden-on-focusable/tabindex.json"),
  );
  assert_diagnostics(
    "fixtures/rules/no-aria-hidden-on-focusable/invalid/unquoted.vue",
    include_str!("../../../fixtures/rules/no-aria-hidden-on-focusable/invalid/unquoted.vue"),
    include_str!("../../../fixtures/snapshots/no-aria-hidden-on-focusable/unquoted.json"),
  );
  assert_diagnostics(
    "fixtures/rules/no-aria-hidden-on-focusable/invalid/v-bind.vue",
    include_str!("../../../fixtures/rules/no-aria-hidden-on-focusable/invalid/v-bind.vue"),
    include_str!("../../../fixtures/snapshots/no-aria-hidden-on-focusable/v-bind.json"),
  );
  let empty = "[]";
  for (path, source) in [
    (
      "fixtures/rules/no-aria-hidden-on-focusable/valid/decorative.vue",
      include_str!("../../../fixtures/rules/no-aria-hidden-on-focusable/valid/decorative.vue"),
    ),
    (
      "fixtures/rules/no-aria-hidden-on-focusable/valid/disabled.vue",
      include_str!("../../../fixtures/rules/no-aria-hidden-on-focusable/valid/disabled.vue"),
    ),
    (
      "fixtures/rules/no-aria-hidden-on-focusable/valid/false.vue",
      include_str!("../../../fixtures/rules/no-aria-hidden-on-focusable/valid/false.vue"),
    ),
    (
      "fixtures/rules/no-aria-hidden-on-focusable/valid/bound-dynamic.vue",
      include_str!("../../../fixtures/rules/no-aria-hidden-on-focusable/valid/bound-dynamic.vue"),
    ),
    (
      "fixtures/rules/no-aria-hidden-on-focusable/valid/hidden-input.vue",
      include_str!("../../../fixtures/rules/no-aria-hidden-on-focusable/valid/hidden-input.vue"),
    ),
  ] {
    assert_diagnostics(path, source, empty);
  }
}

#[test]
fn heading_and_label_a11y_fixtures_match_exact_diagnostics() {
  assert_diagnostics(
    "fixtures/rules/heading-has-content/invalid/empty.vue",
    include_str!("../../../fixtures/rules/heading-has-content/invalid/empty.vue"),
    include_str!("../../../fixtures/snapshots/heading-has-content/empty.json"),
  );
  assert_diagnostics(
    "fixtures/rules/heading-has-content/valid/text.vue",
    include_str!("../../../fixtures/rules/heading-has-content/valid/text.vue"),
    "[]",
  );
  assert_diagnostics(
    "fixtures/rules/label-has-for/invalid/missing.vue",
    include_str!("../../../fixtures/rules/label-has-for/invalid/missing.vue"),
    include_str!("../../../fixtures/snapshots/label-has-for/missing.json"),
  );
  assert_diagnostics(
    "fixtures/rules/label-has-for/valid/for-attr.vue",
    include_str!("../../../fixtures/rules/label-has-for/valid/for-attr.vue"),
    "[]",
  );
  assert_diagnostics(
    "fixtures/rules/label-has-for/valid/nested.vue",
    include_str!("../../../fixtures/rules/label-has-for/valid/nested.vue"),
    "[]",
  );
}

#[test]
fn form_control_has_label_fixtures_match_exact_diagnostics() {
  assert_diagnostics(
    "fixtures/rules/form-control-has-label/invalid/orphan-input.vue",
    include_str!("../../../fixtures/rules/form-control-has-label/invalid/orphan-input.vue"),
    include_str!("../../../fixtures/snapshots/form-control-has-label/orphan-input.json"),
  );
  assert_diagnostics(
    "fixtures/rules/form-control-has-label/invalid/orphan-textarea.vue",
    include_str!("../../../fixtures/rules/form-control-has-label/invalid/orphan-textarea.vue"),
    include_str!("../../../fixtures/snapshots/form-control-has-label/orphan-textarea.json"),
  );
  let empty = "[]";
  for (path, source) in [
    (
      "fixtures/rules/form-control-has-label/valid/for-id.vue",
      include_str!("../../../fixtures/rules/form-control-has-label/valid/for-id.vue"),
    ),
    (
      "fixtures/rules/form-control-has-label/valid/bound-for-id.vue",
      include_str!("../../../fixtures/rules/form-control-has-label/valid/bound-for-id.vue"),
    ),
    (
      "fixtures/rules/form-control-has-label/valid/nested.vue",
      include_str!("../../../fixtures/rules/form-control-has-label/valid/nested.vue"),
    ),
    (
      "fixtures/rules/form-control-has-label/valid/aria-label.vue",
      include_str!("../../../fixtures/rules/form-control-has-label/valid/aria-label.vue"),
    ),
    (
      "fixtures/rules/form-control-has-label/valid/hidden.vue",
      include_str!("../../../fixtures/rules/form-control-has-label/valid/hidden.vue"),
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
    FileId::from(r"fixtures\rules\no-v-html\invalid\basic.vue").as_str(),
    "fixtures/rules/no-v-html/invalid/basic.vue",
    "Windows separators must normalize to the persisted form"
  );
}

#[test]
fn no_deprecated_v_bind_sync_fixtures_match_exact_diagnostics() {
  for (path, source, expected) in [
    (
      "fixtures/rules/no-deprecated-v-bind-sync/invalid/basic.vue",
      include_str!("../../../fixtures/rules/no-deprecated-v-bind-sync/invalid/basic.vue"),
      include_str!("../../../fixtures/snapshots/no-deprecated-v-bind-sync/basic.json"),
    ),
    (
      "fixtures/rules/no-deprecated-v-bind-sync/invalid/v-bind.vue",
      include_str!("../../../fixtures/rules/no-deprecated-v-bind-sync/invalid/v-bind.vue"),
      include_str!("../../../fixtures/snapshots/no-deprecated-v-bind-sync/v-bind.json"),
    ),
    (
      "fixtures/rules/no-deprecated-v-bind-sync/invalid/unquoted.vue",
      include_str!("../../../fixtures/rules/no-deprecated-v-bind-sync/invalid/unquoted.vue"),
      include_str!("../../../fixtures/snapshots/no-deprecated-v-bind-sync/unquoted.json"),
    ),
    (
      "fixtures/rules/no-deprecated-v-bind-sync/invalid/object.vue",
      include_str!("../../../fixtures/rules/no-deprecated-v-bind-sync/invalid/object.vue"),
      include_str!("../../../fixtures/snapshots/no-deprecated-v-bind-sync/object.json"),
    ),
    (
      "fixtures/rules/no-deprecated-v-bind-sync/invalid/dynamic-arg.vue",
      include_str!("../../../fixtures/rules/no-deprecated-v-bind-sync/invalid/dynamic-arg.vue"),
      include_str!("../../../fixtures/snapshots/no-deprecated-v-bind-sync/dynamic-arg.json"),
    ),
    (
      "fixtures/rules/no-deprecated-v-bind-sync/invalid/extra-modifier.vue",
      include_str!("../../../fixtures/rules/no-deprecated-v-bind-sync/invalid/extra-modifier.vue"),
      include_str!("../../../fixtures/snapshots/no-deprecated-v-bind-sync/extra-modifier.json"),
    ),
    (
      "fixtures/rules/no-deprecated-v-bind-sync/invalid/unicode.vue",
      include_str!("../../../fixtures/rules/no-deprecated-v-bind-sync/invalid/unicode.vue"),
      include_str!("../../../fixtures/snapshots/no-deprecated-v-bind-sync/unicode.json"),
    ),
  ] {
    assert_diagnostics(path, source, expected);
  }
  let empty = "[]";
  for (path, source) in [
    (
      "fixtures/rules/no-deprecated-v-bind-sync/valid/safe.vue",
      include_str!("../../../fixtures/rules/no-deprecated-v-bind-sync/valid/safe.vue"),
    ),
    (
      "fixtures/rules/no-deprecated-v-bind-sync/valid/plain-bind.vue",
      include_str!("../../../fixtures/rules/no-deprecated-v-bind-sync/valid/plain-bind.vue"),
    ),
  ] {
    assert_diagnostics(path, source, empty);
  }
}

#[test]
fn helper_follow_and_style_v_bind_fixtures_match_exact_diagnostics() {
  for (path, source, expected) in [
    (
      "fixtures/rules/no-side-effects-in-computed/invalid/helper-write.vue",
      include_str!("../../../fixtures/rules/no-side-effects-in-computed/invalid/helper-write.vue"),
      include_str!("../../../fixtures/snapshots/no-side-effects-in-computed/helper-write.json"),
    ),
    (
      "fixtures/rules/no-side-effects-in-computed/invalid/ident-getter-write.vue",
      include_str!(
        "../../../fixtures/rules/no-side-effects-in-computed/invalid/ident-getter-write.vue"
      ),
      include_str!(
        "../../../fixtures/snapshots/no-side-effects-in-computed/ident-getter-write.json"
      ),
    ),
    (
      "fixtures/rules/prefer-computed/invalid/helper-assign.vue",
      include_str!("../../../fixtures/rules/prefer-computed/invalid/helper-assign.vue"),
      include_str!("../../../fixtures/snapshots/prefer-computed/helper-assign.json"),
    ),
    (
      "fixtures/rules/prefer-computed/invalid/ident-getter-assign.vue",
      include_str!("../../../fixtures/rules/prefer-computed/invalid/ident-getter-assign.vue"),
      include_str!("../../../fixtures/snapshots/prefer-computed/ident-getter-assign.json"),
    ),
    (
      "fixtures/rules/no-unused-computed-binding/invalid/unused.vue",
      include_str!("../../../fixtures/rules/no-unused-computed-binding/invalid/unused.vue"),
      include_str!("../../../fixtures/snapshots/no-unused-computed-binding/unused.json"),
    ),
    (
      "fixtures/rules/no-unused-computed-binding/valid/style-v-bind.vue",
      include_str!("../../../fixtures/rules/no-unused-computed-binding/valid/style-v-bind.vue"),
      include_str!("../../../fixtures/snapshots/no-unused-computed-binding/style-v-bind.json"),
    ),
    (
      "fixtures/rules/no-computed-without-dependency/invalid/helper-uncertain.vue",
      include_str!(
        "../../../fixtures/rules/no-computed-without-dependency/invalid/helper-uncertain.vue"
      ),
      include_str!(
        "../../../fixtures/snapshots/no-computed-without-dependency/helper-uncertain.json"
      ),
    ),
    (
      "fixtures/rules/no-conditional-dependency-in-computed/invalid/helper-ternary.vue",
      include_str!(
        "../../../fixtures/rules/no-conditional-dependency-in-computed/invalid/helper-ternary.vue"
      ),
      include_str!(
        "../../../fixtures/snapshots/no-conditional-dependency-in-computed/helper-ternary.json"
      ),
    ),
    (
      "fixtures/rules/no-conditional-dependency-in-computed/invalid/inline-ternary.vue",
      include_str!(
        "../../../fixtures/rules/no-conditional-dependency-in-computed/invalid/inline-ternary.vue"
      ),
      include_str!(
        "../../../fixtures/snapshots/no-conditional-dependency-in-computed/inline-ternary.json"
      ),
    ),
  ] {
    assert_diagnostics(path, source, expected);
  }
  let empty = "[]";
  for (path, source) in [
    (
      "fixtures/rules/no-conditional-dependency-in-computed/valid/both-arms-helper.vue",
      include_str!(
        "../../../fixtures/rules/no-conditional-dependency-in-computed/valid/both-arms-helper.vue"
      ),
    ),
    (
      "fixtures/rules/no-conditional-dependency-in-computed/valid/unconditional-helper.vue",
      include_str!(
        "../../../fixtures/rules/no-conditional-dependency-in-computed/valid/unconditional-helper.vue"
      ),
    ),
  ] {
    assert_diagnostics(path, source, empty);
  }
}
