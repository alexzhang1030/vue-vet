use super::helpers::*;

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn explain_rule_loads_documentation_without_scan_diagnostics() {
  let session = open_session(fixture("rules/no-v-html/invalid/basic.vue"));
  let Ok(explain) = session.explain_rule("vue-vet/security/no-v-html") else {
    panic!("rule explain");
  };
  assert_eq!(explain.rule_id, "vue-vet/security/no-v-html");
  assert!(explain.body.as_deref().is_some_and(|body| body.contains("v-html")));
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn explain_scope_reports_no_known_dependency_for_static_computed() {
  let session =
    open_session(fixture("rules/no-computed-without-dependency/invalid/placeholder.vue"));
  let Ok((explains, _)) = session.explain_scope("label") else {
    panic!("scope explain must find binding label");
  };
  assert_eq!(explains.len(), 1, "expected one computed scope: {explains:?}");
  let Some(explain) = explains.first() else {
    panic!("expected one computed scope");
  };
  assert_eq!(explain.kind, "computed");
  assert_eq!(explain.binding.as_deref(), Some("label"));
  assert!(explain.tracks.is_empty(), "static computed tracks nothing: {explain:?}");
  assert!(
    explain.summary.contains("no known reactive dependency"),
    "summary must answer would Vue re-run?: {}",
    explain.summary
  );

  let Ok(snapshot) = session.analyze() else {
    panic!("analyze must succeed");
  };
  let Some(diagnostic) = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| diagnostic.rule_id.contains("no-computed-without-dependency"))
  else {
    panic!("fixture must emit no-computed-without-dependency");
  };
  let id = finding_id(diagnostic);
  let Ok(finding) = session.explain_finding(&id) else {
    panic!("finding explain must succeed");
  };
  let Some(tracking) = finding.tracking.as_ref() else {
    panic!("finding on a scope must attach tracking");
  };
  assert_eq!(tracking.binding.as_deref(), Some("label"));
  assert!(tracking.summary.contains("no known reactive dependency"));

  let start = tracking.span.offset;
  let mid = start.saturating_add(tracking.span.length / 2).max(start.saturating_add(1));
  let Ok((at_start, _)) = session.explain_scope(&format!("@{start}")) else {
    panic!("@start must match the computed span start");
  };
  let Ok((at_mid, _)) = session.explain_scope(&format!("@{mid}")) else {
    panic!("mid-span @offset must fall back to the covering computed");
  };
  assert_eq!(at_start.first().and_then(|item| item.binding.clone()), Some("label".into()));
  assert_eq!(at_mid.first().and_then(|item| item.binding.clone()), Some("label".into()));
  assert_eq!(at_mid.first().map(|item| item.summary.as_str()), Some(tracking.summary.as_str()));
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn explain_scope_reuses_full_snapshot_after_diagnostics_only() {
  let session =
    open_session(fixture("rules/no-computed-without-dependency/invalid/placeholder.vue"));
  let lean = session
    .analyze_affected_product(AnalysisProduct::DiagnosticsOnly)
    .unwrap_or_else(|error| panic!("diagnostics-only: {error}"));
  assert!(
    lean.graph.module_reactivity.is_empty(),
    "published DiagnosticsOnly DTO must omit module reactivity"
  );
  let current = session
    .current_snapshot()
    .unwrap_or_else(|error| panic!("current snapshot: {error}"))
    .unwrap_or_else(|| panic!("DiagnosticsOnly must commit a full snapshot"));
  assert!(
    !current.graph.module_reactivity.is_empty(),
    "committed IR must keep module reactivity for explain-scope hover"
  );
  let Ok((explains, _)) = session.explain_scope("label") else {
    panic!("explain-scope must reuse the committed full snapshot");
  };
  assert_eq!(explains.len(), 1, "expected one computed scope: {explains:?}");
  let Some(explain) = explains.first() else {
    panic!("expected one computed scope");
  };
  assert_eq!(explain.binding.as_deref(), Some("label"));
  assert!(explain.summary.contains("no known reactive dependency"));
}

#[test]
#[expect(
  clippy::indexing_slicing,
  clippy::panic,
  reason = "session setup failures must fail the integration test"
)]
fn explain_scope_module_prefix_selects_one_file() {
  let dir = std::env::temp_dir().join(format!("vue-vet-explain-prefix-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("temp dir: {error}"));
  let source = |name: &str| {
    format!(
      "<script setup>\nimport {{ computed }} from 'vue'\nconst label = computed(() => '{name}')\n</script>\n<template>{{{{ label }}}}</template>\n"
    )
  };
  std::fs::write(dir.join("a.vue"), source("a")).unwrap_or_else(|error| panic!("a.vue: {error}"));
  std::fs::write(dir.join("b.vue"), source("b")).unwrap_or_else(|error| panic!("b.vue: {error}"));

  let session = open_session(&dir);
  let (qualified, _) =
    session.explain_scope("b.vue:label").unwrap_or_else(|error| panic!("b.vue:label: {error}"));
  assert_eq!(qualified.len(), 1, "module prefix must not return a.vue: {qualified:?}");
  assert!(
    qualified[0].module_id.ends_with("b.vue"),
    "qualified match must be b.vue: {qualified:?}"
  );
  assert_eq!(qualified[0].binding.as_deref(), Some("label"));

  let (all, _) = session.explain_scope("label").unwrap_or_else(|error| panic!("label: {error}"));
  assert_eq!(all.len(), 2, "bare binding still matches both files: {all:?}");

  let offset = qualified[0].span.offset;
  let (at_file, _) = session
    .explain_scope(&format!("b.vue:@{offset}"))
    .unwrap_or_else(|error| panic!("b.vue:@offset: {error}"));
  assert_eq!(at_file.len(), 1, "file:@offset must stay in b.vue: {at_file:?}");
  assert!(at_file[0].module_id.ends_with("b.vue"), "file:@offset must be b.vue: {at_file:?}");

  let _ignored = std::fs::remove_dir_all(&dir);
}
