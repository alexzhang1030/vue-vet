use super::support::*;

#[test]
fn derivation_practice_sync_ref_one_way_collects_closed_sink() {
  let facts = assert_gated_matches_forced(
    "import { ref } from 'vue';\
     import { syncRef } from '@vueuse/core';\
     const left = ref(1);\
     const right = ref(0);\
     syncRef(left, right);\
     left.value = 5;\
     void right.value;",
    ScriptKind::Setup,
  );
  assert_eq!(facts.derivation_practice.sync_ref_one_way.len(), 1, "{facts:?}");
  let shared = assert_gated_matches_forced(
    "import { ref } from 'vue';\
     import { syncRef } from '@vueuse/shared';\
     const left = ref(1);\
     const right = ref(0);\
     syncRef(left, right, { direction: 'both', flush: 'sync', deep: false, immediate: true, transform: {} });\
     left.value = 6;\
     void right.value;",
    ScriptKind::Setup,
  );
  assert_eq!(shared.derivation_practice.sync_ref_one_way.len(), 1, "{shared:?}");
}

#[test]
fn derivation_practice_sync_ref_stays_quiet_for_controls() {
  for source in [
    "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref(1); const right = ref(0); syncRef(left, right, { direction: 'ltr' }); left.value = 5; void right.value;",
    "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref(1); const right = ref(0); syncRef(left, right); right.value = 9; left.value = 5; void right.value;",
    "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref({ n: 1 }); const right = ref({ n: 0 }); syncRef(left, right); left.value = { n: 2 }; void right.value;",
    "import { ref } from 'vue'; function syncRef(a, b) { void a; void b; } const left = ref(1); const right = ref(0); syncRef(left, right); left.value = 5; void right.value;",
    "import { ref } from 'vue'; import { syncRef } from '#imports'; const left = ref(1); const right = ref(0); syncRef(left, right); left.value = 5; void right.value;",
    "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref(1); const right = ref(0); syncRef(left, right, { transform: { ltr: (v) => v } }); left.value = 5; void right.value;",
    "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref(1); const right = ref(0); syncRef(left, right, { direction: 'r' + 'tl' }); left.value = 5; void right.value;",
    "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref(1); const right = ref(0); const stop = syncRef(left, right); stop(); left.value = 5; void right.value;",
    "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref(1); const right = ref(0); if (false) syncRef(left, right); left.value = 5; void right.value;",
  ] {
    let facts = contract_collect(source, ScriptKind::Setup, true).0;
    assert!(
      facts.derivation_practice.sync_ref_one_way.is_empty(),
      "must stay quiet: {source} {facts:?}"
    );
  }
}

#[test]
fn derivation_practice_conditional_watch_collects_idle_producer() {
  let facts = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const flag = ref(false);\
     const source = ref(2);\
     const sink = ref(5);\
     const heavy = computed(() => source.value);\
     watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; });\
     source.value = 3;",
    ScriptKind::Setup,
  );
  assert_eq!(facts.derivation_practice.conditional_watch_source.len(), 1, "{facts:?}");
}

#[test]
fn derivation_practice_conditional_watch_stays_quiet_for_controls() {
  for source in [
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; console.log(value); }); source.value = 3;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }); void heavy.value; source.value = 3;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(true); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }); source.value = 3;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); watch([flag, heavy], ([active, value], old) => { if (active) sink.value = value; void old; }); source.value = 3;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const events = []; const heavy = computed(() => source.value); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }, { flush: 'sync', onTrigger: event => events.push(event.type) }); source.value = 3;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const oneRun = true; const heavy = computed(() => source.value); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }, { flush: 'sync', once: oneRun }); source.value = 3; function enable(target: { value: boolean }) { target.value = true; } enable(flag); void sink.value;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const events = []; const heavy = computed(() => source.value, { onTrack: event => events.push(event.type) }); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }, { flush: 'sync' }); source.value = 3;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }); function unused() { source.value = 3; }",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); if (false) watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }); source.value = 3;",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }); false && (source.value = 3);",
    "import { computed, ref, watch } from 'vue'; const flag = ref(false); const source = ref(2); const sink = ref(5); const heavy = computed(() => source.value); function enable(target: { value: boolean }) { target.value = true; } enable(flag); watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }, { flush: 'sync' }); source.value = 3;",
  ] {
    let facts = contract_collect(source, ScriptKind::Setup, true).0;
    assert!(
      facts.derivation_practice.conditional_watch_source.is_empty(),
      "must stay quiet: {source} {facts:?}"
    );
  }
}

