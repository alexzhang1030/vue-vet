use super::support::*;

#[test]
fn scheduling_practice_queued_flush_collects_after_next_tick() {
  let facts = assert_gated_matches_forced(
    "import { nextTick, ref, watch } from 'vue';\
     const source = ref(0);\
     const sink = ref(0);\
     watch(source, (value) => { sink.value = value; }, { flush: 'sync' });\
     source.value = 1;\
     source.value = 2;\
     await nextTick();\
     void sink.value;",
    ScriptKind::Setup,
  );
  assert_eq!(facts.scheduling_practice.queued_watch_flush.len(), 1, "{facts:?}");
}

#[test]
fn scheduling_practice_queued_flush_stays_quiet_for_controls() {
  for source in [
    "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); watch(source, (value) => { sink.value = value; }); source.value = 1; source.value = 2; await nextTick(); void sink.value;",
    "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); source.value = 1; void sink.value; source.value = 2; await nextTick(); void sink.value;",
    "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); const stop = watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); source.value = 1; source.value = 2; stop(); await nextTick(); void sink.value;",
    "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); watch(source, (value) => { sink.value = value; }, { flush: 'sync', once: true }); source.value = 1; source.value = 2; await nextTick(); void sink.value;",
    "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(9); watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); source.value = 1; source.value = 0; await nextTick(); void sink.value;",
    "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); const observed = ref(0); watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); source.value = 1; source.value = 2; observed.value = sink.value; await nextTick(); void sink.value;",
    "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); source.value = 1; source.value = 2; if (false) { await nextTick(); } void sink.value;",
    "import { effectScope, nextTick, ref, watch } from 'vue'; const parent = effectScope(); parent.run(async () => { const source = ref(0); const sink = ref(0); watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); source.value = 1; source.value = 2; await nextTick(); void sink.value; }); parent.stop();",
  ] {
    let facts = contract_collect(source, ScriptKind::Setup, true).0;
    assert!(
      facts.scheduling_practice.queued_watch_flush.is_empty(),
      "must stay quiet: {source} {facts:?}"
    );
  }
}

#[test]
fn scheduling_practice_attached_scope_collects_paused_child() {
  let facts = assert_gated_matches_forced(
    "import { effectScope, onScopeDispose, ref, watch } from 'vue';\
     const source = ref(0);\
     const sink = ref(0);\
     const parent = effectScope();\
     parent.run(() => {\
       const child = effectScope(true);\
       child.run(() => { watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); });\
       onScopeDispose(() => child.stop());\
     });\
     parent.pause();\
     source.value = 2;\
     source.value = 3;\
     parent.resume();\
     void sink.value;",
    ScriptKind::Setup,
  );
  assert_eq!(facts.scheduling_practice.attached_effect_scope.len(), 1, "{facts:?}");
}

#[test]
fn scheduling_practice_attached_scope_stays_quiet_for_controls() {
  for source in [
    "import { effectScope, onScopeDispose, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); const parent = effectScope(); parent.run(() => { const child = effectScope(); child.run(() => { watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); }); onScopeDispose(() => child.stop()); }); parent.pause(); source.value = 2; source.value = 3; parent.resume(); void sink.value;",
    "import { effectScope, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); const parent = effectScope(); parent.run(() => { const child = effectScope(true); child.run(() => { watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); }); }); parent.pause(); source.value = 2; source.value = 3; parent.resume(); void sink.value;",
    "import { effectScope, onScopeDispose, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); const parent = effectScope(); parent.run(() => { const child = effectScope(true); child.run(() => { watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); }); onScopeDispose(() => child.stop()); child.pause(); }); parent.pause(); source.value = 2; source.value = 3; parent.resume(); void sink.value;",
    "import { effectScope, onScopeDispose, ref, watch } from 'vue'; const source = ref(0); const sink = ref(9); const parent = effectScope(); parent.run(() => { const child = effectScope(true); child.run(() => { watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); }); onScopeDispose(() => child.stop()); }); parent.pause(); source.value = 1; source.value = 0; parent.resume(); void sink.value;",
    "import { effectScope, onScopeDispose, ref, watch, watchSyncEffect } from 'vue'; const source = ref(0); const sink = ref(0); const parent = effectScope(); parent.run(() => { const child = effectScope(true); child.run(() => { watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); }); onScopeDispose(() => child.stop()); }); const seen = []; watchSyncEffect(() => { seen.push(sink.value); }); parent.pause(); source.value = 2; source.value = 3; parent.resume(); void sink.value;",
    "import { effectScope, onScopeDispose, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0); const parent = effectScope(); parent.run(() => { const child = effectScope(true); child.run(() => { watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); }); onScopeDispose(() => child.stop()); }); if (false) { parent.pause(); } source.value = 2; source.value = 3; parent.resume(); void sink.value;",
  ] {
    let facts = contract_collect(source, ScriptKind::Setup, true).0;
    assert!(
      facts.scheduling_practice.attached_effect_scope.is_empty(),
      "must stay quiet: {source} {facts:?}"
    );
  }
}

