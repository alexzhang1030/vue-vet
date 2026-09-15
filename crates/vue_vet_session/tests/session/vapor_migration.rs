#![expect(
  clippy::expect_used,
  clippy::panic,
  reason = "session vapor-migration fixtures fail closed"
)]

use vue_vet_core::{Convertible, LineIndex, MIGRATION_CATEGORY, OptIn, SourceSpan, Verdict};
use vue_vet_session::{RuleGroupId, SessionOptions};

use super::helpers::*;

const ASSESSMENT: &str = "vue-vet/migration/vapor-assessment";
const SFC: &str = "vue-vet/migration/vapor-sfc-compile-contract";
const MEMO: &str = "vue-vet/migration/vapor-memo-contract-dropped";
const INTEROP: &str = "vue-vet/migration/vapor-interop-required";
const ENVELOPE: &str = "vue-vet/migration/vapor-runtime-envelope";

fn matched_root() -> std::path::PathBuf {
  fixture("projects/vapor-migration/matched")
}

fn analyze_fixture(name: &str) -> vue_vet_session::AnalysisSnapshot {
  let session = open_session(fixture(name));
  session.analyze().unwrap_or_else(|error| panic!("analyze {name}: {error}"))
}

fn source_span(source: &str, needle: &str) -> SourceSpan {
  source_span_len(source, needle, needle.len())
}

fn source_span_len(source: &str, needle: &str, length: usize) -> SourceSpan {
  let offset = source.find(needle).unwrap_or_else(|| panic!("missing `{needle}`"));
  let (line, column) = LineIndex::new(source).byte_to_line_column(offset);
  SourceSpan { offset, length, line, column }
}

fn migration<'a>(
  snapshot: &'a vue_vet_session::AnalysisSnapshot,
  file: &str,
) -> Vec<&'a vue_vet_core::Diagnostic> {
  snapshot
    .summary
    .diagnostics
    .iter()
    .filter(|diagnostic| {
      diagnostic.file.as_str() == file && diagnostic.category == MIGRATION_CATEGORY
    })
    .collect()
}

#[test]
fn matched_complete_file_is_ready() {
  let snapshot = analyze_fixture("projects/vapor-migration/matched");
  let source = std::fs::read_to_string(matched_root().join("Complete.vue")).expect("Complete.vue");
  let found = snapshot.summary.diagnostics.iter().find(|diagnostic| {
    diagnostic.rule_id == ASSESSMENT && diagnostic.file.as_str() == "Complete.vue"
  });
  let Some(diagnostic) = found else {
    panic!("missing Complete.vue assessment; {:?}", snapshot.summary.diagnostics);
  };
  assert_eq!(diagnostic.span, source_span(&source, "<script setup vapor>"));
  assert!(
    diagnostic.message.contains("ready — direct conversion recommended"),
    "{}",
    diagnostic.message
  );
  let Some(assessment) = &diagnostic.assessment else {
    panic!("Complete.vue must carry assessment");
  };
  assert_eq!(assessment.kind, "vapor-migration");
  assert_eq!(assessment.opt_in, OptIn::ScriptVaporAttr);
  assert!(assessment.complete, "unknown={:?}", assessment.unknown);
  assert!(assessment.unknown.is_empty());
  assert_eq!(assessment.aggregate, Verdict::Ready);
  assert_eq!(assessment.convertible, Convertible::Yes);
  assert_eq!(
    assessment.checks.iter().map(|check| check.check.as_str()).collect::<Vec<_>>(),
    [
      "toolchain-tuple",
      "sfc-compile-contract",
      "memo-contract-dropped",
      "interop-required",
      "ssr-hydration",
      "runtime-envelope"
    ]
  );
  let envelope =
    assessment.checks.iter().find(|check| check.check == "runtime-envelope").expect("envelope");
  assert_eq!(envelope.verdict, Verdict::CompilerCandidate);
}

