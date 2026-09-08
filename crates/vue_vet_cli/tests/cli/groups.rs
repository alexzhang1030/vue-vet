#![expect(
  clippy::expect_used,
  clippy::panic,
  reason = "CLI integration tests fail closed on malformed JSON"
)]

use super::helpers::*;
use vue_vet_project::PROJECT_RULE_IDS;
use vue_vet_session::{file_analysis_registry, rule_inventory};

#[test]
#[expect(clippy::panic, reason = "malformed inventory JSON must fail the integration test")]
fn list_rules_is_sorted_unique_and_includes_project_ids() {
  let output = run(&["--list-rules", "--format", "json", "--progress", "always"]);
  let stdout = String::from_utf8_lossy(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert!(output.status.success(), "list-rules must succeed: {stderr}{stdout}");
  assert!(
    serde_json::from_str::<Value>(stdout.trim()).is_ok(),
    "JSON stdout must stay parseable with progress: stdout={stdout} stderr={stderr}"
  );
  let parsed: Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|_| panic!("{stdout}"));
  assert_eq!(parsed.get("kind").and_then(Value::as_str), Some("rule_inventory"));
  assert_eq!(parsed.get("schema_version").and_then(Value::as_u64), Some(1));
  let rules = parsed.get("rules").and_then(Value::as_array).cloned().unwrap_or_default();
  let mut raw: Vec<_> =
    file_analysis_registry().metadata().into_iter().map(|meta| meta.id).collect();
  raw.extend(PROJECT_RULE_IDS);
  raw.sort_unstable();
  let mut seen = std::collections::BTreeSet::new();
  for id in &raw {
    assert!(seen.insert(*id), "duplicate registration `{id}` must appear in inventory");
  }
  let printed: Vec<_> =
    rules.iter().map(|rule| rule.get("id").and_then(Value::as_str).unwrap_or_default()).collect();
  assert_eq!(printed, raw, "inventory IDs must match the raw composed registry");
  assert_eq!(rules.len(), raw.len());
  assert!(printed.contains(&"vue-vet/project/unresolved-import"));
  assert!(printed.contains(&"vue-vet/project/unused-component"));
  let expected = rule_inventory(&[]);
  assert_eq!(
    parsed.pointer("/counts/total").and_then(Value::as_u64),
    Some(expected.counts.total as u64)
  );
  assert_eq!(
    parsed.pointer("/counts/total").and_then(Value::as_u64),
    Some(115),
    "composed CLI inventory must be 115 after source-contract, notification, and watch-api rules"
  );
}

#[test]
fn list_rules_lifetime_includes_four_and_tracking_excludes_them() {
  const LIFETIME_IDS: &[&str] = &[
    "vue-vet/reactivity/no-late-scope-dispose",
    "vue-vet/reactivity/no-late-watcher-cleanup",
    "vue-vet/reactivity/no-orphaned-scope-watcher",
    "vue-vet/reactivity/no-returned-watcher-cleanup",
  ];
  let lifetime = run(&["--list-rules", "--format", "json", "--group", "lifetime"]);
  let tracking = run(&["--list-rules", "--format", "json", "--group", "tracking"]);
  assert!(lifetime.status.success(), "{}", String::from_utf8_lossy(&lifetime.stderr));
  assert!(tracking.status.success(), "{}", String::from_utf8_lossy(&tracking.stderr));
  let lifetime_json: Value = serde_json::from_slice(&lifetime.stdout).expect("lifetime json");
  let tracking_json: Value = serde_json::from_slice(&tracking.stdout).expect("tracking json");
  let lifetime_ids: Vec<_> = lifetime_json
    .get("rules")
    .and_then(Value::as_array)
    .map(|rules| {
      rules.iter().filter_map(|row| row.get("id").and_then(Value::as_str)).collect::<Vec<_>>()
    })
    .unwrap_or_default();
  let tracking_ids: Vec<_> = tracking_json
    .get("rules")
    .and_then(Value::as_array)
    .map(|rules| {
      rules.iter().filter_map(|row| row.get("id").and_then(Value::as_str)).collect::<Vec<_>>()
    })
    .unwrap_or_default();
  for id in LIFETIME_IDS {
    assert!(lifetime_ids.contains(id), "lifetime inventory missing {id}: {lifetime_ids:?}");
    assert!(!tracking_ids.contains(id), "tracking inventory must exclude {id}");
  }
}