#[test]
fn scheduling_practice_lazy_async_collects_eager_startup() {
  let facts = assert_gated_matches_forced(
    "import { ref, watch } from 'vue';\
     import { computedAsync } from '@vueuse/core';\
     const source = ref(1);\
     const sink = ref(0);\
     const value = computedAsync(async () => source.value * 10, -1);\
     source.value = 3;\
     watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    ScriptKind::Setup,
  );
  assert_eq!(facts.scheduling_practice.lazy_computed_async.len(), 1, "{facts:?}");
}

#[test]
fn scheduling_practice_lazy_async_stays_quiet_for_controls() {
  for source in [
    "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, -1, { lazy: true }); source.value = 3; watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); export const value = computedAsync(async () => source.value * 10, -1); source.value = 3; watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, -1); source.value = 3; value.value = 99; watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, -1); source.value = 3; void value.value; watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, -1, { lazy: !false }); source.value = 3; watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    "import { nextTick, ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, -1); source.value = 3; await nextTick(); await nextTick(); watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true, once: true });",
    "import { nextTick, ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, -1); source.value = 3; await nextTick(); await nextTick(); const stop = watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true }); stop();",
    "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); let currentInput = 1; Object.defineProperty(source, 'value', { get() { return currentInput; }, set(_value) {} }); const value = computedAsync(async () => source.value * 10, -1); source.value = 3; currentInput = 3; watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, -1); function neverCalled() { source.value = 3; } watch(value, (current) => { if (current !== -1) sink.value = current; }, { immediate: true });",
    "import { nextTick, ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const sink = ref(0); const value = computedAsync(async () => source.value * 10, 'loading'); source.value = 3; await nextTick(); await nextTick(); watch(value, (current) => { if (current !== 'missing') sink.value = current; }, { immediate: true });",
  ] {
    let facts = contract_collect(source, ScriptKind::Setup, true).0;
    assert!(
      facts.scheduling_practice.lazy_computed_async.is_empty(),
      "must stay quiet: {source} {facts:?}"
    );
  }
}

