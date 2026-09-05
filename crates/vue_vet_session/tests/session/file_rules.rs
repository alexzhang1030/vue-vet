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
          && diagnostic.rule_id == "vue-vet/reactivity/no-side-effects-in-computed"
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
    "import { unref } from 'vue'\nexport function unwrap(x){return unref(x)}\n",
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
