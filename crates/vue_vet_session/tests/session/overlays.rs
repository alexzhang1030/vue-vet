use super::helpers::*;

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn analyze_with_overlays_uses_unsaved_buffer_source() {
  let root = fixture("rules/no-v-html/invalid/basic.vue");
  let Ok(session) = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  }) else {
    panic!("session must open");
  };
  let Ok(disk) = session.analyze() else {
    panic!("disk analyze must succeed");
  };
  assert!(
    disk.summary.diagnostics.iter().any(|diagnostic| diagnostic.rule_id.contains("no-v-html")),
    "disk fixture must report no-v-html"
  );

  let clean = "<template>\n  <main>{{ html }}</main>\n</template>\n";
  let mut overlays = BTreeMap::new();
  overlays.insert(root, clean.into());
  let Ok(overlay) = session.analyze_with_overlays(&overlays) else {
    panic!("overlay analyze must succeed");
  };
  assert!(
    !overlay.summary.diagnostics.iter().any(|diagnostic| diagnostic.rule_id.contains("no-v-html")),
    "unsaved buffer without v-html must clear the finding"
  );
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn resolve_workspace_path_rejects_escape() {
  let Ok(session) = ProjectSession::open(SessionOptions {
    root: fixture("rules/no-v-html/invalid"),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  }) else {
    panic!("session must open");
  };
  let Ok(inside) = session.resolve_workspace_path(std::path::Path::new("basic.vue")) else {
    panic!("inside path must resolve");
  };
  assert!(inside.ends_with("basic.vue"));
  let Err(error) = session.resolve_workspace_path(std::path::Path::new("../secret")) else {
    panic!("parent escape must fail");
  };
  assert!(error.to_string().contains("escapes"));
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn analyzes_tsx_jsx_v_html_via_template_facts() {
  let root = std::env::temp_dir().join(format!("vue-vet-jsx-vhtml-{}", std::process::id()));
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("temp workspace: {error}"));
  std::fs::write(
    root.join("Comp.tsx"),
    "import { defineComponent } from 'vue'\n\
     const html = '<b>x</b>'\n\
     export default defineComponent({\n\
       setup() { return () => <div v-html={html} /> }\n\
     })\n",
  )
  .unwrap_or_else(|error| panic!("tsx: {error}"));
  let session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  })
  .unwrap_or_else(|error| panic!("session: {error}"));
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  assert!(
    snapshot.summary.diagnostics.iter().any(|diagnostic| {
      diagnostic.file == FileId::from("Comp.tsx") && diagnostic.rule_id.contains("no-v-html")
    }),
    "tsx v-html must fire no-v-html; got {:?}",
    snapshot.summary.diagnostics
  );
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn first_discover_includes_overlay_only_unsaved_vue() {
  let root = std::env::temp_dir().join(format!("vue-vet-overlay-first-{}", std::process::id()));
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("temp workspace: {error}"));
  std::fs::write(root.join("Existing.vue"), "<template><main /></template>")
    .unwrap_or_else(|error| panic!("existing: {error}"));
  let session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  })
  .unwrap_or_else(|error| panic!("session: {error}"));
  session
    .apply_changes(ChangeSet::upsert(
      root.join("NewComponent.vue"),
      "<template><main v-html=\"html\" /></template>".into(),
    ))
    .unwrap_or_else(|error| panic!("overlay: {error}"));
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("first analyze: {error}"));
  assert!(
    snapshot.summary.diagnostics.iter().any(|diagnostic| {
      diagnostic.file == FileId::from("NewComponent.vue")
        && diagnostic.rule_id.contains("no-v-html")
    }),
    "unsaved overlay-only files must be analyzed on first discovery"
  );
  assert_eq!(session.stats().workspace_discoveries, 1);
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn file_id_for_path_matches_diagnostic_identity() {
  let root = fixture("rules/no-v-html/invalid");
  let session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  })
  .unwrap_or_else(|error| panic!("session: {error}"));
  let file_id = session
    .file_id_for_path(&root.join("basic.vue"))
    .unwrap_or_else(|error| panic!("file id: {error}"));
  assert_eq!(file_id, FileId::from("basic.vue"));
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn duplicate_suffix_paths_keep_distinct_file_ids() {
  let root = std::env::temp_dir().join(format!("vue-vet-file-id-{}", std::process::id()));
  for directory in ["apps/admin/src", "apps/customer/src"] {
    std::fs::create_dir_all(root.join(directory))
      .unwrap_or_else(|error| panic!("temp workspace: {error}"));
    std::fs::write(
      root.join(directory).join("App.vue"),
      "<template><main v-html=\"html\" /></template>",
    )
    .unwrap_or_else(|error| panic!("fixture: {error}"));
  }
  let session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(2),
  })
  .unwrap_or_else(|error| panic!("session: {error}"));
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  let files = snapshot
    .summary
    .diagnostics
    .iter()
    .filter(|diagnostic| diagnostic.rule_id.contains("no-v-html"))
    .map(|diagnostic| diagnostic.file.clone())
    .collect::<std::collections::BTreeSet<_>>();
  assert_eq!(
    files,
    [FileId::from("apps/admin/src/App.vue"), FileId::from("apps/customer/src/App.vue"),]
      .into_iter()
      .collect::<std::collections::BTreeSet<_>>()
  );
  let _ignored = std::fs::remove_dir_all(root);
}