#[test]
fn list_rules_text_has_dense_columns() {
  let output = run(&["--list-rules", "--color", "never"]);
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(output.status.success(), "{stdout}");
  assert!(stdout.contains("ID"));
  assert!(stdout.contains("CATEGORY"));
  assert!(stdout.contains("GROUP"));
  assert!(stdout.contains("SEVERITY"));
  assert!(stdout.contains("vue-vet/project/unresolved-import"));
  assert!(stdout.contains("tracking") || stdout.contains("source-contracts"));
}

#[test]
fn list_rules_source_contracts_includes_five_ids() {
  let output = run(&["--list-rules", "--format", "json", "--group", "source-contracts"]);
  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
  let parsed: Value = serde_json::from_slice(&output.stdout).expect("source-contracts json");
  let ids: Vec<_> = parsed
    .get("rules")
    .and_then(Value::as_array)
    .map(|rules| {
      rules
        .iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect()
    })
    .unwrap_or_default();
  for id in [
    "vue-vet/reactivity/no-lost-shallow-nested-notification",
    "vue-vet/reactivity/no-primitive-reactive-target",
    "vue-vet/reactivity/no-toraw-write-of-tracked-state",
    "vue-vet/reactivity/no-torefs-on-non-proxy",
    "vue-vet/reactivity/no-trigger-ref-on-non-ref",
    "vue-vet/reactivity/no-watch-ignored-option",
    "vue-vet/reactivity/no-watch-replaced-object-source",
    "vue-vet/reactivity/no-watch-signature-mismatch",
    "vue-vet/reactivity/no-watch-unwrapped-source",
  ] {
    assert!(ids.iter().any(|row| row == id), "missing {id} in {ids:?}");
  }
}

#[test]
fn list_rules_group_union_is_idempotent() {
  let once = run(&["--list-rules", "--format", "json", "--group", "tracking"]);
  let twice =
    run(&["--list-rules", "--format", "json", "--group", "tracking", "--group", "tracking"]);
  assert_eq!(once.stdout, twice.stdout);
  let union =
    run(&["--list-rules", "--format", "json", "--group", "project", "--group", "tracking"]);
  let union_alt =
    run(&["--list-rules", "--format", "json", "--group", "tracking", "--group", "project"]);
  assert_eq!(union.stdout, union_alt.stdout);
  let parsed: Value = serde_json::from_slice(&union.stdout).expect("union json");
  let rules = parsed.get("rules").and_then(Value::as_array).cloned().unwrap_or_default();
  assert!(rules.iter().any(|row| row.get("id").and_then(Value::as_str)
    == Some("vue-vet/project/unresolved-import")));
  assert!(rules.iter().any(|row| {
    row.get("id").and_then(Value::as_str)
      == Some("vue-vet/reactivity/no-reactive-read-during-pause-tracking")
  }));
}

#[test]
fn unknown_group_fails_before_scan() {
  let path = fixture("rules/no-v-html/invalid/basic.vue");
  let output = run(&[path.to_string_lossy().as_ref(), "--group", "nope", "--format", "json"]);
  let stdout = String::from_utf8_lossy(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert_eq!(output.status.code(), Some(2));
  assert!(
    stderr.contains("unknown rule group") || stdout.contains("unknown rule group"),
    "stderr={stderr} stdout={stdout}"
  );
  let parsed: Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|_| panic!("{stdout}"));
  assert_eq!(parsed.get("ok").and_then(Value::as_bool), Some(false));
  assert_eq!(
    parsed.get("diagnostics").and_then(Value::as_array).map(Vec::len),
    Some(0),
    "unknown group must not scan: {stdout}"
  );
}

