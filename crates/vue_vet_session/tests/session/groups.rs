use super::helpers::*;
use vue_vet_session::{ProjectSession, RuleGroupId, SessionOptions};

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn selected_groups_filter_file_and_project_findings() {
  let session = open_session(fixture("projects/nuxt-graph/pages/broken.vue"));
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  assert!(
    snapshot
      .summary
      .diagnostics
      .iter()
      .any(|diagnostic| diagnostic.rule_id == "vue-vet/project/unresolved-import"),
    "default scan must emit project findings: {:?}",
    snapshot.summary.diagnostics
  );

  let tracking = ProjectSession::open(SessionOptions {
    root: fixture("projects/nuxt-graph/pages/broken.vue"),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
    selected_groups: vec![RuleGroupId::Tracking],
  })
  .unwrap_or_else(|error| panic!("open tracking: {error}"));
  let tracking_snap = tracking.analyze().unwrap_or_else(|error| panic!("tracking: {error}"));
  assert!(
    tracking_snap
      .summary
      .diagnostics
      .iter()
      .all(|diagnostic| diagnostic.rule_id != "vue-vet/project/unresolved-import"),
    "tracking group must drop project findings: {:?}",
    tracking_snap.summary.diagnostics
  );

  let project = ProjectSession::open(SessionOptions {
    root: fixture("projects/nuxt-graph/pages/broken.vue"),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
    selected_groups: vec![RuleGroupId::Project],
  })
  .unwrap_or_else(|error| panic!("open project: {error}"));
  let project_snap = project.analyze().unwrap_or_else(|error| panic!("project: {error}"));
  assert!(
    project_snap
      .summary
      .diagnostics
      .iter()
      .any(|diagnostic| diagnostic.rule_id == "vue-vet/project/unresolved-import"),
    "project group must keep unresolved-import: {:?}",
    project_snap.summary.diagnostics
  );
}
