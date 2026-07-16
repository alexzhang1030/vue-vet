use std::path::{Path, PathBuf};

use vue_vet_vize::{AnalyzeError, analyze_sfc};

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

#[test]
#[expect(clippy::panic, reason = "unexpected fixture analysis errors must fail golden tests")]
fn recommended_rule_pack_covers_all_rules_with_valid_spans() {
  let source = include_str!("../../../fixtures/rules/recommended/invalid.vue");
  let diagnostics = match analyze_sfc(Path::new("fixtures/rules/recommended/invalid.vue"), source) {
    Ok(diagnostics) => diagnostics,
    Err(error) => panic!("recommended rule fixture unexpectedly failed: {error}"),
  };
  let ids = diagnostics
    .iter()
    .map(|diagnostic| diagnostic.rule_id.as_str())
    .collect::<std::collections::BTreeSet<_>>();
  assert_eq!(ids.len(), 25, "every recommended rule needs a positive fixture");
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

#[test]
#[expect(clippy::panic, reason = "unexpected fixture analysis errors must fail golden tests")]
fn recommended_rule_pack_safe_patterns_are_quiet() {
  let source = include_str!("../../../fixtures/rules/recommended/valid.vue");
  let diagnostics = match analyze_sfc(Path::new("fixtures/rules/recommended/valid.vue"), source) {
    Ok(diagnostics) => diagnostics,
    Err(error) => panic!("recommended safe fixture unexpectedly failed: {error}"),
  };
  assert!(diagnostics.is_empty(), "safe patterns must not produce recommended findings");
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
