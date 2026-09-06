use std::path::Path;

use super::super::*;
use vue_vet_core::{
  Diagnostic, PRACTICE_CATEGORY, RuleEnvironment, RuleRegistry, Severity, VueVersion,
};
use vue_vet_practice::practice_rules;
use vue_vet_rules::builtin_rules;

fn analyze(path: &str, source: &str) -> Vec<Diagnostic> {
  analyze_env(
    path,
    source,
    RuleEnvironment {
      vue_version: Some(VueVersion { major: 3, minor: 5, patch: 0 }),
      packages: vec!["vue".into()],
    },
  )
}

#[expect(clippy::panic, reason = "unexpected parser errors must fail the policy tests")]
fn analyze_env(path: &str, source: &str, environment: RuleEnvironment) -> Vec<Diagnostic> {
  match analyze_sfc_with_facts(Path::new(path), source) {
    Ok(analysis) => {
      let mut rules = builtin_rules();
      rules.extend(practice_rules());
      RuleRegistry::new(rules).run_with_environment(
        Path::new(path),
        source,
        &analysis.facts.template,
        &analysis.facts.script,
        environment,
      )
    }
    Err(error) => panic!("analysis unexpectedly failed: {error}"),
  }
}

fn rule<'a>(diagnostics: &'a [Diagnostic], suffix: &str) -> Vec<&'a Diagnostic> {
  diagnostics.iter().filter(|diagnostic| diagnostic.rule_id.ends_with(suffix)).collect()
}

#[test]
fn duplicate_computed_writes_share_one_side_effect_diagnostic() {
  let source = include_str!(
    "../../../../fixtures/rules/no-side-effects-in-computed/invalid/duplicate-write.vue"
  );
  let diagnostics = analyze("duplicate-write.vue", source);
  let side = rule(&diagnostics, "/no-side-effects-in-computed");
  assert_eq!(side.len(), 1, "same-path writes must collapse: {side:?}");
  let Some(primary) = side.first() else {
    return;
  };
  assert!(primary.message.contains("`b.value`"), "{}", primary.message);
}

#[test]
fn same_display_name_distinct_declarations_stay_separate() {
  let source = include_str!(
    "../../../../fixtures/rules/no-side-effects-in-computed/invalid/same-display-distinct.vue"
  );
  let diagnostics = analyze("same-display-distinct.vue", source);
  let side = rule(&diagnostics, "/no-side-effects-in-computed");
  assert_eq!(side.len(), 2, "same display name across declarations must stay distinct: {side:?}");
}

#[test]
fn static_computed_ref_contract_is_practice() {
  let source = include_str!(
    "../../../../fixtures/rules/no-computed-without-dependency/invalid/ref-contract.vue"
  );
  let diagnostics = analyze("ref-contract.vue", source);
  let rows = rule(&diagnostics, "/no-computed-without-dependency");
  assert!(!rows.is_empty());
  assert!(
    rows.iter().all(|row| row.category == PRACTICE_CATEGORY && row.severity == Severity::Info)
  );
}

#[test]
fn route_snapshot_is_practice() {
  let source =
    include_str!("../../../../fixtures/rules/no-route-destructure/invalid/placeholder.vue");
  let diagnostics = analyze("route-snapshot.vue", source);
  let rows = rule(&diagnostics, "/no-route-destructure");
  assert!(!rows.is_empty());
  assert!(
    rows.iter().all(|row| row.category == PRACTICE_CATEGORY && row.severity == Severity::Info)
  );
  let Some(primary) = rows.first() else {
    return;
  };
  assert_eq!(primary.rule_id, "vue-vet/reactivity/no-route-destructure");
  assert_eq!(primary.span.offset, 31);
  assert_eq!(primary.span.length, 8);
  assert_eq!(primary.span.line, 2);
  assert_eq!(primary.span.column, 7);
}

#[test]
fn single_source_effect_is_practice_and_mentions_immediate() {
  let source = include_str!(
    "../../../../fixtures/rules/prefer-watch-over-effect-for-single-source/invalid/single-source.vue"
  );
  let diagnostics = analyze("single-source.vue", source);
  let rows = rule(&diagnostics, "/prefer-watch-over-effect-for-single-source");
  assert!(!rows.is_empty());
  for row in rows {
    assert_eq!(row.category, PRACTICE_CATEGORY);
    assert!(row.help.as_deref().is_some_and(|help| help.contains("immediate")));
  }
}

