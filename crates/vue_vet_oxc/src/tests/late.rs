use super::support::*;

#[test]
fn late_cleanup_skips_explicit_owner_and_fail_silently() {
  let facts = analyze(
    "import { getCurrentWatcher, onWatcherCleanup, watchEffect } from 'vue';\
     watchEffect(async () => {\
       const owner = getCurrentWatcher();\
       await Promise.resolve();\
       onWatcherCleanup(() => {}, false, owner);\
       onWatcherCleanup(() => {}, true);\
     });",
    "ts",
  );
  assert!(
    facts.lifetime.late_watcher_cleanups.is_empty(),
    "explicit owner / failSilently must stay quiet: {:?}",
    facts.lifetime.late_watcher_cleanups
  );
}

#[test]
fn late_cancellation_guard_fires_on_bound_oncleanup_after_await() {
  let facts = settlement_facts(
    "watch(source, async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
  );
  assert_eq!(
    facts.late_cancellation_guards.len(),
    1,
    "late bound onCleanup must emit: {:?}",
    facts.late_cancellation_guards
  );
}

#[test]
fn late_cancellation_guard_stays_quiet_when_registered_before_await() {
  let facts = settlement_facts(
    "watch(source, async (value, _previous, onCleanup) => {\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       const data = await Promise.resolve(value);\
       if (!cancelled) result.value = data;\
     });",
  );
  assert!(
    facts.late_cancellation_guards.is_empty(),
    "sync registration must stay quiet: {:?}",
    facts.late_cancellation_guards
  );
}

#[test]
fn late_cancellation_guard_stays_quiet_for_generation_and_equality() {
  let generation = settlement_facts(
    "let generation = 0;\
     watch(source, async (value) => {\
       const current = (generation += 1);\
       const data = await Promise.resolve(value);\
       if (current === generation) result.value = data;\
     });",
  );
  let equality = settlement_facts(
    "watch(source, async (value) => {\
       const data = await Promise.resolve(value);\
       if (source.value === value) result.value = data;\
     });",
  );
  assert!(
    generation.late_cancellation_guards.is_empty() && equality.late_cancellation_guards.is_empty(),
    "generation/equality must stay quiet: {generation:?} {equality:?}"
  );
}

#[test]
fn late_cancellation_guard_skips_onwatcher_cleanup_owner_loss() {
  let facts = analyze(
    "import { onWatcherCleanup, ref, watch } from 'vue';\
     const source = ref('one');\
     const result = ref(null);\
     watch(source, async (value) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onWatcherCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
    "ts",
  )
  .lifetime;
  assert!(
    facts.late_cancellation_guards.is_empty(),
    "onWatcherCleanup after await belongs to no-late-watcher-cleanup: {:?}",
    facts.late_cancellation_guards
  );
  assert_eq!(facts.late_watcher_cleanups.len(), 1);
}

#[test]
fn late_cancellation_guard_covers_alias_watch_effect_and_named_callback() {
  let alias = analyze(
    "import { ref, watch as vueWatch } from 'vue';\
     const source = ref('one');\
     const result = ref(null);\
     vueWatch(source, async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
    "ts",
  )
  .lifetime;
  let effect = settlement_facts(
    "watchEffect(async (onCleanup) => {\
       const data = await Promise.resolve(source.value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
  );
  let named = settlement_facts(
    "const load = async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     };\
     watch(source, load);",
  );
  assert_eq!(alias.late_cancellation_guards.len(), 1, "{alias:?}");
  assert_eq!(effect.late_cancellation_guards.len(), 1, "{effect:?}");
  assert_eq!(named.late_cancellation_guards.len(), 1, "{named:?}");
}

#[test]
fn late_cancellation_guard_unknown_shapes_stay_quiet() {
  for body in [
    "watch(source, async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       for (const _ of [0]) onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
    "watch(source, async (value, _previous, onCleanup) => {\
       await Promise.resolve();\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
    "watch(source, async (value, _previous, onCleanup) => {\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = value;\
     });",
    "function hold(flag) {}\
     watch(source, async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       hold(cancelled);\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
    "watch(source, async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     }, { once: true });",
    "const opts = { once: true };\
     watch(source, async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     }, opts);",
  ] {
    let facts = settlement_facts(body);
    assert!(
      facts.late_cancellation_guards.is_empty(),
      "unknown/safe shape must stay quiet: {body} {:?}",
      facts.late_cancellation_guards
    );
  }
}

