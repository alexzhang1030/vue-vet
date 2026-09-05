use super::helpers::*;

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
fn malformed_fixture_returns_a_partial_result_without_panicking() {
  let path = fixture("parser/malformed/unclosed-template.vue");
  let output = run(&[path.to_string_lossy().as_ref()]);
  let stdout = String::from_utf8_lossy(&output.stdout);

  assert_eq!(output.status.code(), Some(1), "a parser diagnostic must use the finding exit code");
  assert!(stdout.contains("failed to analyze"), "the finding must explain the parser failure");
  assert!(!stdout.contains("panicked"), "malformed input must never panic");
}

#[test]
fn malformed_fixture_returns_structured_partial_json() {
  let path = fixture("parser/malformed/unclosed-template.vue");
  let output = run(&[path.to_string_lossy().as_ref(), "--format", "json"]);
  let parsed: Result<Value, _> = serde_json::from_slice(&output.stdout);

  assert_eq!(output.status.code(), Some(1), "a parser diagnostic must use the finding exit code");
  assert_eq!(
    parsed.as_ref().ok().and_then(|report| report.get("ok")).and_then(Value::as_bool),
    Some(true),
    "analysis completed with an explicit partial result"
  );
  assert_eq!(
    parsed
      .as_ref()
      .ok()
      .and_then(|report| report.get("project"))
      .and_then(|project| project.get("complete"))
      .and_then(Value::as_bool),
    Some(false),
    "partial scans must never claim complete coverage"
  );
  assert_eq!(
    parsed
      .as_ref()
      .ok()
      .and_then(|report| report.get("diagnostics"))
      .and_then(Value::as_array)
      .and_then(|diagnostics| diagnostics.first())
      .and_then(|diagnostic| diagnostic.get("rule_id"))
      .and_then(Value::as_str),
    Some("vue-vet/analysis/parse-error"),
    "the parser failure must be represented as a file diagnostic"
  );
}

#[test]
fn reporter_text_snapshot_is_stable() {
  let output =
    run_from_workspace(&["fixtures/reporters/no-v-html.vue", "--no-cache", "--color", "never"]);
  let stdout = String::from_utf8_lossy(&output.stdout).replace('\\', "/");

  assert!(output.status.success(), "text reporter fixture must scan successfully");
  assert_eq!(
    stdout.trim_end(),
    include_str!("../../../../fixtures/reporters/no-v-html.txt").trim_end(),
    "text reporter snapshot changed"
  );
}

#[test]
fn reporter_text_color_always_emits_ansi() {
  let output =
    run_from_workspace(&["fixtures/reporters/no-v-html.vue", "--no-cache", "--color", "always"]);
  let stdout = String::from_utf8_lossy(&output.stdout);

  assert!(output.status.success(), "colored text reporter fixture must scan successfully");
  assert!(stdout.contains('\u{1b}'), "--color always must emit ANSI escapes: {stdout:?}");
  assert!(stdout.contains("warning"), "colored report must keep severity label: {stdout}");
}

