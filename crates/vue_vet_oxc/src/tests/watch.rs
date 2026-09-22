use super::support::*;

#[test]
fn watch_spread_arguments_stay_quiet() {
  let facts = analyze(
    "import { watch } from 'vue';\
     watch(...[], () => () => {}, () => {}, { immediate: true });\
     const source = { value: 0 };\
     watch(source, () => () => {});",
    "ts",
  );
  assert_eq!(
    facts.lifetime.returned_watcher_cleanups.len(),
    1,
    "spread watch must stay quiet: {:?}",
    facts.lifetime.returned_watcher_cleanups
  );
}

#[test]
fn watch_cleanup_current_source_emits_for_reread_event_target() {
  let facts = analyze(
    &cleanup_identity_source(
      "const handler = () => {};\
       const source = ref(new EventTarget());\
       watch(source, (target, _prev, onCleanup) => {\
         target.addEventListener('click', handler);\
         onCleanup(() => { source.value.removeEventListener('click', handler); });\
       }, { immediate: true, flush: 'sync' });\
       source.value = new EventTarget();",
    ),
    "ts",
  );
  assert_eq!(
    facts.lifetime.watch_cleanup_current_sources.len(),
    1,
    "reread source.value must emit: {:?}",
    facts.lifetime.watch_cleanup_current_sources
  );
}

#[test]
fn watch_cleanup_current_source_shares_flush_and_cleanup_apis() {
  let facts = analyze(
    &cleanup_identity_source(
      "const handler = () => {};\
       const source = ref(new EventTarget());\
       watch(source, (target, _prev, onCleanup) => {\
         target.addEventListener('click', handler);\
         onCleanup(() => { source.value.removeEventListener('click', handler); });\
       }, { flush: 'sync' });\
       source.value = new EventTarget();\
       source.value = new EventTarget();\
       const other = ref(new EventTarget());\
       watch(other, (target) => {\
         target.addEventListener('click', handler);\
         onWatcherCleanup(() => { other.value.removeEventListener('click', handler); });\
       }, { immediate: true, flush: 'sync' });\
       other.value = new EventTarget();",
    ),
    "ts",
  );
  assert_eq!(
    facts.lifetime.watch_cleanup_current_sources.len(),
    2,
    "sync flush and onWatcherCleanup share the same fact: {:?}",
    facts.lifetime.watch_cleanup_current_sources
  );
}