#[test]
fn late_cancellation_guard_scales_near_linearly() {
  fn source(count: usize, kind: &str) -> String {
    let mut out = String::from("import { ref, watch } from 'vue';\nconst source = ref('one');\n");
    match kind {
      "independent" => {
        for index in 0..count {
          out.push_str("const r");
          out.push_str(&index.to_string());
          out.push_str(
            " = ref(null);\nwatch(source, async (value, _previous, onCleanup) => {\
             const data = await Promise.resolve(value);\
             let cancelled = false;\
             onCleanup(() => { cancelled = true; });\
             if (!cancelled) r",
          );
          out.push_str(&index.to_string());
          out.push_str(".value = data; });\n");
        }
      }
      "shared-source" => {
        out.push_str("const result = ref(null);\n");
        for index in 0..count {
          out.push_str(
            "watch(source, async (value, _previous, onCleanup) => {\
             const data = await Promise.resolve(value);\
             let cancelled = false;\
             onCleanup(() => { cancelled = true; });\
             if (!cancelled) result.value = data; void ",
          );
          out.push_str(&index.to_string());
          out.push_str("; });\n");
        }
      }
      "aliases" => {
        out.push_str("const result = ref(null);\nconst s0 = source;\n");
        for index in 1..=count {
          out.push_str("const s");
          out.push_str(&index.to_string());
          out.push_str(" = s");
          out.push_str(&(index - 1).to_string());
          out.push_str(";\n");
        }
        out.push_str(
          "watch(s0, async (value, _previous, onCleanup) => {\
           const data = await Promise.resolve(value);\
           let cancelled = false;\
           onCleanup(() => { cancelled = true; });\
           if (!cancelled) result.value = data; });\n",
        );
      }
      "wrappers" => {
        out.push_str(
          "const result = ref(null);\nwatch(source, async (value, _previous, onCleanup) => {\n",
        );
        out.push_str("  const data = await Promise.resolve(");
        for _ in 0..count {
          out.push_str("(((((");
        }
        out.push_str("value");
        for _ in 0..count {
          out.push_str(")))))");
        }
        out.push_str(
          ");\n  let cancelled = false;\n  onCleanup(() => { cancelled = true; });\n  if (!cancelled) result.value = data;\n});\n",
        );
      }
      _ => {
        out.push_str(
          "const result = ref(null);\nwatch(source, async (value, _previous, onCleanup) => {\n",
        );
        for index in 0..count {
          out.push_str("  void ");
          out.push_str(&index.to_string());
          out.push_str(";\n");
        }
        out.push_str(
          "  const data = await Promise.resolve(value);\n  let cancelled = false;\n  onCleanup(() => { cancelled = true; });\n  if (!cancelled) result.value = data;\n});\n",
        );
      }
    }
    out
  }
  for kind in ["independent", "shared-source", "aliases", "wrappers", "irrelevant"] {
    let (facts_20, work_20) = settlement_work(&source(20, kind));
    let (facts_40, work_40) = settlement_work(&source(40, kind));
    let (facts_80, work_80) = settlement_work(&source(80, kind));
    if kind == "independent" || kind == "shared-source" {
      assert_eq!((facts_20, facts_40, facts_80), (20, 40, 80), "{kind} facts");
    } else {
      assert_eq!((facts_20, facts_40, facts_80), (1, 1, 1), "{kind} facts");
    }
    assert!(
      work_80 <= work_20.saturating_mul(6).saturating_add(256),
      "{kind} settlement work must stay near-linear: 20={work_20} 40={work_40} 80={work_80}"
    );
  }
}

#[test]
fn late_cancellation_guard_stays_quiet_for_sync_onwatchercleanup_plus_late_bound() {
  let facts = analyze(
    "import { onWatcherCleanup, ref, watch } from 'vue';\
     const source = ref('one');\
     const result = ref(null);\
     watch(source, async (value, _previous, onCleanup) => {\
       let cancelled = false;\
       onWatcherCleanup(() => { cancelled = true; });\
       const data = await Promise.resolve(value);\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     });",
    "ts",
  )
  .lifetime;
  assert!(
    facts.late_cancellation_guards.is_empty(),
    "sync onWatcherCleanup already guards the flag: {:?}",
    facts.late_cancellation_guards
  );
}

#[test]
fn late_cancellation_guard_still_fires_when_once_is_literally_false() {
  let facts = settlement_facts(
    "watch(source, async (value, _previous, onCleanup) => {\
       const data = await Promise.resolve(value);\
       let cancelled = false;\
       onCleanup(() => { cancelled = true; });\
       if (!cancelled) result.value = data;\
     }, { once: false });",
  );
  assert_eq!(
    facts.late_cancellation_guards.len(),
    1,
    "closed once: false must not abstain: {:?}",
    facts.late_cancellation_guards
  );
}
