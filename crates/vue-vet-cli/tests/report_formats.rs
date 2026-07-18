use std::{
  path::PathBuf,
  process::{Command, Output},
};

use serde_json::Value;

fn workspace_root() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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
fn sarif_cli_output_is_parseable_and_snapshot_stable() {
  let output = run_from_workspace(&[
    "fixtures/reporters/no-v-html.vue",
    "--format",
    "sarif",
    "--no-cache",
  ]);
  let stdout = String::from_utf8_lossy(&output.stdout).replace('\\', "/");
  let parsed: Result<Value, _> = serde_json::from_slice(&output.stdout);

  assert!(output.status.success(), "warning-only SARIF scans must pass");
  assert_eq!(
    parsed.as_ref().ok().and_then(|log| log.get("version")).and_then(Value::as_str),
    Some("2.1.0"),
    "SARIF must declare version 2.1.0"
  );
  assert_eq!(
    stdout.trim_end(),
    include_str!("../../../fixtures/reporters/no-v-html.sarif").trim_end(),
    "the CLI SARIF snapshot must match the reporter contract"
  );
}

#[test]
fn github_cli_output_is_annotation_safe_and_snapshot_stable() {
  let output = run_from_workspace(&[
    "fixtures/reporters/no-v-html.vue",
    "--format",
    "github",
    "--no-cache",
  ]);
  let stdout = String::from_utf8_lossy(&output.stdout).replace('\\', "/");

  assert!(output.status.success(), "warning-only GitHub annotation scans must pass");
  assert_eq!(
    stdout.trim_end(),
    include_str!("../../../fixtures/reporters/no-v-html.github").trim_end(),
    "the CLI annotation snapshot must match the reporter contract"
  );
  assert!(
    !stdout.contains("\nhelp:"),
    "annotation help must be escaped inside one workflow command"
  );
}