#[test]
fn watch_cleanup_current_source_stays_quiet_for_safe_and_unknown() {
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const captured = ref(new EventTarget());\
     watch(captured, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { target.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     captured.value = new EventTarget();",
    "captured target",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const logged = ref(new EventTarget());\
     watch(logged, (target, _prev, onCleanup) => {\
       void target;\
       onCleanup(() => { console.log(logged.value); });\
     }, { immediate: true, flush: 'sync' });\
     logged.value = new EventTarget();",
    "logging source",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const writeback = ref(new EventTarget());\
     watch(writeback, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { writeback.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     writeback.value = writeback.value;",
    "same-target writeback",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const once = ref(new EventTarget());\
     watch(once, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler, { once: true });\
       onCleanup(() => { once.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     once.value = new EventTarget();",
    "listener once option",
  );
  assert_cleanup_identity_quiet(
    "class EventTarget {}\
     const handler = () => {};\
     const shadowed = ref(new EventTarget());\
     watch(shadowed, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { shadowed.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     shadowed.value = new EventTarget();",
    "shadowed EventTarget",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     });\
     source.value = new EventTarget();\
     source.value = new EventTarget();",
    "default pre coalesced assignments",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync', once: true });\
     source.value = new EventTarget();\
     stop();",
    "watch once option",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     stop();\
     source.value = new EventTarget();",
    "stopped before replacement",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     function unusedReplacement() {\
       source.value = new EventTarget();\
       source.value = new EventTarget();\
     }",
    "uninvoked replacement",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = initial;\
     source.value = initial;",
    "same-target alias writeback",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     initial.addEventListener = () => {};\
     const source = ref(initial);\
     watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();",
    "payload method mutation",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     function disableListeners(value) { value.addEventListener = () => {}; }\
     const source = ref(new EventTarget());\
     watch(source, (target, _prev, onCleanup) => {\
       disableListeners(target);\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();",
    "helper method escape",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => {\
         target.removeEventListener('click', handler);\
         source.value.removeEventListener('click', handler);\
       });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();",
    "captured release discharge",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     const options = { immediate: true, flush: 'sync' };\
     watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, options);\
     source.value = new EventTarget();",
    "unknown options object",
  );
}

#[test]
fn watch_cleanup_current_source_stays_quiet_for_second_review_safes() {
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { flush: 'sync' });\
     source.value = initial;\
     source.value = new EventTarget();\
     stop();",
    "same-value write before first sync change",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true });\
     source.value = new EventTarget();\
     source.value = initial;\
     await nextTick();\
     stop();",
    "immediate pre round-trip before nextTick",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     if (false) {\
       watch(source, (target, _prev, onCleanup) => {\
         target.addEventListener('click', handler);\
         onCleanup(() => { source.value.removeEventListener('click', handler); });\
       }, { immediate: true, flush: 'sync' });\
     }\
     source.value = new EventTarget();",
    "inactive conditional watch",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     const cancel = stop;\
     cancel();\
     source.value = new EventTarget();",
    "const stop-handle alias",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     const firstAcquired = new EventTarget();\
     firstAcquired.addEventListener = () => {};\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { flush: 'sync' });\
     source.value = firstAcquired;\
     source.value = new EventTarget();\
     stop();",
    "later mutated acquisition payload",
  );
  assert_cleanup_identity_quiet(
    "const handler = null;\
     const source = ref(new EventTarget());\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "null listener argument",
  );
  assert_cleanup_identity_quiet(
    "const handler = { notAListener: true };\
     const source = ref(new EventTarget());\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "unknown object listener",
  );
}

#[test]
fn watch_cleanup_current_source_stays_quiet_for_third_review_safes() {
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let replacement = new EventTarget();\
     replacement = initial;\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = replacement;\
     stop();",
    "mutable payload alias rebound to the acquired allocation",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     if (true) stop();\
     source.value = new EventTarget();\
     stop();",
    "earlier conditional stop before a later definite stop",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const source = ref(new EventTarget());\
     const handle = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     const cancel = handle;\
     if (true) cancel.stop();\
     source.value = new EventTarget();\
     handle.stop();",
    "earlier uncertain .stop() on a const handle alias",
  );
}

#[test]
fn watch_cleanup_current_source_stays_quiet_for_fourth_review_safes() {
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let replacement = initial;\
     if (false) replacement = new EventTarget();\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = replacement;\
     stop();",
    "conditional write on a payload alias is not executed replacement",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let replacement = initial;\
     function unusedReplacement() { replacement = new EventTarget(); }\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = replacement;\
     stop();",
    "uninvoked function write on a payload alias is not executed replacement",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let replacement = new EventTarget();\
     replacement = initial;\
     watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     replacement = new EventTarget();\
     source.value = replacement;",
    "executed mutable payload rebind stays Unknown",
  );
}

#[test]
fn watch_cleanup_current_source_stays_quiet_for_fifth_review_safes() {
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let replacement = new EventTarget();\
     replacement &&= initial;\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = replacement;\
     stop();",
    "logical assignment on a payload alias is Unknown",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let replacement = new EventTarget();\
     function run() {\
       const stop = watch(source, (target, _prev, onCleanup) => {\
         target.addEventListener('click', handler);\
         onCleanup(() => { source.value.removeEventListener('click', handler); });\
       }, { immediate: true, flush: 'sync' });\
       source.value = replacement;\
       stop();\
     }\
     replacement = initial;\
     run();",
    "later-owner write before call is Unknown",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let candidate = initial;\
     candidate = initial;\
     candidate.addEventListener = () => {};\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "method mutation through a written receiver alias is Unknown",
  );
}

