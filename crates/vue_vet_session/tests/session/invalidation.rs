use vue_vet_core::ModuleId;

use super::helpers::*;

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn syntax_errors_are_partial_results() {
  let root = std::env::temp_dir().join(format!("vue-vet-partial-{}", std::process::id()));
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("temp workspace: {error}"));
  std::fs::write(root.join("Good.vue"), "<template><main v-html=\"html\" /></template>")
    .unwrap_or_else(|error| panic!("good fixture: {error}"));
  std::fs::write(root.join("Broken.vue"), "<script setup>const = ;</script>")
    .unwrap_or_else(|error| panic!("broken fixture: {error}"));
  let session = open_session_threads(root.clone(), 2);
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
  let session = open_session_threads(root.clone(), 2);
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
  let session = open_session_threads(root.clone(), 2);
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
  let clean_session = open_session_threads(root.clone(), 2);
  let clean = clean_session
    .analyze_with_overlays(&BTreeMap::from([(child.clone(), edited_child.into())]))
    .unwrap_or_else(|error| panic!("clean overlay analyze: {error}"));
  assert_analysis_parity(&incremental, &clean);
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
  let session = open_session_threads(root.clone(), 2);
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

  let clean_session = open_session_threads(root.clone(), 2);
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
fn package_imports_changes_match_a_clean_analysis() {
  let root = std::env::temp_dir().join(format!("vue-vet-package-imports-{}", std::process::id()));
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
import { gate, payload } from '#state'
watchEffect(() => {
  if (!gate.value) return
  console.log(payload.value)
})
</script>",
  )
  .unwrap_or_else(|error| panic!("app fixture: {error}"));
  let package = root.join("package.json");
  std::fs::write(
    &package,
    r##"{"imports":{"#state":"./src/missing.ts"},"dependencies":{"vue":"3.5.0"}}"##,
  )
  .unwrap_or_else(|error| panic!("initial package: {error}"));
  let session = open_session_threads(root.clone(), 2);
  session.analyze().unwrap_or_else(|error| panic!("initial analyze: {error}"));

  std::fs::write(
    &package,
    r##"{"imports":{"#state":"./src/state.ts"},"dependencies":{"vue":"3.5.0"}}"##,
  )
  .unwrap_or_else(|error| panic!("updated package: {error}"));
  session
    .apply_changes(ChangeSet::remove(package))
    .unwrap_or_else(|error| panic!("package imports change: {error}"));
  let incremental =
    session.analyze_affected().unwrap_or_else(|error| panic!("incremental analyze: {error}"));
  let clean_session = open_session_threads(root.clone(), 2);
  let clean = clean_session.analyze().unwrap_or_else(|error| panic!("clean analyze: {error}"));
  assert!(
    clean.summary.diagnostics.iter().any(|diagnostic| {
      diagnostic.rule_id == "vue-vet/reactivity/no-conditional-watch-effect-dependency"
    }),
    "package imports must seed the cross-module reactivity diagnostic"
  );
  assert_analysis_parity(&incremental, &clean);
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn consecutive_context_mutations_do_not_drop_prior_epochs() {
  let root = std::env::temp_dir().join(format!("vue-vet-context-epochs-{}", std::process::id()));
  std::fs::create_dir_all(root.join("src"))
    .unwrap_or_else(|error| panic!("temp workspace: {error}"));
  std::fs::create_dir_all(root.join("components/base"))
    .unwrap_or_else(|error| panic!("component workspace: {error}"));
  std::fs::create_dir_all(root.join(".nuxt"))
    .unwrap_or_else(|error| panic!("Nuxt workspace: {error}"));
  std::fs::write(
    root.join("src/state.ts"),
    "import { ref } from 'vue'; export const gate = ref(false); export const payload = ref(0);",
  )
  .unwrap_or_else(|error| panic!("state fixture: {error}"));
  std::fs::write(
    root.join("components/base/Button.vue"),
    "<template><button>ok</button></template>",
  )
  .unwrap_or_else(|error| panic!("component fixture: {error}"));
  std::fs::write(
    root.join("App.vue"),
    "<script setup lang=\"ts\">
import { watchEffect } from 'vue'
import { gate, payload } from '@state'
watchEffect(() => {
  if (!gate.value) return
  console.log(payload.value)
})
</script>
<template><CustomButton /></template>",
  )
  .unwrap_or_else(|error| panic!("app fixture: {error}"));
  let tsconfig = root.join("tsconfig.json");
  std::fs::write(
    &tsconfig,
    r#"{"compilerOptions":{"baseUrl":".","paths":{"@state":["src/missing.ts"]}}}"#,
  )
  .unwrap_or_else(|error| panic!("initial tsconfig: {error}"));
  let declarations = root.join(".nuxt/components.d.ts");
  std::fs::write(
    &declarations,
    r"export const OtherButton: typeof import('../components/base/Button.vue')['default']",
  )
  .unwrap_or_else(|error| panic!("initial declarations: {error}"));
  let session = open_session_threads(root.clone(), 2);
  session.analyze().unwrap_or_else(|error| panic!("initial analyze: {error}"));

  std::fs::write(
    &tsconfig,
    r#"{"compilerOptions":{"baseUrl":".","paths":{"@state":["src/state.ts"]}}}"#,
  )
  .unwrap_or_else(|error| panic!("updated tsconfig: {error}"));
  session
    .apply_changes(ChangeSet::remove(tsconfig))
    .unwrap_or_else(|error| panic!("tsconfig change: {error}"));
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
  let clean_session = open_session_threads(root.clone(), 2);
  let clean = clean_session.analyze().unwrap_or_else(|error| panic!("clean analyze: {error}"));
  assert!(
    clean.summary.diagnostics.iter().any(|diagnostic| {
      diagnostic.rule_id == "vue-vet/reactivity/no-conditional-watch-effect-dependency"
    }),
    "tsconfig epoch must still be consumed after a later Nuxt mutation"
  );
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
  let session = open_session(root.clone());
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
  let clean_session = open_session(root.clone());
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
  let session = open_session(root.clone());
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
  let clean_session = open_session(root.clone());
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
  let session = open_session_threads(root.clone(), 2);
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
  let clean_session = open_session_threads(root.clone(), 2);
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
  let session = open_session(root.clone());
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
fn failed_apply_changes_preserves_revision_and_analysis() {
  let root = std::env::temp_dir().join(format!("vue-vet-tx-{}", std::process::id()));
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("temp workspace: {error}"));
  let good = root.join("Good.vue");
  let bad = root.join("bad.ts");
  std::fs::write(&good, "<template><main v-html=\"html\" /></template>")
    .unwrap_or_else(|error| panic!("good: {error}"));
  std::fs::write(&bad, "export const ok = 1;\n").unwrap_or_else(|error| panic!("bad: {error}"));
  let session = open_session(root.clone());
  let initial = session.analyze().unwrap_or_else(|error| panic!("initial: {error}"));
  let stats = session.stats();
  std::fs::write(&bad, [0xff, 0xfe, 0xfd]).unwrap_or_else(|error| panic!("invalid: {error}"));
  let Err(error) = session.apply_changes(ChangeSet {
    files: BTreeMap::from([
      (good, Some("<template><main>{{ html }}</main></template>".into())),
      (bad, None),
    ]),
  }) else {
    panic!("invalid UTF-8 refresh must fail");
  };
  assert!(error.to_string().contains("UTF-8"), "{error}");
  assert_eq!(session.stats().committed_analyses, stats.committed_analyses);
  assert_eq!(session.stats().incremental_file_updates, stats.incremental_file_updates);
  let after =
    session.analyze_affected().unwrap_or_else(|error| panic!("analyze after failure: {error}"));
  assert_analysis_parity(&after, &initial);
  assert!(
    after.summary.diagnostics.iter().any(|diagnostic| diagnostic.rule_id.contains("no-v-html")),
    "failed mutation must not install the successful overlay from the same batch"
  );
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn diagnostics_only_product_omits_graph_dto() {
  let session = open_session(fixture("rules/no-v-html/invalid"));
  let full = session.analyze().unwrap_or_else(|error| panic!("full: {error}"));
  assert!(!full.graph.nodes.is_empty() || !full.graph.module_reactivity.is_empty());
  let lean = session
    .analyze_affected_product(AnalysisProduct::DiagnosticsOnly)
    .unwrap_or_else(|error| panic!("diagnostics-only: {error}"));
  assert!(lean.graph.nodes.is_empty(), "DiagnosticsOnly must not publish nodes");
  assert!(lean.graph.edges.is_empty(), "DiagnosticsOnly must not publish edges");
  assert!(
    lean.graph.module_reactivity.is_empty(),
    "DiagnosticsOnly must not publish module reactivity DTO"
  );
  assert_eq!(lean.summary.diagnostics, full.summary.diagnostics);
  let file = full
    .summary
    .diagnostics
    .first()
    .map_or_else(|| panic!("fixture must emit diagnostics"), |diagnostic| diagnostic.file.clone());
  let by_file =
    session.diagnostics_for(&file).unwrap_or_else(|error| panic!("diagnostics_for: {error}"));
  assert!(!by_file.is_empty());
  assert!(by_file.iter().all(|diagnostic| diagnostic.file == file));
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn noop_analyze_affected_does_not_recommit() {
  let session = open_session(fixture("rules/no-v-html/invalid"));
  let first = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  let stats = session.stats();
  let second = session.analyze_affected().unwrap_or_else(|error| panic!("noop: {error}"));
  assert_eq!(session.stats().committed_analyses, stats.committed_analyses);
  assert_eq!(first.summary.diagnostics.len(), second.summary.diagnostics.len());
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn independent_leaf_edit_keeps_affected_set_local() {
  let root = std::env::temp_dir().join(format!("vue-vet-locality-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(root.join("src")).unwrap_or_else(|error| panic!("workspace: {error}"));
  for index in 0..40 {
    std::fs::write(
      root.join(format!("src/module-{index}.ts")),
      format!("export const value{index} = {index};\n"),
    )
    .unwrap_or_else(|error| panic!("module: {error}"));
  }
  let session = open_session_threads(root.clone(), 2);
  let _baseline = session.analyze().unwrap_or_else(|error| panic!("baseline: {error}"));
  session
    .apply_changes(ChangeSet::upsert(
      root.join("src/module-39.ts"),
      "export const value39 = 3900;\n".into(),
    ))
    .unwrap_or_else(|error| panic!("edit: {error}"));
  let after = session.analyze_affected().unwrap_or_else(|error| panic!("affected: {error}"));
  let affected = session.affected_files().unwrap_or_else(|error| panic!("affected files: {error}"));
  assert!(
    affected.len() <= 4,
    "independent leaf edit must not mark the whole workspace dirty, got {}",
    affected.len()
  );
  assert!(
    affected.iter().any(|file| file.as_str().contains("module-39")),
    "edited leaf must be among affected files"
  );
  assert_eq!(
    after.work.files_parsed, 1,
    "independent leaf edit must parse only the edited file, got {:?}",
    after.work
  );
  assert!(after.work.files_reused >= 39, "unchanged modules must be reused, got {:?}", after.work);
  assert!(
    !after.work.export_resolve_ran,
    "unseeded leaf body edit must not rerun export resolve: {:?}",
    after.work
  );
  assert_eq!(
    after.work.seed_plans_recomputed, 0,
    "unseeded leaf body edit must not recompute seed plans: {:?}",
    after.work
  );
  assert_eq!(
    after.work.seeded_reparses, 0,
    "unseeded leaf body edit must not reparse seeded consumers: {:?}",
    after.work
  );
  let plan = session.last_dirty_plan().unwrap_or_else(|error| panic!("dirty plan: {error}"));
  assert_eq!(plan.parse_files.len(), 1);
  assert!(
    plan.export_closure.is_empty(),
    "export_closure is the A6 seed-dirty set, not every module summary: {:?}",
    plan.export_closure
  );
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn producer_export_change_limits_export_closure() {
  let root = std::env::temp_dir().join(format!("vue-vet-export-closure-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  std::fs::write(
    root.join("producer.ts"),
    "import { ref } from 'vue'; export const count = ref(0);\n",
  )
  .unwrap_or_else(|error| panic!("producer: {error}"));
  std::fs::write(
    root.join("consumer.ts"),
    "import { watchEffect } from 'vue'; import { count } from './producer'; watchEffect(() => count.value);\n",
  )
  .unwrap_or_else(|error| panic!("consumer: {error}"));
  std::fs::write(
    root.join("unrelated.ts"),
    "import { ref } from 'vue'; export const other = ref(1);\n",
  )
  .unwrap_or_else(|error| panic!("unrelated: {error}"));
  let session = open_session_threads(root.clone(), 2);
  session.analyze().unwrap_or_else(|error| panic!("baseline: {error}"));
  session
    .apply_changes(ChangeSet::upsert(
      root.join("producer.ts"),
      "import { ref } from 'vue'; export const count = ref(0); export const flag = ref(true);\n"
        .into(),
    ))
    .unwrap_or_else(|error| panic!("edit: {error}"));
  let after = session.analyze_affected().unwrap_or_else(|error| panic!("affected: {error}"));
  assert!(
    after.work.export_resolve_ran,
    "new named export must rerun export resolve: {:?}",
    after.work
  );
  assert_eq!(after.work.seed_plans_recomputed, 2, "producer + consumer only: {:?}", after.work);
  let plan = session.last_dirty_plan().unwrap_or_else(|error| panic!("dirty plan: {error}"));
  assert!(
    plan.export_closure.contains(&ModuleId::from("producer.ts"))
      && plan.export_closure.contains(&ModuleId::from("consumer.ts"))
      && !plan.export_closure.iter().any(|id| id.as_str().contains("unrelated")),
    "export_closure is the seed-dirty pair, not the whole workspace: {:?}",
    plan.export_closure
  );
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn tsconfig_only_change_parses_zero_source_files() {
  let root =
    std::env::temp_dir().join(format!("vue-vet-tsconfig-parse-zero-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(root.join("src")).unwrap_or_else(|error| panic!("workspace: {error}"));
  std::fs::write(
    root.join("src/state.ts"),
    "import { ref } from 'vue'; export const gate = ref(false); export const payload = ref(0);",
  )
  .unwrap_or_else(|error| panic!("state: {error}"));
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
  .unwrap_or_else(|error| panic!("app: {error}"));
  let tsconfig = root.join("tsconfig.json");
  std::fs::write(
    &tsconfig,
    r#"{"compilerOptions":{"baseUrl":".","paths":{"@state":["src/missing.ts"]}}}"#,
  )
  .unwrap_or_else(|error| panic!("tsconfig: {error}"));
  let session = open_session_threads(root.clone(), 2);
  session.analyze().unwrap_or_else(|error| panic!("baseline: {error}"));

  std::fs::write(
    &tsconfig,
    r#"{"compilerOptions":{"baseUrl":".","paths":{"@state":["src/state.ts"]}}}"#,
  )
  .unwrap_or_else(|error| panic!("updated tsconfig: {error}"));
  session
    .apply_changes(ChangeSet::remove(tsconfig))
    .unwrap_or_else(|error| panic!("tsconfig change: {error}"));
  let incremental =
    session.analyze_affected().unwrap_or_else(|error| panic!("incremental: {error}"));
  assert_eq!(
    incremental.work.files_parsed, 0,
    "tsconfig-only invalidation must not re-parse unchanged sources: {:?}",
    incremental.work
  );
  assert!(
    incremental.work.files_reused >= 2,
    "sources must be reused after tsconfig change: {:?}",
    incremental.work
  );

  let clean_session = open_session_threads(root.clone(), 2);
  let clean = clean_session.analyze().unwrap_or_else(|error| panic!("clean: {error}"));
  assert_analysis_parity(&incremental, &clean);
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn package_json_change_parses_zero_source_files() {
  let root =
    std::env::temp_dir().join(format!("vue-vet-package-parse-zero-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  std::fs::write(
    root.join("App.vue"),
    "<script setup>\nimport { ref } from 'vue'\nconst n = ref(0)\n</script><template>{{ n }}</template>",
  )
  .unwrap_or_else(|error| panic!("app: {error}"));
  let package = root.join("package.json");
  std::fs::write(&package, r#"{"dependencies":{"vue":"^3.4.0","lodash":"^4.17.21"}}"#)
    .unwrap_or_else(|error| panic!("package: {error}"));
  let session = open_session(root.clone());
  session.analyze().unwrap_or_else(|error| panic!("baseline: {error}"));

  std::fs::write(&package, r#"{"dependencies":{"vue":"^3.4.0","lodash-es":"^4.17.21"}}"#)
    .unwrap_or_else(|error| panic!("updated package: {error}"));
  session
    .apply_changes(ChangeSet::remove(package))
    .unwrap_or_else(|error| panic!("package change: {error}"));
  let incremental =
    session.analyze_affected().unwrap_or_else(|error| panic!("incremental: {error}"));
  assert_eq!(
    incremental.work.files_parsed, 0,
    "package.json change must refresh environment/resolution without re-parse: {:?}",
    incremental.work
  );

  let clean_session = open_session(root.clone());
  let clean = clean_session.analyze().unwrap_or_else(|error| panic!("clean: {error}"));
  assert_analysis_parity(&incremental, &clean);
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn warm_disk_cache_hit_stays_cheap_and_first_edit_seeds_ir() {
  let root =
    std::env::temp_dir().join(format!("vue-vet-warm-cache-lazy-ir-{}", std::process::id()));
  let cache_dir = root.join(".vue-vet-cache");
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(root.join("src")).unwrap_or_else(|error| panic!("workspace: {error}"));
  for index in 0..12 {
    std::fs::write(
      root.join(format!("src/module-{index}.ts")),
      format!("export const value{index} = {index};\n"),
    )
    .unwrap_or_else(|error| panic!("module: {error}"));
  }
  let cold = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: Some(cache_dir.clone()),
    no_cache: false,
    threads: Some(2),
  })
  .unwrap_or_else(|error| panic!("cold session: {error}"));
  let cold_snap = cold.analyze().unwrap_or_else(|error| panic!("cold analyze: {error}"));
  assert_eq!(cold_snap.cache_status, "miss");

  let warm = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: Some(cache_dir),
    no_cache: false,
    threads: Some(2),
  })
  .unwrap_or_else(|error| panic!("warm session: {error}"));
  let warm_snap = warm.analyze().unwrap_or_else(|error| panic!("warm analyze: {error}"));
  assert_eq!(warm_snap.cache_status, "hit");
  assert_eq!(warm_snap.summary, cold_snap.summary);
  assert_eq!(
    warm_snap.work.files_parsed, 0,
    "warm disk hit must not re-scan sources: {:?}",
    warm_snap.work
  );

  // First dirty analyze after a cache-only open has no file facts, so it pays
  // one full parse to seed IR (overlays also disable disk cache).
  warm
    .apply_changes(ChangeSet::upsert(
      root.join("src/module-11.ts"),
      "export const value11 = 1100;\n".into(),
    ))
    .unwrap_or_else(|error| panic!("edit: {error}"));
  let first_edit = warm.analyze_affected().unwrap_or_else(|error| panic!("first edit: {error}"));
  assert!(
    first_edit.work.files_parsed >= 12,
    "empty IR after warm hit must seed facts on first dirty analyze: {:?}",
    first_edit.work
  );

  warm
    .apply_changes(ChangeSet::upsert(
      root.join("src/module-11.ts"),
      "export const value11 = 1101;\n".into(),
    ))
    .unwrap_or_else(|error| panic!("second edit: {error}"));
  let second_edit = warm.analyze_affected().unwrap_or_else(|error| panic!("second edit: {error}"));
  assert_eq!(
    second_edit.work.files_parsed, 1,
    "after IR is seeded, leaf edits must parse only the edited file: {:?}",
    second_edit.work
  );
  assert!(
    second_edit.work.files_reused >= 11,
    "unchanged modules must be reused: {:?}",
    second_edit.work
  );
  let _ignored = std::fs::remove_dir_all(root);
}