#[test]
fn opaque_watch_effect_flush_help_stays_conservative_until_explicit() {
  let source = include_str!(
    "../../../../fixtures/rules/prefer-watch-over-effect-for-single-source/invalid/opaque-flush.vue"
  );
  let diagnostics = analyze("opaque-flush.vue", source);
  let rows = rule(&diagnostics, "/prefer-watch-over-effect-for-single-source");
  assert_eq!(rows.len(), 4, "{rows:?}");
  let helps: Vec<&str> = rows.iter().filter_map(|row| row.help.as_deref()).collect();
  assert_eq!(helps.len(), 4);
  for help in helps.iter().copied().take(2).chain(helps.iter().copied().skip(3).take(1)) {
    assert!(!help.contains("flush: 'sync'") && !help.contains("flush `'pre'`"), "{help}");
    assert!(
      help.contains("current")
        || help.contains("original")
        || help.contains("preserve")
        || help.contains("keep"),
      "{help}"
    );
  }
  let explicit = helps.iter().copied().nth(2).unwrap_or("");
  assert!(explicit.contains("flush: 'sync'"), "{explicit}");
}

#[test]
fn nonempty_title_is_an_accessible_name() {
  let source = include_str!("../../../../fixtures/rules/button-has-content/valid/title.vue");
  let diagnostics = analyze("title-button.vue", source);
  assert!(rule(&diagnostics, "/button-has-content").is_empty());
  let link = include_str!("../../../../fixtures/rules/anchor-has-content/valid/title.vue");
  let diagnostics = analyze("title-link.vue", link);
  assert!(rule(&diagnostics, "/anchor-has-content").is_empty());
}

#[test]
fn empty_title_and_aria_hidden_content_still_report() {
  let empty = include_str!("../../../../fixtures/rules/button-has-content/invalid/empty-title.vue");
  let hidden =
    include_str!("../../../../fixtures/rules/button-has-content/invalid/aria-hidden.vue");
  assert!(!rule(&analyze("empty-title.vue", empty), "/button-has-content").is_empty());
  assert!(!rule(&analyze("aria-hidden.vue", hidden), "/button-has-content").is_empty());
}

#[test]
fn one_shot_raf_stays_quiet() {
  let source = include_str!("../../../../fixtures/rules/vueuse-use-raf-fn/valid/one-shot.vue");
  let diagnostics = analyze("one-shot.vue", source);
  assert!(rule(&diagnostics, "/vueuse-use-raf-fn").is_empty());
}

#[test]
fn named_one_shot_and_two_frame_raf_stay_quiet() {
  let named = include_str!("../../../../fixtures/rules/vueuse-use-raf-fn/valid/named-one-shot.vue");
  let two = include_str!("../../../../fixtures/rules/vueuse-use-raf-fn/valid/two-frame.vue");
  let deferred = include_str!("../../../../fixtures/rules/vueuse-use-raf-fn/valid/deferred.vue");
  assert!(rule(&analyze("named-one-shot.vue", named), "/vueuse-use-raf-fn").is_empty());
  assert!(rule(&analyze("two-frame.vue", two), "/vueuse-use-raf-fn").is_empty());
  assert!(rule(&analyze("deferred.vue", deferred), "/vueuse-use-raf-fn").is_empty());
}

#[test]
fn timeout_inside_watch_does_not_claim_lifecycle_containment() {
  let source =
    include_str!("../../../../fixtures/rules/vueuse-use-timeout-fn/valid/watch-callback.vue");
  let diagnostics = analyze("watch-timeout.vue", source);
  assert!(rule(&diagnostics, "/vueuse-use-timeout-fn").is_empty());
}

#[test]
fn maybe_ref_numeric_unref_stays_quiet() {
  let source =
    include_str!("../../../../fixtures/rules/prefer-to-value/valid/maybe-ref-number.vue");
  let diagnostics = analyze("typed-unref.vue", source);
  assert!(rule(&diagnostics, "/prefer-to-value").is_empty());
}

#[test]
fn known_ref_unref_stays_quiet() {
  let source = include_str!("../../../../fixtures/rules/prefer-to-value/valid/known-ref.vue");
  let diagnostics = analyze("known-ref.vue", source);
  assert!(rule(&diagnostics, "/prefer-to-value").is_empty());
}
