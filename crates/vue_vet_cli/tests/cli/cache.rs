use super::helpers::*;

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
#[expect(clippy::panic, reason = "test setup failures must fail the integration test")]
fn cache_key_ignores_node_modules_package_directories() {
  let project = TempProject::new("nm-pixi-js", "<template><div /></template>\n");
  let package_dir = project.root().join("node_modules").join("pixi.js");
  if let Err(error) = fs::create_dir_all(&package_dir) {
    panic!("failed to create node_modules/pixi.js directory: {error}");
  }
  if let Err(error) =
    fs::write(package_dir.join("package.json"), r#"{"name":"pixi.js","version":"1.0.0"}"#)
  {
    panic!("failed to write nested package.json: {error}");
  }
  if let Err(error) = fs::write(package_dir.join("index.js"), "export default {}\n") {
    panic!("failed to write package entry: {error}");
  }
  // Symlink install shape: alias pointing at a directory package.
  let link = project.root().join("node_modules").join("alias.js");
  #[cfg(unix)]
  if let Err(error) = std::os::unix::fs::symlink(&package_dir, &link) {
    panic!("failed to create symlink to package dir: {error}");
  }
  #[cfg(windows)]
  if let Err(error) = std::os::windows::fs::symlink_dir(&package_dir, &link) {
    panic!("failed to create symlink to package dir: {error}");
  }

  // Project-root symlink to a directory: ensures filtering works outside node_modules.
  let root_link = project.root().join("alias-root.js");
  #[cfg(unix)]
  if let Err(error) = std::os::unix::fs::symlink(&package_dir, &root_link) {
    panic!("failed to create project-root symlink to package dir: {error}");
  }
  #[cfg(windows)]
  if let Err(error) = std::os::windows::fs::symlink_dir(&package_dir, &root_link) {
    panic!("failed to create project-root symlink to package dir: {error}");
  }

  let cache = project.root().join("cache");
  let output = run(&[
    project.root().to_string_lossy().as_ref(),
    "--format",
    "json",
    "--cache-dir",
    cache.to_string_lossy().as_ref(),
    "--cache-stats",
  ]);
  assert!(
    output.status.success(),
    "cache key must tolerate directory packages and symlinks in node_modules and project root: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  assert!(
    !String::from_utf8_lossy(&output.stderr).contains("Is a directory"),
    "must not try to read directory symlinks as source files: {}",
    String::from_utf8_lossy(&output.stderr)
  );
}

#[test]
fn cached_diagnostics_preserve_safe_edit_previews() {
  let source = "<template>\n  <input autofocus aria-label=\"Field\">\n</template>\n";
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