#[test]
fn matched_memo_spans_and_unicode_line_column() {
  let snapshot = analyze_fixture("projects/vapor-migration/matched");
  let memo = std::fs::read_to_string(matched_root().join("Memo.vue")).expect("Memo.vue");
  let found = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| diagnostic.rule_id == MEMO && diagnostic.file.as_str() == "Memo.vue");
  let Some(diagnostic) = found else {
    panic!("missing Memo.vue memo diagnostic");
  };
  assert_eq!(diagnostic.span, source_span(&memo, "v-memo"));
  let assessment = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| diagnostic.rule_id == ASSESSMENT && diagnostic.file.as_str() == "Memo.vue")
    .and_then(|diagnostic| diagnostic.assessment.as_ref())
    .unwrap_or_else(|| panic!("Memo assessment"));
  assert_eq!(assessment.aggregate, Verdict::Blocked);
  assert_eq!(assessment.convertible, Convertible::No);
  assert!(assessment.complete);

  let unicode = std::fs::read_to_string(matched_root().join("MemoUnicode.vue")).expect("unicode");
  let unicode_diag = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| diagnostic.rule_id == MEMO && diagnostic.file.as_str() == "MemoUnicode.vue")
    .unwrap_or_else(|| panic!("unicode memo"));
  assert_eq!(unicode_diag.span, source_span(&unicode, "v-memo"));
  assert!(unicode.contains("你好"));
  assert!(unicode_diag.span.column > 1);

  let crlf = std::fs::read(matched_root().join("MemoCrlf.vue")).expect("crlf bytes");
  assert!(crlf.windows(2).any(|window| window == b"\r\n"), "CRLF fixture must keep CR");
  let crlf_text = String::from_utf8(crlf).expect("crlf utf8");
  let crlf_diag = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| diagnostic.rule_id == MEMO && diagnostic.file.as_str() == "MemoCrlf.vue")
    .unwrap_or_else(|| panic!("crlf memo"));
  assert_eq!(crlf_diag.span, source_span(&crlf_text, "v-memo"));
}

#[test]
fn matched_sfc_compile_contract_reasons() {
  let snapshot = analyze_fixture("projects/vapor-migration/matched");
  let ordinary =
    std::fs::read_to_string(matched_root().join("OrdinaryScript.vue")).expect("ordinary");
  let ordinary_diag = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| {
      diagnostic.rule_id == SFC && diagnostic.file.as_str() == "OrdinaryScript.vue"
    })
    .unwrap_or_else(|| panic!("ordinary-script-only"));
  assert!(ordinary_diag.message.contains("ordinary-script-only"));
  assert_eq!(ordinary_diag.span, source_span(&ordinary, "<script>"));

  let exported =
    std::fs::read_to_string(matched_root().join("ScriptVaporExport.vue")).expect("export");
  let export_diag = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| {
      diagnostic.rule_id == SFC && diagnostic.file.as_str() == "ScriptVaporExport.vue"
    })
    .unwrap_or_else(|| panic!("script vapor export"));
  assert!(export_diag.message.contains("rejects runtime export"));
  assert_eq!(
    export_diag.span,
    source_span(&exported, "export default { name: 'ScriptVaporExport' }")
  );

  let dual = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| diagnostic.rule_id == SFC && diagnostic.file.as_str() == "DualScript.vue")
    .unwrap_or_else(|| panic!("dual-script"));
  assert!(dual.message.contains("inherited Options API"));

  let template_vapor = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| {
      diagnostic.rule_id == SFC && diagnostic.file.as_str() == "TemplateVaporOptions.vue"
    })
    .unwrap_or_else(|| panic!("template vapor options"));
  assert!(template_vapor.message.contains("template-vapor with Options script"));
}

#[test]
fn matched_interop_and_unresolved_child() {
  let snapshot = analyze_fixture("projects/vapor-migration/matched");
  let host = std::fs::read_to_string(matched_root().join("InteropHost.vue")).expect("host");
  let interop: Vec<_> = snapshot
    .summary
    .diagnostics
    .iter()
    .filter(|diagnostic| {
      diagnostic.rule_id == INTEROP && diagnostic.file.as_str() == "InteropHost.vue"
    })
    .collect();
  assert_eq!(interop.len(), 2, "{interop:?}");
  assert!(interop.iter().any(|diagnostic| diagnostic.span == source_span(&host, "<Suspense>")));
  assert!(interop.iter().any(|diagnostic| diagnostic.span == source_span(&host, "<Child />")));

  let unresolved = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| {
      diagnostic.rule_id == ASSESSMENT && diagnostic.file.as_str() == "UnresolvedChild.vue"
    })
    .and_then(|diagnostic| diagnostic.assessment.as_ref())
    .unwrap_or_else(|| panic!("unresolved assessment"));
  assert!(!unresolved.complete);
  assert_eq!(unresolved.convertible, Convertible::Unknown);
  assert!(
    unresolved.unknown.iter().any(|reason| reason == "child component UnknownChild unresolved"),
    "{:?}",
    unresolved.unknown
  );
  assert!(snapshot.summary.diagnostics.iter().all(|diagnostic| !(diagnostic.rule_id == INTEROP
    && diagnostic.file.as_str() == "UnresolvedChild.vue")));
}

