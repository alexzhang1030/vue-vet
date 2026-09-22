use super::support::*;

#[test]
fn snapshot_contracts_json_clone_date_demand_and_string_consumer() {
  let positive = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when.getTime();",
    ScriptKind::Setup,
  );
  assert_eq!(positive.json_clone_lossy_type.len(), 1, "{positive:?}");
  let Some(site) = positive.json_clone_lossy_type.first() else {
    return;
  };
  assert_eq!(site.path, "when");
  assert_eq!(site.method, "getTime");
  assert_eq!(site.output_kind, "string");
  let year = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when.getUTCFullYear();",
    ScriptKind::Setup,
  );
  assert_eq!(
    year.json_clone_lossy_type.first().map(|site| site.method.as_str()),
    Some("getUTCFullYear")
  );
  let quiet = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when.slice(0, 4);",
    ScriptKind::Setup,
  );
  assert!(quiet.json_clone_lossy_type.is_empty(), "{quiet:?}");
}

#[test]
fn snapshot_contracts_json_clone_unknown_and_safe_controls() {
  for source in [
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') }), { clone: (value: object) => value }); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') }), { immediate: false }); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const extra = { deep: true }; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') }), { ...extra }); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const make = () => ({ when: new Date('2020-01-01') }); const { cloned } = useCloned(ref(make())); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value?.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); if (cloned.value.when) cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value = { when: new Date('2021-01-01') }; cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; class Date { getTime() { return 1 } } const { cloned } = useCloned(ref({ when: new Date() })); cloned.value.when.getTime();",
    "import { useCloned } from '@vueuse/shared'; import { ref } from 'vue'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when.getTime();",
    "import type { useCloned } from '@vueuse/core'; import { ref } from 'vue'; const useCloned = (value: unknown) => ({ cloned: value }); const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })) as { cloned: { value: { when: Date } } }; cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when = new Date('2021-01-01'); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const bag = useCloned(ref({ when: new Date('2020-01-01') })); bag.cloned.value = { when: new Date('2021-01-01') }; bag.cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); const payload = cloned.value; payload.when = new Date('2021-01-01'); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; String.prototype.getTime = () => 42; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; String.prototype['getTime'] = () => 42; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; JSON.parse = () => ({ when: new Date('2021-01-01') }); const { cloned } = useCloned(ref({ when: new Date('2020-01-01') })); cloned.value.when.getTime();",
    "import { ref } from 'vue'; import { useCloned } from '@vueuse/core'; const { cloned } = useCloned(ref({ when: new Date('2020-01-01') }), { onTrack: () => { throw new Error('stopped'); } }); cloned.value.when.getTime();",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.json_clone_lossy_type.is_empty(),
      "json clone control must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn snapshot_contracts_history_alias_and_safe_clone() {
  let positive = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source); commit(); source.value.n = 2; undo(); void source.value.n;",
    ScriptKind::Setup,
  );
  assert_eq!(positive.ref_history_snapshot_alias.len(), 1, "{positive:?}");
  let Some(site) = positive.ref_history_snapshot_alias.first() else {
    return;
  };
  assert_eq!(site.property, "n");
  assert_eq!(site.demand_kind, "undo");
  let reset = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { reset } = useManualRefHistory(source); source.value.n = 2; reset(); void source.value.n;",
    ScriptKind::Setup,
  );
  assert_eq!(
    reset.ref_history_snapshot_alias.first().map(|site| site.demand_kind.as_str()),
    Some("reset")
  );
  let snapshot_read = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { history } = useManualRefHistory(source); source.value.n = 2; void history.value[0].snapshot.n;",
    ScriptKind::Setup,
  );
  assert_eq!(
    snapshot_read.ref_history_snapshot_alias.first().map(|site| site.demand_kind.as_str()),
    Some("history")
  );
  let clone_true = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source, { clone: true }); commit(); source.value.n = 2; undo(); void source.value.n;",
    ScriptKind::Setup,
  );
  assert!(clone_true.ref_history_snapshot_alias.is_empty(), "{clone_true:?}");
  let replaced = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source); commit(); source.value = { n: 2 }; undo(); void source.value.n;",
    ScriptKind::Setup,
  );
  assert!(replaced.ref_history_snapshot_alias.is_empty(), "{replaced:?}");
}