#[test]
fn watch_cleanup_current_source_stays_quiet_for_sixth_review_safes() {
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let candidate = initial;\
     candidate = initial;\
     Reflect.set(candidate, 'addEventListener', () => {});\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "escape of a written payload alias is Unknown",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     const candidate = initial;\
     Reflect.set(candidate, 'addEventListener', () => {});\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "escape of a stable const payload alias stays quiet",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let replacement = new EventTarget();\
     [replacement] = [initial];\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = replacement;\
     stop();",
    "destructured payload write is Unknown",
  );
}

#[test]
fn watch_cleanup_current_source_stays_quiet_for_seventh_review_safes() {
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let candidate;\
     candidate = initial;\
     Reflect.set(candidate, 'addEventListener', () => {});\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "assignment of a native payload then generic escape is Unknown",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     let candidate = initial;\
     candidate = initial;\
     let forwarded = candidate;\
     Reflect.set(forwarded, 'addEventListener', () => {});\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "copy of a written native payload then generic escape is Unknown",
  );
  assert_cleanup_identity_quiet(
    "const handler = () => {};\
     const initial = new EventTarget();\
     const source = ref(initial);\
     function disable() { Reflect.set(candidate, 'addEventListener', () => {}); }\
     const candidate = initial;\
     disable();\
     const stop = watch(source, (target, _prev, onCleanup) => {\
       target.addEventListener('click', handler);\
       onCleanup(() => { source.value.removeEventListener('click', handler); });\
     }, { immediate: true, flush: 'sync' });\
     source.value = new EventTarget();\
     stop();",
    "escape before a later declaration is resolved after the identity index",
  );
}

#[test]
fn watch_cleanup_current_source_const_chain_alias_still_emits() {
  let facts = analyze(
    &cleanup_identity_source(
      "const handler = () => {};\
       const initial = new EventTarget();\
       const source = ref(initial);\
       const fresh = new EventTarget();\
       const middle = fresh;\
       const replacement = middle;\
       watch(source, (target, _prev, onCleanup) => {\
         target.addEventListener('click', handler);\
         onCleanup(() => { source.value.removeEventListener('click', handler); });\
       }, { immediate: true, flush: 'sync' });\
       source.value = replacement;",
    ),
    "ts",
  );
  assert_eq!(
    facts.lifetime.watch_cleanup_current_sources.len(),
    1,
    "two-hop const payload alias must still emit: {:?}",
    facts.lifetime.watch_cleanup_current_sources
  );
}

#[test]
fn watch_cleanup_current_source_const_payload_alias_still_emits() {
  let facts = analyze(
    &cleanup_identity_source(
      "const handler = () => {};\
       const initial = new EventTarget();\
       const source = ref(initial);\
       const replacement = new EventTarget();\
       watch(source, (target, _prev, onCleanup) => {\
         target.addEventListener('click', handler);\
         onCleanup(() => { source.value.removeEventListener('click', handler); });\
       }, { immediate: true, flush: 'sync' });\
       source.value = replacement;",
    ),
    "ts",
  );
  assert_eq!(
    facts.lifetime.watch_cleanup_current_sources.len(),
    1,
    "stable const payload alias must still emit: {:?}",
    facts.lifetime.watch_cleanup_current_sources
  );
}