#[test]
fn list_rules_conflicts_with_protocol_and_fix() {
  let cases: &[&[&str]] = &[
    &["--list-rules", "--lsp"],
    &["--list-rules", "--mcp"],
    &["--list-rules", "--print-config"],
    &["--list-rules", "--explain", "vue-vet/security/no-v-html"],
    &["--list-rules", "--reactivity-tui"],
  ];
  for args in cases {
    let output = run(args);
    assert_eq!(
      output.status.code(),
      Some(2),
      "list-rules must conflict with {args:?}: {}",
      String::from_utf8_lossy(&output.stderr)
    );
  }
  let output = run(&["--list-rules", "--fix-dry-run"]);
  assert_eq!(output.status.code(), Some(2), "{}", String::from_utf8_lossy(&output.stderr));
  let output = run(&["--group", "tracking", "--lsp"]);
  assert_eq!(output.status.code(), Some(2), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn group_tracking_does_not_emit_unmapped_a11y() {
  let project = TempProject::new(
    "group-tracking-a11y",
    include_str!(
      "../../../../fixtures/rules/no-reactive-read-during-pause-tracking/invalid/paused.vue"
    ),
  );
  project.write_source(
    "BareImg.vue",
    include_str!("../../../../fixtures/projects/a11y-forms/BareImg.vue"),
  );
  let root = project.root().to_string_lossy();
  let default = run(&[root.as_ref(), "--format", "json", "--no-cache"]);
  let default_json: Value = serde_json::from_slice(&default.stdout)
    .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&default.stdout)));
  let default_ids = diagnostic_ids(&default_json);
  assert!(
    default_ids.iter().any(|id| id.contains("no-reactive-read-during-pause-tracking")),
    "{default_ids:?}"
  );
  assert!(default_ids.iter().any(|id| id.contains("img-has-alt")), "{default_ids:?}");

  let tracking = run(&[root.as_ref(), "--format", "json", "--no-cache", "--group", "tracking"]);
  let tracking_json: Value = serde_json::from_slice(&tracking.stdout)
    .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&tracking.stdout)));
  let tracking_ids = diagnostic_ids(&tracking_json);
  assert!(
    tracking_ids.iter().any(|id| id.contains("no-reactive-read-during-pause-tracking")),
    "{tracking_ids:?}"
  );
  assert!(tracking_ids.iter().all(|id| !id.contains("img-has-alt")), "{tracking_ids:?}");
}

#[test]
fn explicit_off_is_not_reenabled_by_selected_group() {
  let path = fixture("rules/prefer-computed/invalid/watch-effect-sync.vue");
  let path_argument = path.to_string_lossy();
  let control = parse_scan(&run(&[
    path_argument.as_ref(),
    "--format",
    "json",
    "--no-cache",
    "--group",
    "derivation",
  ]));
  assert!(
    diagnostic_ids(&control).iter().any(|id| id.contains("prefer-computed")),
    "control must emit prefer-computed: {:?}",
    diagnostic_ids(&control)
  );

  let project = TempProject::new(
    "group-explicit-off",
    include_str!("../../../../fixtures/rules/prefer-computed/invalid/watch-effect-sync.vue"),
  );
  project.write_source(
    "vue-vet.toml",
    "version = 1\npreset = \"recommended\"\n[rules]\n\"vue-vet/reactivity/prefer-computed\" = \"off\"\n",
  );
  let root = project.root().to_string_lossy();
  let config = project.root().join("vue-vet.toml").to_string_lossy().into_owned();
  let filtered = parse_scan(&run(&[
    root.as_ref(),
    "--format",
    "json",
    "--no-cache",
    "--group",
    "derivation",
    "--config",
    &config,
  ]));
  assert!(
    diagnostic_ids(&filtered).iter().all(|id| !id.contains("prefer-computed")),
    "explicit off must remain off: {:?}",
    diagnostic_ids(&filtered)
  );
}

#[test]
fn preset_none_is_not_reenabled_by_selected_group() {
  let path = fixture("rules/prefer-computed/invalid/watch-effect-sync.vue");
  let path_argument = path.to_string_lossy();
  let control = parse_scan(&run(&[
    path_argument.as_ref(),
    "--format",
    "json",
    "--no-cache",
    "--group",
    "derivation",
  ]));
  assert!(
    diagnostic_ids(&control).iter().any(|id| id.contains("prefer-computed")),
    "control must emit prefer-computed: {:?}",
    diagnostic_ids(&control)
  );

  let project = TempProject::new(
    "group-preset-none",
    include_str!("../../../../fixtures/rules/prefer-computed/invalid/watch-effect-sync.vue"),
  );
  project.write_source("vue-vet.toml", "version = 1\npreset = \"none\"\n");
  let root = project.root().to_string_lossy();
  let config = project.root().join("vue-vet.toml").to_string_lossy().into_owned();
  let filtered = parse_scan(&run(&[
    root.as_ref(),
    "--format",
    "json",
    "--no-cache",
    "--group",
    "derivation",
    "--config",
    &config,
  ]));
  assert!(
    diagnostic_ids(&filtered).is_empty(),
    "preset none without overrides must stay empty: {:?}",
    diagnostic_ids(&filtered)
  );
}

