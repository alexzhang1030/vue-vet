use std::{
  path::PathBuf,
  process::{Command, Output},
};

use serde_json::Value;

fn fixture(name: &str) -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

fn workspace_root() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[expect(clippy::panic, reason = "an unexpected process error must fail the integration test")]
fn run(arguments: &[&str]) -> Output {
  match Command::new(env!("CARGO_BIN_EXE_vue-vet")).args(arguments).output() {
    Ok(output) => output,
    Err(error) => panic!("failed to run vue-vet: {error}"),
  }
}

#[expect(clippy::panic, reason = "an unexpected process error must fail the integration test")]
fn run_from_workspace(arguments: &[&str]) -> Output {
  match Command::new(env!("CARGO_BIN_EXE_vue-vet"))
    .current_dir(workspace_root())
    .args(arguments)
    .output()
  {
    Ok(output) => output,
    Err(error) => panic!("failed to run vue-vet from the workspace root: {error}"),
  }
}

#[test]
fn unsafe_fixture_has_stable_text_output_and_exit_code() {
  let path = fixture("rules/no-v-html/invalid/basic.vue");
  let output = run(&[path.to_string_lossy().as_ref(), "--deny-warnings"]);
  let stdout = String::from_utf8_lossy(&output.stdout);

  assert_eq!(output.status.code(), Some(1), "a denied warning must return exit code 1");
  assert!(
    stdout.contains("vue-vet/security/no-v-html"),
    "text output must contain the stable rule ID; stdout was: {stdout}"
  );
}

#[test]
fn unsafe_fixture_has_machine_readable_json_output() {
  let path = fixture("rules/no-v-html/invalid/basic.vue");
  let output = run(&[path.to_string_lossy().as_ref(), "--format", "json"]);
  let parsed: Result<Value, _> = serde_json::from_slice(&output.stdout);

  assert!(output.status.success(), "warnings are non-fatal without --deny-warnings");
  assert_eq!(
    parsed
      .as_ref()
      .ok()
      .and_then(|value| value.get("diagnostics"))
      .and_then(Value::as_array)
      .and_then(|diagnostics| diagnostics.first())
      .and_then(|diagnostic| diagnostic.get("rule_id"))
      .and_then(Value::as_str),
    Some("vue-vet/security/no-v-html"),
    "JSON output must contain the stable rule ID"
  );
}

#[test]
fn malformed_fixture_returns_an_operational_error_without_panicking() {
  let path = fixture("parser/malformed/unclosed-template.vue");
  let output = run(&[path.to_string_lossy().as_ref()]);
  let stderr = String::from_utf8_lossy(&output.stderr);

  assert_eq!(output.status.code(), Some(2), "a parser failure must return exit code 2");
  assert!(stderr.contains("failed to analyze"), "stderr must explain the parser failure: {stderr}");
  assert!(!stderr.contains("panicked"), "malformed input must never panic: {stderr}");
}

#[test]
fn reporter_text_snapshot_is_stable() {
  let output = run_from_workspace(&["fixtures/reporters/no-v-html.vue"]);
  let stdout = String::from_utf8_lossy(&output.stdout).replace('\\', "/");

  assert!(output.status.success(), "text reporter fixture must scan successfully");
  assert_eq!(
    stdout.trim_end(),
    include_str!("../../../fixtures/reporters/no-v-html.txt").trim_end(),
    "text reporter snapshot changed"
  );
}

#[test]
fn reporter_json_snapshot_is_stable() {
  let output = run_from_workspace(&["fixtures/reporters/no-v-html.vue", "--format", "json"]);
  let stdout = String::from_utf8_lossy(&output.stdout).replace('\\', "/");

  assert!(output.status.success(), "JSON reporter fixture must scan successfully");
  assert_eq!(
    stdout.trim_end(),
    include_str!("../../../fixtures/reporters/no-v-html.json").trim_end(),
    "JSON reporter snapshot changed"
  );
}

#[test]
fn severity_override_changes_exit_policy() {
  let project = fixture("projects/configured");
  let config = project.join("vue-vet.toml");
  let output =
    run(&[project.to_string_lossy().as_ref(), "--config", config.to_string_lossy().as_ref()]);
  let stdout = String::from_utf8_lossy(&output.stdout);

  assert_eq!(output.status.code(), Some(1), "an error override must fail without --deny-warnings");
  assert!(stdout.contains("  error  vue-vet/security/no-v-html"));
}