#[test]
fn safe_patterns_do_not_emit_memo_or_builtin_interop() {
  let snapshot = analyze_fixture("projects/vapor-migration/matched");
  let safe = migration(&snapshot, "SafePatterns.vue");
  assert!(safe.iter().all(|diagnostic| diagnostic.rule_id != MEMO), "{safe:?}");
  assert!(safe.iter().all(|diagnostic| diagnostic.rule_id != INTEROP), "{safe:?}");
  assert!(safe.iter().all(|diagnostic| diagnostic.rule_id != ENVELOPE), "{safe:?}");
}

#[test]
fn envelope_reports_each_untested_construct_and_stays_convertible() {
  let snapshot = analyze_fixture("projects/vapor-migration/matched");
  let source = std::fs::read_to_string(matched_root().join("Envelope.vue")).expect("Envelope.vue");
  let findings: Vec<_> = snapshot
    .summary
    .diagnostics
    .iter()
    .filter(|diagnostic| {
      diagnostic.rule_id == ENVELOPE && diagnostic.file.as_str() == "Envelope.vue"
    })
    .collect();
  let expected = [
    ("v-model", "v-model", 7),
    ("@click.stop", "@click.stop", 1),
    ("slot", "<slot>", 6),
    ("v-show", "v-show", 6),
    ("unkeyed v-for", "v-for", 5),
    ("custom directive v-focus", "v-focus", 7),
    ("provide", "provide('key', n)", 17),
  ];
  assert_eq!(findings.len(), expected.len(), "{findings:?}");
  for (reason, needle, length) in expected {
    let found = findings.iter().find(|diagnostic| diagnostic.message == reason);
    let Some(diagnostic) = found else {
      panic!("missing envelope reason `{reason}`: {findings:?}");
    };
    assert_eq!(diagnostic.span, source_span_len(&source, needle, length), "{reason}");
  }
  let assessment = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| {
      diagnostic.rule_id == ASSESSMENT && diagnostic.file.as_str() == "Envelope.vue"
    })
    .and_then(|diagnostic| diagnostic.assessment.as_ref())
    .unwrap_or_else(|| panic!("Envelope assessment"));
  assert_eq!(assessment.aggregate, Verdict::NeedsVerification);
  assert_eq!(assessment.convertible, Convertible::Yes);
  assert_ne!(assessment.aggregate, Verdict::Ready);
  assert!(
    snapshot
      .summary
      .diagnostics
      .iter()
      .find(|diagnostic| {
        diagnostic.rule_id == ASSESSMENT && diagnostic.file.as_str() == "Envelope.vue"
      })
      .is_some_and(|diagnostic| diagnostic.message.contains("convertible, verification pending")),
    "envelope assessment message"
  );

  let unicode =
    std::fs::read_to_string(matched_root().join("EnvelopeUnicode.vue")).expect("unicode envelope");
  let unicode_diag = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| {
      diagnostic.rule_id == ENVELOPE && diagnostic.file.as_str() == "EnvelopeUnicode.vue"
    })
    .unwrap_or_else(|| panic!("unicode envelope"));
  assert_eq!(unicode_diag.span, source_span(&unicode, "v-model"));
  assert!(unicode.contains("你好"));
  assert!(unicode_diag.span.column > 1);
}

#[test]
fn script_vapor_type_only_comment_and_string_export_are_not_blocked() {
  let snapshot = analyze_fixture("projects/vapor-migration/matched");
  assert!(
    snapshot.summary.diagnostics.iter().all(|diagnostic| {
      !(diagnostic.rule_id == SFC && diagnostic.file.as_str() == "ScriptVaporSafeExport.vue")
    }),
    "type-only / comment / string `export` must not trip sfc-compile-contract: {:?}",
    snapshot
      .summary
      .diagnostics
      .iter()
      .filter(|diagnostic| diagnostic.file.as_str() == "ScriptVaporSafeExport.vue")
      .collect::<Vec<_>>()
  );
  let assessment = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| {
      diagnostic.rule_id == ASSESSMENT && diagnostic.file.as_str() == "ScriptVaporSafeExport.vue"
    })
    .and_then(|diagnostic| diagnostic.assessment.as_ref())
    .unwrap_or_else(|| panic!("ScriptVaporSafeExport assessment"));
  let sfc = assessment
    .checks
    .iter()
    .find(|check| check.check == "sfc-compile-contract")
    .unwrap_or_else(|| panic!("sfc-compile-contract"));
  assert_eq!(sfc.verdict, Verdict::CompilerCandidate);
}

