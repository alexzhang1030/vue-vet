use std::{
  fs,
  path::{Path, PathBuf},
  process::{Command, Output},
  sync::atomic::{AtomicUsize, Ordering},
};

use serde_json::Value;

static NEXT_TEMP_PROJECT: AtomicUsize = AtomicUsize::new(0);

struct TempProject {
  root: PathBuf,
}

impl TempProject {
  #[expect(clippy::panic, reason = "test setup failures must fail the integration test")]
  fn new(name: &str, source: &str) -> Self {
    let sequence = NEXT_TEMP_PROJECT.fetch_add(1, Ordering::Relaxed);
    let root = workspace_root()
      .join("target")
      .join(format!("test-{name}-{}-{sequence}", std::process::id()));
    let _ignored = fs::remove_dir_all(&root);
    if let Err(error) = fs::create_dir_all(&root) {
      panic!("failed to create temporary project {}: {error}", root.display());
    }
    let source_path = root.join("App.vue");
    if let Err(error) = fs::write(&source_path, source) {
      panic!("failed to write temporary source {}: {error}", source_path.display());
    }
    Self { root }
  }

  fn source_path(&self) -> PathBuf {
    self.root.join("App.vue")
  }

  fn root(&self) -> &Path {
    &self.root
  }

  #[expect(clippy::panic, reason = "test setup failures must fail the integration test")]
  fn write_source(&self, name: &str, source: &str) -> PathBuf {
    let path = self.root.join(name);
    if let Err(error) = fs::write(&path, source) {
      panic!("failed to write temporary source {}: {error}", path.display());
    }
    path
  }
}

impl Drop for TempProject {
  fn drop(&mut self) {
    let _ignored = fs::remove_dir_all(&self.root);
  }
}

fn fixture(name: &str) -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