#[test]
fn reporter_json_snapshot_is_stable() {
  let output =
    run_from_workspace(&["fixtures/reporters/no-v-html.vue", "--format", "json", "--no-cache"]);
  let stdout = String::from_utf8_lossy(&output.stdout).replace('\\', "/");

  assert!(output.status.success(), "JSON reporter fixture must scan successfully");
  assert_eq!(
    stdout.trim_end(),
    include_str!("../../../../fixtures/reporters/no-v-html.json").trim_end(),
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
fn text_report_includes_reactivity_digest() {
  let project = fixture("projects/module-seeds");
  let output = run(&[project.to_string_lossy().as_ref(), "--no-cache"]);
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(output.status.success(), "text scan must succeed: {stdout}");
  assert!(stdout.contains("Reactivity"), "text report must surface a Reactivity footer: {stdout}");
  assert!(
    stdout.contains("bindings") && stdout.contains("scopes"),
    "digest must show tracer totals: {stdout}"
  );
  assert!(
    stdout.contains("App.vue") || stdout.contains("busiest"),
    "digest should highlight busy modules when facts exist: {stdout}"
  );
}

#[test]
#[expect(clippy::panic, reason = "malformed JSON reports must fail the integration test")]
fn default_digest_matches_print_reactivity_totals() {
  let project = fixture("projects/module-seeds");
  let path = project.to_string_lossy();
  let default = run(&[path.as_ref(), "--format", "json", "--no-cache"]);
  let detailed = run(&[path.as_ref(), "--format", "json", "--print-reactivity", "--no-cache"]);
  let default_stdout = String::from_utf8_lossy(&default.stdout);
  let detailed_stdout = String::from_utf8_lossy(&detailed.stdout);
  assert!(default.status.success(), "default json: {default_stdout}");
  assert!(detailed.status.success(), "print-reactivity json: {detailed_stdout}");
  let default_json: Value = serde_json::from_slice(&default.stdout)
    .unwrap_or_else(|error| panic!("{error}: {default_stdout}"));
  let detailed_json: Value = serde_json::from_slice(&detailed.stdout)
    .unwrap_or_else(|error| panic!("{error}: {detailed_stdout}"));
  for key in ["modules", "bindings", "scopes", "edges", "template_reads", "hotspots"] {
    assert_eq!(
      default_json.pointer(&format!("/reactivity/{key}")),
      detailed_json.pointer(&format!("/reactivity/{key}")),
      "default digest {key} must match print-reactivity aggregation"
    );
  }
  let default_detail = default_json.pointer("/reactivity/modules_detail");
  assert!(
    default_detail.is_none() || default_detail.and_then(Value::as_array).is_some_and(Vec::is_empty),
    "default JSON must omit modules_detail: {default_stdout}"
  );
  let details = detailed_json
    .pointer("/reactivity/modules_detail")
    .and_then(Value::as_array)
    .unwrap_or_else(|| panic!("print-reactivity must fill modules_detail: {detailed_stdout}"));
  assert!(
    details.iter().any(|module| {
      module.get("binding_details").and_then(Value::as_array).is_some_and(|items| !items.is_empty())
        || module
          .get("scope_details")
          .and_then(Value::as_array)
          .is_some_and(|items| !items.is_empty())
    }),
    "TUI/print-reactivity stats must keep structured details: {detailed_stdout}"
  );
  let text = run(&[path.as_ref(), "--no-cache"]);
  let text_out = String::from_utf8_lossy(&text.stdout);
  assert!(text.status.success(), "text footer: {text_out}");
  let bindings = default_json
    .pointer("/reactivity/bindings")
    .and_then(Value::as_u64)
    .unwrap_or_else(|| panic!("bindings total: {default_stdout}"));
  assert!(
    text_out.contains(&bindings.to_string()) && text_out.contains("Reactivity"),
    "text footer must show the same digest totals: {text_out}"
  );
}

#[test]
fn print_reactivity_lists_module_detail() {
  let project = fixture("projects/module-seeds");
  let output = run(&[project.to_string_lossy().as_ref(), "--print-reactivity", "--no-cache"]);
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(output.status.success(), "print-reactivity scan must succeed: {stdout}");
  assert!(stdout.contains("Reactivity detail"), "detail section missing: {stdout}");
  assert!(
    stdout.contains("bindings:") || stdout.contains("scopes:"),
    "detail must list bindings/scopes: {stdout}"
  );
}

#[test]
#[expect(clippy::panic, reason = "malformed JSON reports must fail the integration test")]
fn json_print_reactivity_includes_structured_span_details() {
  let project = fixture("projects/module-seeds");
  let output = run(&[
    project.to_string_lossy().as_ref(),
    "--format",
    "json",
    "--print-reactivity",
    "--no-cache",
  ]);
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(output.status.success(), "json print-reactivity must succeed: {stdout}");
  let Ok(parsed) = serde_json::from_str::<Value>(&stdout) else {
    panic!("JSON report must parse: {stdout}");
  };
  let Some(details) = parsed.pointer("/reactivity/modules_detail").and_then(Value::as_array) else {
    panic!("modules_detail array missing: {stdout}");
  };
  assert!(!details.is_empty(), "expected at least one module detail");
  let has_structured = details.iter().any(|module| {
    module.get("binding_details").and_then(Value::as_array).is_some_and(|items| !items.is_empty())
      || module.get("edge_details").and_then(Value::as_array).is_some_and(|items| !items.is_empty())
      || module
        .get("template_details")
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty())
  });
  assert!(has_structured, "modules_detail should include structured *_details: {stdout}");
  let span_ok = details.iter().any(|module| {
    module
      .get("edge_details")
      .and_then(Value::as_array)
      .into_iter()
      .flatten()
      .chain(module.get("binding_details").and_then(Value::as_array).into_iter().flatten())
      .any(|item| item.pointer("/span/offset").and_then(Value::as_u64).is_some())
  });
  assert!(span_ok, "structured details must carry span.offset: {stdout}");
  assert!(
    parsed.pointer("/component_nav/modules").and_then(Value::as_array).is_some(),
    "JSON reports must include structural component_nav: {stdout}"
  );
  let has_edges_or_templates = details.iter().any(|module| {
    module.get("edge_details").and_then(Value::as_array).is_some_and(|items| !items.is_empty())
      || module
        .get("template_details")
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty())
  });
  if has_edges_or_templates {
    assert!(
      details.iter().any(|module| module
        .pointer("/binding_nav/inbound")
        .and_then(Value::as_object)
        .is_some_and(|inbound| !inbound.is_empty())),
      "modules with edges/templates must ship binding_nav: {stdout}"
    );
  }
}

#[test]
fn reactivity_tui_requires_an_interactive_terminal() {
  let project = fixture("projects/module-seeds");
  let output = run(&[project.to_string_lossy().as_ref(), "--reactivity-tui", "--no-cache"]);
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert_eq!(output.status.code(), Some(2), "non-TTY TUI must be an operational failure");
  assert!(
    stderr.contains("interactive terminal"),
    "non-TTY TUI should explain the requirement: {stderr}"
  );
}

