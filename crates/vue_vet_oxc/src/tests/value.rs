use super::support::*;

#[test]
fn value_contracts_emit_demanded_custom_ref_and_stay_quiet_without_demand() {
  let (missing, _) = contract_stats(
    "import { customRef } from 'vue'; const count = customRef(() => ({ set() {} })); void count.value;",
  );
  assert_eq!(missing.invalid_custom_ref_interface.len(), 1, "{missing:?}");
  let (unused, _) = contract_stats(
    "import { customRef } from 'vue'; const count = customRef(() => ({ set() {} })); void count;",
  );
  assert!(unused.invalid_custom_ref_interface.is_empty(), "{unused:?}");
  let (getter, _) = contract_stats(
    "import { customRef } from 'vue'; const count = customRef(() => ({ get() { return 1 } })); void count.value;",
  );
  assert!(getter.invalid_custom_ref_interface.is_empty(), "{getter:?}");
}

#[test]
fn value_contracts_stopped_scope_result_and_torefs_missing_key() {
  let (scope, _) = contract_stats(
    "import { effectScope } from 'vue'; const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); void result.count;",
  );
  assert_eq!(scope.inactive_scope_result.len(), 1, "{scope:?}");
  let (live, _) = contract_stats(
    "import { effectScope } from 'vue'; const scope = effectScope(); const result = scope.run(() => ({ count: 1 })); void result.count; scope.stop();",
  );
  assert!(live.inactive_scope_result.is_empty(), "{live:?}");
  let (missing, _) = contract_stats(
    "import { reactive, toRefs } from 'vue'; void toRefs(reactive({ count: 1 })).missing.value;",
  );
  assert_eq!(missing.missing_torefs_key.len(), 1, "{missing:?}");
  let (known, _) = contract_stats(
    "import { reactive, toRefs } from 'vue'; void toRefs(reactive({ count: 1 })).count.value;",
  );
  assert!(known.missing_torefs_key.is_empty(), "{known:?}");
}

#[test]
fn value_contracts_unicode_crlf_and_namespace() {
  let source = "import * as Vue from 'vue';\r\nconst 计数 = Vue.customRef(() => ({ set() {} }));\r\nvoid 计数.value;\r\n";
  let (facts, _) = contract_stats(source);
  assert_eq!(facts.invalid_custom_ref_interface.len(), 1, "{facts:?}");
  if let Some(site) = facts.invalid_custom_ref_interface.first() {
    assert!(site.demand_span.line >= 3, "{site:?}");
  }
}