#[test]
#[expect(clippy::panic, reason = "independent syncRef fixture construction must fail the test")]
fn derivation_practice_independent_sources_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [50_u64, 100, 200] {
    let mut source =
      String::from("import { ref } from 'vue'; import { syncRef } from '@vueuse/core';");
    for index in 0..size {
      write!(
        source,
        "const l{index} = ref(1); const r{index} = ref(0); syncRef(l{index}, r{index}); l{index}.value = 5; void r{index}.value;"
      )
      .unwrap_or_else(|error| panic!("independent syncRef fixture write: {error}"));
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.derivation_practice.sync_ref_one_way.len(),
      usize::try_from(size).unwrap_or(usize::MAX)
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "independent syncRef work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
#[expect(clippy::panic, reason = "alias fanout fixture construction must fail the test")]
fn derivation_practice_alias_fanout_and_deep_wrappers_stay_linear() {
  fn source(aliases: usize) -> String {
    let mut out = String::from(
      "import { computed, ref, watch } from 'vue'; import { syncRef } from '@vueuse/core'; const left = ref(1); const right = ref(0);",
    );
    let mut current = String::from("right");
    for index in 0..aliases {
      write!(out, "const a{index} = {current};")
        .unwrap_or_else(|error| panic!("alias fanout fixture write: {error}"));
      current = format!("a{index}");
    }
    out.push_str("syncRef(left, right); left.value = 5; void right.value;");
    out.push_str("const flag = ref(false); const src = ref(2); const sink = ref(5);");
    out.push_str("const heavy = computed(() => src.value);");
    out.push_str("watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; }); src.value = 3;");
    out
  }
  let (small, small_work) = contract_stats(&source(20));
  let (large, large_work) = contract_stats(&source(80));
  assert_eq!(small.derivation_practice.sync_ref_one_way.len(), 1, "{small:?}");
  assert_eq!(large.derivation_practice.sync_ref_one_way.len(), 1, "{large:?}");
  assert!(
    large_work <= small_work.saturating_mul(8),
    "alias fanout work must stay near-linear: 20={small_work} 80={large_work}"
  );
}

#[test]
#[expect(clippy::panic, reason = "many-consumer fixture construction must fail the test")]
fn derivation_practice_one_source_many_consumers_stays_linear() {
  fn source(consumers: usize) -> String {
    let mut out = String::from(
      "import { computed, ref, watch } from 'vue'; const flag = ref(false); const src = ref(2); const heavy = computed(() => src.value);",
    );
    for index in 0..consumers {
      write!(
        out,
        "const sink{index} = ref(5); watch([flag, heavy], ([active, value]) => {{ if (active) sink{index}.value = value; }});"
      )
      .unwrap_or_else(|error| panic!("many-consumer fixture write: {error}"));
    }
    out.push_str("src.value = 3;");
    out
  }
  let (small, small_work) = contract_stats(&source(20));
  let (large, large_work) = contract_stats(&source(80));
  assert!(
    small.derivation_practice.conditional_watch_source.is_empty(),
    "extra consumers keep sole-demand unknown: {small:?}"
  );
  assert!(
    large.derivation_practice.conditional_watch_source.is_empty(),
    "extra consumers keep sole-demand unknown: {large:?}"
  );
  assert!(
    large_work <= small_work.saturating_mul(8),
    "one source many consumers must stay near-linear: 20={small_work} 80={large_work}"
  );
}

#[test]
#[expect(clippy::panic, reason = "joint alias/consumer fixture construction must fail the test")]
fn derivation_practice_joint_alias_and_consumer_fanout_stays_linear() {
  fn source(size: usize) -> String {
    let mut out = String::from(
      "import { computed, ref, watch } from 'vue'; const flag = ref(false); const src = ref(2); const heavy = computed(() => src.value);",
    );
    let mut current = String::from("heavy");
    for index in 0..size {
      write!(out, "const a{index} = {current};")
        .unwrap_or_else(|error| panic!("joint alias write: {error}"));
      current = format!("a{index}");
    }
    for index in 0..size {
      write!(
        out,
        "const sink{index} = ref(5); watch([flag, a{}], ([active, value]) => {{ if (active) sink{index}.value = value; }});",
        size.saturating_sub(1)
      )
      .unwrap_or_else(|error| panic!("joint consumer write: {error}"));
    }
    out.push_str("src.value = 3;");
    out
  }
  let mut previous: Option<(usize, u64)> = None;
  for size in [20_usize, 40, 80] {
    let (facts, work) = contract_stats(&source(size));
    assert!(
      facts.derivation_practice.conditional_watch_source.is_empty(),
      "joint fanout stays sole-demand unknown: {facts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "joint alias+consumer work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn derivation_practice_deep_wrappers_count_inner_traversal() {
  fn source(depth: usize) -> String {
    let mut getter = String::from("src.value");
    for _ in 0..depth {
      getter.insert(0, '(');
      getter.push(')');
    }
    format!(
      "import {{ computed, ref, watch }} from 'vue'; const flag = ref(false); const src = ref(2); const sink = ref(5); const heavy = computed(() => {getter}); watch([flag, heavy], ([active, value]) => {{ if (active) sink.value = value; }}); src.value = 3;"
    )
  }
  let (shallow, shallow_work) = contract_stats(&source(4));
  let (deep, deep_work) = contract_stats(&source(32));
  assert_eq!(shallow.derivation_practice.conditional_watch_source.len(), 1, "{shallow:?}");
  assert_eq!(deep.derivation_practice.conditional_watch_source.len(), 1, "{deep:?}");
  assert!(
    deep_work > shallow_work,
    "deeper wrappers must record extra peel work: 4={shallow_work} 32={deep_work}"
  );
  assert!(
    deep_work < shallow_work.saturating_mul(20),
    "wrapper peel must stay bounded: 4={shallow_work} 32={deep_work}"
  );
}

#[test]
fn derivation_practice_records_supported_alias_names() {
  let sync = assert_gated_matches_forced(
    "import { ref } from 'vue';\
     import { syncRef } from '@vueuse/core';\
     const left = ref(1);\
     const right = ref(0);\
     const alias = right;\
     syncRef(left, right);\
     left.value = 5;\
     void right.value;",
    ScriptKind::Setup,
  );
  let sink = sync.derivation_practice.sync_ref_one_way.first();
  assert!(
    sink.is_some_and(|site| site.sink_names.iter().any(|name| name == "alias")
      && site.sink_names.iter().any(|name| name == "right")),
    "sink aliases must be recorded: {sync:?}"
  );
  let watch = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const flag = ref(false);\
     const source = ref(2);\
     const sink = ref(5);\
     const heavy = computed(() => source.value);\
     const shown = heavy;\
     watch([flag, heavy], ([active, value]) => { if (active) sink.value = value; });\
     source.value = 3;",
    ScriptKind::Setup,
  );
  let producer = watch.derivation_practice.conditional_watch_source.first();
  assert!(
    producer.is_some_and(|site| site.producer_names.iter().any(|name| name == "shown")
      && site.producer_names.iter().any(|name| name == "heavy")),
    "producer aliases must be recorded: {watch:?}"
  );
}

#[test]
#[expect(clippy::panic, reason = "shared-right fixture construction must fail the test")]
fn derivation_practice_shared_right_many_sync_calls_stays_linear() {
  fn source(size: usize) -> String {
    let mut out = String::from(
      "import { ref } from 'vue'; import { syncRef } from '@vueuse/core'; const right = ref(0);",
    );
    for index in 0..size {
      write!(out, "const l{index} = ref(1); syncRef(l{index}, right); l{index}.value = 5;")
        .unwrap_or_else(|error| panic!("shared-right fixture write: {error}"));
    }
    out.push_str("void right.value;");
    out
  }
  let mut previous: Option<(usize, u64)> = None;
  for size in [20_usize, 40, 80] {
    let (facts, work) = contract_stats(&source(size));
    assert!(
      facts.derivation_practice.sync_ref_one_way.is_empty(),
      "shared right stays closed-sink unknown: {facts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-right many-sync-calls work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}