#[test]
fn scoped_suppression_hides_a_matching_finding() {
  let project = fixture("projects/suppressed");
  let output = run(&[project.to_string_lossy().as_ref(), "--format", "json"]);
  let parsed: Result<Value, _> = serde_json::from_slice(&output.stdout);

  assert!(output.status.success(), "a used suppression must keep the scan passing");
  assert_eq!(
    parsed
      .as_ref()
      .ok()
      .and_then(|value| value.get("diagnostics"))
      .and_then(Value::as_array)
      .map(Vec::len),
    Some(0),
    "the matching diagnostic must be suppressed"
  );
}

#[test]
fn effective_config_is_machine_readable() {
  let project = fixture("projects/configured");
  let output = run(&[project.to_string_lossy().as_ref(), "--print-config"]);
  let parsed: Result<Value, _> = serde_json::from_slice(&output.stdout);

  assert!(output.status.success(), "effective configuration must serialize");
  assert_eq!(
    parsed
      .as_ref()
      .ok()
      .and_then(|value| value.get("rules"))
      .and_then(|rules| rules.get("vue-vet/security/no-v-html"))
      .and_then(Value::as_str),
    Some("error")
  );
}

#[test]
fn project_graph_reports_nuxt_edges_cycles_and_cross_file_findings() {
  let project = fixture("projects/nuxt-graph");
  let output = run(&[project.to_string_lossy().as_ref(), "--print-graph"]);
  let parsed: Result<Value, _> = serde_json::from_slice(&output.stdout);

  assert!(output.status.success(), "debug graph output must not apply the diagnostic exit policy");
  let graph = parsed.as_ref().ok();
  let edges = graph.and_then(|value| value.get("edges")).and_then(Value::as_array);
  assert!(
    edges.is_some_and(|edges| {
      ["component_usage", "auto_component", "auto_composable"]
        .iter()
        .all(|kind| edges.iter().any(|edge| edge.get("kind").and_then(Value::as_str) == Some(kind)))
    }),
    "Nuxt and explicit project relationships must be serialized"
  );
  let diagnostics = graph.and_then(|value| value.get("diagnostics")).and_then(Value::as_array);
  assert!(
    diagnostics.is_some_and(|diagnostics| {
      ["vue-vet/project/unresolved-import", "vue-vet/project/unused-component"].iter().all(|rule| {
        diagnostics
          .iter()
          .any(|diagnostic| diagnostic.get("rule_id").and_then(Value::as_str) == Some(rule))
      })
    }),
    "both graph-backed rules must report through debug output"
  );
  assert!(
    edges.is_some_and(|edges| {
      edges
        .iter()
        .filter(|edge| {
          edge.get("specifier").and_then(Value::as_str) == Some("./a")
            || edge.get("specifier").and_then(Value::as_str) == Some("./b")
        })
        .count()
        == 2
    }),
    "monorepo import cycles must retain both directed edges"
  );
}

#[test]
fn cold_and_warm_cache_results_are_byte_equivalent() {
  let project = fixture("projects/nuxt-graph");
  let cache = workspace_root().join("target").join(format!("test-cache-{}", std::process::id()));
  let project_argument = project.to_string_lossy();
  let cache_argument = cache.to_string_lossy();
  let arguments = [
    project_argument.as_ref(),
    "--format",
    "json",
    "--cache-dir",
    cache_argument.as_ref(),
    "--cache-stats",
  ];
  let cold = run(&arguments);
  let warm = run(&arguments);
  assert_eq!(cold.stdout, warm.stdout, "warm and cold normalized output must be identical");
  assert!(String::from_utf8_lossy(&cold.stderr).contains("cache: miss"));
  assert!(String::from_utf8_lossy(&warm.stderr).contains("cache: hit"));
  let _ignored = std::fs::remove_dir_all(cache);
}

#[test]
fn written_baseline_hides_only_the_existing_fixture_findings() {
  let project = fixture("rules/no-v-html/invalid/basic.vue");
  let baseline =
    workspace_root().join("target").join(format!("test-baseline-{}.json", std::process::id()));
  let written = run(&[
    project.to_string_lossy().as_ref(),
    "--write-baseline",
    baseline.to_string_lossy().as_ref(),
    "--no-cache",
  ]);
  assert!(written.status.success(), "writing a warning-only baseline must succeed");
  let filtered = run(&[
    project.to_string_lossy().as_ref(),
    "--baseline",
    baseline.to_string_lossy().as_ref(),
    "--format",
    "json",
    "--no-cache",
  ]);
  let parsed: Result<Value, _> = serde_json::from_slice(&filtered.stdout);
  assert_eq!(
    parsed
      .as_ref()
      .ok()
      .and_then(|value| value.get("diagnostics"))
      .and_then(Value::as_array)
      .map(Vec::len),
    Some(0),
    "the exact existing finding must be hidden by its baseline fingerprint"
  );
  let _ignored = std::fs::remove_file(baseline);
}