#[test]
fn value_contracts_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source =
      String::from("import { customRef, effectScope, reactive, toRefs } from 'vue';");
    for index in 0..size {
      source.push_str("const r");
      source.push_str(&index.to_string());
      source.push_str(" = customRef(() => ({ set() {}, extra");
      source.push_str(&index.to_string());
      source.push_str(": 1 })); void r");
      source.push_str(&index.to_string());
      source.push_str(".value;");
      source.push_str("const s");
      source.push_str(&index.to_string());
      source.push_str(" = effectScope(); s");
      source.push_str(&index.to_string());
      source.push_str(".stop(); const out");
      source.push_str(&index.to_string());
      source.push_str(" = s");
      source.push_str(&index.to_string());
      source.push_str(".run(() => ({ count: 1 })); void out");
      source.push_str(&index.to_string());
      source.push_str(".count;");
      source.push_str("void toRefs(reactive({ count: 1, k");
      source.push_str(&index.to_string());
      source.push_str(": 1 })).missing.value;");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.invalid_custom_ref_interface.len(), expected, "{contracts:?}");
    assert_eq!(contracts.inactive_scope_result.len(), expected, "{contracts:?}");
    assert_eq!(contracts.missing_torefs_key.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "value-contract work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn value_contracts_review_safe_probes_stay_quiet() {
  for source in [
    "import { customRef } from 'vue'; function make(undefined: () => number) { const count = customRef(() => ({ get: undefined })); void count.value; } make(() => 7);",
    "import { customRef } from 'vue'; let out: unknown; out = customRef(() => ({ get() { return 7 } })).value;",
    "import { customRef } from 'vue'; const count = customRef(() => ({ set() {} })); delete count.value;",
    "import { customRef } from 'vue'; const count = customRef(() => ({ set() {} })); count._get = () => 7; void count.value;",
    "import { effectScope } from 'vue'; const scope = effectScope(); false && scope.stop(); const result = scope.run(() => ({ count: 1 })); void result.count;",
    "import { effectScope } from 'vue'; const scope = effectScope(); scope.stop = () => {}; scope.stop(); const result = scope.run(() => ({ count: 1 })); void result.count;",
    "import { effectScope } from 'vue'; const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); if (result) void result.count;",
    "import { effectScope } from 'vue'; const scope = effectScope(); scope.stop(); false && scope.run(() => ({ count: 1 })).count;",
    "import { reactive, ref, toRefs } from 'vue'; const refs = toRefs(reactive({ count: 1 })); refs.missing = ref(2); void refs.missing.value;",
    "import { reactive, ref, toRefs } from 'vue'; let { missing } = toRefs(reactive({ count: 1 })); missing = ref(2); void missing.value;",
    "import { reactive, toRefs } from 'vue'; void toRefs(reactive({ __proto__: { inherited: 1 }, count: 1 })).inherited.value;",
    "import { reactive, toRefs } from 'vue'; void toRefs(reactive({ count: 1 })).toString.value;",
    "import { reactive, toRefs } from 'vue'; const refs = toRefs(reactive({ count: 1 })); if (refs.missing) void refs.missing.value;",
    "import { customRef } from 'vue'; const count = customRef(() => ({ get() { this._set = (value: number) => { this.saved = value }; return 1 } })); void count.value; count.value = 7;",
    "import { customRef } from 'vue'; let saved = 0; const field = customRef(() => ({ get() { const marker = { [this._set = (value: number) => { saved = value }]: 1 }; return marker; } })); void field.value; field.value = 7;",
    "import { reactive, toRefs } from 'vue'; function install(target: object) { Object.assign(target, { added: 7 }); } const state = reactive({ initial: 1 }); install(state); const fields = toRefs(state); void fields.added.value;",
    "import { effectScope, reactive, toRefs } from 'vue'; function guardedScope() { const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); if (!result) return; void result.count; } function guardedBag() { const bag = toRefs(reactive({ count: 1 })); if (!bag.missing) return; void bag.missing.value; } guardedScope(); guardedBag();",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.invalid_custom_ref_interface.is_empty()
        && facts.inactive_scope_result.is_empty()
        && facts.missing_torefs_key.is_empty(),
      "safe probe must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn value_contracts_order_picks_min_offset_consumer_deterministically() {
  let source = "import { effectScope } from 'vue'; const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ first: 1, second: 2 })); void result.first; void result.second;";
  let first = contract_stats(source);
  let second = contract_stats(source);
  assert_eq!(first.0.inactive_scope_result, second.0.inactive_scope_result);
  assert_eq!(first.0.inactive_scope_result.len(), 1, "{:?}", first.0);
  let Some(site) = first.0.inactive_scope_result.first() else {
    return;
  };
  assert_eq!(site.consumer_span.length, "result.first".len(), "{site:?}");
}

#[test]
fn value_contracts_mixed_root_growth_counts_per_root_queries() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source =
      String::from("import { customRef, effectScope, reactive, toRefs } from 'vue';");
    for index in 0..size {
      source.push_str("const bag");
      source.push_str(&index.to_string());
      source.push_str(" = toRefs(reactive({ count: 1 })); void bag");
      source.push_str(&index.to_string());
      source.push_str(".missing.value;");
      source.push_str("const keep");
      source.push_str(&index.to_string());
      source.push_str(" = toRefs(reactive({ missing: 1 })); void keep");
      source.push_str(&index.to_string());
      source.push_str(".missing.value;");
      source.push_str("const r");
      source.push_str(&index.to_string());
      source.push_str(" = customRef(() => ({ set() {} })); void r");
      source.push_str(&index.to_string());
      source.push_str(".value;");
      source.push_str("const s");
      source.push_str(&index.to_string());
      source.push_str(" = effectScope(); s");
      source.push_str(&index.to_string());
      source.push_str(".stop(); const out");
      source.push_str(&index.to_string());
      source.push_str(" = s");
      source.push_str(&index.to_string());
      source.push_str(".run(() => ({ count: 1 })); void out");
      source.push_str(&index.to_string());
      source.push_str(".count;");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.missing_torefs_key.len(), expected, "{contracts:?}");
    assert_eq!(contracts.invalid_custom_ref_interface.len(), expected, "{contracts:?}");
    assert_eq!(contracts.inactive_scope_result.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "mixed-root demand work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn value_contracts_prior_exit_and_uninvoked_nested_stay_quiet() {
  for source in [
    "import { effectScope } from 'vue'; function run() { const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); if (!result) return; void result.count; } run();",
    "import { effectScope } from 'vue'; function run() { const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); if (!result) throw new Error('empty'); void result.count; } run();",
    "import { effectScope } from 'vue'; function run() { const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); function inner() { void result.count; } } run();",
    "import { reactive, toRefs } from 'vue'; function run() { const bag = toRefs(reactive({ count: 1 })); if (!bag.missing) return; void bag.missing.value; } run();",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.inactive_scope_result.is_empty() && facts.missing_torefs_key.is_empty(),
      "prior-exit / uninvoked nested must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn value_contracts_same_region_demand_still_emits() {
  let (facts, _) = contract_stats(
    "import { effectScope, reactive, toRefs } from 'vue'; function run() { const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); void result.count; const bag = toRefs(reactive({ count: 1 })); void bag.missing.value; } run();",
  );
  assert_eq!(facts.inactive_scope_result.len(), 1, "{facts:?}");
  assert_eq!(facts.missing_torefs_key.len(), 1, "{facts:?}");
}

#[test]
fn value_contracts_receiver_mutation_suppresses_later_set() {
  let (facts, _) = contract_stats(
    "import { customRef } from 'vue'; const count = customRef(() => ({ get() { this._set = (value: number) => { this.saved = value }; return 1 } })); void count.value; count.value = 7;",
  );
  assert!(
    facts.invalid_custom_ref_interface.is_empty(),
    "getter receiver mutation must make later set demand unknown: {facts:?}"
  );
}

#[test]
fn value_contracts_closed_getter_then_missing_set_still_emits() {
  let (facts, _) = contract_stats(
    "import { customRef } from 'vue'; const count = customRef(() => ({ get() { return 1 } })); void count.value; count.value = 7;",
  );
  assert_eq!(facts.invalid_custom_ref_interface.len(), 1, "{facts:?}");
  let missing = facts.invalid_custom_ref_interface.first().map(|site| site.missing);
  assert_eq!(missing, Some(vue_vet_core::CustomRefCapability::Set), "{facts:?}");
}

#[test]
fn value_contracts_shared_scope_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source =
      String::from("import { effectScope } from 'vue'; const scope = effectScope(); scope.stop();");
    for index in 0..size {
      source.push_str("const out");
      source.push_str(&index.to_string());
      source.push_str(" = scope.run(() => ({ count: 1 })); void out");
      source.push_str(&index.to_string());
      source.push_str(".count;");
    }
    for index in 0..size {
      source.push_str("const live");
      source.push_str(&index.to_string());
      source.push_str(" = effectScope(); const keep");
      source.push_str(&index.to_string());
      source.push_str(" = live");
      source.push_str(&index.to_string());
      source.push_str(".run(() => ({ count: 1 })); void keep");
      source.push_str(&index.to_string());
      source.push_str(".count;");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.inactive_scope_result.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-scope demand work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn value_contracts_shared_bag_and_destructure_growth_stays_subquadratic() {
  let mut previous: Option<(u64, SourceContractStats)> = None;
  for size in [16_u64, 32, 64] {
    let mut keys = Vec::new();
    for index in 0..size {
      keys.push(format!("k{index}: 1"));
    }
    let mut source = format!(
      "import {{ reactive, toRefs }} from 'vue'; const state = reactive({{ count: 1, {} }});",
      keys.join(", ")
    );
    for index in 0..size {
      source.push_str("const bag");
      source.push_str(&index.to_string());
      source.push_str(" = toRefs(state); void bag");
      source.push_str(&index.to_string());
      source.push_str(".missing.value;");
      source.push_str("const keep");
      source.push_str(&index.to_string());
      source.push_str(" = toRefs(state); void keep");
      source.push_str(&index.to_string());
      source.push_str(".count.value;");
      source.push_str("const { missing: m");
      source.push_str(&index.to_string());
      source.push_str(" } = toRefs(reactive({ count: 1 })); void m");
      source.push_str(&index.to_string());
      source.push_str(".value;");
    }
    let (contracts, stats) = contract_full_stats(&source);
    let work = stats.work();
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.missing_torefs_key.len(), expected.saturating_mul(2), "{contracts:?}");
    let cloned_set_work = size.saturating_mul(2).saturating_mul(size.saturating_add(1));
    assert!(
      stats.key_copies < cloned_set_work,
      "key copies {copies} must stay below 2N(N+1)={cloned} whole-set clones for n={size}",
      copies = stats.key_copies,
      cloned = cloned_set_work,
    );
    if let Some((prev_size, prev)) = previous {
      assert_eq!(size, prev_size * 2);
      let prev_work = prev.work();
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-bag demand work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
      assert!(
        stats.key_copies.saturating_mul(10) < prev.key_copies.saturating_mul(30),
        "shared-bag key copies grew from {prev} to {now} on {prev_size}->{size}",
        prev = prev.key_copies,
        now = stats.key_copies,
      );
      assert!(
        stats.key_lookups.saturating_mul(10) < prev.key_lookups.saturating_mul(30),
        "shared-bag key lookups grew from {prev} to {now} on {prev_size}->{size}",
        prev = prev.key_lookups,
        now = stats.key_lookups,
      );
    }
    previous = Some((size, stats));
  }
}

#[test]
fn value_contracts_torefs_helper_escape_is_unknown() {
  for source in [
    "import { reactive, toRefs } from 'vue'; function install(target: object) { Object.assign(target, { added: 7 }); } const state = reactive({ initial: 1 }); install(state); const fields = toRefs(state); void fields.added.value;",
    "import { reactive, toRefs } from 'vue'; function install(target: object) { Object.assign(target, { added: 7 }); } const state = reactive({ initial: 1 }); install(state); void toRefs(state).added.value;",
    "import { reactive, toRefs } from 'vue'; function install(target: object) { Object.assign(target, { added: 7 }); } const state = reactive({ initial: 1 }); install(state); const { added } = toRefs(state); void added.value;",
    "import { reactive, toRefs } from 'vue'; function install(target: object) { Object.assign(target, { added: 7 }); } const state = reactive({ initial: 1 }); const alias = state; install(alias); void toRefs(state).added.value;",
    "import { reactive, toRefs as split } from 'vue'; function toRefs(target: object) { Object.assign(target, { added: 7 }); return {}; } const state = reactive({ initial: 1 }); toRefs(state); void split(state).added.value;",
    "import { reactive, toRefs } from 'vue'; const state = reactive({ count: 1 }); const box = { state }; void toRefs(state).missing.value;",
    "import { reactive, toRefs } from 'vue'; const state = reactive({ count: 1 }); new WeakSet([state]); void toRefs(state).missing.value;",
    "import { reactive, toRefs } from 'vue'; function tag(_strings: TemplateStringsArray, value: object) { return value; } const state = reactive({ count: 1 }); tag`${state}`; void toRefs(state).missing.value;",
    "import { reactive, toRefs } from 'vue'; const state = reactive({ count: 1 }); state.toJSON(); void toRefs(state).missing.value;",
    "import { reactive, toRefs } from 'vue'; function install<T>(this: { added?: number }) { this.added = 7 } const state = reactive({ initial: 1, install }); (state.install)``; const fields = toRefs(state); void fields.added.value;",
    "import { reactive, toRefs } from 'vue'; function install<T>(this: { added?: number }) { this.added = 7 } const state = reactive({ initial: 1, install }); (state.install<number>)``; const fields = toRefs(state); void fields.added.value;",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.missing_torefs_key.is_empty(),
      "helper/store/new/tag/member escape must keep keys unknown: {source} => {facts:?}"
    );
  }
}

#[test]
fn value_contracts_tagged_receiver_including_ts_instantiation_is_unknown() {
  for source in [
    "import { reactive, toRefs } from 'vue'; function install<T>(this: { added?: number }) { this.added = 7 } const state = reactive({ initial: 1, install }); (state.install)``; const fields = toRefs(state); void fields.added.value;",
    "import { reactive, toRefs } from 'vue'; function install<T>(this: { added?: number }) { this.added = 7 } const state = reactive({ initial: 1, install }); (state.install<number>)``; const fields = toRefs(state); void fields.added.value;",
  ] {
    let facts = assert_gated_matches_forced(source, ScriptKind::Setup);
    assert!(
      facts.missing_torefs_key.is_empty(),
      "tagged receiver must keep closed keys unknown: {source} => {facts:?}"
    );
  }
  let positive = assert_gated_matches_forced(
    "import { reactive, toRefs } from 'vue'; void toRefs(reactive({ count: 1 })).missing.value;",
    ScriptKind::Setup,
  );
  assert_eq!(
    positive.missing_torefs_key.len(),
    1,
    "ordinary missing-key demand must stay positive: {positive:?}"
  );
}

#[test]
fn value_contracts_torefs_known_borrows_still_report_missing_keys() {
  let (local, _) = contract_stats(
    "import { reactive, toRefs } from 'vue'; function install(target: object) { Object.assign(target, { added: 7 }); } const state = reactive({ initial: 1 }); const other = reactive({ initial: 1 }); install(other); void toRefs(state).missing.value;",
  );
  assert_eq!(local.missing_torefs_key.len(), 1, "{local:?}");
  let Some(site) = local.missing_torefs_key.first() else {
    return;
  };
  assert_eq!(site.key, "missing");
  assert_eq!(site.demand_span.length, "toRefs(state).missing.value".len(), "{site:?}");
  let (repeated, _) = contract_stats(
    "import { reactive, toRefs } from 'vue'; const state = reactive({ count: 1 }); void toRefs(state).missing.value; const bag = toRefs(state); void bag.missing.value;",
  );
  assert_eq!(repeated.missing_torefs_key.len(), 2, "{repeated:?}");
}

#[test]
fn value_contracts_source5_still_treats_torefs_arg_as_escape() {
  let (facts, _) = contract_stats(
    "import { reactive, toRefs, watch } from 'vue'; const state = reactive({ child: { count: 1 } }); toRefs(state); watch(state.child, () => {}); state.child = { count: 2 };",
  );
  assert!(
    facts.watch_replaced_object_source.is_empty(),
    "generic source5 escape of a toRefs argument must stay uncertain: {facts:?}"
  );
}

#[test]
fn value_contracts_computed_key_receiver_mutation_is_uncertain() {
  for source in [
    "import { customRef } from 'vue'; let saved = 0; const field = customRef(() => ({ get() { const marker = { [this._set = (value: number) => { saved = value }]: 1 }; return marker; } })); void field.value; field.value = 7;",
    "import { customRef } from 'vue'; let saved = 0; const field = customRef(() => ({ get() { const marker = { [(this._set = (value: number) => { saved = value })]: 1 }; return marker; } })); void field.value; field.value = 7;",
    "import { customRef } from 'vue'; let saved = 0; const field = customRef(() => ({ get() { const { [(this._set = (value: number) => { saved = value })]: marker } = { 1: 1 }; return marker; } })); void field.value; field.value = 7;",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.invalid_custom_ref_interface.is_empty(),
      "computed-key receiver mutation must make later set demand unknown: {source} => {facts:?}"
    );
  }
}

