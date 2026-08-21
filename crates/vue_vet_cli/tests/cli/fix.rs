use super::helpers::*;

#[test]
fn safe_fix_preserves_unicode_and_crlf_then_reports_the_rescan() {
  let source =
    "<template>\r\n  <p>你好</p>\r\n  <input autofocus aria-label=\"Field\">\r\n</template>\r\n";
  let expected = "<template>\r\n  <p>你好</p>\r\n  <input aria-label=\"Field\">\r\n</template>\r\n";
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
  let source = "<template>\n  <input autofocus aria-label=\"Field\">\n</template>\n";
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
    "  <input autofocus aria-label=\"Field\">\n",
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
fn safe_fix_does_not_apply_a_disabled_rule() {
  let source = "<template>\n  <input autofocus aria-label=\"Field\">\n</template>\n";
  let project = TempProject::new("safe-fix-disabled", source);
  let config = concat!(
    "version = 1\n",
    "preset = \"recommended\"\n",
    "[rules]\n",
    "\"vue-vet/accessibility/no-autofocus\" = \"off\"\n",
  );
  project.write_source("vue-vet.toml", config);
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
  let source = "<template>\n  <input autofocus=\"true\" aria-label=\"Field\">\n</template>\n";
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
fn safe_fix_removes_quoted_aria_hidden_on_focusable() {
  let source = "<template>\n  <button aria-hidden=\"true\">Save</button>\n</template>\n";
  let expected = "<template>\n  <button>Save</button>\n</template>\n";
  let project = TempProject::new("safe-fix-aria-hidden", source);
  let source_path = project.source_path();
  let output =
    run(&[source_path.to_string_lossy().as_ref(), "--fix-safe", "--format", "json", "--no-cache"]);
  let rewritten = fs::read_to_string(&source_path);
  let report: Result<Value, _> = serde_json::from_slice(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);

  assert!(output.status.success(), "removing aria-hidden should leave a clean button: {stderr}");
  assert_eq!(rewritten.as_deref().ok(), Some(expected), "quoted aria-hidden=true must be removed");
  assert_eq!(
    report
      .as_ref()
      .ok()
      .and_then(|value| value.get("diagnostics"))
      .and_then(Value::as_array)
      .map(Vec::len),
    Some(0),
    "stdout must report the post-fix rescan"
  );
  assert!(stderr.contains("applied 1 safe edit"), "stderr must summarize the mutation: {stderr}");
}

#[test]
fn safe_fix_leaves_unquoted_aria_hidden_for_manual_review() {
  let source = "<template>\n  <button aria-hidden=true>Save</button>\n</template>\n";
  let project = TempProject::new("safe-fix-unquoted-aria-hidden", source);
  let source_path = project.source_path();
  let output =
    run(&[source_path.to_string_lossy().as_ref(), "--fix-safe", "--format", "json", "--no-cache"]);
  let unchanged = fs::read_to_string(&source_path);
  let report: Result<Value, _> = serde_json::from_slice(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);

  assert_eq!(
    output.status.code(),
    Some(1),
    "an unfixed error must fail the default exit policy: {stderr}"
  );
  assert_eq!(
    unchanged.as_deref().ok(),
    Some(source),
    "an unquoted value has no complete replacement span"
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
    Some("vue-vet/accessibility/no-aria-hidden-on-focusable"),
    "the unfixed diagnostic must remain visible"
  );
  assert!(stderr.contains("applied 0 safe edits"), "no incomplete edit may be applied: {stderr}");
}

#[test]
fn safe_fix_rejects_a_multi_file_plan_without_partial_writes() {
  let source = "<template>\n  <input autofocus aria-label=\"Field\">\n</template>\n";
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
