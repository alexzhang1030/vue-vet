use super::support::*;

#[test]
fn custom_then_and_timeout_data_slot_are_not_deferred() {
  let facts = analyze(
    "import { onWatcherCleanup, watchEffect } from 'vue';\
     const immediate = { then(fn) { fn(); } };\
     watchEffect(() => { immediate.then(() => { onWatcherCleanup(() => {}); }); });\
     watchEffect(() => { setTimeout(() => {}, 0, () => { onWatcherCleanup(() => {}); }); });",
    "ts",
  );
  assert!(
    facts.lifetime.late_watcher_cleanups.is_empty(),
    "unproven then / data-slot timeout must stay quiet: {:?}",
    facts.lifetime.late_watcher_cleanups
  );
}

#[test]
fn custom_ref_lost_notification_emits_track_and_trigger_reasons() {
  let lost_track = analyze(
    &custom_ref_source(
      "const r = customRef((_track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r, () => {}); r.value = 1;",
    ),
    "ts",
  );
  assert_eq!(
    lost_track.source_contracts.custom_ref_lost_notification.len(),
    1,
    "{:?}",
    lost_track.source_contracts
  );
  assert_eq!(
    lost_track.source_contracts.custom_ref_lost_notification.first().map(|site| site.reason),
    Some(vue_vet_core::CustomRefLostNotificationReason::GetTracking)
  );
  let lost_trigger = analyze(
    &custom_ref_source(
      "const r = customRef((track, _trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    "ts",
  );
  assert_eq!(
    lost_trigger.source_contracts.custom_ref_lost_notification.len(),
    1,
    "{:?}",
    lost_trigger.source_contracts
  );
  assert_eq!(
    lost_trigger.source_contracts.custom_ref_lost_notification.first().map(|site| site.reason),
    Some(vue_vet_core::CustomRefLostNotificationReason::SetNotification)
  );
  let track_in_setter = analyze(
    &custom_ref_source(
      "const r = customRef((track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { track(); value = next; trigger(); } }; }); watch(r, () => {}); r.value = 1;",
    ),
    "ts",
  );
  assert_eq!(
    track_in_setter.source_contracts.custom_ref_lost_notification.len(),
    1,
    "{:?}",
    track_in_setter.source_contracts
  );
  assert_eq!(
    track_in_setter.source_contracts.custom_ref_lost_notification.first().map(|site| site.reason),
    Some(vue_vet_core::CustomRefLostNotificationReason::GetTracking)
  );
  let unused_branch = analyze(
    &custom_ref_source(
      "const r = customRef((_track, trigger) => { let value = 0; const initialized = true ? 0 : (value = 1); return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r, () => {}); r.value = 1;",
    ),
    "ts",
  );
  assert_eq!(
    unused_branch.source_contracts.custom_ref_lost_notification.len(),
    1,
    "unused false branch must not unprove storage; {:?}",
    unused_branch.source_contracts
  );
}

#[test]
fn custom_ref_lost_notification_stays_quiet_for_safe_and_unknown() {
  let cases = [
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r, () => {}); r.value = 1;",
    "const backing = ref(0); const r = customRef(() => ({ get() { return backing.value; }, set(next: number) { backing.value = next; } })); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1; triggerRef(r);",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((_track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; trigger(); } }; }); const stop = watch(r, () => {}); stop(); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; trigger(); } }; }); const handle = watchEffect(() => { void r.value; }); handle.pause(); r.value = 1;",
    "function delegate(track: () => void, trigger: () => void) { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; trigger(); } }; } const r = customRef((track, trigger) => delegate(track, trigger)); watch(r, () => {}); r.value = 1;",
    "const factory = (track: () => void, trigger: () => void) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; } }; }; const r = customRef(factory); watch(r, () => {}); r.value = 1;",
    "const r = customRef(() => ({ get: 1, set: 2 })); watch(r, () => {}); r.value = 1;",
    "const r = customRef((_track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); void r.value; r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchPostEffect(() => { void r.value; }); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(() => { void r.value; }, { flush: 'post' }); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); false && watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}, { immediate: true, once: true }); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(() => { return; void r.value; }); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const hooks = { track, trigger }; return { get() { hooks.track(); return value; }, set(next: number) { value = next; hooks.trigger(); } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { (() => track())(); return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); r.value = 1; watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(_next: number) { value = 0; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { if (value == next) return; value = next; } }; }); watch(r, () => {}); r.value = false as never;",
    "const backing = ref(0); const r = customRef((track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; } }; }); r._get = () => backing.value; r._set = (next: number) => { backing.value = next; }; watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value *= next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { next = 0; value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); value = 0; return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const initialized = (value = 1); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const hooks = { track, trigger }; const delegate = hooks; return { get() { delegate.track(); return value; }, set(next: number) { value = next; delegate.trigger(); } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(async () => { await Promise.resolve(); void r.value; }); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { const result = { value: (value = 0) }; track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { const result = { [next = 0]: true }; value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const initialized = { [value = 1]: true }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const initialized = true ? (value = 1) : (value = 0); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const initialized = true && (value = 1); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const initialized = false || (value = 1); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const initialized = Math.random() ? (value = 1) : (value = 0); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, *set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, async set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(async () => { const resolved = { [await Promise.resolve()]: true }; void r.value; }); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1) } = { supplied: true }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((track, trigger) => { let value = 0; const { [value = 0]: supplied } = (value = 1, {}); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1) } = { supplied: undefined }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1), [value = 0]: unused } = {}; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((track, trigger) => { let value = 0; const { missing: { leaf = (value = 1) } = { leaf: true } } = {}; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1) } = { __proto__: { supplied: true } }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((track, trigger) => { let value = 0; const { toString = (value = 1) } = {}; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((track, trigger) => { let value = 0; const undefined = 7; const initialized = undefined ?? (value = 1); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { const result = { Reset: class { static { value = 0 } } }; track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(() => { { return; } void r.value; }); r.value = 1;",
  ];
  for source in cases {
    let facts = analyze(&custom_ref_source(source), "ts");
    assert!(
      facts.source_contracts.custom_ref_lost_notification.is_empty(),
      "quiet control fired: {source} {:?}",
      facts.source_contracts.custom_ref_lost_notification
    );
  }
}