#[test]
fn watch_cleanup_current_source_handle_event_still_emits() {
  let facts = analyze(
    &cleanup_identity_source(
      "const handler = { handleEvent() {} };\
       const source = ref(new EventTarget());\
       watch(source, (target, _prev, onCleanup) => {\
         target.addEventListener('click', handler);\
         onCleanup(() => { source.value.removeEventListener('click', handler); });\
       }, { immediate: true, flush: 'sync' });\
       source.value = new EventTarget();",
    ),
    "ts",
  );
  assert_eq!(
    facts.lifetime.watch_cleanup_current_sources.len(),
    1,
    "proven handleEvent listener must still emit: {:?}",
    facts.lifetime.watch_cleanup_current_sources
  );
}

#[test]
fn watch_cleanup_current_source_spans_cover_unicode_and_crlf() {
  let facts = analyze(
    "import { ref, watch } from 'vue'\r\nconst 处理 = () => {}\r\nconst 源 = ref(new EventTarget())\r\nwatch(源, (目标, _prev, onCleanup) => {\r\n  目标.addEventListener('click', 处理)\r\n  onCleanup(() => { 源.value.removeEventListener('click', 处理) })\r\n}, { immediate: true, flush: 'sync' })\r\n源.value = new EventTarget()\r\n",
    "ts",
  );
  let fact = facts.lifetime.watch_cleanup_current_sources.first();
  assert!(fact.is_some(), "unicode/CRLF must emit: {:?}", facts.lifetime);
  let Some(fact) = fact else {
    return;
  };
  assert!(fact.release_span.length > 0);
  assert!(fact.acquisition_span.length > 0);
  assert!(fact.replacement_span.length > 0);
}

#[test]
fn watch_cleanup_current_source_index_scales_linearly() {
  fn independent(count: usize, aliases: usize) -> String {
    let mut out = String::from("import { ref, watch } from 'vue';\nconst handler = () => {};\n");
    for _ in 0..count {
      out.push_str("{\nconst source = ref(new EventTarget());\n");
      for alias in 0..aliases {
        out.push_str("const a");
        out.push_str(&alias.to_string());
        out.push_str(" = source;\n");
      }
      out.push_str(
        "watch(source, (target, _prev, onCleanup) => {\n  target.addEventListener('click', handler);\n  onCleanup(() => { source.value.removeEventListener('click', handler); });\n}, { immediate: true, flush: 'sync' });\nsource.value = new EventTarget();\n}\n",
      );
    }
    out
  }
  fn shared_source(watchers: usize, writes: usize) -> String {
    let mut out = String::from(
      "import { ref, watch } from 'vue';\nconst handler = () => {};\nconst source = ref(new EventTarget());\n",
    );
    for _ in 0..watchers {
      out.push_str(
        "watch(source, (target, _prev, onCleanup) => {\n  target.addEventListener('click', handler);\n  onCleanup(() => { source.value.removeEventListener('click', handler); });\n}, { immediate: true, flush: 'sync' });\n",
      );
    }
    for _ in 0..writes {
      out.push_str("source.value = new EventTarget();\n");
    }
    out
  }
  fn shared_callback(count: usize) -> String {
    let mut out = String::from(
      "import { ref, watch } from 'vue';\nconst handler = () => {};\nconst source = ref(new EventTarget());\nfunction cb(target, _prev, onCleanup) {\n  target.addEventListener('click', handler);\n  onCleanup(() => { source.value.removeEventListener('click', handler); });\n}\n",
    );
    for _ in 0..count {
      out.push_str("watch(source, cb, { immediate: true, flush: 'sync' });\n");
    }
    out.push_str("source.value = new EventTarget();\n");
    out
  }
  fn multiple_resources(count: usize) -> String {
    let mut out =
      String::from("import { ref, watch } from 'vue';\nconst source = ref(new EventTarget());\n");
    for index in 0..count {
      out.push_str("const h");
      out.push_str(&index.to_string());
      out.push_str(" = () => {};\n");
    }
    out.push_str("watch(source, (target, _prev, onCleanup) => {\n");
    for index in 0..count {
      out.push_str("  target.addEventListener('click', h");
      out.push_str(&index.to_string());
      out.push_str(");\n");
    }
    out.push_str("  onCleanup(() => {\n");
    for index in 0..count {
      out.push_str("    source.value.removeEventListener('click', h");
      out.push_str(&index.to_string());
      out.push_str(");\n");
    }
    out.push_str(
      "  });\n}, { immediate: true, flush: 'sync' });\nsource.value = new EventTarget();\n",
    );
    out
  }
  fn assert_linear(
    label: &str,
    facts20: usize,
    facts40: usize,
    facts80: usize,
    source20: &str,
    source40: &str,
    source80: &str,
  ) {
    let (got20, stats20) = cleanup_identity_visits(source20);
    let (got40, stats40) = cleanup_identity_visits(source40);
    let (got80, stats80) = cleanup_identity_visits(source80);
    assert_eq!((got20, got40, got80), (facts20, facts40, facts80), "{label} facts");
    assert!(
      stats20.identity_comparisons > 0
        && stats20.identity_registrations > 0
        && stats20.identity_alias_work > 0,
      "{label} must count comparisons/registrations/alias work: {stats20:?}"
    );
    assert!(
      stats80.total() <= stats20.total().saturating_mul(6),
      "{label} inner work must stay near-linear: 20={} 40={} 80={} comparisons {}/{}/{} registrations {}/{}/{} alias {}/{}/{}",
      stats20.total(),
      stats40.total(),
      stats80.total(),
      stats20.identity_comparisons,
      stats40.identity_comparisons,
      stats80.identity_comparisons,
      stats20.identity_registrations,
      stats40.identity_registrations,
      stats80.identity_registrations,
      stats20.identity_alias_work,
      stats40.identity_alias_work,
      stats80.identity_alias_work
    );
  }
  assert_linear(
    "independent sources",
    20,
    40,
    80,
    &independent(20, 8),
    &independent(40, 8),
    &independent(80, 8),
  );
  assert_linear(
    "shared source",
    20,
    40,
    80,
    &shared_source(20, 20),
    &shared_source(40, 40),
    &shared_source(80, 80),
  );
  assert_linear(
    "shared callback",
    20,
    40,
    80,
    &shared_callback(20),
    &shared_callback(40),
    &shared_callback(80),
  );
  assert_linear(
    "multiple resources",
    20,
    40,
    80,
    &multiple_resources(20),
    &multiple_resources(40),
    &multiple_resources(80),
  );
}