fn workspace_root() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn collect_reference_sources(directory: &Path, sources: &mut Vec<PathBuf>) {
  let Ok(entries) = fs::read_dir(directory) else {
    return;
  };
  for entry in entries.flatten() {
    let path = entry.path();
    if path.is_dir() {
      collect_reference_sources(&path, sources);
    } else if matches!(
      path.extension().and_then(|extension| extension.to_str()),
      Some("vue" | "js" | "jsx" | "ts" | "tsx")
    ) {
      sources.push(path);
    }
  }
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
    parsed.as_ref().ok().and_then(|value| value.get("schema_version")).and_then(Value::as_u64),
    Some(1),
    "JSON output must declare its contract version"
  );
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
  assert_eq!(
    parsed
      .as_ref()
      .ok()
      .and_then(|value| value.get("project"))
      .and_then(|project| project.get("complete"))
      .and_then(Value::as_bool),
    Some(true),
    "a successful scan must make completeness explicit"
  );
  assert!(
    parsed
      .as_ref()
      .ok()
      .and_then(|value| value.get("diagnostics"))
      .and_then(Value::as_array)
      .and_then(|diagnostics| diagnostics.first())
      .and_then(|diagnostic| diagnostic.get("id"))
      .and_then(Value::as_str)
      .is_some_and(|id| id.starts_with("basic.vue::2:9::vue-vet/security/no-v-html::")),
    "JSON output must expose a deterministic normalized diagnostic identity"
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
fn malformed_fixture_returns_a_structured_json_error() {
  let path = fixture("parser/malformed/unclosed-template.vue");
  let output = run(&[path.to_string_lossy().as_ref(), "--format", "json"]);
  let parsed: Result<Value, _> = serde_json::from_slice(&output.stdout);

  assert_eq!(output.status.code(), Some(2), "a parser failure must return exit code 2");
  assert_eq!(
    parsed.as_ref().ok().and_then(|report| report.get("ok")).and_then(Value::as_bool),
    Some(false),
    "JSON mode must keep operational failures machine readable"
  );
  assert_eq!(
    parsed
      .as_ref()
      .ok()
      .and_then(|report| report.get("project"))
      .and_then(|project| project.get("complete"))
      .and_then(Value::as_bool),
    Some(false),
    "failed scans must never claim complete coverage"
  );
  assert!(
    parsed
      .as_ref()
      .ok()
      .and_then(|report| report.get("error"))
      .and_then(|error| error.get("message"))
      .and_then(Value::as_str)
      .is_some_and(|message| message.contains("failed to analyze")),
    "the structured error must retain the actionable parser failure"
  );
}

#[test]
fn safe_fix_preserves_unicode_and_crlf_then_reports_the_rescan() {
  let source = "<template>\r\n  <p>你好</p>\r\n  <input autofocus>\r\n</template>\r\n";
  let expected = "<template>\r\n  <p>你好</p>\r\n  <input>\r\n</template>\r\n";
  let project = TempProject::new("safe-fix-unicode-crlf", source);
  let source_path = project.source_path();
  let output = run(&[
    project.root().to_string_lossy().as_ref(),
    "--fix-safe",
    "--format",
    "json",
    "--no-cache",
  ]);
  let rewritten = fs::read_to_string(&source_path);
  let report: Result<Value, _> = serde_json::from_slice(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);

  assert!(output.status.success(), "a successful safe fix and clean rescan must exit 0: {stderr}");
  assert_eq!(
    rewritten.as_deref().ok(),
    Some(expected),
    "the edit must preserve Unicode and CRLF bytes"
  );
  assert_eq!(
    report
      .as_ref()
      .ok()
      .and_then(|value| value.get("diagnostics"))
      .and_then(Value::as_array)
      .map(Vec::len),
    Some(0),
    "stdout must report the post-fix rescan rather than the stale finding"
  );
  assert!(stderr.contains("applied 1 safe edit"), "stderr must summarize the mutation: {stderr}");
}

#[test]
fn safe_fix_rescan_reports_residual_diagnostics() {
  let project =
    TempProject::new("safe-fix-residual", "<template>\n  <img autofocus>\n</template>\n");
  let output = run(&[
    project.root().to_string_lossy().as_ref(),
    "--fix-safe",
    "--format",
    "json",
    "--no-cache",
  ]);
  let report: Result<Value, _> = serde_json::from_slice(&output.stdout);
  let rule_ids = report
    .as_ref()
    .ok()
    .and_then(|value| value.get("diagnostics"))
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(|diagnostic| diagnostic.get("rule_id"))
    .filter_map(Value::as_str)
    .collect::<Vec<_>>();

  assert!(output.status.success(), "warning-only residual findings must keep the default exit 0");
  assert!(
    !rule_ids.contains(&"vue-vet/accessibility/no-autofocus"),
    "the applied finding must disappear from the rescan"
  );
  assert!(
    rule_ids.contains(&"vue-vet/accessibility/img-has-alt"),
    "unrelated residual diagnostics must remain in the post-fix report"
  );
}

#[test]
fn safe_fix_dry_run_validates_without_writing() {
  let source = "<template>\n  <input autofocus>\n</template>\n";
  let project = TempProject::new("safe-fix-dry-run", source);
  let source_path = project.source_path();
  let output = run(&[
    source_path.to_string_lossy().as_ref(),
    "--fix-dry-run",
    "--format",
    "json",
    "--no-cache",
  ]);
  let unchanged = fs::read_to_string(&source_path);
  let report: Result<Value, _> = serde_json::from_slice(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);

  assert!(output.status.success(), "a warning-only dry run must exit 0: {stderr}");
  assert_eq!(unchanged.as_deref().ok(), Some(source), "dry-run mode must never write the file");
  assert_eq!(
    report
      .as_ref()
      .ok()
      .and_then(|value| value.get("diagnostics"))
      .and_then(Value::as_array)
      .and_then(|diagnostics| diagnostics.first())
      .and_then(|diagnostic| diagnostic.get("rule_id"))
      .and_then(Value::as_str),
    Some("vue-vet/accessibility/no-autofocus"),
    "dry-run stdout must retain the current finding"
  );
  let preview = report
    .as_ref()
    .ok()
    .and_then(|value| value.get("diagnostics"))
    .and_then(Value::as_array)
    .and_then(|diagnostics| diagnostics.first())
    .and_then(|diagnostic| diagnostic.get("edits"))
    .and_then(Value::as_array)
    .and_then(|edits| edits.first());
  assert_eq!(
    preview.and_then(|edit| edit.get("file")).and_then(Value::as_str),
    Some("App.vue"),
    "the preview path must be normalized relative to the scan root"
  );
  assert_eq!(
    preview.and_then(|edit| edit.get("applicability")).and_then(Value::as_str),
    Some("safe"),
    "dry-run JSON must expose only explicitly classified edits"
  );
  assert_eq!(
    preview.and_then(|edit| edit.get("replacement")).and_then(Value::as_str),
    Some(""),
    "the preview must expose the exact replacement text"
  );
  assert!(
    stderr.contains("would apply 1 safe edit"),
    "stderr must summarize the validated preview: {stderr}"
  );
}

#[test]
fn safe_fix_does_not_apply_a_suppressed_finding() {
  let source = concat!(
    "<template>\n",
    "  <!-- vue-vet-disable-next-line vue-vet/accessibility/no-autofocus -->\n",
    "  <input autofocus>\n",
    "</template>\n",
  );
  let project = TempProject::new("safe-fix-suppressed", source);
  let source_path = project.source_path();
  let output =
    run(&[source_path.to_string_lossy().as_ref(), "--fix-safe", "--format", "json", "--no-cache"]);
  let unchanged = fs::read_to_string(&source_path);
  let report: Result<Value, _> = serde_json::from_slice(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);

  assert!(output.status.success(), "a fully suppressed scan must exit 0: {stderr}");
  assert_eq!(
    unchanged.as_deref().ok(),
    Some(source),
    "a suppression must remove the associated edit as well as its diagnostic"
  );
  assert_eq!(
    report
      .as_ref()
      .ok()
      .and_then(|value| value.get("diagnostics"))
      .and_then(Value::as_array)
      .map(Vec::len),
    Some(0),
    "the used suppression must keep the report clean"
  );
  assert!(stderr.contains("applied 0 safe edits"), "no hidden edit may be applied: {stderr}");
}

#[test]
#[expect(clippy::panic, reason = "test setup failures must fail the integration test")]
fn safe_fix_does_not_apply_a_disabled_rule() {
  let source = "<template>\n  <input autofocus>\n</template>\n";
  let project = TempProject::new("safe-fix-disabled", source);
  let config = concat!(
    "version = 1\n",
    "preset = \"recommended\"\n",
    "[rules]\n",
    "\"vue-vet/accessibility/no-autofocus\" = \"off\"\n",
  );
  if let Err(error) = fs::write(project.root().join("vue-vet.toml"), config) {
    panic!("failed to write temporary configuration: {error}");
  }
  let output = run(&[
    project.root().to_string_lossy().as_ref(),
    "--fix-safe",
    "--format",
    "json",
    "--no-cache",
  ]);
  let unchanged = fs::read_to_string(project.source_path());
  let report: Result<Value, _> = serde_json::from_slice(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);

  assert!(output.status.success(), "a scan with the rule disabled must exit 0: {stderr}");
  assert_eq!(unchanged.as_deref().ok(), Some(source), "disabled rules must not mutate files");
  assert_eq!(
    report
      .as_ref()
      .ok()
      .and_then(|value| value.get("diagnostics"))
      .and_then(Value::as_array)
      .map(Vec::len),
    Some(0),
    "disabled findings must not appear in the post-fix report"
  );
  assert!(stderr.contains("applied 0 safe edits"), "disabled edits must be discarded: {stderr}");
}

#[test]
fn safe_fix_leaves_valued_autofocus_for_manual_review() {
  let source = "<template>\n  <input autofocus=\"true\">\n</template>\n";
  let project = TempProject::new("safe-fix-valued-autofocus", source);
  let source_path = project.source_path();
  let output =
    run(&[source_path.to_string_lossy().as_ref(), "--fix-safe", "--format", "json", "--no-cache"]);
  let unchanged = fs::read_to_string(&source_path);
  let report: Result<Value, _> = serde_json::from_slice(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);

  assert!(output.status.success(), "the remaining warning must not fail without deny-warnings");
  assert_eq!(
    unchanged.as_deref().ok(),
    Some(source),
    "a partial name-only replacement would make a valued attribute invalid"
  );
  assert_eq!(
    report
      .as_ref()
      .ok()
      .and_then(|value| value.get("diagnostics"))
      .and_then(Value::as_array)
      .and_then(|diagnostics| diagnostics.first())
      .and_then(|diagnostic| diagnostic.get("rule_id"))
      .and_then(Value::as_str),
    Some("vue-vet/accessibility/no-autofocus"),
    "the unfixed diagnostic must remain visible"
  );
  assert!(stderr.contains("applied 0 safe edits"), "no incomplete edit may be applied: {stderr}");
}

#[test]
fn safe_fix_rejects_a_multi_file_plan_without_partial_writes() {
  let source = "<template>\n  <input autofocus>\n</template>\n";
  let project = TempProject::new("safe-fix-multi-file", source);
  let second_path = project.write_source("Second.vue", source);
  let output = run(&[project.root().to_string_lossy().as_ref(), "--fix-safe", "--no-cache"]);
  let first_source = fs::read_to_string(project.source_path());
  let second_source = fs::read_to_string(second_path);
  let stderr = String::from_utf8_lossy(&output.stderr);

  assert_eq!(output.status.code(), Some(2), "unsupported multi-file plans must fail closed");
  assert!(
    stderr.contains("supports one file at a time"),
    "the operational error must explain the current phase limit: {stderr}"
  );
  assert_eq!(first_source.as_deref().ok(), Some(source), "the first file must remain unchanged");
  assert_eq!(second_source.as_deref().ok(), Some(source), "the second file must remain unchanged");
}

#[test]
fn safe_fix_modes_are_mutually_exclusive() {
  let path = fixture("rules/no-v-html/invalid/basic.vue");
  let output = run(&[path.to_string_lossy().as_ref(), "--fix-dry-run", "--fix-safe"]);
  let stderr = String::from_utf8_lossy(&output.stderr);

  assert_eq!(
    output.status.code(),
    Some(2),
    "ambiguous mutation intent must fail in argument parsing"
  );
  assert!(
    stderr.contains("cannot be used with"),
    "the CLI must explain the conflicting modes: {stderr}"
  );
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
fn cached_diagnostics_preserve_safe_edit_previews() {
  let source = "<template>\n  <input autofocus>\n</template>\n";
  let project = TempProject::new("safe-fix-cache", source);
  let cache = project.root().join("cache");
  let arguments = [
    project.root().to_string_lossy().into_owned(),
    "--format".into(),
    "json".into(),
    "--cache-dir".into(),
    cache.to_string_lossy().into_owned(),
    "--cache-stats".into(),
  ];
  let borrowed = arguments.iter().map(String::as_str).collect::<Vec<_>>();
  let cold = run(&borrowed);
  let warm = run(&borrowed);
  let report: Result<Value, _> = serde_json::from_slice(&warm.stdout);
  let edit_count = report
    .as_ref()
    .ok()
    .and_then(|value| value.get("diagnostics"))
    .and_then(Value::as_array)
    .and_then(|diagnostics| diagnostics.first())
    .and_then(|diagnostic| diagnostic.get("edits"))
    .and_then(Value::as_array)
    .map(Vec::len);

  assert_eq!(cold.stdout, warm.stdout, "cache hits must retain machine-readable edit previews");
  assert!(String::from_utf8_lossy(&cold.stderr).contains("cache: miss"));
  assert!(String::from_utf8_lossy(&warm.stderr).contains("cache: hit"));
  assert_eq!(edit_count, Some(1), "the cached diagnostic must retain its safe edit");
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

#[test]
fn project_vue_version_gates_reactivity_rules() {
  let old = fixture("projects/vue-3.4");
  let old_output = run(&[old.to_string_lossy().as_ref(), "--format", "json", "--no-cache"]);
  let old_report: Result<Value, _> = serde_json::from_slice(&old_output.stdout);
  let old_ids = old_report
    .as_ref()
    .ok()
    .and_then(|report| report.get("diagnostics"))
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(|diagnostic| diagnostic.get("rule_id"))
    .filter_map(Value::as_str)
    .collect::<Vec<_>>();
  assert!(
    old_ids.contains(&"vue-vet/reactivity/no-nonreactive-props-destructure"),
    "Vue 3.4 must report direct props destructuring"
  );
  assert!(!old_ids.contains(&"vue-vet/reactivity/prefer-use-template-ref"));

  let current = fixture("projects/vue-3.5");
  let current_output = run(&[current.to_string_lossy().as_ref(), "--format", "json", "--no-cache"]);
  let current_report: Result<Value, _> = serde_json::from_slice(&current_output.stdout);
  let current_ids = current_report
    .as_ref()
    .ok()
    .and_then(|report| report.get("diagnostics"))
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(|diagnostic| diagnostic.get("rule_id"))
    .filter_map(Value::as_str)
    .collect::<Vec<_>>();
  assert!(!current_ids.contains(&"vue-vet/reactivity/no-nonreactive-props-destructure"));
  assert!(
    current_ids.contains(&"vue-vet/reactivity/prefer-use-template-ref"),
    "Vue 3.5 must prefer useTemplateRef for matching ref(null) bindings"
  );
}

#[test]
fn reference_fixture_corpus_never_crashes() {
  let mut sources = Vec::new();
  collect_reference_sources(&fixture(""), &mut sources);
  sources.sort();
  assert!(!sources.is_empty(), "the reference fixture corpus must contain source files");

  for source in sources {
    let argument = source.to_string_lossy();
    let output = run(&[argument.as_ref(), "--no-cache"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
      output.status.code().is_some(),
      "fixture terminated without an exit code: {}",
      source.display()
    );
    assert!(
      !stderr.contains("panicked at") && !stderr.contains("fatal runtime error"),
      "fixture crashed: {}\n{stderr}",
      source.display()
    );
  }
}
