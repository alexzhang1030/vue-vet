use std::{collections::BTreeMap, path::PathBuf};

use vue_vet_core::{FileId, finding_id};
use vue_vet_session::{AnalysisSnapshot, ChangeSet, ProjectSession, SessionOptions};

fn fixture(name: &str) -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

fn assert_analysis_parity(incremental: &AnalysisSnapshot, clean: &AnalysisSnapshot) {
  assert_eq!(incremental.summary, clean.summary, "incremental diagnostics must equal clean");
  assert_eq!(incremental.graph, clean.graph, "incremental graph must equal clean");
  assert_eq!(incremental.coverage, clean.coverage, "incremental coverage must equal clean");
  assert_eq!(incremental.issues, clean.issues, "incremental issues must equal clean");
  assert_eq!(
    incremental.analyzed_files, clean.analyzed_files,
    "incremental analyzed file identities must equal clean"
  );
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn analyze_diagnostic_ids_match_report_identity() {
  let root = fixture("rules/no-v-html/invalid/basic.vue");
  let Ok(session) = ProjectSession::open(SessionOptions {
    root,
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  }) else {
    panic!("session must open");
  };
  let Ok(snapshot) = session.analyze() else {
    panic!("analyze must succeed");
  };
  assert!(!snapshot.summary.diagnostics.is_empty(), "fixture must emit at least one finding");
  for diagnostic in &snapshot.summary.diagnostics {
    let id = finding_id(diagnostic);
    assert!(id.contains("vue-vet/security/no-v-html"), "opaque id must include rule: {id}");
    let Ok(explained) = session.explain_finding(&id) else {
      panic!("finding explain must match analyze");
    };
    assert_eq!(explained.id, id);
    assert!(explained.message.contains("v-html"));
  }
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn explain_rule_loads_documentation_without_scan_diagnostics() {
  let Ok(session) = ProjectSession::open(SessionOptions {
    root: fixture("rules/no-v-html/invalid/basic.vue"),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  }) else {
    panic!("session must open");
  };
  let Ok(explain) = session.explain_rule("vue-vet/security/no-v-html") else {
    panic!("rule explain");
  };
  assert_eq!(explain.rule_id, "vue-vet/security/no-v-html");
  assert!(explain.body.as_deref().is_some_and(|body| body.contains("v-html")));
}

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
fn syntax_errors_are_partial_results() {
  let root = std::env::temp_dir().join(format!("vue-vet-partial-{}", std::process::id()));
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("temp workspace: {error}"));
  std::fs::write(root.join("Good.vue"), "<template><main v-html=\"html\" /></template>")
    .unwrap_or_else(|error| panic!("good fixture: {error}"));
  std::fs::write(root.join("Broken.vue"), "<script setup>const = ;</script>")
    .unwrap_or_else(|error| panic!("broken fixture: {error}"));
  let session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(2),
  })
  .unwrap_or_else(|error| panic!("session: {error}"));
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("partial analyze: {error}"));
  assert!(!snapshot.complete());
  assert_eq!(snapshot.issues.len(), 1);
  assert!(snapshot.summary.diagnostics.iter().any(|diagnostic| {
    diagnostic.file == FileId::from("Good.vue") && diagnostic.rule_id.contains("no-v-html")
  }));
  assert!(snapshot.summary.diagnostics.iter().any(|diagnostic| {
    diagnostic.file == FileId::from("Broken.vue")
      && diagnostic.rule_id == "vue-vet/analysis/parse-error"
  }));
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn broken_module_keeps_healthy_cross_module_reactivity() {
  let root = std::env::temp_dir().join(format!("vue-vet-partial-modules-{}", std::process::id()));
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("temp workspace: {error}"));
  std::fs::write(
    root.join("producer.ts"),
    "import { ref } from 'vue'; export const count = ref(0);",
  )
  .unwrap_or_else(|error| panic!("producer fixture: {error}"));
  std::fs::write(
    root.join("consumer.ts"),
    "import { watchEffect } from 'vue'; import { count } from './producer'; watchEffect(() => count.value);",
  )
  .unwrap_or_else(|error| panic!("consumer fixture: {error}"));
  std::fs::write(root.join("broken.ts"), "const = ;")
    .unwrap_or_else(|error| panic!("broken fixture: {error}"));
  let session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(2),
  })
  .unwrap_or_else(|error| panic!("session: {error}"));
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("partial analyze: {error}"));
  assert!(!snapshot.complete());
  assert!(snapshot.issues.iter().any(|issue| issue.file == Some(FileId::from("broken.ts"))));
  let consumer = snapshot.graph.module_reactivity.iter().find(|module| module.id == "consumer.ts");
  assert!(
    consumer.is_some_and(|module| {
      module
        .graph
        .effects
        .iter()
        .any(|effect| effect.reads.iter().any(|read| read.binding == "count"))
    }),
    "a broken third module must not erase the healthy producer → consumer seed"
  );
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn affected_analysis_reuses_facts_and_invalidates_reverse_dependencies() {
  let root = std::env::temp_dir().join(format!("vue-vet-incremental-{}", std::process::id()));
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("temp workspace: {error}"));
  let child = root.join("Child.vue");
  std::fs::write(&child, "<template><span>one</span></template>")
    .unwrap_or_else(|error| panic!("child fixture: {error}"));
  std::fs::write(
    root.join("App.vue"),
    "<script setup>import Child from './Child.vue'</script><template><Child /></template>",
  )
  .unwrap_or_else(|error| panic!("app fixture: {error}"));
  let session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(2),
  })
  .unwrap_or_else(|error| panic!("session: {error}"));
  session.analyze().unwrap_or_else(|error| panic!("initial analyze: {error}"));
  assert_eq!(session.stats().workspace_discoveries, 1);
  let edited_child = "<template><span>two</span></template>";
  session
    .apply_changes(ChangeSet::upsert(child.clone(), edited_child.into()))
    .unwrap_or_else(|error| panic!("apply change: {error}"));
  let incremental =
    session.analyze_affected().unwrap_or_else(|error| panic!("affected analyze: {error}"));
  let affected = session.affected_files().unwrap_or_else(|error| panic!("affected files: {error}"));
  assert!(affected.contains(&FileId::from("Child.vue")));
  assert!(affected.contains(&FileId::from("App.vue")));
  assert_eq!(
    session.stats().workspace_discoveries,
    1,
    "an affected edit must update the retained source snapshot without a second workspace walk"
  );
  assert_eq!(session.stats().incremental_file_updates, 1);
  let clean_session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(2),
  })
  .unwrap_or_else(|error| panic!("clean session: {error}"));
  let clean = clean_session
    .analyze_with_overlays(&BTreeMap::from([(child.clone(), edited_child.into())]))
    .unwrap_or_else(|error| panic!("clean overlay analyze: {error}"));
  assert_eq!(incremental.summary, clean.summary);
  assert_eq!(incremental.graph, clean.graph);
  std::fs::remove_file(&child).unwrap_or_else(|error| panic!("remove child fixture: {error}"));
  session
    .apply_changes(ChangeSet::remove(child))
    .unwrap_or_else(|error| panic!("remove overlay: {error}"));
  session.analyze_affected().unwrap_or_else(|error| panic!("removed-file analyze: {error}"));
  let affected = session.affected_files().unwrap_or_else(|error| panic!("affected files: {error}"));
  assert!(affected.contains(&FileId::from("Child.vue")));
  assert!(affected.contains(&FileId::from("App.vue")));
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn tsconfig_path_changes_match_a_clean_analysis() {
  let root = std::env::temp_dir().join(format!("vue-vet-tsconfig-context-{}", std::process::id()));
  std::fs::create_dir_all(root.join("src"))
    .unwrap_or_else(|error| panic!("temp workspace: {error}"));
  std::fs::write(
    root.join("src/state.ts"),
    "import { ref } from 'vue'; export const gate = ref(false); export const payload = ref(0);",
  )
  .unwrap_or_else(|error| panic!("state fixture: {error}"));
  std::fs::write(
    root.join("App.vue"),
    "<script setup lang=\"ts\">
import { watchEffect } from 'vue'
import { gate, payload } from '@state'
watchEffect(() => {
  if (!gate.value) return
  console.log(payload.value)
})
</script>",
  )
  .unwrap_or_else(|error| panic!("app fixture: {error}"));
  let tsconfig = root.join("tsconfig.json");
  std::fs::write(
    &tsconfig,
    r#"{"compilerOptions":{"baseUrl":".","paths":{"@state":["src/missing.ts"]}}}"#,
  )
  .unwrap_or_else(|error| panic!("initial tsconfig: {error}"));
  let session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(2),
  })
  .unwrap_or_else(|error| panic!("session: {error}"));
  session.analyze().unwrap_or_else(|error| panic!("initial analyze: {error}"));

  std::fs::write(
    &tsconfig,
    r#"{"compilerOptions":{"baseUrl":".","paths":{"@state":["src/state.ts"]}}}"#,
  )
  .unwrap_or_else(|error| panic!("updated tsconfig: {error}"));
  session
    .apply_changes(ChangeSet::remove(tsconfig))
    .unwrap_or_else(|error| panic!("tsconfig change: {error}"));
  let incremental =
    session.analyze_affected().unwrap_or_else(|error| panic!("incremental analyze: {error}"));

  let clean_session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(2),
  })
  .unwrap_or_else(|error| panic!("clean session: {error}"));
  let clean = clean_session.analyze().unwrap_or_else(|error| panic!("clean analyze: {error}"));
  assert!(
    clean.summary.diagnostics.iter().any(|diagnostic| {
      diagnostic.rule_id == "vue-vet/reactivity/no-conditional-watch-effect-dependency"
    }),
    "the resolved alias must seed the cross-module reactivity diagnostic"
  );
  let affected = session.affected_files().unwrap_or_else(|error| panic!("affected files: {error}"));
  assert_eq!(affected, [FileId::from("App.vue"), FileId::from("src/state.ts")]);
  assert_analysis_parity(&incremental, &clean);
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn package_environment_changes_match_a_clean_analysis() {
  let root = std::env::temp_dir().join(format!("vue-vet-package-context-{}", std::process::id()));
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("temp workspace: {error}"));
  let package = root.join("package.json");
  std::fs::write(&package, r#"{"dependencies":{"vue":"3.4.0"}}"#)
    .unwrap_or_else(|error| panic!("initial package: {error}"));
  std::fs::write(
    root.join("App.vue"),
    "<script setup lang=\"ts\">
const { title } = defineProps<{ title: string }>()
console.log(title)
</script>",
  )
  .unwrap_or_else(|error| panic!("app fixture: {error}"));
  let session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  })
  .unwrap_or_else(|error| panic!("session: {error}"));
  let initial = session.analyze().unwrap_or_else(|error| panic!("initial analyze: {error}"));
  assert!(initial.summary.diagnostics.iter().any(|diagnostic| {
    diagnostic.rule_id == "vue-vet/reactivity/no-nonreactive-props-destructure"
  }));

  std::fs::write(&package, r#"{"dependencies":{"vue":"3.5.0"}}"#)
    .unwrap_or_else(|error| panic!("updated package: {error}"));
  session
    .apply_changes(ChangeSet::remove(package))
    .unwrap_or_else(|error| panic!("package change: {error}"));
  let incremental =
    session.analyze_affected().unwrap_or_else(|error| panic!("incremental analyze: {error}"));
  let clean_session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  })
  .unwrap_or_else(|error| panic!("clean session: {error}"));
  let clean = clean_session.analyze().unwrap_or_else(|error| panic!("clean analyze: {error}"));
  assert!(
    clean
      .summary
      .diagnostics
      .iter()
      .all(|diagnostic| diagnostic.rule_id
        != "vue-vet/reactivity/no-nonreactive-props-destructure"),
    "Vue 3.5 must clear the pre-3.5 props destructure diagnostic"
  );
  assert_analysis_parity(&incremental, &clean);
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn lockfile_changes_invalidate_consumers_and_match_a_clean_analysis() {
  let root = std::env::temp_dir().join(format!("vue-vet-lockfile-context-{}", std::process::id()));
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("temp workspace: {error}"));
  let lockfile = root.join("pnpm-lock.yaml");
  std::fs::write(&lockfile, "lockfileVersion: '9.0'\n")
    .unwrap_or_else(|error| panic!("initial lockfile: {error}"));
  std::fs::write(root.join("App.vue"), "<template><main v-html=\"html\" /></template>")
    .unwrap_or_else(|error| panic!("app fixture: {error}"));
  let session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  })
  .unwrap_or_else(|error| panic!("session: {error}"));
  session.analyze().unwrap_or_else(|error| panic!("initial analyze: {error}"));

  std::fs::write(&lockfile, "lockfileVersion: '9.0'\nsnapshots: {}\n")
    .unwrap_or_else(|error| panic!("updated lockfile: {error}"));
  session
    .apply_changes(ChangeSet::remove(lockfile))
    .unwrap_or_else(|error| panic!("lockfile change: {error}"));
  let incremental =
    session.analyze_affected().unwrap_or_else(|error| panic!("incremental analyze: {error}"));
  let affected = session.affected_files().unwrap_or_else(|error| panic!("affected files: {error}"));
  assert_eq!(affected, [FileId::from("App.vue")]);
  let clean_session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  })
  .unwrap_or_else(|error| panic!("clean session: {error}"));
  let clean = clean_session.analyze().unwrap_or_else(|error| panic!("clean analyze: {error}"));
  assert_analysis_parity(&incremental, &clean);
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn nuxt_component_declaration_changes_match_a_clean_analysis() {
  let root = std::env::temp_dir().join(format!("vue-vet-nuxt-parity-{}", std::process::id()));
  std::fs::create_dir_all(root.join("components/base"))
    .unwrap_or_else(|error| panic!("component workspace: {error}"));
  std::fs::create_dir_all(root.join(".nuxt"))
    .unwrap_or_else(|error| panic!("Nuxt workspace: {error}"));
  std::fs::write(
    root.join("components/base/Button.vue"),
    "<template><button>ok</button></template>",
  )
  .unwrap_or_else(|error| panic!("component fixture: {error}"));
  std::fs::write(root.join("App.vue"), "<template><CustomButton /></template>")
    .unwrap_or_else(|error| panic!("app fixture: {error}"));
  let declarations = root.join(".nuxt/components.d.ts");
  std::fs::write(
    &declarations,
    r"export const OtherButton: typeof import('../components/base/Button.vue')['default']",
  )
  .unwrap_or_else(|error| panic!("initial declarations: {error}"));
  let session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(2),
  })
  .unwrap_or_else(|error| panic!("session: {error}"));
  session.analyze().unwrap_or_else(|error| panic!("initial analyze: {error}"));

  std::fs::write(
    &declarations,
    r"export const CustomButton: typeof import('../components/base/Button.vue')['default']",
  )
  .unwrap_or_else(|error| panic!("updated declarations: {error}"));
  session
    .apply_changes(ChangeSet::remove(declarations))
    .unwrap_or_else(|error| panic!("declaration change: {error}"));
  let incremental =
    session.analyze_affected().unwrap_or_else(|error| panic!("incremental analyze: {error}"));
  let affected = session.affected_files().unwrap_or_else(|error| panic!("affected files: {error}"));
  assert_eq!(affected, [FileId::from("App.vue"), FileId::from("components/base/Button.vue")]);
  assert!(
    !incremental.coverage.analyzed_source_files.contains(&FileId::from(".nuxt/components.d.ts")),
    "generated declarations must remain resolver inputs rather than source modules"
  );
  let clean_session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(2),
  })
  .unwrap_or_else(|error| panic!("clean session: {error}"));
  let clean = clean_session.analyze().unwrap_or_else(|error| panic!("clean analyze: {error}"));
  assert_analysis_parity(&incremental, &clean);
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn generated_nuxt_declarations_invalidate_resolution_without_becoming_sources() {
  let root = std::env::temp_dir().join(format!("vue-vet-nuxt-context-{}", std::process::id()));
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("temp workspace: {error}"));
  std::fs::write(root.join("App.vue"), "<template><main /></template>")
    .unwrap_or_else(|error| panic!("fixture: {error}"));
  let session = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  })
  .unwrap_or_else(|error| panic!("session: {error}"));
  session.analyze().unwrap_or_else(|error| panic!("initial analyze: {error}"));
  session
    .apply_changes(ChangeSet::upsert(
      root.join(".nuxt/components.d.ts"),
      "export const NuxtLink: typeof import('#components')['NuxtLink'];".into(),
    ))
    .unwrap_or_else(|error| panic!("generated declaration change: {error}"));
  let snapshot =
    session.analyze_affected().unwrap_or_else(|error| panic!("affected analyze: {error}"));
  assert!(
    !snapshot.coverage.analyzed_source_files.contains(&FileId::from(".nuxt/components.d.ts")),
    "generated Nuxt declarations are resolver inputs, not analyzed source files"
  );
  assert!(
    snapshot.coverage.invalidation_inputs.iter().any(|input| input == ".nuxt/components.d.ts"),
    "generated Nuxt declarations must still invalidate project resolution"
  );
  let _ignored = std::fs::remove_dir_all(root);
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