#[test]
fn custom_ref_lost_notification_many_roots_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { customRef, watch } from 'vue';");
    for index in 0..size {
      source.push_str("const r");
      source.push_str(&index.to_string());
      source.push_str(
        " = customRef((_track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r",
      );
      source.push_str(&index.to_string());
      source.push_str(", () => {}); r");
      source.push_str(&index.to_string());
      source.push_str(".value = 1;");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.custom_ref_lost_notification.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "{contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "many-root customRef work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn custom_ref_lost_notification_shared_root_and_dense_negatives_scale() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { customRef, watch } from 'vue'; const r = customRef((_track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; });",
    );
    for index in 0..size {
      source.push_str("const a");
      source.push_str(&index.to_string());
      source.push_str(" = r;");
    }
    source.push_str("watch(r, () => {}); r.value = 1;");
    for index in 0..size {
      source.push_str("const s");
      source.push_str(&index.to_string());
      source.push_str(
        " = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; trigger(); } }; }); watch(s",
      );
      source.push_str(&index.to_string());
      source.push_str(", () => {}); s");
      source.push_str(&index.to_string());
      source.push_str(".value = 1;");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.custom_ref_lost_notification.len(),
      1,
      "shared-root aliases plus dense standard factories must emit one defect; {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-root/dense-negative work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn custom_ref_lost_notification_repeated_writes_and_foreign_handle_calls_scale() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { customRef, watch } from 'vue'; const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); const handle = watch(r, () => {}); function later() {",
    );
    for _ in 0..size {
      source.push_str("handle();");
    }
    source.push('}');
    for _ in 0..size {
      source.push_str("r.value = 0;");
    }
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.custom_ref_lost_notification.is_empty(),
      "same-value writes plus foreign-block handle calls must stay quiet; {contracts:?}"
    );
    assert!(
      work > size.saturating_mul(4),
      "nested inactivity and write visits must be counted for size {size}: work={work}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "repeated-write/foreign-handle work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn custom_ref_lost_notification_many_roots_with_aliases_scale() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [64_u64, 128, 256] {
    let mut source = String::from("import { customRef, watch } from 'vue';");
    for index in 0..size {
      let n = index.to_string();
      source.push_str("const r");
      source.push_str(&n);
      source.push_str(
        " = customRef((_track, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); const a",
      );
      source.push_str(&n);
      source.push_str(" = r");
      source.push_str(&n);
      source.push_str("; const b");
      source.push_str(&n);
      source.push_str(" = a");
      source.push_str(&n);
      source.push_str("; const c");
      source.push_str(&n);
      source.push_str(" = b");
      source.push_str(&n);
      source.push_str("; watch(c");
      source.push_str(&n);
      source.push_str(", () => {}); r");
      source.push_str(&n);
      source.push_str(".value = 1;");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.custom_ref_lost_notification.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "many roots with aliases must retain one finding per root; {contracts:?}"
    );
    assert!(
      work > size.saturating_mul(4),
      "root-member construction and lookups must be counted for size {size}: work={work}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "many-root-with-alias work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn custom_ref_lost_notification_nested_binding_and_branch_scale() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { customRef, watch } from 'vue';");
    for index in 0..size {
      let n = index.to_string();
      source.push_str("const r");
      source.push_str(&n);
      source.push_str(
        " = customRef((_track, trigger) => { let value = 0; const { a: { b: { c = (value = 1) } } } = { a: { b: { c: true } } }; const flag = true ? (true ? 0 : 1) : 1; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r",
      );
      source.push_str(&n);
      source.push_str(", () => {}); r");
      source.push_str(&n);
      source.push_str(".value = 1;");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.custom_ref_lost_notification.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "nested dormant defaults must keep lost-track findings; {contracts:?}"
    );
    assert!(
      work > size.saturating_mul(8),
      "nested binding and branch visits must be counted for size {size}: work={work}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "nested binding/branch work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn custom_ref_lost_notification_factory_binding_width_scales_near_linearly() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [64_u64, 128, 256] {
    let (contracts, work) = contract_stats(&factory_width_source(size));
    assert_eq!(
      contracts.custom_ref_lost_notification.len(),
      1,
      "width {size} must retain one lost-track finding; {contracts:?}"
    );
    assert!(
      work > size.saturating_mul(4),
      "width {size} must count summary and lookup visits: work={work}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(25),
        "factory-binding width work grew from {prev_work} to {work} on {prev_size}->{size} (must stay near-linear, <2.5x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn custom_ref_lost_notification_factory_bind_budget_exhausts_quietly() {
  let supported = analyze(&factory_width_source(256), "ts");
  assert_eq!(
    supported.source_contracts.custom_ref_lost_notification.len(),
    1,
    "supported width must keep the lost-track finding; {:?}",
    supported.source_contracts
  );
  let exhausted = analyze(&factory_width_source(800), "ts");
  assert!(
    exhausted.source_contracts.custom_ref_lost_notification.is_empty(),
    "exhausted bind budget must fail closed; {:?}",
    exhausted.source_contracts
  );
}

#[test]
fn custom_ref_lost_notification_closed_grammar_stays_quiet() {
  let cases = [
    (
      "getter object value write",
      "const r = customRef((track, trigger) => { let value = 0; return { get() { const result = { value: (value = 0) }; track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "setter computed-key param write",
      "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { const result = { [next = 0]: true }; value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "factory computed-key storage write",
      "const r = customRef((track, trigger) => { let value = 0; const initialized = { [value = 1]: true }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "factory proven conditional write",
      "const r = customRef((track, trigger) => { let value = 0; const initialized = true ? (value = 1) : (value = 0); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "factory proven logical write",
      "const r = customRef((track, trigger) => { let value = 0; const initialized = true && (value = 1); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "generator setter is dormant",
      "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, *set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "await in computed object key ends the prefix",
      "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(async () => { const resolved = { [await Promise.resolve()]: true }; void r.value; }); r.value = 1;",
    ),
    (
      "factory dormant default",
      "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1) } = { supplied: true }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
    (
      "factory binding key after initializer",
      "const r = customRef((track, trigger) => { let value = 0; const { [value = 0]: supplied } = (value = 1, {}); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
    (
      "shadowed undefined is not language undefined",
      "const r = customRef((track, trigger) => { let value = 0; const undefined = 7; const initialized = undefined ?? (value = 1); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
    (
      "object-valued class static block is unknown",
      "const r = customRef((track, trigger) => { let value = 0; return { get() { const result = { Reset: class { static { value = 0 } } }; track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "nested block return ends the prefix",
      "const r = customRef((track, trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(() => { { return; } void r.value; }); r.value = 1;",
    ),
    (
      "factory explicit undefined activates default",
      "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1) } = { supplied: undefined }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 1;",
    ),
    (
      "factory interleaved computed key after default",
      "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1), [value = 0]: unused } = {}; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
    (
      "sequence shadowed undefined is not nullish",
      "const r = customRef((track, trigger) => { let value = 0; const undefined = 7; const initialized = (true, undefined) ?? (value = 1); return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
    (
      "nested default object supplies inner properties",
      "const r = customRef((track, trigger) => { let value = 0; const { missing: { leaf = (value = 1) } = { leaf: true } } = {}; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
    (
      "object-literal proto setter supplies inherited own-looking keys",
      "const r = customRef((track, trigger) => { let value = 0; const { supplied = (value = 1) } = { __proto__: { supplied: true } }; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
    (
      "standard prototype keys are not proven missing",
      "const r = customRef((track, trigger) => { let value = 0; const { toString = (value = 1) } = {}; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watch(r, () => {}); r.value = 0;",
    ),
  ];
  for (label, source) in cases {
    let facts = analyze(&custom_ref_source(source), "ts");
    assert!(
      facts.source_contracts.custom_ref_lost_notification.is_empty(),
      "{label} must stay quiet: {source} {:?}",
      facts.source_contracts.custom_ref_lost_notification
    );
  }
}

#[test]
fn custom_ref_lost_notification_pre_await_read_still_emits() {
  let facts = analyze(
    &custom_ref_source(
      "const r = customRef((track, _trigger) => { let value = 0; return { get() { track(); return value; }, set(next: number) { value = next; } }; }); watchEffect(async () => { void r.value; await Promise.resolve(); }); r.value = 1;",
    ),
    "ts",
  );
  assert_eq!(
    facts.source_contracts.custom_ref_lost_notification.len(),
    1,
    "synchronous prefix read before await remains a subscribed consumer; {:?}",
    facts.source_contracts
  );
  assert_eq!(
    facts.source_contracts.custom_ref_lost_notification.first().map(|site| site.reason),
    Some(vue_vet_core::CustomRefLostNotificationReason::SetNotification)
  );
}
