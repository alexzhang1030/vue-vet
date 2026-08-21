use super::helpers::*;

#[test]
fn explain_prints_builtin_rule_metadata_and_documentation() {
  let output = run_from_workspace(&["--explain", "vue-vet/security/no-v-html"]);
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(output.status.success(), "explain known rules must succeed: {stdout}");
  assert!(stdout.contains("vue-vet/security/no-v-html"));
  assert!(stdout.contains("category: security"));
  assert!(stdout.contains("documentation: docs/rules/security/no-v-html.md"));
  assert!(stdout.contains("v-html"), "body must include rule markdown: {stdout}");
}

#[test]
#[expect(clippy::panic, reason = "malformed explain JSON must fail the integration test")]
fn explain_json_is_structured_and_early_exit() {
  let output = run_from_workspace(&["--explain", "vue-vet/security/no-v-html", "--format", "json"]);
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(output.status.success(), "json explain must succeed: {stdout}");
  let Ok(parsed) = serde_json::from_str::<Value>(&stdout) else {
    panic!("explain JSON must parse: {stdout}");
  };
  assert_eq!(parsed.get("rule_id").and_then(Value::as_str), Some("vue-vet/security/no-v-html"));
  assert_eq!(
    parsed.get("documentation").and_then(Value::as_str),
    Some("docs/rules/security/no-v-html.md")
  );
  assert!(parsed.get("body").and_then(Value::as_str).is_some_and(|body| body.contains("v-html")));
  assert!(parsed.get("diagnostics").is_none(), "explain must not emit a scan report");
}

#[test]
fn explain_supports_project_rules() {
  let output = run_from_workspace(&["--explain", "vue-vet/project/unresolved-import"]);
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(output.status.success(), "project rule explain must succeed: {stdout}");
  assert!(stdout.contains("documentation: docs/project-graph.md"));
  assert!(stdout.contains("Project graph") || stdout.contains("project graph"));
}

#[test]
fn lsp_flag_is_advertised_in_help() {
  let output = run(&["--help"]);
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(output.status.success(), "help must succeed: {stdout}");
  assert!(stdout.contains("--lsp"), "help must advertise the language server flag: {stdout}");
  assert!(stdout.contains("--mcp"), "help must advertise the MCP server flag: {stdout}");
}

#[test]
fn explain_rejects_unknown_rule_ids() {
  let output = run_from_workspace(&["--explain", "vue-vet/missing/rule"]);
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert_eq!(output.status.code(), Some(2), "unknown explain targets are operational failures");
  assert!(
    stderr.contains("unknown rule") && stderr.contains("vue-vet/missing/rule"),
    "stderr must name the unknown id: {stderr}"
  );
}

#[test]
#[expect(clippy::panic, reason = "malformed scan/explain JSON must fail the integration test")]
fn explain_finding_includes_evidence_and_rule_docs() {
  let path = fixture("rules/no-v-html/invalid/basic.vue");
  let path_argument = path.to_string_lossy();
  let scan = run(&[path_argument.as_ref(), "--format", "json"]);
  let scan_stdout = String::from_utf8_lossy(&scan.stdout);
  assert!(scan.status.success(), "fixture scan must succeed: {scan_stdout}");
  let Ok(scan_json) = serde_json::from_str::<Value>(&scan_stdout) else {
    panic!("scan JSON must parse: {scan_stdout}");
  };
  let Some(finding_id) =
    scan_json.pointer("/diagnostics/0/id").and_then(Value::as_str).map(str::to_owned)
  else {
    panic!("fixture must emit a diagnostic id: {scan_stdout}");
  };

  let explained = run(&[path_argument.as_ref(), "--explain", &finding_id, "--format", "json"]);
  let stdout = String::from_utf8_lossy(&explained.stdout);
  assert!(explained.status.success(), "finding explain must succeed: {stdout}");
  let Ok(parsed) = serde_json::from_str::<Value>(&stdout) else {
    panic!("explain JSON must parse: {stdout}");
  };
  assert_eq!(parsed.get("id").and_then(Value::as_str), Some(finding_id.as_str()));
  assert!(
    parsed.get("message").and_then(Value::as_str).is_some_and(|message| message.contains("v-html")),
    "finding evidence must include the diagnostic message: {stdout}"
  );
  assert_eq!(
    parsed.pointer("/rule/rule_id").and_then(Value::as_str),
    Some("vue-vet/security/no-v-html")
  );
  assert!(
    parsed
      .pointer("/rule/body")
      .and_then(Value::as_str)
      .is_some_and(|body| body.contains("v-html")),
    "finding explain must nest rule markdown: {stdout}"
  );
  assert!(parsed.get("diagnostics").is_none(), "finding explain must not emit a scan report");

  let text = run(&[path_argument.as_ref(), "--explain", &finding_id]);
  let text_stdout = String::from_utf8_lossy(&text.stdout);
  assert!(text.status.success(), "text finding explain must succeed: {text_stdout}");
  assert!(text_stdout.contains("finding: "));
  assert!(text_stdout.contains("message: "));
  assert!(text_stdout.contains("vue-vet/security/no-v-html"));
}

