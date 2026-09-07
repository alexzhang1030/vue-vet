use vue_vet_core::TrackingScopeKind;
use vue_vet_project::PROJECT_RULE_IDS;

use super::helpers::*;

const SIDE_EFFECT: &str = "import { computed, ref } from 'vue'\n\
const count=ref(0)\n\
const result=computed(()=>{count.value=1; return count.value})\n\
export { result }\n";

const SAFE_EXPORTS: &str = "import { computed, ref } from 'vue'\n\
export const count = ref(1)\n\
export const doubled = computed(() => count.value * 2)\n\
export function useCount() { const local = ref(1); return { local } }\n";

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn lost_notification_rules_run_on_ts_and_vue() {
  let source = "import { shallowRef, watchSyncEffect } from 'vue'\n\
const state = shallowRef({ count: 1 })\n\
watchSyncEffect(() => { void state.value.count })\n\
state.value.count = 2\n";
  for (name, body) in [
    ("lost.ts", source.to_string()),
    ("Lost.vue", format!("<script setup lang=\"ts\">\n{source}</script>\n<template></template>\n")),
  ] {
    let root = std::env::temp_dir().join(format!("vue-vet-lost-{name}-{}", std::process::id()));
    let _ignored = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
    std::fs::write(root.join(name), &body).unwrap_or_else(|error| panic!("write {name}: {error}"));
    let session = open_session_threads(root.clone(), 1);
    let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze {name}: {error}"));
    assert!(
      snapshot.summary.diagnostics.iter().any(|diagnostic| {
        diagnostic.file == FileId::from(name)
          && diagnostic.rule_id == "vue-vet/reactivity/no-lost-shallow-nested-notification"
      }),
      "{name} must report lost shallow notification; {:?}",
      snapshot.summary.diagnostics
    );
    let _ignored = std::fs::remove_dir_all(root);
  }
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn js_ts_side_effect_in_computed_is_diagnosed() {
  for extension in ["js", "ts"] {
    let root =
      std::env::temp_dir().join(format!("vue-vet-p4-side-{extension}-{}", std::process::id()));
    let _ignored = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
    let relative = format!("App.{extension}");
    std::fs::write(root.join(&relative), SIDE_EFFECT)
      .unwrap_or_else(|error| panic!("write {relative}: {error}"));
    let session = open_session_threads(root.clone(), 1);
    let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze {extension}: {error}"));
    assert!(
      snapshot.summary.diagnostics.iter().any(|diagnostic| {
        diagnostic.file == FileId::from(relative.as_str())
          && (diagnostic.rule_id == "vue-vet/reactivity/no-side-effects-in-computed"
            || diagnostic.rule_id == "vue-vet/reactivity/no-computed-self-trigger")
      }),
      "{extension} computed side-effect must run file rules; {:?}",
      snapshot.summary.diagnostics
    );
    let _ignored = std::fs::remove_dir_all(root);
  }
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn exported_js_ts_jsx_tsx_apis_are_not_unused() {
  for extension in ["js", "ts", "jsx", "tsx"] {
    let root =
      std::env::temp_dir().join(format!("vue-vet-p4-export-{extension}-{}", std::process::id()));
    let _ignored = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
    let relative = format!("useCount.{extension}");
    std::fs::write(root.join(&relative), SAFE_EXPORTS)
      .unwrap_or_else(|error| panic!("write {relative}: {error}"));
    let session = open_session_threads(root.clone(), 1);
    let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze {extension}: {error}"));
    let unused = snapshot
      .summary
      .diagnostics
      .iter()
      .filter(|diagnostic| {
        diagnostic.rule_id.contains("unused") && diagnostic.rule_id.contains("binding")
      })
      .collect::<Vec<_>>();
    assert!(unused.is_empty(), "{extension} exported APIs must stay unused-safe; {unused:?}");
    let _ignored = std::fs::remove_dir_all(root);
  }
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn seeded_readonly_mutation_is_diagnosed_on_plain_ts() {
  let root = std::env::temp_dir().join(format!("vue-vet-p4-seed-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  std::fs::write(
    root.join("state.ts"),
    "import { readonly } from 'vue'\nexport const state = readonly({ count: 0 })\n",
  )
  .unwrap_or_else(|error| panic!("state: {error}"));
  std::fs::write(root.join("consumer.ts"), "import { state } from './state'\nstate.count = 2\n")
    .unwrap_or_else(|error| panic!("consumer: {error}"));
  std::fs::write(root.join("control.tsx"), "import { state } from './state'\nstate.count = 2\n")
    .unwrap_or_else(|error| panic!("control: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  let rule = "vue-vet/reactivity/no-readonly-mutation";
  assert!(
    snapshot.summary.diagnostics.iter().any(|diagnostic| {
      diagnostic.file == FileId::from("consumer.ts") && diagnostic.rule_id == rule
    }),
    "seed-only .ts consumer must run file rules; {:?}",
    snapshot.summary.diagnostics
  );
  assert!(
    snapshot.summary.diagnostics.iter().any(|diagnostic| {
      diagnostic.file == FileId::from("control.tsx") && diagnostic.rule_id == rule
    }),
    "control.tsx must keep the readonly mutation diagnostic; {:?}",
    snapshot.summary.diagnostics
  );
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn auto_imported_exported_ref_operand_is_diagnosed() {
  let root =
    std::env::temp_dir().join(format!("vue-vet-autoimport-operand-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  std::fs::write(
    root.join("state.ts"),
    "import { computed, ref } from 'vue'\n\
export const currentUser = ref(false)\n\
export const label = computed(() => 'x')\n",
  )
  .unwrap_or_else(|error| panic!("state: {error}"));
  std::fs::write(
    root.join("auto-imports.d.ts"),
    "export {}\n\
declare global {\n\
  const currentUser: typeof import('./state')['currentUser']\n\
  const label: typeof import('./state')['label']\n\
}\n",
  )
  .unwrap_or_else(|error| panic!("auto-imports: {error}"));
  std::fs::write(
    root.join("Consumer.vue"),
    "<script setup lang=\"ts\">\n\
import { watchEffect } from 'vue'\n\
watchEffect(() => console.log(currentUser.value, label.value))\n\
const broken = !currentUser\n\
const tagged = label && 'x'\n\
</script>\n\
<template><div>{{ broken }}{{ tagged }}</div></template>\n",
  )
  .unwrap_or_else(|error| panic!("consumer: {error}"));
  std::fs::write(
    root.join("Nested.vue"),
    "<script setup lang=\"ts\">\n\
function nested() {\n\
  const currentUser = false\n\
  const label = 'local'\n\
  return !currentUser && label.length > 0\n\
}\n\
void nested\n\
</script>\n\
<template><div /></template>\n",
  )
  .unwrap_or_else(|error| panic!("nested: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  let operand: Vec<_> = snapshot
    .summary
    .diagnostics
    .iter()
    .filter(|diagnostic| {
      diagnostic.file == FileId::from("Consumer.vue")
        && matches!(
          diagnostic.rule_id.as_str(),
          "vue-vet/reactivity/no-ref-as-operand" | "vue-vet/reactivity/no-computed-as-operand"
        )
    })
    .collect();
  assert!(
    operand.iter().any(|diagnostic| {
      diagnostic.rule_id == "vue-vet/reactivity/no-ref-as-operand"
        && diagnostic.message.contains("`currentUser`")
    }),
    "auto-imported exported ref operand must report; {operand:?} all={:?}",
    snapshot.summary.diagnostics
  );
  assert!(
    operand.iter().any(|diagnostic| {
      diagnostic.rule_id == "vue-vet/reactivity/no-computed-as-operand"
        && diagnostic.message.contains("`label`")
    }),
    "auto-imported exported computed operand must report; {operand:?}"
  );
  assert!(
    snapshot.summary.diagnostics.iter().all(|diagnostic| {
      diagnostic.file != FileId::from("Nested.vue")
        || !matches!(
          diagnostic.rule_id.as_str(),
          "vue-vet/reactivity/no-ref-as-operand" | "vue-vet/reactivity/no-computed-as-operand"
        )
    }),
    "nested same-name locals must not inherit auto-import seeds; {:?}",
    snapshot.summary.diagnostics
  );
  assert_eq!(operand.len(), 2, "only the two auto-imported operands must report; {operand:?}");
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn call_only_unref_picks_up_package_vue_version_refresh() {
  let root = std::env::temp_dir().join(format!("vue-vet-p4-unref-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  std::fs::write(
    root.join("vue-vet.toml"),
    "version = 1\npreset = \"recommended\"\npractice = \"on\"\n",
  )
  .unwrap_or_else(|error| panic!("config: {error}"));
  std::fs::write(
    root.join("unwrap.ts"),
    "import { unref } from 'vue'\nexport function unwrap(x){return unref(() => x)}\n",
  )
  .unwrap_or_else(|error| panic!("unwrap: {error}"));
  let package = root.join("package.json");
  std::fs::write(&package, r#"{"dependencies":{"vue":"3.2.0"}}"#)
    .unwrap_or_else(|error| panic!("package 3.2: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let initial = session.analyze().unwrap_or_else(|error| panic!("initial: {error}"));
  assert!(
    initial
      .summary
      .diagnostics
      .iter()
      .all(|diagnostic| { diagnostic.rule_id != "vue-vet/practice/prefer-to-value" }),
    "Vue 3.2 must stay quiet for prefer-to-value; {:?}",
    initial.summary.diagnostics
  );
  std::fs::write(&package, r#"{"dependencies":{"vue":"3.5.40"}}"#)
    .unwrap_or_else(|error| panic!("package 3.5: {error}"));
  session
    .apply_changes(ChangeSet::remove(package))
    .unwrap_or_else(|error| panic!("package refresh: {error}"));
  let incremental =
    session.analyze_affected().unwrap_or_else(|error| panic!("incremental: {error}"));
  let cold =
    open_session_threads(root.clone(), 1).analyze().unwrap_or_else(|error| panic!("cold: {error}"));
  assert!(
    incremental.summary.diagnostics.iter().any(|diagnostic| {
      diagnostic.file == FileId::from("unwrap.ts")
        && diagnostic.rule_id == "vue-vet/practice/prefer-to-value"
    }),
    "Vue 3.5 package refresh must run call-only practice rules; {:?}",
    incremental.summary.diagnostics
  );
  assert_analysis_parity(&incremental, &cold);
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn package_json_add_replace_remove_matches_clean_scan() {
  let root = std::env::temp_dir().join(format!("vue-vet-package-lifecycle-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  let demo = root.join("apps").join("demo");
  std::fs::create_dir_all(&demo).unwrap_or_else(|error| panic!("workspace: {error}"));
  std::fs::write(
    root.join("vue-vet.toml"),
    "version = 1\npreset = \"recommended\"\npractice = \"on\"\n",
  )
  .unwrap_or_else(|error| panic!("config: {error}"));
  std::fs::write(root.join("package.json"), r#"{"dependencies":{"vue":"3.2.0"}}"#)
    .unwrap_or_else(|error| panic!("root package: {error}"));
  std::fs::write(
    demo.join("unwrap.ts"),
    "import { unref } from 'vue'\nexport function unwrap(x){return unref(() => x)}\n",
  )
  .unwrap_or_else(|error| panic!("unwrap: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let initial = session.analyze().unwrap_or_else(|error| panic!("initial: {error}"));
  assert!(
    initial
      .summary
      .diagnostics
      .iter()
      .all(|diagnostic| diagnostic.rule_id != "vue-vet/practice/prefer-to-value"),
    "root Vue 3.2 must stay quiet; {:?}",
    initial.summary.diagnostics
  );

  let nested = demo.join("package.json");
  let unwrap = FileId::from("apps/demo/unwrap.ts");
  let vue_32 = r#"{"dependencies":{"vue":"3.2.0"}}"#;
  let vue_35 = r#"{"dependencies":{"vue":"3.5.40"}}"#;
  let prefer_to_value = |snapshot: &AnalysisSnapshot| {
    snapshot.summary.diagnostics.iter().any(|diagnostic| {
      diagnostic.file == unwrap && diagnostic.rule_id == "vue-vet/practice/prefer-to-value"
    })
  };

  for (label, body, expect_prefer) in [
    ("add3.5", Some(vue_35), true),
    ("replace3.2", Some(vue_32), false),
    ("restore3.5", Some(vue_35), true),
    ("remove", None, false),
  ] {
    match body {
      Some(source) => {
        std::fs::write(&nested, source).unwrap_or_else(|error| panic!("{label} write: {error}"));
      }
      None => {
        std::fs::remove_file(&nested).unwrap_or_else(|error| panic!("{label} unlink: {error}"));
      }
    }
    session
      .apply_changes(ChangeSet::remove(nested.clone()))
      .unwrap_or_else(|error| panic!("{label} refresh: {error}"));
    let incremental =
      session.analyze_affected().unwrap_or_else(|error| panic!("{label} incremental: {error}"));
    assert_eq!(
      incremental.work.files_parsed, 0,
      "{label} nested package change must not re-parse: {:?}",
      incremental.work
    );
    assert_eq!(
      prefer_to_value(&incremental),
      expect_prefer,
      "{label} prefer-to-value; {:?}",
      incremental.summary.diagnostics
    );
    let clean = open_session_threads(root.clone(), 1)
      .analyze()
      .unwrap_or_else(|error| panic!("{label} clean: {error}"));
    assert_analysis_parity(&incremental, &clean);
  }
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn jsx_dynamic_dependency_regressions_stay_quiet() {
  let names = [
    "former-invalid-guarded-render.tsx",
    "former-invalid-ident-getter.tsx",
    "ident-getter.tsx",
    "read-before-guard.tsx",
  ];
  let root = std::env::temp_dir().join(format!("vue-vet-jsx-dynamic-deps-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  install_module_seeds_vue_stub(&root);
  for name in names {
    let source =
      std::fs::read_to_string(fixture(&format!("reactivity-semantics/dynamic-deps/{name}")))
        .unwrap_or_else(|error| panic!("read {name}: {error}"));
    std::fs::write(root.join(name), source).unwrap_or_else(|error| panic!("write {name}: {error}"));
  }
  let session = open_session_threads(root.clone(), 1);
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  assert!(snapshot.complete(), "scan issues: {:?}", snapshot.issues);
  let analyzed: std::collections::BTreeSet<&str> =
    snapshot.analyzed_files.iter().map(String::as_str).collect();
  for name in names {
    assert!(analyzed.contains(name), "fixture {name} must be analyzed; {analyzed:?}");
    let module = snapshot
      .graph
      .module_reactivity
      .iter()
      .find(|module| module.id.as_str() == name)
      .unwrap_or_else(|| panic!("missing module graph for {name}"));
    let render = module
      .graph
      .scopes
      .iter()
      .find(|scope| scope.kind == TrackingScopeKind::Render)
      .unwrap_or_else(|| {
        panic!("{name} must have a Render tracking scope; {:?}", module.graph.scopes)
      });
    assert!(!render.reads.is_empty(), "{name} Render scope must keep reactive reads; {render:?}");
  }
  let live_noise = snapshot
    .summary
    .diagnostics
    .iter()
    .filter(|diagnostic| {
      names.iter().any(|name| diagnostic.file == FileId::from(*name))
        && diagnostic.rule_id != PROJECT_RULE_IDS[0]
    })
    .collect::<Vec<_>>();
  assert!(
    live_noise.is_empty(),
    "live semantic rules must stay quiet on TSX dynamic-dep fixtures; {live_noise:?}"
  );
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn plain_ts_lifetime_rules_run() {
  let root = std::env::temp_dir().join(format!("vue-vet-lifetime-ts-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  std::fs::write(
    root.join("watcher.ts"),
    "import { watchEffect } from 'vue'\n\
const source = { value: 0 }\n\
watchEffect(() => { source.value; return () => {} })\n",
  )
  .unwrap_or_else(|error| panic!("write: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  assert!(
    snapshot.summary.diagnostics.iter().any(|diagnostic| {
      diagnostic.file == FileId::from("watcher.ts")
        && diagnostic.rule_id == "vue-vet/reactivity/no-returned-watcher-cleanup"
    }),
    "plain TS must run lifetime rules; {:?}",
    snapshot.summary.diagnostics
  );
  let _ignored = std::fs::remove_dir_all(root);
}

const SOURCE_CONTRACT_AND_NOTIFICATION_IDS: [&str; 11] = [
  "vue-vet/reactivity/no-watch-unwrapped-source",
  "vue-vet/reactivity/no-trigger-ref-on-non-ref",
  "vue-vet/reactivity/no-torefs-on-non-proxy",
  "vue-vet/reactivity/no-primitive-reactive-target",
  "vue-vet/reactivity/no-watch-replaced-object-source",
  "vue-vet/reactivity/no-lost-shallow-nested-notification",
  "vue-vet/reactivity/no-toraw-write-of-tracked-state",
  "vue-vet/reactivity/no-watch-ignored-option",
  "vue-vet/reactivity/no-watch-signature-mismatch",
  "vue-vet/reactivity/no-once-immediate-discard",
  "vue-vet/reactivity/no-watch-alias-old-new",
];

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn source_contract_findings_keep_incremental_identity() {
  let root = std::env::temp_dir().join(format!("vue-vet-source-contracts-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  let source = "<script setup lang=\"ts\">\n\
import { reactive, ref, shallowRef, toRaw, triggerRef, toRefs, watch, watchSyncEffect } from 'vue'\n\
const n = ref(0)\n\
watch(n.value, () => {})\n\
triggerRef(reactive({ n: 1 }))\n\
toRefs({ a: 1 })\n\
void reactive(0)\n\
const state = shallowRef({ count: 1 })\n\
watchSyncEffect(() => { void state.value.count })\n\
state.value.count = 2\n\
const proxy = reactive({ n: 1 })\n\
watchSyncEffect(() => { void proxy.n })\n\
const raw = toRaw(proxy)\n\
raw.n = 2\n\
const quiet = reactive({ n: 1 })\n\
const quietRaw = toRaw(quiet)\n\
quietRaw.n = 2\n\
</script>\n\
<template><p /></template>\n";
  let replaced = "<script setup lang=\"ts\">\n\
import { reactive, watch } from 'vue'\n\
const obj = reactive({ nested: { x: 1 } })\n\
watch(obj.nested, () => {})\n\
obj.nested = { x: 9 }\n\
</script>\n\
<template><p /></template>\n";
  std::fs::write(root.join("App.vue"), source).unwrap_or_else(|error| panic!("write: {error}"));
  std::fs::write(root.join("Replace.vue"), replaced)
    .unwrap_or_else(|error| panic!("write replace: {error}"));
  std::fs::write(
    root.join("WatchApi.vue"),
    "<script setup lang=\"ts\">\n\
import { ref, watch, watchEffect } from 'vue'\n\
const n = ref(0)\n\
watch(n, (v) => v, { equals: () => true })\n\
watch(n, { handler() { void n.value } })\n\
watchEffect(() => n.value, (x) => x)\n\
</script>\n\
<template><p /></template>\n",
  )
  .unwrap_or_else(|error| panic!("write watch api: {error}"));
  std::fs::write(
    root.join("Callback.vue"),
    "<script setup lang=\"ts\">\n\
import { reactive, ref, watch } from 'vue'\n\
function accept(_value: unknown) {}\n\
const n = ref(0)\n\
watch(n, (next, old) => { if (old === undefined) return; accept(next) }, { once: true, immediate: true })\n\
const state = reactive({ n: 1 })\n\
watch(state, (next, old) => { if (next === old) return; accept(next) })\n\
</script>\n\
<template><p /></template>\n",
  )
  .unwrap_or_else(|error| panic!("write callback: {error}"));
  let session = open_session_threads(root.clone(), 1);
  let cold = session.analyze().unwrap_or_else(|error| panic!("cold: {error}"));
  let expected: std::collections::BTreeSet<String> =
    SOURCE_CONTRACT_AND_NOTIFICATION_IDS.into_iter().map(str::to_owned).collect();
  let contract_ids = |snapshot: &AnalysisSnapshot| -> std::collections::BTreeSet<String> {
    snapshot
      .summary
      .diagnostics
      .iter()
      .map(|diagnostic| diagnostic.rule_id.clone())
      .filter(|rule_id| SOURCE_CONTRACT_AND_NOTIFICATION_IDS.contains(&rule_id.as_str()))
      .collect()
  };
  assert_eq!(
    contract_ids(&cold),
    expected,
    "cold scan must emit source-contract, watch-api, notification, and callback IDs; {:?}",
    cold.summary.diagnostics
  );
  let toraw = "vue-vet/reactivity/no-toraw-write-of-tracked-state";
  let toraw_hits: Vec<_> =
    cold.summary.diagnostics.iter().filter(|diagnostic| diagnostic.rule_id == toraw).collect();
  assert_eq!(
    toraw_hits.len(),
    1,
    "tracked toRaw write must emit once; quiet no-consumer control must stay silent; {toraw_hits:?}"
  );
  assert!(
    toraw_hits.iter().all(|diagnostic| {
      diagnostic.file == FileId::from("App.vue")
        && source.get(
          diagnostic.span.offset..diagnostic.span.offset.saturating_add(diagnostic.span.length),
        ) == Some("raw.n")
    }),
    "the toRaw finding must highlight the subscribed proxy write; {toraw_hits:?}"
  );
  session
    .apply_changes(ChangeSet::upsert(root.join("App.vue"), source.into()))
    .unwrap_or_else(|error| panic!("touch: {error}"));
  let warm = session.analyze_affected().unwrap_or_else(|error| panic!("warm: {error}"));
  assert_eq!(
    contract_ids(&warm),
    expected,
    "warm scan must keep the same source-contract and notification IDs; {:?}",
    warm.summary.diagnostics
  );
  let clean = open_session_threads(root.clone(), 1)
    .analyze()
    .unwrap_or_else(|error| panic!("clean: {error}"));
  assert_analysis_parity(&warm, &clean);
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn source_contract_findings_keep_disk_cache_identity() {
  let root =
    std::env::temp_dir().join(format!("vue-vet-source-contracts-cache-{}", std::process::id()));
  let cache_dir = root.join(".vue-vet-cache");
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  std::fs::write(
    root.join("WatchApi.vue"),
    "<script setup lang=\"ts\">\n\
import { watchSyncEffect } from 'vue'\n\
watchSyncEffect(() => {}, { flush: 'post', once: true })\n\
</script>\n\
<template><p /></template>\n",
  )
  .unwrap_or_else(|error| panic!("write: {error}"));
  let open = |cache: std::path::PathBuf| {
    ProjectSession::open(SessionOptions {
      root: root.clone(),
      config_path: None,
      cache_dir: Some(cache),
      no_cache: false,
      threads: Some(1),
      selected_groups: Vec::new(),
    })
    .unwrap_or_else(|error| panic!("session: {error}"))
  };
  let cold = open(cache_dir.clone()).analyze().unwrap_or_else(|error| panic!("cold: {error}"));
  assert_eq!(cold.cache_status, "miss", "first scan must miss");
  assert!(
    cold
      .summary
      .diagnostics
      .iter()
      .any(|diagnostic| { diagnostic.rule_id == "vue-vet/reactivity/no-watch-ignored-option" }),
    "named watchSyncEffect without source5 APIs must report ignored once; {:?}",
    cold.summary.diagnostics
  );
  let warm = open(cache_dir).analyze().unwrap_or_else(|error| panic!("warm: {error}"));
  assert_eq!(warm.cache_status, "hit", "second scan must hit");
  assert_eq!(warm.summary, cold.summary, "warm diagnostics must equal cold");
  let _ignored = std::fs::remove_dir_all(root);
}

#[test]
#[expect(clippy::panic, reason = "session setup failures must fail the integration test")]
fn source_contracts_group_keeps_watch_api_and_drops_tracking() {
  let root =
    std::env::temp_dir().join(format!("vue-vet-source-contracts-group-{}", std::process::id()));
  let _ignored = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
  std::fs::write(
    root.join("App.vue"),
    "<script setup lang=\"ts\">\n\
import { ref, watchEffect } from 'vue'\n\
const n = ref(0)\n\
watchEffect(() => n.value, { once: true })\n\
const unused = 1\n\
</script>\n\
<template><img></template>\n",
  )
  .unwrap_or_else(|error| panic!("write: {error}"));
  let source = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
    selected_groups: vec![vue_vet_session::RuleGroupId::SourceContracts],
  })
  .unwrap_or_else(|error| panic!("open source-contracts: {error}"));
  let snapshot = source.analyze().unwrap_or_else(|error| panic!("analyze: {error}"));
  assert!(
    snapshot
      .summary
      .diagnostics
      .iter()
      .any(|diagnostic| { diagnostic.rule_id == "vue-vet/reactivity/no-watch-ignored-option" }),
    "source-contracts group must keep watch-api; {:?}",
    snapshot.summary.diagnostics
  );
  assert!(
    snapshot.summary.diagnostics.iter().all(|diagnostic| {
      !diagnostic.rule_id.contains("img-has-alt") && !diagnostic.rule_id.contains("empty-watch")
    }),
    "source-contracts group must drop unmapped a11y and tracking IDs; {:?}",
    snapshot.summary.diagnostics
  );
  let tracking = ProjectSession::open(SessionOptions {
    root: root.clone(),
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
    selected_groups: vec![vue_vet_session::RuleGroupId::Tracking],
  })
  .unwrap_or_else(|error| panic!("open tracking: {error}"));
  let tracking_snap = tracking.analyze().unwrap_or_else(|error| panic!("tracking: {error}"));
  assert!(
    tracking_snap
      .summary
      .diagnostics
      .iter()
      .all(|diagnostic| { diagnostic.rule_id != "vue-vet/reactivity/no-watch-ignored-option" }),
    "tracking group must drop watch-api; {:?}",
    tracking_snap.summary.diagnostics
  );
  let _ignored = std::fs::remove_dir_all(root);
}