#[test]
fn value_contracts_literal_computed_key_and_unrun_uncertain_getter_still_report() {
  let (literal, _) = contract_stats(
    "import { customRef } from 'vue'; const field = customRef(() => ({ get() { const marker = { [1]: 1 }; return marker; } })); void field.value; field.value = 7;",
  );
  assert_eq!(literal.invalid_custom_ref_interface.len(), 1, "{literal:?}");
  let missing = literal.invalid_custom_ref_interface.first().map(|site| site.missing);
  assert_eq!(missing, Some(vue_vet_core::CustomRefCapability::Set), "{literal:?}");
  let Some(site) = literal.invalid_custom_ref_interface.first() else {
    return;
  };
  assert_eq!(site.demand_span.length, "field.value".len(), "{site:?}");
  let (unrun, _) = contract_stats(
    "import { customRef } from 'vue'; const field = customRef(() => ({ get() { const marker = { [this._set = (value: number) => { void value }]: 1 }; return 1; } })); field.value = 7;",
  );
  assert_eq!(unrun.invalid_custom_ref_interface.len(), 1, "{unrun:?}");
  let unrun_missing = unrun.invalid_custom_ref_interface.first().map(|site| site.missing);
  assert_eq!(unrun_missing, Some(vue_vet_core::CustomRefCapability::Set), "{unrun:?}");
}