#[test]
fn practice_off_is_not_reenabled_by_selected_group() {
  let path = fixture("rules/prefer-watch-over-effect-for-single-source/invalid/single-source.vue");
  let path_argument = path.to_string_lossy();
  let control = parse_scan(&run(&[
    path_argument.as_ref(),
    "--format",
    "json",
    "--no-cache",
    "--group",
    "derivation",
  ]));
  assert!(
    diagnostic_ids(&control)
      .iter()
      .any(|id| id.contains("prefer-watch-over-effect-for-single-source")),
    "control must emit prefer-watch: {:?}",
    diagnostic_ids(&control)
  );

  let project = TempProject::new(
    "group-practice-off",
    include_str!(
      "../../../../fixtures/rules/prefer-watch-over-effect-for-single-source/invalid/single-source.vue"
    ),
  );
  project
    .write_source("vue-vet.toml", "version = 1\npreset = \"recommended\"\npractice = \"off\"\n");
  let root = project.root().to_string_lossy();
  let config = project.root().join("vue-vet.toml").to_string_lossy().into_owned();
  let filtered = parse_scan(&run(&[
    root.as_ref(),
    "--format",
    "json",
    "--no-cache",
    "--group",
    "derivation",
    "--config",
    &config,
  ]));
  assert!(
    diagnostic_ids(&filtered)
      .iter()
      .all(|id| !id.contains("prefer-watch-over-effect-for-single-source")),
    "practice off must drop the practice finding: {:?}",
    diagnostic_ids(&filtered)
  );
}

#[test]
fn selected_group_preserves_severity_override_and_exit() {
  let project = TempProject::new(
    "group-severity-override",
    include_str!("../../../../fixtures/rules/prefer-computed/invalid/watch-effect-sync.vue"),
  );
  project.write_source(
    "vue-vet.toml",
    "version = 1\npreset = \"recommended\"\n[rules]\n\"vue-vet/reactivity/prefer-computed\" = \"error\"\n",
  );
  let root = project.root().to_string_lossy();
  let config = project.root().join("vue-vet.toml").to_string_lossy().into_owned();
  let output = run(&[
    root.as_ref(),
    "--format",
    "json",
    "--no-cache",
    "--group",
    "derivation",
    "--config",
    &config,
  ]);
  assert_eq!(output.status.code(), Some(1), "{}", String::from_utf8_lossy(&output.stderr));
  let parsed = parse_scan(&output);
  let diagnostics =
    parsed.get("diagnostics").and_then(Value::as_array).cloned().unwrap_or_default();
  let prefer = diagnostics.iter().find(|diagnostic| {
    diagnostic.get("rule_id").and_then(Value::as_str) == Some("vue-vet/reactivity/prefer-computed")
  });
  assert_eq!(
    prefer.and_then(|diagnostic| diagnostic.get("severity").and_then(Value::as_str)),
    Some("error"),
    "{diagnostics:?}"
  );
}