#[test]
fn watch_cleanup_current_source_review_shapes_scale() {
  fn prefix_then_watchers(count: usize) -> String {
    let mut out = String::from(
      "import {ref,watch} from 'vue'; const handler=()=>{}; const initial=new EventTarget(); const source=ref(initial);\n",
    );
    out.push_str(&"source.value=new EventTarget();\n".repeat(count));
    out.push_str(
      &"watch(source,(target,prev,onCleanup)=>{target.addEventListener('click',handler);onCleanup(()=>{source.value.removeEventListener('click',handler)})},{immediate:true,flush:'sync'});\n".repeat(count),
    );
    out.push_str("source.value=new EventTarget();\n");
    out
  }
  fn watchers_then_same_writes(count: usize) -> String {
    let mut out = String::from(
      "import {ref,watch} from 'vue'; const handler=()=>{}; const initial=new EventTarget(); const source=ref(initial);\n",
    );
    out.push_str(
      &"watch(source,(target,prev,onCleanup)=>{target.addEventListener('click',handler);onCleanup(()=>{source.value.removeEventListener('click',handler)})},{immediate:true,flush:'sync'});\n".repeat(count),
    );
    out.push_str(&"source.value=initial;\n".repeat(count));
    out.push_str("source.value=new EventTarget();\n");
    out
  }
  fn separate_cleanups(count: usize) -> String {
    let mut out = String::from(
      "import {ref,watch} from 'vue'; const handler=()=>{}; const initial=new EventTarget(); const source=ref(initial);\n",
    );
    out.push_str("watch(source,(target,prev,onCleanup)=>{\n");
    for index in 0..count {
      out.push_str("target.addEventListener('event");
      out.push_str(&index.to_string());
      out.push_str("',handler); onCleanup(()=>{source.value.removeEventListener('event");
      out.push_str(&index.to_string());
      out.push_str("',handler)});\n");
    }
    out.push_str("},{immediate:true,flush:'sync'});\nsource.value=new EventTarget();\n");
    out
  }
  fn written_alias_chains(count: usize) -> String {
    let mut out = String::from(
      "import {ref,watch} from 'vue'; const handler=()=>{}; const initial=new EventTarget(); const source=ref(initial);\n",
    );
    out.push_str("let replacement=initial;watch(source,(target,prev,onCleanup)=>{target.addEventListener('click',handler);onCleanup(()=>{source.value.removeEventListener('click',handler)})},{immediate:true,flush:'sync'});\n");
    for _ in 0..count {
      out.push_str("replacement=new EventTarget();source.value=replacement;\n");
    }
    out
  }
  fn payload_flow_chain(count: usize) -> String {
    let mut out = String::from(
      "import {ref,watch} from 'vue'; const handler=()=>{}; const initial=new EventTarget(); const source=ref(initial);\nlet x0=initial;\n",
    );
    for index in 1..count {
      out.push_str("let x");
      out.push_str(&index.to_string());
      out.push_str("=x");
      out.push_str(&(index.saturating_sub(1)).to_string());
      out.push_str(";\n");
    }
    out.push_str("Reflect.set(x");
    out.push_str(&(count.saturating_sub(1)).to_string());
    out.push_str(",'addEventListener',()=>{});watch(source,(target,prev,onCleanup)=>{target.addEventListener('click',handler);onCleanup(()=>{source.value.removeEventListener('click',handler)})},{immediate:true,flush:'sync'});source.value=new EventTarget();\n");
    out
  }
  for (label, source_at, expected_facts) in [
    ("prefix", prefix_then_watchers as fn(usize) -> String, true),
    ("suffix", watchers_then_same_writes as fn(usize) -> String, true),
    ("cleanup", separate_cleanups as fn(usize) -> String, true),
    ("binding", written_alias_chains as fn(usize) -> String, false),
    ("flow", payload_flow_chain as fn(usize) -> String, false),
  ] {
    let mut totals = [0; 4];
    for (slot, count) in [20, 40, 80, 160].into_iter().enumerate() {
      let (facts, stats) = cleanup_identity_visits(&source_at(count));
      let want = if expected_facts { count } else { 0 };
      assert_eq!(facts, want, "{label} N={count} facts");
      assert!(
        stats.identity_construction > 0
          && stats.identity_queries > 0
          && stats.identity_comparisons > 0
          && stats.identity_reference_visits > 0,
        "{label} N={count} must count construction/query/comparison/reference visits: {stats:?}"
      );
      if label == "flow" {
        assert!(
          stats.identity_copies > 0 && stats.identity_alias_work > 0,
          "{label} N={count} must count seed/flow edge visits: {stats:?}"
        );
      }
      let expected_visits = match label {
        "prefix" | "suffix" => count.saturating_mul(3).saturating_add(1),
        "cleanup" | "binding" | "flow" => count.saturating_add(2),
        _ => 0,
      };
      assert!(
        stats.identity_reference_visits >= expected_visits,
        "{label} N={count} must pre-index at least each source reference once: {stats:?}"
      );
      if let Some(total) = totals.get_mut(slot) {
        *total = stats.total();
      }
    }
    let Some(&total20) = totals.first() else {
      continue;
    };
    let Some(&total80) = totals.get(2) else {
      continue;
    };
    assert!(
      total80 <= total20.saturating_mul(6),
      "{label} 80/20 must stay <= 6: 20={total20} 80={total80}"
    );
  }
}
