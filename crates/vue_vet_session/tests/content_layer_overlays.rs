use std::{error::Error, path::Path};

use vue_vet_session::{AnalysisSnapshot, ChangeSet, ProjectSession, SessionOptions};

const ENABLED: &str = "export default defineNuxtConfig({ modules: ['@nuxt/content'] })\n";
const DISABLED: &str = "export default defineNuxtConfig({ modules: [] })\n";
const PANEL: &str = "app/components/content/GuidePanel.vue";

fn open(root: &Path) -> Result<ProjectSession, vue_vet_session::SessionError> {
  ProjectSession::open(SessionOptions {
    root: root.to_owned(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  })
}

#[track_caller]
fn assert_unused(snapshot: &AnalysisSnapshot, expected: bool) {
  assert_eq!(
    snapshot.summary.diagnostics.iter().any(|diagnostic| {
      diagnostic.file.as_str() == PANEL && diagnostic.rule_id == "vue-vet/project/unused-component"
    }),
    expected,
    "Content entrypoint policy must follow the current layer bytes"
  );
  assert!(
    snapshot.analyzed_files.iter().all(|file| !file.starts_with("node_modules/")),
    "package directories must not be analyzed as source: {:?}",
    snapshot.analyzed_files
  );
}

fn assert_parity(incremental: &AnalysisSnapshot, clean: &AnalysisSnapshot) {
  assert_eq!(incremental.summary, clean.summary, "incremental summary must match a clean scan");
  assert_eq!(incremental.graph, clean.graph, "incremental graph must match a clean scan");
  assert_eq!(
    incremental.analyzed_files, clean.analyzed_files,
    "incremental analyzed files must match a clean scan"
  );
  assert_eq!(incremental.coverage, clean.coverage, "incremental coverage must match a clean scan");
  assert_eq!(incremental.issues, clean.issues, "incremental issues must match a clean scan");
}

#[test]
#[expect(
  clippy::panic_in_result_fn,
  reason = "session overlay assertions must fail the integration test"
)]
fn layer_config_overlay_refresh_and_deletion_preserve_scan_parity() -> Result<(), Box<dyn Error>> {
  let root =
    std::env::temp_dir().join(format!("vue-vet-content-layer-overlay-{}", std::process::id()));
  let layer = root.join("node_modules/theme-kit");
  let config = layer.join("nuxt.config.ts");
  std::fs::create_dir_all(&layer)?;
  std::fs::create_dir_all(root.join("app/components/content"))?;
  std::fs::write(
    root.join("package.json"),
    r#"{"private":true,"dependencies":{"nuxt":"4.0.0","theme-kit":"1.0.0"}}"#,
  )?;
  std::fs::write(
    layer.join("package.json"),
    r#"{"name":"theme-kit","version":"1.0.0","exports":{"./*":"./*"}}"#,
  )?;
  std::fs::write(
    root.join("nuxt.config.ts"),
    "export default defineNuxtConfig({ extends: ['theme-kit'] })\n",
  )?;
  std::fs::write(root.join(PANEL), "<template><span>Item</span></template>\n")?;
  std::fs::write(&config, ENABLED)?;

  let session = open(&root)?;
  let initial = session.analyze()?;
  assert_unused(&initial, false);

  session.apply_changes(ChangeSet::upsert(config.clone(), DISABLED.into()))?;
  let disabled = session.analyze_affected()?;
  assert_eq!(
    std::fs::read_to_string(&config)?,
    ENABLED,
    "overlay must not write the layer file on disk"
  );
  assert_unused(&disabled, true);
  assert_eq!(
    disabled.analyzed_files, initial.analyzed_files,
    "layer overlay must keep the same analyzed files"
  );

  std::fs::write(&config, DISABLED)?;
  let clean_disabled = open(&root)?.analyze()?;
  assert_parity(&disabled, &clean_disabled);

  session.apply_changes(ChangeSet::upsert(config.clone(), ENABLED.into()))?;
  assert_unused(&session.analyze_affected()?, false);

  session.apply_changes(ChangeSet::remove(config.clone()))?;
  let refreshed = session.analyze_affected()?;
  assert_unused(&refreshed, true);
  assert_parity(&refreshed, &clean_disabled);

  std::fs::remove_file(&config)?;
  session.apply_changes(ChangeSet::remove(config.clone()))?;
  let deleted = session.analyze_affected()?;
  assert_unused(&deleted, true);
  assert_parity(&deleted, &open(&root)?.analyze()?);

  std::fs::write(&config, "export default defineNuxtConfig({})\n")?;
  let package = layer.join("package.json");
  std::fs::write(
    &package,
    r#"{"name":"theme-kit","version":"1.0.0","exports":{"./*":"./*"},"dependencies":{"@nuxt/content":"3.0.0"}}"#,
  )?;
  session.apply_changes(ChangeSet::remove(config))?;
  session.apply_changes(ChangeSet::remove(package.clone()))?;
  assert_unused(&session.analyze_affected()?, false);
  session.apply_changes(ChangeSet::upsert(
    package.clone(),
    r#"{"name":"theme-kit","version":"1.0.0","exports":{"./*":"./*"}}"#.into(),
  ))?;
  let package_overlay = session.analyze_affected()?;
  assert_unused(&package_overlay, true);
  std::fs::write(&package, r#"{"name":"theme-kit","version":"1.0.0","exports":{"./*":"./*"}}"#)?;
  assert_parity(&package_overlay, &open(&root)?.analyze()?);
  std::fs::remove_dir_all(root)?;
  Ok(())
}