#[test]
fn snapshot_contracts_history_unknown_controls() {
  for source in [
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { undo } = useManualRefHistory(source); source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo, clear } = useManualRefHistory(source); commit(); clear(); source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source, { dump: (value: unknown) => value }); commit(); source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref(1); const { commit, undo } = useManualRefHistory(source); commit(); source.value = 2; undo(); void source.value;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source); commit(); source.value.n = 1; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { history } = useRefHistory(source); source.value.n = 2; void history.value;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source, { clone: !false }); commit(); source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source, { clone: (value: { n: number }) => ({ n: value.n }) }); commit(); source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source); commit(); source.value = { n: 3 }; source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); source.value.n = 2; const { commit, undo } = useManualRefHistory(source); commit(); source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source); commit(); source.value.n = 2; source.value.n = 1; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source); commit(); undo(); source.value.n = 2; undo(); void source.value.n;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source); commit(); source.value.n = 2; undo(); const result = 42; void result;",
    "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const bag = useManualRefHistory(source); source.value.n = 2; const result = typeof bag.history; void result;",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.ref_history_snapshot_alias.is_empty(),
      "history control must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn snapshot_contracts_namespace_alias_unicode_and_import_preflight() {
  let json = assert_gated_matches_forced(
    "import { ref } from 'vue';\r\nimport * as VueUse from '@vueuse/core';\r\nconst { cloned } = VueUse.useCloned(ref({ when: new Date('2020-01-01') }));\r\ncloned.value.when.getTime();\r\n",
    ScriptKind::Setup,
  );
  assert_eq!(json.json_clone_lossy_type.len(), 1, "{json:?}");
  let history = assert_gated_matches_forced(
    "import { ref } from 'vue'; import { useManualRefHistory as historyOf } from '@vueuse/core'; const 源 = ref({ n: 1 }); const { commit, undo } = historyOf(源); commit(); 源.value.n = 2; undo(); void 源.value.n;",
    ScriptKind::Setup,
  );
  assert_eq!(history.ref_history_snapshot_alias.len(), 1, "{history:?}");
  let json_demand = "cloned.value.when.getTime()";
  let json_source = "import { ref } from 'vue';\r\nimport * as VueUse from '@vueuse/core';\r\nconst { cloned } = VueUse.useCloned(ref({ when: new Date('2020-01-01') }));\r\ncloned.value.when.getTime();\r\n";
  let Some(json_site) = json.json_clone_lossy_type.first() else {
    return;
  };
  let Some(json_offset) = json_source.find(json_demand) else {
    return;
  };
  assert_eq!(json_site.demand_span.offset, json_offset, "{json_site:?}");
  assert_eq!(json_site.demand_span.length, json_demand.len(), "{json_site:?}");
  assert_eq!(json_site.demand_span.line, 4, "{json_site:?}");
  assert_eq!(json_site.demand_span.column, 1, "{json_site:?}");
  let history_write = "源.value.n";
  let history_source = "import { ref } from 'vue'; import { useManualRefHistory as historyOf } from '@vueuse/core'; const 源 = ref({ n: 1 }); const { commit, undo } = historyOf(源); commit(); 源.value.n = 2; undo(); void 源.value.n;";
  let Some(history_site) = history.ref_history_snapshot_alias.first() else {
    return;
  };
  let Some(history_offset) = history_source.find(history_write) else {
    return;
  };
  assert_eq!(history_site.write_span.offset, history_offset, "{history_site:?}");
  assert_eq!(history_site.write_span.length, history_write.len(), "{history_site:?}");
  assert_eq!(history_site.write_span.line, 1, "{history_site:?}");
  assert_eq!(history_site.write_span.column, history_offset + 1, "{history_site:?}");
  assert_bypass("const n = 1; console.log(n);", ScriptKind::Setup);
  assert_bypass(
    "import { useTimeoutFn } from '@vueuse/core'; useTimeoutFn(() => {}, 0);",
    ScriptKind::Setup,
  );
}

#[test]
fn snapshot_contracts_independent_producers_grow_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { useCloned, useManualRefHistory } from '@vueuse/core';",
    );
    for index in 0..size {
      source.push_str("const { cloned: cloned");
      source.push_str(&index.to_string());
      source.push_str(" } = useCloned(ref({ when: new Date('2020-01-01') })); cloned");
      source.push_str(&index.to_string());
      source.push_str(".value.when.getTime();");
      source.push_str("const src");
      source.push_str(&index.to_string());
      source.push_str(" = ref({ n: 1 }); const { commit: c");
      source.push_str(&index.to_string());
      source.push_str(", undo: u");
      source.push_str(&index.to_string());
      source.push_str(" } = useManualRefHistory(src");
      source.push_str(&index.to_string());
      source.push_str("); c");
      source.push_str(&index.to_string());
      source.push_str("(); src");
      source.push_str(&index.to_string());
      source.push_str(".value.n = 2; u");
      source.push_str(&index.to_string());
      source.push_str("(); void src");
      source.push_str(&index.to_string());
      source.push_str(".value.n;");
    }
    let (facts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(facts.json_clone_lossy_type.len(), expected, "{facts:?}");
    assert_eq!(facts.ref_history_snapshot_alias.len(), expected, "{facts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "snapshot-contract work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn snapshot_contracts_joint_history_ops_grow_linearly() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [20_u64, 80] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { useManualRefHistory } from '@vueuse/core'; const source = ref({ n: 1 }); const { commit, undo } = useManualRefHistory(source);",
    );
    for _ in 0..size {
      source.push_str("commit();");
    }
    for _ in 0..size {
      source.push_str("source.value.n = 2;");
    }
    for _ in 0..size {
      source.push_str("undo();");
    }
    source.push_str("void source.value.n;");
    let (facts, work) = contract_stats(&source);
    assert_eq!(facts.ref_history_snapshot_alias.len(), 1, "{facts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 4);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(90),
        "joint history work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <9x for 4x size)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn snapshot_contracts_keep_source5_and_value3_outputs() {
  let facts = analyze(
    "import { reactive, triggerRef, toRefs, ref, watch, customRef, effectScope } from 'vue';\
     triggerRef(reactive({ n: 1 }));\
     toRefs({ a: 1 });\
     reactive(0);\
     const n = ref(0); watch(n.value, () => {});\
     const bad = customRef(() => ({ set() {} })); void bad.value;\
     const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); void result.count;\
     void toRefs(reactive({ count: 1 })).missing.value;",
    "ts",
  );
  assert_eq!(facts.source_contracts.trigger_ref_non_ref.len(), 1);
  assert_eq!(facts.source_contracts.torefs_non_proxy.len(), 1);
  assert_eq!(facts.source_contracts.primitive_reactive_target.len(), 1);
  assert_eq!(facts.source_contracts.watch_unwrapped_source.len(), 1);
  assert_eq!(facts.source_contracts.invalid_custom_ref_interface.len(), 1);
  assert_eq!(facts.source_contracts.inactive_scope_result.len(), 1);
  assert_eq!(facts.source_contracts.missing_torefs_key.len(), 1);
  assert!(facts.source_contracts.json_clone_lossy_type.is_empty());
  assert!(facts.source_contracts.ref_history_snapshot_alias.is_empty());
}