#[test]
fn practice_only_selected_group_is_excluded_from_score_and_default_exit() {
  let path = fixture("rules/prefer-watch-over-effect-for-single-source/invalid/single-source.vue");
  let path_argument = path.to_string_lossy();
  let project = TempProject::new(
    "group-practice-score",
    include_str!(
      "../../../../fixtures/rules/prefer-watch-over-effect-for-single-source/invalid/single-source.vue"
    ),
  );
  project.write_source(
    "vue-vet.toml",
    "version = 1\npreset = \"recommended\"\n[rules]\n\"vue-vet/reactivity/prefer-computed\" = \"off\"\n\"vue-vet/reactivity/no-unused-computed-binding\" = \"off\"\n\"vue-vet/reactivity/no-side-effects-in-computed\" = \"off\"\n\"vue-vet/reactivity/no-computed-self-trigger\" = \"off\"\n\"vue-vet/reactivity/no-multiple-effects-same-target\" = \"off\"\n\"vue-vet/reactivity/no-deep-watch-on-reactive-root\" = \"off\"\n",
  );
  let root = project.root().to_string_lossy();
  let config = project.root().join("vue-vet.toml").to_string_lossy().into_owned();
  let control = parse_scan(&run(&[path_argument.as_ref(), "--format", "json", "--no-cache"]));
  assert!(
    diagnostic_ids(&control)
      .iter()
      .any(|id| id.contains("prefer-watch-over-effect-for-single-source")),
    "committed fixture must emit prefer-watch: {:?}",
    diagnostic_ids(&control)
  );
  let output = run(&[
    root.as_ref(),
    "--format",
    "json",
    "--no-cache",
    "--group",
    "derivation",
    "--config",
    &config,
  ]);
  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
  let parsed = parse_scan(&output);
  assert!(
    diagnostic_ids(&parsed)
      .iter()
      .any(|id| id.contains("prefer-watch-over-effect-for-single-source")),
    "{:?}",
    diagnostic_ids(&parsed)
  );
  assert!(
    diagnostic_ids(&parsed)
      .iter()
      .all(|id| id.contains("prefer-watch-over-effect-for-single-source")),
    "only the practice finding should remain: {:?}",
    diagnostic_ids(&parsed)
  );
  assert_eq!(parsed.pointer("/summary/score").and_then(Value::as_u64), Some(100));
}

#[test]
fn group_project_keeps_unresolved_import() {
  let path = fixture("projects/nuxt-graph/pages/broken.vue");
  let path_argument = path.to_string_lossy();
  let project =
    run(&[path_argument.as_ref(), "--format", "json", "--group", "project", "--no-cache"]);
  let tracking =
    run(&[path_argument.as_ref(), "--format", "json", "--group", "tracking", "--no-cache"]);
  let project_json: Value = serde_json::from_slice(&project.stdout).expect("project json");
  let tracking_json: Value = serde_json::from_slice(&tracking.stdout).expect("tracking json");
  let project_ids = diagnostic_ids(&project_json);
  let tracking_ids = diagnostic_ids(&tracking_json);
  assert!(project_ids.iter().any(|id| id.contains("unresolved-import")), "{project_ids:?}");
  assert!(tracking_ids.iter().all(|id| !id.contains("unresolved-import")), "{tracking_ids:?}");
}

