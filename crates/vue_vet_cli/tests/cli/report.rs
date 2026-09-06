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
    "vue-vet: parsing files",
    "vue-vet: building project graph",
    "vue-vet: checking rules",
    "vue-vet: writing report",
  ] {
    assert!(stderr.contains(stage), "stderr must stream `{stage}`: {stderr}");
  }
  assert!(!stderr.contains("analyzed "), "plain progress must not emit a line per file: {stderr}");
  let parsed: Value = serde_json::from_slice(&output.stdout).expect("stdout must stay JSON");
  assert!(parsed.get("ok").and_then(Value::as_bool).unwrap_or(false));
}

#[test]
fn text_batches_file_findings_once_after_scan() {
  let project = TempProject::new(
    "text-batch-a",
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
  assert!(stderr.contains("vue-vet: checking rules"), "stderr must emit the rules phase: {stderr}");
  let v_html = stdout.matches("no-v-html").count();
  assert_eq!(v_html, 2, "each file finding must appear exactly once: {stdout}");
  let score = stdout.find("Score:").or_else(|| stdout.find("/100"));
  let first_finding = stdout.find("no-v-html");
  assert!(
    matches!((first_finding, score), (Some(finding), Some(footer)) if finding < footer),
    "final text must print diagnostics before the score footer: {stdout}"
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

#[test]
#[expect(clippy::expect_used, reason = "integration test asserts JSON diagnostic ids")]
fn text_report_includes_project_findings_exactly_once() {
  let project = fixture("projects/nuxt-graph");
  let text = run(&[
    project.to_string_lossy().as_ref(),
    "--progress",
    "always",
    "--color",
    "never",
    "--no-cache",
  ]);
  let json = run(&[
    project.to_string_lossy().as_ref(),
    "--progress",
    "never",
    "--format",
    "json",
    "--no-cache",
  ]);
  assert!(
    text.status.success() || text.status.code() == Some(1),
    "text scan must complete: {}",
    String::from_utf8_lossy(&text.stderr)
  );
  assert!(
    json.status.success() || json.status.code() == Some(1),
    "json scan must complete: {}",
    String::from_utf8_lossy(&json.stderr)
  );
  let stdout = String::from_utf8_lossy(&text.stdout);
  let parsed: Value = serde_json::from_slice(&json.stdout).expect("json stdout");
  let diagnostics =
    parsed.get("diagnostics").and_then(Value::as_array).cloned().unwrap_or_default();
  assert!(!diagnostics.is_empty(), "nuxt-graph must produce diagnostics: {parsed}");
  for diagnostic in &diagnostics {
    let rule = diagnostic.get("rule_id").and_then(Value::as_str).unwrap_or_default();
    assert!(stdout.contains(rule), "text must include project finding {rule} from JSON: {stdout}");
  }
  assert!(
    stdout.contains("unused-component") || stdout.contains("unresolved-import"),
    "generic nuxt-graph project findings must be visible: {stdout}"
  );
}

#[test]
#[expect(clippy::panic, reason = "an unexpected process error must fail the integration test")]
fn progress_always_stays_plain_under_term_dumb() {
  let project = TempProject::new(
    "progress-dumb",
    "<script setup>\nconst n = 1\n</script>\n<template><p>{{ n }}</p></template>\n",
  );
  let path = project.source_path();
  let output = match Command::new(env!("CARGO_BIN_EXE_vue-vet"))
    .args([
      path.to_string_lossy().as_ref(),
      "--progress",
      "always",
      "--format",
      "json",
      "--no-cache",
    ])
    .env("TERM", "dumb")
    .output()
  {
    Ok(output) => output,
    Err(error) => panic!("failed to run vue-vet: {error}"),
  };
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert!(output.status.success(), "scan must succeed: {stderr}");
  assert!(
    stderr.contains("vue-vet: discovering workspace"),
    "TERM=dumb --progress always must still log phases: {stderr}"
  );
  assert!(!stderr.contains('\u{1b}'), "dumb terminals must not receive ANSI rewrites: {stderr:?}");
}

#[test]
fn many_file_plain_progress_stays_bounded() {
  let project = TempProject::new(
    "progress-many",
    "<script setup>\nconst n = 1\n</script>\n<template><p>{{ n }}</p></template>\n",
  );
  for index in 0..40 {
    project.write_source(
      &format!("File{index}.vue"),
      "<script setup>\nconst n = 1\n</script>\n<template><p>{{ n }}</p></template>\n",
    );
  }
  let output = run(&[
    project.root().to_string_lossy().as_ref(),
    "--progress",
    "always",
    "--format",
    "json",
    "--no-cache",
    "--color",
    "never",
  ]);
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert!(output.status.success(), "scan must succeed: {stderr}");
  let progress_lines = stderr.lines().filter(|line| line.starts_with("vue-vet:")).count();
  assert!(
    progress_lines <= 16,
    "plain progress must stay bounded for many files ({progress_lines}): {stderr}"
  );
}

#[test]
#[expect(clippy::expect_used, reason = "JSON equality across progress settings")]
fn json_is_stable_across_progress_settings() {
  let path = fixture("projects/nuxt-graph");
  let always = run(&[
    path.to_string_lossy().as_ref(),
    "--progress",
    "always",
    "--format",
    "json",
    "--no-cache",
  ]);
  let never = run(&[
    path.to_string_lossy().as_ref(),
    "--progress",
    "never",
    "--format",
    "json",
    "--no-cache",
  ]);
  let left: Value = serde_json::from_slice(&always.stdout).expect("always json");
  let right: Value = serde_json::from_slice(&never.stdout).expect("never json");
  assert_eq!(
    left.get("diagnostics"),
    right.get("diagnostics"),
    "progress must not change JSON diagnostics"
  );
}

#[test]
fn text_cold_and_warm_cache_keep_project_findings() {
  let project = fixture("projects/nuxt-graph");
  let cache = workspace_root().join("target").join(format!(
    "test-text-cache-{}-{}",
    std::process::id(),
    NEXT_TEMP_PROJECT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
  ));
  let project_argument = project.to_string_lossy();
  let cache_argument = cache.to_string_lossy();
  let arguments = [
    project_argument.as_ref(),
    "--format",
    "text",
    "--color",
    "never",
    "--progress",
    "never",
    "--cache-dir",
    cache_argument.as_ref(),
  ];
  let cold = run(&arguments);
  let warm = run(&arguments);
  let cold_text = String::from_utf8_lossy(&cold.stdout);
  let _ignored = std::fs::remove_dir_all(cache);
  assert_eq!(cold.stdout, warm.stdout, "cold and warm text reports must match");
  assert_eq!(cold_text.matches("vue-vet/project/unused-component").count(), 1, "{cold_text}");
  assert_eq!(cold_text.matches("vue-vet/project/unresolved-import").count(), 1, "{cold_text}");
  assert!(cold_text.contains("2 finding(s)"), "{cold_text}");
}

#[test]
fn text_safe_fix_rescan_drops_applied_finding() {
  let project =
    TempProject::new("safe-fix-text-rescan", "<template>\n  <img autofocus>\n</template>\n");
  let output = run(&[
    project.root().to_string_lossy().as_ref(),
    "--fix-safe",
    "--format",
    "text",
    "--color",
    "never",
    "--progress",
    "never",
  ]);
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(
    output.status.success(),
    "warning-only residual findings must keep the default exit 0: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  assert!(
    !stdout.contains("no-autofocus"),
    "applied finding must not remain after rescan: {stdout}"
  );
  assert_eq!(
    stdout.matches("img-has-alt").count(),
    1,
    "residual finding must appear once after rescan: {stdout}"
  );
  assert!(stdout.contains("1 finding(s)"), "{stdout}");
}