#[test]
fn explain_rejects_unknown_finding_ids() {
  let path = fixture("rules/no-v-html/invalid/basic.vue");
  let output = run(&[
    path.to_string_lossy().as_ref(),
    "--explain",
    "basic.vue::1:1::vue-vet/security/no-v-html::0000000000000000000000000000000000000000000000000000000000000000",
  ]);
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert_eq!(output.status.code(), Some(2), "unknown finding ids are operational failures");
  assert!(
    stderr.contains("no finding with id"),
    "stderr must say the finding was missing: {stderr}"
  );
}

#[test]
#[expect(clippy::panic, reason = "malformed explain-scope JSON must fail the integration test")]
fn explain_scope_answers_would_vue_rerun() {
  let path = fixture("rules/no-computed-without-dependency/invalid/placeholder.vue");
  let path_argument = path.to_string_lossy();
  let output = run(&[path_argument.as_ref(), "--explain-scope", "label", "--format", "json"]);
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(output.status.success(), "explain-scope must succeed: {stdout}");
  let Ok(parsed) = serde_json::from_str::<Value>(&stdout) else {
    panic!("explain-scope JSON must parse: {stdout}");
  };
  assert_eq!(parsed.get("kind").and_then(Value::as_str), Some("computed"));
  assert_eq!(parsed.get("binding").and_then(Value::as_str), Some("label"));
  assert!(
    parsed
      .get("summary")
      .and_then(Value::as_str)
      .is_some_and(|summary| summary.contains("no known reactive dependency")),
    "scope explain must state Vue will not re-run: {stdout}"
  );
  assert!(parsed.get("diagnostics").is_none(), "explain-scope must not emit a scan report");

  let text = run(&[path_argument.as_ref(), "--explain-scope", "label"]);
  let text_stdout = String::from_utf8_lossy(&text.stdout);
  assert!(text.status.success(), "text explain-scope must succeed: {text_stdout}");
  assert!(text_stdout.contains("tracking scope"));
  assert!(text_stdout.contains("summary:"));
  assert!(text_stdout.contains("no known reactive dependency"));
}

#[test]
#[expect(clippy::panic, reason = "malformed explain-scope JSON must fail the integration test")]
fn explain_scope_at_offset_covers_mid_span() {
  let path = fixture("rules/no-computed-without-dependency/invalid/placeholder.vue");
  let path_argument = path.to_string_lossy();
  let by_name = run(&[path_argument.as_ref(), "--explain-scope", "label", "--format", "json"]);
  let by_name_stdout = String::from_utf8_lossy(&by_name.stdout);
  assert!(by_name.status.success(), "binding query must succeed: {by_name_stdout}");
  let Ok(named) = serde_json::from_str::<Value>(&by_name_stdout) else {
    panic!("explain-scope JSON must parse: {by_name_stdout}");
  };
  let Some(offset) = named.pointer("/span/offset").and_then(Value::as_u64) else {
    panic!("named explain must include span.offset: {by_name_stdout}");
  };
  let Some(length) = named.pointer("/span/length").and_then(Value::as_u64) else {
    panic!("named explain must include span.length: {by_name_stdout}");
  };
  let mid = offset.saturating_add(length / 2).max(offset.saturating_add(1));
  let output =
    run(&[path_argument.as_ref(), "--explain-scope", &format!("@{mid}"), "--format", "json"]);
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(output.status.success(), "mid-span @offset must resolve: {stdout}");
  let Ok(parsed) = serde_json::from_str::<Value>(&stdout) else {
    panic!("explain-scope JSON must parse: {stdout}");
  };
  assert_eq!(parsed.get("binding").and_then(Value::as_str), Some("label"));

  let printed =
    run(&[path_argument.as_ref(), "--format", "json", "--print-reactivity", "--no-cache"]);
  let printed_stdout = String::from_utf8_lossy(&printed.stdout);
  let Ok(report) = serde_json::from_str::<Value>(&printed_stdout) else {
    panic!("print-reactivity JSON must parse: {printed_stdout}");
  };
  let Some(summary) =
    report.pointer("/reactivity/modules_detail/0/scope_details/0/summary").and_then(Value::as_str)
  else {
    panic!("scope_details must carry summary: {printed_stdout}");
  };
  assert!(
    summary.contains("no known reactive dependency"),
    "digest summary must match explain-scope: {summary}"
  );
}

#[test]
fn explain_scope_rejects_unknown_queries() {
  let path = fixture("rules/no-computed-without-dependency/invalid/placeholder.vue");
  let output = run(&[path.to_string_lossy().as_ref(), "--explain-scope", "missingBinding"]);
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert_eq!(output.status.code(), Some(2), "unknown scope queries are operational failures");
  assert!(
    stderr.contains("no tracking scope matched"),
    "stderr must say no scope matched: {stderr}"
  );
}