#[test]
fn group_cache_cold_warm_and_separated_from_full_scan() {
  let project = TempProject::new(
    "group-cache",
    include_str!(
      "../../../../fixtures/rules/no-reactive-read-during-pause-tracking/invalid/paused.vue"
    ),
  );
  project.write_source(
    "BareImg.vue",
    include_str!("../../../../fixtures/projects/a11y-forms/BareImg.vue"),
  );
  let cache = project.root().join("cache");
  let root = project.root().to_string_lossy();
  let cache_dir = cache.to_string_lossy();
  let tracking_args = [
    root.as_ref(),
    "--format",
    "json",
    "--group",
    "tracking",
    "--cache-dir",
    cache_dir.as_ref(),
    "--cache-stats",
  ];
  let cold = run(&tracking_args);
  assert!(
    cold.status.success(),
    "tracking-only warnings must not fail: {}",
    String::from_utf8_lossy(&cold.stderr)
  );
  let cold_json = parse_scan(&cold);
  assert_eq!(cold_json.pointer("/project/complete").and_then(Value::as_bool), Some(true));
  assert!(
    diagnostic_ids(&cold_json)
      .iter()
      .any(|id| id.contains("no-reactive-read-during-pause-tracking")),
    "{:?}",
    diagnostic_ids(&cold_json)
  );
  assert!(String::from_utf8_lossy(&cold.stderr).contains("cache: miss"));

  let warm = run(&tracking_args);
  assert!(warm.status.success(), "{}", String::from_utf8_lossy(&warm.stderr));
  assert_eq!(cold.stdout, warm.stdout);
  let warm_json = parse_scan(&warm);
  assert_eq!(warm_json.pointer("/project/complete").and_then(Value::as_bool), Some(true));
  assert!(
    diagnostic_ids(&warm_json)
      .iter()
      .any(|id| id.contains("no-reactive-read-during-pause-tracking")),
    "{:?}",
    diagnostic_ids(&warm_json)
  );
  assert!(String::from_utf8_lossy(&warm.stderr).contains("cache: hit"));

  let full =
    run(&[root.as_ref(), "--format", "json", "--cache-dir", cache_dir.as_ref(), "--cache-stats"]);
  assert_eq!(full.status.code(), Some(1), "full scan includes project errors");
  let full_json = parse_scan(&full);
  assert_eq!(full_json.pointer("/project/complete").and_then(Value::as_bool), Some(true));
  let full_ids = diagnostic_ids(&full_json);
  assert!(full_ids.iter().any(|id| id.contains("img-has-alt")), "{full_ids:?}");
  assert!(full_ids.iter().any(|id| id.contains("unresolved-import")), "{full_ids:?}");
  assert_ne!(diagnostic_ids(&cold_json), full_ids);
  assert!(String::from_utf8_lossy(&full.stderr).contains("cache: miss"));

  let config_a =
    run(&[root.as_ref(), "--print-config", "--group", "tracking", "--group", "project"]);
  let config_b = run(&[
    root.as_ref(),
    "--print-config",
    "--group",
    "project",
    "--group",
    "tracking",
    "--group",
    "tracking",
  ]);
  assert!(config_a.status.success() && config_b.status.success());
  assert_eq!(
    config_a.stdout, config_b.stdout,
    "identical group unions must share effective config"
  );

  let union_a = run(&[
    root.as_ref(),
    "--format",
    "json",
    "--group",
    "tracking",
    "--group",
    "project",
    "--cache-dir",
    cache_dir.as_ref(),
    "--cache-stats",
  ]);
  assert!(String::from_utf8_lossy(&union_a.stderr).contains("cache: miss"));
  let union_b = run(&[
    root.as_ref(),
    "--format",
    "json",
    "--group",
    "project",
    "--group",
    "tracking",
    "--group",
    "tracking",
    "--cache-dir",
    cache_dir.as_ref(),
    "--cache-stats",
  ]);
  assert_eq!(union_a.stdout, union_b.stdout);
  assert!(String::from_utf8_lossy(&union_b.stderr).contains("cache: hit"));
}

#[test]
#[expect(clippy::panic, reason = "malformed explain JSON must fail the integration test")]
fn explain_finding_retains_tracking_evidence() {
  let project = TempProject::new(
    "group-explain-tracking",
    r#"<script setup lang="ts">
import { enableTracking, pauseTracking, ref, watchEffect } from 'vue'
const value = ref(0)
watchEffect(() => {
  pauseTracking()
  console.log(value.value)
  enableTracking()
})
</script>
<template><p></p></template>
"#,
  );
  let root = project.root().to_string_lossy();
  let scan = run(&[root.as_ref(), "--format", "json", "--group", "tracking", "--no-cache"]);
  let scan_json: Value = serde_json::from_slice(&scan.stdout)
    .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&scan.stdout)));
  let finding_id = scan_json
    .pointer("/diagnostics/0/id")
    .and_then(Value::as_str)
    .unwrap_or_else(|| panic!("missing finding: {scan_json}"))
    .to_owned();
  let explained = run(&[
    root.as_ref(),
    "--explain",
    &finding_id,
    "--format",
    "json",
    "--group",
    "tracking",
    "--no-cache",
  ]);
  let stdout = String::from_utf8_lossy(&explained.stdout);
  assert!(explained.status.success(), "{stdout}");
  let parsed: Value = serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}"));
  assert!(parsed.get("tracking").is_some(), "finding explain must retain tracking: {stdout}");
}

fn parse_scan(output: &std::process::Output) -> Value {
  serde_json::from_slice(&output.stdout)
    .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&output.stdout)))
}

fn diagnostic_ids(report: &Value) -> Vec<String> {
  report
    .get("diagnostics")
    .and_then(Value::as_array)
    .map(|diagnostics| {
      diagnostics
        .iter()
        .filter_map(|diagnostic| {
          diagnostic.get("rule_id").and_then(Value::as_str).map(str::to_owned)
        })
        .collect()
    })
    .unwrap_or_default()
}