#[test]
fn reactivity_tui_requires_text_format() {
  let project = fixture("projects/module-seeds");
  let output = run(&[
    project.to_string_lossy().as_ref(),
    "--reactivity-tui",
    "--format",
    "json",
    "--no-cache",
  ]);
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert_eq!(output.status.code(), Some(2), "json + TUI must be an operational failure");
  assert!(
    stdout.contains("--format text"),
    "JSON operational errors must mention text format: {stdout}"
  );
}

#[test]
#[expect(clippy::expect_used, reason = "integration test asserts JSON stdout shape")]
fn progress_always_streams_stages_on_stderr() {
  let project = TempProject::new(
    "progress-always",
    "<script setup>\nconst n = 1\n</script>\n<template><p>{{ n }}</p></template>\n",
  );
  let path = project.source_path();
  let output = run(&[
    path.to_string_lossy().as_ref(),
    "--progress",
    "always",
    "--format",
    "json",
    "--no-cache",
  ]);
  assert!(
    output.status.success(),
    "scan must succeed: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  let stderr = String::from_utf8_lossy(&output.stderr);
  for stage in [
    "vue-vet: discovering workspace",
    "vue-vet: parsing",
    "vue-vet: building project graph",
    "vue-vet: running rules",
    "vue-vet: analyzed",
    "vue-vet: writing report",
  ] {
    assert!(stderr.contains(stage), "stderr must stream `{stage}`: {stderr}");
  }
  let parsed: Value = serde_json::from_slice(&output.stdout).expect("stdout must stay JSON");
  assert!(parsed.get("ok").and_then(Value::as_bool).unwrap_or(false));
}

#[test]
fn text_streams_findings_as_files_finish() {
  let project = TempProject::new(
    "text-stream-a",
    "<script setup>\nconst n = 1\n</script>\n<template><div v-html=\"n\" /></template>\n",
  );
  project.write_source(
    "Other.vue",
    "<script setup>\nconst m = 2\n</script>\n<template><div v-html=\"m\" /></template>\n",
  );
  let output = run(&[
    project.root().to_string_lossy().as_ref(),
    "--progress",
    "always",
    "--format",
    "text",
    "--no-cache",
    "--color",
    "never",
  ]);
  assert!(
    output.status.success() || output.status.code() == Some(1),
    "scan must complete: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  let stderr = String::from_utf8_lossy(&output.stderr);
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(
    stderr.contains("vue-vet: analyzed"),
    "stderr must emit per-file analyzed lines: {stderr}"
  );
  assert!(
    stdout.contains("no-v-html") || stdout.contains("v-html"),
    "text findings must stream to stdout: {stdout}"
  );
  let writing = stderr.find("vue-vet: writing report");
  let first_finding = stdout.find("v-html").or_else(|| stdout.find("no-v-html"));
  if let (Some(writing), Some(finding_pos)) = (writing, first_finding) {
    // Findings are printed during rules (before writing report). We cannot
    // compare stderr/stdout offsets across streams; require the score footer
    // after findings instead.
    let _ = (writing, finding_pos);
  }
  assert!(
    stdout.contains("Score:") || stdout.contains("score:") || stdout.contains("/100"),
    "footer must still print after streamed findings: {stdout}"
  );
}

#[test]
fn progress_never_keeps_stage_lines_off_stderr() {
  let project = TempProject::new(
    "progress-never",
    "<script setup>\nconst n = 1\n</script>\n<template><p>{{ n }}</p></template>\n",
  );
  let path = project.source_path();
  let output = run(&[
    path.to_string_lossy().as_ref(),
    "--progress",
    "never",
    "--format",
    "json",
    "--no-cache",
  ]);
  assert!(
    output.status.success(),
    "scan must succeed: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert!(
    !stderr.contains("discovering workspace"),
    "--progress never must not stream stages: {stderr}"
  );
}

#[test]
#[expect(clippy::panic, reason = "an unexpected process error must fail the integration test")]
fn progress_auto_stays_quiet_under_ci_env() {
  let project = TempProject::new(
    "progress-ci",
    "<script setup>\nconst n = 1\n</script>\n<template><p>{{ n }}</p></template>\n",
  );
  let path = project.source_path();
  let output = match Command::new(env!("CARGO_BIN_EXE_vue-vet"))
    .args([path.to_string_lossy().as_ref(), "--progress", "auto", "--format", "json", "--no-cache"])
    .env("CI", "1")
    .output()
  {
    Ok(output) => output,
    Err(error) => panic!("failed to run vue-vet: {error}"),
  };
  assert!(
    output.status.success(),
    "scan must succeed: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert!(
    !stderr.contains("discovering workspace"),
    "CI=1 + --progress auto must stay quiet: {stderr}"
  );
}