#[test]
fn compiler_sfc_35_blocks_toolchain() {
  let snapshot = analyze_fixture("projects/vapor-migration/compiler-sfc-3-5");
  let assessment = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| diagnostic.rule_id == ASSESSMENT)
    .and_then(|diagnostic| diagnostic.assessment.as_ref())
    .unwrap_or_else(|| panic!("assessment"));
  assert_eq!(assessment.aggregate, Verdict::Blocked);
  assert_eq!(assessment.convertible, Convertible::No);
  let toolchain =
    assessment.checks.iter().find(|check| check.check == "toolchain-tuple").expect("toolchain");
  assert_eq!(toolchain.verdict, Verdict::Blocked);
}

#[test]
fn unresolved_toolchain_is_needs_verification() {
  let snapshot = analyze_fixture("projects/vapor-migration/unresolved");
  let assessment = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| diagnostic.rule_id == ASSESSMENT)
    .and_then(|diagnostic| diagnostic.assessment.as_ref())
    .unwrap_or_else(|| panic!("assessment"));
  assert_eq!(assessment.aggregate, Verdict::NeedsVerification);
  assert_eq!(assessment.convertible, Convertible::Unknown);
  assert!(!assessment.complete);
  assert!(assessment.unknown.iter().any(|reason| reason.contains("unresolved")));
  let sorted = {
    let mut clone = assessment.unknown.clone();
    clone.sort();
    clone
  };
  assert_eq!(sorted, assessment.unknown);
}

#[test]
fn ssr_fixture_needs_verification() {
  let snapshot = analyze_fixture("projects/vapor-migration/ssr");
  let assessment = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| diagnostic.rule_id == ASSESSMENT && diagnostic.file.as_str() == "App.vue")
    .and_then(|diagnostic| diagnostic.assessment.as_ref())
    .unwrap_or_else(|| panic!("ssr assessment"));
  let ssr = assessment.checks.iter().find(|check| check.check == "ssr-hydration").expect("ssr");
  assert_eq!(ssr.verdict, Verdict::NeedsVerification);
}

#[test]
fn default_config_emits_no_migration_and_score_matches_assessment_on() {
  let disabled = analyze_fixture("projects/vapor-migration/disabled");
  assert!(
    disabled.summary.diagnostics.iter().all(|diagnostic| diagnostic.category != MIGRATION_CATEGORY),
    "{:?}",
    disabled.summary.diagnostics
  );
  let score_off = disabled.summary.score;

  let root = std::env::temp_dir().join(format!("vv-vapor-score-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).expect("score root");
  std::fs::write(
    root.join("App.vue"),
    std::fs::read_to_string(fixture("projects/vapor-migration/disabled/App.vue")).expect("app"),
  )
  .expect("copy app");
  std::fs::write(root.join("vue-vet.toml"), "version = 1\nassessment = \"vapor\"\n").expect("toml");
  let on = open_session(&root).analyze().unwrap_or_else(|error| panic!("on: {error}"));
  assert!(
    on.summary.diagnostics.iter().any(|diagnostic| diagnostic.category == MIGRATION_CATEGORY)
  );
  assert_eq!(on.summary.score, score_off, "assessment must stay off-score");
  let _ignored = std::fs::remove_dir_all(&root);
}

#[test]
fn group_flag_enables_default_off_ids() {
  let root = fixture("projects/vapor-migration/disabled");
  let session = match ProjectSession::open(SessionOptions {
    root,
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
    selected_groups: vec![RuleGroupId::VaporMigration],
  }) {
    Ok(session) => session,
    Err(error) => panic!("open: {error}"),
  };
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  assert!(snapshot.summary.diagnostics.iter().any(|diagnostic| diagnostic.rule_id == MEMO));
}

#[test]
fn matched_json_is_byte_deterministic_across_two_runs() {
  let first = analyze_fixture("projects/vapor-migration/matched");
  let second = analyze_fixture("projects/vapor-migration/matched");
  let left = serde_json::to_vec(&first.summary.diagnostics).expect("first json");
  let right = serde_json::to_vec(&second.summary.diagnostics).expect("second json");
  assert_eq!(left, right, "matched diagnostics JSON must be byte-identical");
}
