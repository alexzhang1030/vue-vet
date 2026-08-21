use super::helpers::*;

#[test]
fn project_module_seeds_feed_per_file_reactivity_rules() {
  let project = fixture("projects/module-seeds");
  // Fixture ships a stub node_modules/vue; --no-cache avoids stale entries keyed
  // without package-tree inputs.
  let output = run(&[project.to_string_lossy().as_ref(), "--format", "json", "--no-cache"]);
  let parsed: Result<Value, _> = serde_json::from_slice(&output.stdout);
  assert!(
    output.status.success(),
    "seeded project scan must succeed without deny-warnings: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  let diagnostics = parsed
    .as_ref()
    .ok()
    .and_then(|value| value.get("diagnostics"))
    .and_then(Value::as_array)
    .cloned()
    .unwrap_or_default();
  assert!(
    diagnostics.iter().any(|diagnostic| {
      diagnostic.get("rule_id").and_then(Value::as_str)
        == Some("vue-vet/reactivity/no-after-await-watch-effect-dependency")
        && diagnostic
          .get("message")
          .and_then(Value::as_str)
          .is_some_and(|message| message.contains("title"))
    }),
    "cross-file composable seeds must backfill per-file rules; diagnostics were: {diagnostics:?}"
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
    "Nuxt and explicit project relationships must be serialized: {}",
    String::from_utf8_lossy(&output.stdout)
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
#[expect(clippy::panic, reason = "test setup failures must fail the integration test")]
fn relative_dot_scan_resolves_nuxt_tilde_imports() {
  let project = TempProject::new(
    "tilde-dot-cli",
    "<script setup lang=\"ts\">\nimport type { Contract } from '~/utils/contract'\n</script>\n<template><div /></template>\n",
  );
  if let Err(error) = fs::create_dir_all(project.root().join("utils")) {
    panic!("failed to create utils: {error}");
  }
  if let Err(error) =
    fs::write(project.root().join("utils/contract.ts"), "export type Contract = string\n")
  {
    panic!("failed to write contract: {error}");
  }
  // Move the SFC under components/ so ~/utils is not a relative look-alike.
  if let Err(error) = fs::create_dir_all(project.root().join("components")) {
    panic!("failed to create components: {error}");
  }
  if let Err(error) = fs::rename(project.source_path(), project.root().join("components/App.vue")) {
    panic!("failed to move App.vue: {error}");
  }
  let output = match Command::new(env!("CARGO_BIN_EXE_vue-vet"))
    .current_dir(project.root())
    .args([".", "--format", "json", "--no-cache"])
    .output()
  {
    Ok(output) => output,
    Err(error) => panic!("failed to run vue-vet: {error}"),
  };
  assert!(
    output.status.success(),
    "relative `.` scan roots must resolve Nuxt ~/ imports: stderr={} stdout={}",
    String::from_utf8_lossy(&output.stderr),
    String::from_utf8_lossy(&output.stdout)
  );
  let report: Result<Value, _> = serde_json::from_slice(&output.stdout);
  let unresolved = report
    .as_ref()
    .ok()
    .and_then(|value| value.get("diagnostics"))
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter(|diagnostic| {
      diagnostic.get("rule_id").and_then(Value::as_str) == Some("vue-vet/project/unresolved-import")
        && diagnostic
          .get("message")
          .and_then(Value::as_str)
          .is_some_and(|message| message.contains("~/utils/contract"))
    })
    .count();
  assert_eq!(unresolved, 0, "must resolve ~/ from a relative scan root: {report:?}");
}

#[test]
#[expect(clippy::panic, reason = "test setup failures must fail the integration test")]
fn scoped_package_import_in_config_is_not_unresolved() {
  let project = TempProject::new("scoped-tailwind", "<template><div /></template>\n");
  let package = project.root().join("node_modules").join("@tailwindcss").join("vite");
  if let Err(error) = fs::create_dir_all(&package) {
    panic!("failed to create scoped package directory: {error}");
  }
  if let Err(error) = fs::write(
    package.join("package.json"),
    r#"{"name":"@tailwindcss/vite","version":"1.0.0","exports":{".":"./index.js"}}"#,
  ) {
    panic!("failed to write scoped package.json: {error}");
  }
  if let Err(error) = fs::write(package.join("index.js"), "export default {}\n") {
    panic!("failed to write scoped package entry: {error}");
  }
  if let Err(error) = fs::write(
    project.root().join("nuxt.config.ts"),
    "import tailwindcss from '@tailwindcss/vite'\nexport default { vite: { plugins: [tailwindcss()] } }\n",
  ) {
    panic!("failed to write nuxt.config.ts: {error}");
  }
  let output = run(&[project.root().to_string_lossy().as_ref(), "--format", "json", "--no-cache"]);
  assert!(
    output.status.success(),
    "scoped package imports must resolve as external: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  let report: Result<Value, _> = serde_json::from_slice(&output.stdout);
  let unresolved = report
    .as_ref()
    .ok()
    .and_then(|value| value.get("diagnostics"))
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter(|diagnostic| {
      diagnostic.get("rule_id").and_then(Value::as_str) == Some("vue-vet/project/unresolved-import")
        && diagnostic
          .get("message")
          .and_then(Value::as_str)
          .is_some_and(|message| message.contains("@tailwindcss/vite"))
    })
    .count();
  assert_eq!(
    unresolved,
    0,
    "must not report unresolved-import for installed scoped packages: {:?}",
    report.as_ref().ok()
  );
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