#[test]
#[expect(clippy::format_push_string, reason = "geometric fixture assembly is clearer as push_str")]
fn scheduling_practice_independent_sources_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [50_u64, 100, 200] {
    let mut source = String::from("import { nextTick, ref, watch } from 'vue';");
    for index in 0..size {
      source.push_str(&format!(
        "const s{index} = ref(0); const k{index} = ref(0); watch(s{index}, (value) => {{ k{index}.value = value; }}, {{ flush: 'sync' }}); s{index}.value = 1; s{index}.value = 2;"
      ));
    }
    source.push_str("await nextTick();");
    for index in 0..size {
      source.push_str(&format!("void k{index}.value;"));
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.scheduling_practice.queued_watch_flush.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "{contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "independent queued-flush work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
#[expect(clippy::format_push_string, reason = "geometric fixture assembly is clearer as push_str")]
fn scheduling_practice_alias_fanout_and_wrappers_stay_linear() {
  fn source(aliases: usize) -> String {
    let mut out = String::from(
      "import { nextTick, ref, watch } from 'vue'; const source = ref(0); const sink = ref(0);",
    );
    let mut current = String::from("sink");
    for index in 0..aliases {
      out.push_str(&format!("const a{index} = {current};"));
      current = format!("a{index}");
    }
    out.push_str("watch(source, (value) => { sink.value = value; }, { flush: 'sync' }); source.value = 1; source.value = 2; await nextTick(); void sink.value;");
    out
  }
  let (small, small_work) = contract_stats(&source(20));
  let (large, large_work) = contract_stats(&source(80));
  assert_eq!(small.scheduling_practice.queued_watch_flush.len(), 1, "{small:?}");
  assert_eq!(large.scheduling_practice.queued_watch_flush.len(), 1, "{large:?}");
  assert!(
    large_work <= small_work.saturating_mul(8),
    "alias fanout work must stay near-linear: 20={small_work} 80={large_work}"
  );
}

#[test]
#[expect(clippy::format_push_string, reason = "geometric fixture assembly is clearer as push_str")]
fn scheduling_practice_one_source_many_consumers_stays_linear() {
  fn source(consumers: usize) -> String {
    let mut out = String::from(
      "import { ref, watch } from 'vue'; import { computedAsync } from '@vueuse/core'; const source = ref(1); const value = computedAsync(async () => source.value * 10, -1);",
    );
    for index in 0..consumers {
      out.push_str(&format!(
        "const sink{index} = ref(0); watch(value, (current) => {{ if (current !== -1) sink{index}.value = current; }}, {{ immediate: true }});"
      ));
    }
    out.push_str("source.value = 3;");
    out
  }
  let (small, small_work) = contract_stats(&source(20));
  let (large, large_work) = contract_stats(&source(80));
  assert!(
    small.scheduling_practice.lazy_computed_async.is_empty(),
    "extra consumers keep sole-demand unknown: {small:?}"
  );
  assert!(
    large.scheduling_practice.lazy_computed_async.is_empty(),
    "extra consumers keep sole-demand unknown: {large:?}"
  );
  assert!(
    large_work <= small_work.saturating_mul(8),
    "one source many consumers must stay near-linear: 20={small_work} 80={large_work}"
  );
}

#[test]
#[expect(clippy::format_push_string, reason = "geometric fixture assembly is clearer as push_str")]
fn scheduling_practice_joint_producer_consumer_owner_growth_stays_linear() {
  fn measure(kind: &str, size: u64) -> (usize, u64) {
    let n = usize::try_from(size).unwrap_or(usize::MAX);
    let mut source = String::from(
      "import { ref, watch, nextTick, effectScope, onScopeDispose } from 'vue'; import { computedAsync } from '@vueuse/core'; const shared = ref(1);",
    );
    if kind == "lazy-shared-source" {
      for index in 0..n {
        source.push_str(&format!(
          "const sink{index}=ref(0);const value{index}=computedAsync(async()=>shared.value*10,-1);"
        ));
      }
      source.push_str("shared.value=3;");
      for index in 0..n {
        source.push_str(&format!(
          "watch(value{index},(current)=>{{if(current!==-1)sink{index}.value=current}},{{immediate:true}});"
        ));
      }
    } else if kind == "attached-shared-source" {
      for index in 0..n {
        source.push_str(&format!(
          "const sink{index}=ref(0);const parent{index}=effectScope();parent{index}.run(()=>{{const child{index}=effectScope(true);child{index}.run(()=>{{watch(shared,(value)=>{{sink{index}.value=value}},{{flush:'sync'}})}});onScopeDispose(()=>child{index}.stop());}});"
        ));
      }
      for index in 0..n {
        source.push_str(&format!("parent{index}.pause();"));
      }
      source.push_str("shared.value=2;shared.value=3;");
      for index in 0..n {
        source.push_str(&format!("parent{index}.resume();void sink{index}.value;"));
      }
    } else {
      for index in 0..n {
        source.push_str(&format!(
          "const source{index}=ref(0);const sink{index}=ref(0);watch(source{index},(value)=>{{sink{index}.value=value}},{{flush:'sync'}});source{index}.value=1;source{index}.value=2;await nextTick();void sink{index}.value;"
        ));
      }
    }
    let (facts, work) = contract_stats(&source);
    let (forced, _) = contract_collect(&source, ScriptKind::Setup, true);
    assert_eq!(facts, forced, "eligible/forced fact parity for {kind} n={size}");
    let count = facts.scheduling_practice.queued_watch_flush.len()
      + facts.scheduling_practice.attached_effect_scope.len()
      + facts.scheduling_practice.lazy_computed_async.len();
    (count, work)
  }

  for kind in ["lazy-shared-source", "attached-shared-source", "queued-distinct-ticks"] {
    let mut previous: Option<(u64, u64)> = None;
    for size in [32_u64, 64, 128] {
      let (facts, work) = measure(kind, size);
      assert_eq!(facts, usize::try_from(size).unwrap_or(usize::MAX), "{kind} n={size} facts");
      if let Some((prev_size, prev_work)) = previous {
        assert_eq!(size, prev_size * 2, "fixture sizes must double");
        assert!(
          work.saturating_mul(10) < prev_work.saturating_mul(30),
          "{kind} work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
        );
      }
      previous = Some((size, work));
    }
  }
}
