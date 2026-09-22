use super::support::*;

#[test]
fn extracts_returned_watcher_cleanup_and_skips_unknowns() {
  let facts = analyze(
    "import { watch, watchEffect } from 'vue';\
     const source = { value: 0 };\
     watchEffect(() => { return () => {}; });\
     watch(source, () => { const cleanup = () => {}; return cleanup; });\
     watchEffect(() => { const nested = () => { return () => {}; }; nested(); });\
     watch(() => { return () => source.value; }, () => {});",
    "ts",
  );
  assert_eq!(
    facts.lifetime.returned_watcher_cleanups.len(),
    2,
    "only proven callback returns; got {:?}",
    facts.lifetime.returned_watcher_cleanups
  );
}

#[test]
fn extracts_late_cleanup_await_and_keeps_oncleanup_quiet() {
  let facts = analyze(
    "import { onWatcherCleanup, watchEffect } from 'vue';\
     watchEffect(async () => { await Promise.resolve(); onWatcherCleanup(() => {}); });\
     watchEffect(async (onCleanup) => { await Promise.resolve(); onCleanup(() => {}); });",
    "ts",
  );
  assert_eq!(
    facts.lifetime.late_watcher_cleanups.len(),
    1,
    "callback-bound onCleanup after await must stay quiet; got {:?}",
    facts.lifetime.late_watcher_cleanups
  );
}

#[test]
fn extracts_orphaned_scope_watcher_and_late_dispose() {
  let facts = analyze(
    "import { effectScope, onScopeDispose, watchEffect } from 'vue';\
     const scope = effectScope();\
     scope.run(async () => { await Promise.resolve(); watchEffect(() => {}); onScopeDispose(() => {}); });\
     scope.run(async () => { await Promise.resolve(); onScopeDispose(() => {}, true); });\
     scope.run(async () => { await Promise.resolve(); scope.run(() => { watchEffect(() => {}); }); });",
    "ts",
  );
  assert_eq!(
    facts.lifetime.orphaned_scope_watchers.len(),
    1,
    "sync re-entry and failSilently must stay quiet; {:?}",
    facts.lifetime
  );
  assert_eq!(
    facts.lifetime.late_scope_disposes.len(),
    1,
    "{:?}",
    facts.lifetime.late_scope_disposes
  );
}

#[test]
fn extracts_nested_watch_without_cleanup_and_keeps_controls_quiet() {
  let facts = analyze(
    "import { ref, watch } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     watch(outer, () => { watch(inner, () => {}, { flush: 'sync' }); }, { flush: 'sync' });\
     watch(outer, () => { const stop = watch(inner, () => {}, { flush: 'sync' }); stop(); }, { flush: 'sync' });\
     watch(outer, () => { watch(inner, () => {}, { flush: 'sync' }); }, { flush: 'sync', once: true });\
     watch(outer, () => { const local = ref(0); watch(local, () => {}, { flush: 'sync' }); }, { flush: 'sync' });\
     watch(outer, () => { if (true) watch(inner, () => {}, { flush: 'sync' }); }, { flush: 'sync' });",
    "ts",
  );
  assert_eq!(
    facts.lifetime.nested_watch_without_cleanups.len(),
    1,
    "only discarded repeating inner with external source: {:?}",
    facts.lifetime.nested_watch_without_cleanups
  );
}

#[test]
fn extracts_detached_scope_without_stop_and_suppresses_nested_child() {
  let facts = analyze(
    "import { effectScope, ref, watch } from 'vue';\
     const outer = ref(0);\
     const inner = ref(0);\
     watch(outer, () => {\
       const scope = effectScope(true);\
       scope.run(() => { watch(inner, () => {}, { flush: 'sync' }); });\
     }, { flush: 'sync' });\
     watch(outer, () => {\
       const owned = effectScope(true);\
       owned.run(() => { watch(inner, () => {}, { flush: 'sync' }); });\
       return () => owned.stop();\
     }, { flush: 'sync' });\
     function factory() {\
       const scope = effectScope(true);\
       scope.run(() => { watch(inner, () => {}, { flush: 'sync' }); });\
     }\
     factory();",
    "ts",
  );
  assert_eq!(
    facts.lifetime.detached_effect_scopes_without_stop.len(),
    1,
    "function factories and returned disposers stay quiet: {:?}",
    facts.lifetime.detached_effect_scopes_without_stop
  );
  assert!(
    facts.lifetime.nested_watch_without_cleanups.is_empty(),
    "detached scope must suppress nested child: {:?}",
    facts.lifetime.nested_watch_without_cleanups
  );
}
