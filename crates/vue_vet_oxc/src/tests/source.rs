use super::support::*;

#[test]
fn source_contracts_classify_vue_identity_and_provenance() {
  let trigger = analyze(
    "import { reactive, triggerRef } from 'vue'; const obj = reactive({ n: 1 }); triggerRef(obj);",
    "ts",
  );
  assert_eq!(
    trigger.source_contracts.trigger_ref_non_ref.len(),
    1,
    "{:?}",
    trigger.source_contracts
  );
  let torefs = analyze("import { toRefs } from 'vue'; toRefs({ a: 1 });", "ts");
  assert_eq!(torefs.source_contracts.torefs_non_proxy.len(), 1, "{:?}", torefs.source_contracts);
  let primitive = analyze("import { reactive } from 'vue'; reactive(0);", "ts");
  assert_eq!(
    primitive.source_contracts.primitive_reactive_target.len(),
    1,
    "{:?}",
    primitive.source_contracts
  );
  let unwrapped = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); watch((n.value) as number, () => {});",
    "ts",
  );
  assert_eq!(
    unwrapped.source_contracts.watch_unwrapped_source.len(),
    1,
    "{:?}",
    unwrapped.source_contracts
  );
  let replaced = analyze(
    "import { reactive, watch } from 'vue';\
     const obj = reactive({ nested: { x: 1 } });\
     watch(obj.nested, () => {});\
     obj.nested = { x: 2 };",
    "ts",
  );
  assert_eq!(
    replaced.source_contracts.watch_replaced_object_source.len(),
    1,
    "{:?}",
    replaced.source_contracts
  );
  let ignored =
    analyze("import { ref, toRef } from 'vue'; const n = ref(0); toRef(n as object, 'k');", "ts");
  assert_eq!(ignored.source_contracts.toref_ignored_key.len(), 1, "{:?}", ignored.source_contracts);
  assert_eq!(
    ignored.source_contracts.toref_ignored_key.first().map(|site| site.reason),
    Some(ToRefIgnoredKeyReason::Ref),
    "{:?}",
    ignored.source_contracts
  );
  let writeback =
    analyze("import { ref, toRef } from 'vue'; const n = ref(0); toRef(n, 'value');", "ts");
  assert!(
    writeback.source_contracts.toref_ignored_key.is_empty(),
    "{:?}",
    writeback.source_contracts
  );
  let cleared = analyze(
    "import { ref, toRef } from 'vue'; const state = ref({ count: 0 }); state.__v_isRef = false; toRef(state, 'count');",
    "ts",
  );
  assert!(
    cleared.source_contracts.toref_ignored_key.is_empty(),
    "cleared __v_isRef must abstain; {:?}",
    cleared.source_contracts
  );
  let deleted = analyze(
    "import { ref, toRef } from 'vue'; const object = ref(1); delete object.__v_isRef; toRef(object, 'future');",
    "ts",
  );
  assert!(
    deleted.source_contracts.toref_ignored_key.is_empty(),
    "deleted __v_isRef must abstain; {:?}",
    deleted.source_contracts
  );
  let tagged = analyze(
    "import { toRef } from 'vue'; const callable = () => 1; callable.__v_isRef = true; callable.value = 2; toRef(callable, 'value');",
    "ts",
  );
  assert!(
    tagged.source_contracts.toref_ignored_key.is_empty(),
    "tagged callable must abstain; {:?}",
    tagged.source_contracts
  );
  let receiver = analyze(
    "import { ref, toRef } from 'vue'; const n = ref(0); n.toString(); toRef(n, 'k');",
    "ts",
  );
  assert!(
    receiver.source_contracts.toref_ignored_key.is_empty(),
    "method receiver must abstain; {:?}",
    receiver.source_contracts
  );
  let helper = analyze(
    "import { ref, toRef } from 'vue'; const n = ref(0); opaque(n); toRef(n, 'k'); function opaque(_value: unknown) {}",
    "ts",
  );
  assert!(
    helper.source_contracts.toref_ignored_key.is_empty(),
    "helper argument must abstain; {:?}",
    helper.source_contracts
  );
  let scope =
    analyze("import { effectScope } from 'vue'; const run = () => {}; effectScope(run);", "ts");
  assert_eq!(scope.source_contracts.effect_scope_callback.len(), 1, "{:?}", scope.source_contracts);
  let detached = analyze("import { effectScope } from 'vue'; effectScope(true);", "ts");
  assert!(
    detached.source_contracts.effect_scope_callback.is_empty(),
    "{:?}",
    detached.source_contracts
  );
}

#[test]
fn source_contracts_toref_capability_escape_routes_stay_quiet() {
  let pattern = analyze(
    "import { ref, toRef } from 'vue';\
     const pattern = ref({ count: 1 });\
     ({ flag: pattern.__v_isRef } = { flag: false });\
     toRef(pattern, 'count');",
    "ts",
  );
  assert!(
    pattern.source_contracts.toref_ignored_key.is_empty(),
    "static pattern __v_isRef write must abstain; {:?}",
    pattern.source_contracts
  );
  let nested_default_rest = analyze(
    "import { ref, toRef } from 'vue';\
     const pattern = ref({ count: 1 });\
     ({ nested: { flag: pattern.__v_isRef = false } = {}, ..._rest } = { nested: {} });\
     toRef(pattern, 'count');",
    "ts",
  );
  assert!(
    nested_default_rest.source_contracts.toref_ignored_key.is_empty(),
    "nested default/rest __v_isRef pattern must abstain; {:?}",
    nested_default_rest.source_contracts
  );
  let ts_wrapper = analyze(
    "import { ref, toRef } from 'vue';\
     const pattern = ref({ count: 1 });\
     ({ flag: (pattern.__v_isRef as boolean) } = { flag: false });\
     toRef(pattern, 'count');",
    "ts",
  );
  assert!(
    ts_wrapper.source_contracts.toref_ignored_key.is_empty(),
    "TS-wrapped pattern __v_isRef write must abstain; {:?}",
    ts_wrapper.source_contracts
  );
  let computed = analyze(
    "import { ref, toRef } from 'vue';\
     const pattern = ref({ count: 1 });\
     const key = '__v_isRef';\
     ({ flag: pattern[key] } = { flag: false });\
     toRef(pattern, 'count');",
    "ts",
  );
  assert!(
    computed.source_contracts.toref_ignored_key.is_empty(),
    "computed pattern marker write must abstain; {:?}",
    computed.source_contracts
  );
  let constructor = analyze(
    "import { ref, toRef } from 'vue';\
     const created = ref({ count: 1 });\
     class ClearMarker { constructor(value: object) { delete (value as { __v_isRef?: boolean }).__v_isRef } }\
     new ClearMarker(created);\
     toRef(created, 'count');",
    "ts",
  );
  assert!(
    constructor.source_contracts.toref_ignored_key.is_empty(),
    "constructor argument must abstain; {:?}",
    constructor.source_contracts
  );
  let tagged = analyze(
    "import { ref, toRef } from 'vue';\
     const tagged = ref({ count: 1 });\
     tagged.clear = function () { delete this.__v_isRef };\
     tagged.clear``;\
     toRef(tagged, 'count');",
    "ts",
  );
  assert!(
    tagged.source_contracts.toref_ignored_key.is_empty(),
    "tagged-template receiver must abstain; {:?}",
    tagged.source_contracts
  );
  let instantiated_tagged = analyze(
    "import { ref, toRef } from 'vue';\
     const tagged = ref({ count: 1 });\
     tagged.clear = function () { delete this.__v_isRef };\
     (tagged.clear<number>)``;\
     toRef(tagged, 'count');",
    "ts",
  );
  assert!(
    instantiated_tagged.source_contracts.toref_ignored_key.is_empty(),
    "TS instantiation tagged-template receiver must abstain; {:?}",
    instantiated_tagged.source_contracts
  );
  let instantiated_call = analyze(
    "import { ref, toRef } from 'vue';\
     const tagged = ref({ count: 1 });\
     tagged.clear = function () { delete this.__v_isRef };\
     tagged.clear<number>();\
     toRef(tagged, 'count');",
    "ts",
  );
  assert!(
    instantiated_call.source_contracts.toref_ignored_key.is_empty(),
    "TS instantiation call receiver must abstain; {:?}",
    instantiated_call.source_contracts
  );
  let positive = analyze(
    "import { ref, toRef } from 'vue'; const native = ref({ count: 1 }); toRef(native, 'count');",
    "ts",
  );
  assert_eq!(
    positive.source_contracts.toref_ignored_key.len(),
    1,
    "direct toRef(existingRef, staticKey) must stay positive; {:?}",
    positive.source_contracts
  );
  let value_write = analyze(
    "import { ref, toRef } from 'vue';\
     const native = ref({ count: 1 });\
     native.value = { count: 2 };\
     native.count = 3;\
     toRef(native, 'count');",
    "ts",
  );
  assert_eq!(
    value_write.source_contracts.toref_ignored_key.len(),
    1,
    "direct .value/.count writes must keep ignored-key; {:?}",
    value_write.source_contracts
  );
  let writeback =
    analyze("import { ref, toRef } from 'vue'; const n = ref(0); toRef(n, 'value');", "ts");
  assert!(
    writeback.source_contracts.toref_ignored_key.is_empty(),
    "value writeback must stay quiet; {:?}",
    writeback.source_contracts
  );
  let shadow = analyze(
    "function toRef(_source: unknown, _key: string) { return { value: 0 } }\
     const count = { value: 0 };\
     toRef(count, 'n');",
    "ts",
  );
  assert!(
    shadow.source_contracts.toref_ignored_key.is_empty(),
    "local toRef shadow must stay quiet; {:?}",
    shadow.source_contracts
  );
  let source5 =
    analyze("import { ref, watch } from 'vue'; const n = ref(0); watch(n.value, () => {});", "ts");
  assert_eq!(
    source5.source_contracts.watch_unwrapped_source.len(),
    1,
    "direct .value reads must keep source5 unwrapped-watch; {:?}",
    source5.source_contracts
  );
  let source5_data = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ nested: { x: 1 } });\
     watch(state.nested, () => {});\
     state.nested = { x: 2 };",
    "ts",
  );
  assert_eq!(
    source5_data.source_contracts.watch_replaced_object_source.len(),
    1,
    "direct data writes must keep source5 replaced-object; {:?}",
    source5_data.source_contracts
  );
  let mixed = analyze(
    "import { ref, toRef, watch } from 'vue';\
     const tagged = ref({ count: 1 });\
     tagged.clear = function () { delete this.__v_isRef };\
     (tagged.clear<number>)``;\
     toRef(tagged, 'count');\
     const n = ref(0);\
     watch(n.value, () => {});",
    "ts",
  );
  assert!(
    mixed.source_contracts.toref_ignored_key.is_empty(),
    "instantiated tagged receiver must quiet toRef identity; {:?}",
    mixed.source_contracts
  );
  assert_eq!(
    mixed.source_contracts.watch_unwrapped_source.len(),
    1,
    "direct .value source5 must stay positive beside instantiated receiver; {:?}",
    mixed.source_contracts
  );
}

#[test]
fn source_contracts_toref_capability_roles_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { ref, toRef } from 'vue'; class Clear { constructor(value: object) { delete (value as { __v_isRef?: boolean }).__v_isRef } }",
    );
    for index in 0..size {
      source.push_str("const n");
      source.push_str(&index.to_string());
      source.push_str(" = ref({ count: 1 }); new Clear(n");
      source.push_str(&index.to_string());
      source.push_str("); toRef(n");
      source.push_str(&index.to_string());
      source.push_str(", 'count');");
    }
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.toref_ignored_key.is_empty(),
      "constructor-arg toRef sites must abstain; {contracts:?}"
    );
    assert!(work > 0, "capability-role walks must count work");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "toref capability-role work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_stay_quiet_for_shadowing_and_unknowns() {
  let facts = analyze(
    "function triggerRef(_value: unknown) {}\
     function toRefs(_value: object) { return {}; }\
     function reactive(_value: unknown) { return _value; }\
     function watch(_source: unknown, _cb: () => void) {}\
     triggerRef({ n: 1 });\
     toRefs({ a: 1 });\
     reactive(0);\
     watch(1, () => {});",
    "ts",
  );
  assert!(
    facts.source_contracts.is_empty(),
    "local shadows must not match Vue APIs; {:?}",
    facts.source_contracts
  );
}

#[test]
fn source_contracts_ignore_type_only_default_and_unresolved_vue_spelling() {
  let type_only = analyze(
    "import { type triggerRef } from 'vue'; const triggerRef = (_value: unknown) => {}; triggerRef({ n: 1 });",
    "ts",
  );
  assert!(type_only.source_contracts.is_empty(), "{:?}", type_only.source_contracts);
  let default_ns =
    analyze("import Vue from 'vue'; const obj = { n: 1 }; Vue.triggerRef(obj);", "ts");
  assert!(default_ns.source_contracts.is_empty(), "{:?}", default_ns.source_contracts);
  let toolkit = analyze("import { triggerRef } from '@vue/toolkit'; triggerRef({ n: 1 });", "ts");
  assert!(toolkit.source_contracts.is_empty(), "{:?}", toolkit.source_contracts);
  let unresolved = analyze("triggerRef(1); reactive(0); watch(1, () => {});", "ts");
  assert!(unresolved.source_contracts.is_empty(), "{:?}", unresolved.source_contracts);
}

#[test]
fn source_contracts_abstain_on_unknown_wrapper_alias_and_unordered_writes() {
  let unknown_wrapper = analyze(
    "import { reactive, triggerRef } from 'vue'; declare const externalRef: { value: number }; triggerRef(reactive(externalRef));",
    "ts",
  );
  assert!(
    unknown_wrapper.source_contracts.trigger_ref_non_ref.is_empty(),
    "{:?}",
    unknown_wrapper.source_contracts
  );
  let alias_write = analyze(
    "import { reactive, ref, watch } from 'vue'; const r = ref(0); const a = r; a.value = reactive({ n: 1 }); watch(r.value, () => {});",
    "ts",
  );
  assert!(
    alias_write.source_contracts.watch_unwrapped_source.is_empty(),
    "{:?}",
    alias_write.source_contracts
  );
  let helper = analyze(
    "import { ref, watch } from 'vue'; const r = ref(0); function initialize(target: { value: unknown }) { target.value = {}; } initialize(r); watch(r.value, () => {});",
    "ts",
  );
  assert!(
    helper.source_contracts.watch_unwrapped_source.is_empty(),
    "{:?}",
    helper.source_contracts
  );
  let scan = analyze(
    "import { reactive, ref, watch } from 'vue'; const r = ref(0); function scan() { watch(r.value, () => {}); } r.value = reactive({ n: 1 }); scan();",
    "ts",
  );
  assert!(scan.source_contracts.watch_unwrapped_source.is_empty(), "{:?}", scan.source_contracts);
  let let_alias = analyze(
    "import { reactive, ref, watch } from 'vue'; const r = ref(0); let alias = r; alias.value = reactive({ n: 1 }); watch(r.value, () => {});",
    "ts",
  );
  assert!(
    let_alias.source_contracts.watch_unwrapped_source.is_empty(),
    "{:?}",
    let_alias.source_contracts
  );
  let boxed = analyze(
    "import { reactive, ref, watch } from 'vue'; const r = ref(0); const box = {}; box.ref = r; box.ref.value = reactive({ n: 1 }); watch(r.value, () => {});",
    "ts",
  );
  assert!(boxed.source_contracts.watch_unwrapped_source.is_empty(), "{:?}", boxed.source_contracts);
  let pattern = analyze(
    "import { reactive, ref, watch } from 'vue'; const r = ref(0); ({ x: r.value } = { x: reactive({ n: 1 }) }); watch(r.value, () => {});",
    "ts",
  );
  assert!(
    pattern.source_contracts.watch_unwrapped_source.is_empty(),
    "{:?}",
    pattern.source_contracts
  );
  let shadowed_map =
    analyze("import { reactive } from 'vue'; class Map {}; reactive(new Map());", "ts");
  assert!(
    shadowed_map.source_contracts.primitive_reactive_target.is_empty(),
    "{:?}",
    shadowed_map.source_contracts
  );
}

#[test]
fn source_contracts_replacement_stays_quiet_for_control_flow_and_shallow() {
  let branched = analyze(
    "import { reactive, watch } from 'vue'; const state = reactive({ p: { x: 1 } }); const flag = true; if (flag) { watch(state.p, () => {}); } else { state.p = {}; }",
    "ts",
  );
  assert!(
    branched.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    branched.source_contracts
  );
  let same = analyze(
    "import { reactive, watch } from 'vue'; const state = reactive({ p: { x: 1 } }); watch(state.p, () => {}); state.p = state.p;",
    "ts",
  );
  assert!(
    same.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    same.source_contracts
  );
  let stopped = analyze(
    "import { reactive, watch } from 'vue'; const state = reactive({ p: { x: 1 } }); const stop = watch(state.p, () => {}); stop(); state.p = {};",
    "ts",
  );
  assert!(
    stopped.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    stopped.source_contracts
  );
  let readonly_wrap = analyze(
    "import { reactive, readonly, watch } from 'vue'; const state = readonly(reactive({ p: { x: 1 } })); watch(state.p, () => {}); state.p = { x: 2 };",
    "ts",
  );
  assert!(
    readonly_wrap.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    readonly_wrap.source_contracts
  );
  let shallow = analyze(
    "import { shallowReactive, watch } from 'vue'; const state = shallowReactive({ p: { x: 1 } }); watch(state.p, () => {}); state.p = { x: 2 };",
    "ts",
  );
  assert!(
    shallow.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    shallow.source_contracts
  );
  let spread = analyze(
    "import { reactive, watch } from 'vue'; declare const input: { p: { x: number } }; const state = reactive({ p: 1, ...input }); watch(state.p, () => {}); state.p = { x: 2 };",
    "ts",
  );
  assert!(
    spread.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    spread.source_contracts
  );
  let compound = analyze(
    "import { reactive, watch } from 'vue'; const state = reactive({ p: { x: 1 } }); watch(state.p, () => {}); state.p ||= {};",
    "ts",
  );
  assert!(
    compound.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    compound.source_contracts
  );
  let cached = analyze(
    "import { reactive, watch } from 'vue'; const obj = {}; const state = reactive({ p: reactive(obj) }); watch(state.p, () => {}); state.p = reactive(obj);",
    "ts",
  );
  assert!(
    cached.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    cached.source_contracts
  );
  let spread_args = analyze(
    "import { watch, ref } from 'vue'; const n = ref(0); const extra = []; watch(n.value, ...extra);",
    "ts",
  );
  assert!(
    spread_args.source_contracts.watch_unwrapped_source.is_empty(),
    "{:?}",
    spread_args.source_contracts
  );
}

#[test]
fn source_contracts_many_watch_sites_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [50_u64, 100, 200] {
    let mut source = String::from("import { ref, watch } from 'vue'; const n = ref(0);");
    for _ in 0..size {
      source.push_str("watch(n.value, () => {});");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.watch_unwrapped_source.len(), usize::try_from(size).unwrap_or(usize::MAX));
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "watch-site work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_assignment_patterns_parse_and_stay_quiet() {
  let facts = analyze(
    "import { reactive, ref, watch } from 'vue';\
     const r = ref(0);\
     const source = { x: 1, y: 2, z: 3 };\
     ({ x: r.value = 0, y: r.value, ...rest } = source);\
     const arr = [1, 2, 3];\
     ([r.value = 1, ...tail] = arr);\
     ({ [String('k')]: r.value } = { k: 9 });\
     watch(r.value, () => {}); void rest; void tail;",
    "ts",
  );
  assert!(facts.source_contracts.watch_unwrapped_source.is_empty(), "{:?}", facts.source_contracts);
}

#[test]
fn source_contracts_shadowed_map_is_not_fresh_allocation() {
  let facts = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ p: { x: 1 } });\
     watch(state.p, () => {});\
     function Map() { return state.p; }\
     state.p = new Map();",
    "ts",
  );
  assert!(
    facts.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    facts.source_contracts
  );
}

#[test]
#[expect(clippy::panic, reason = "missing span evidence must fail the regression")]
fn source_contracts_nested_wrapper_budget_does_not_poison_direct_use() {
  let source = "import { reactive, triggerRef } from 'vue';\
     const shared = { n: 1 };\
     triggerRef(reactive(reactive(reactive(reactive(reactive(reactive(reactive(reactive(reactive(shared))))))))));\
     triggerRef(shared);";
  let facts = analyze(source, "ts");
  let needle = "triggerRef(shared)";
  let Some(call) = source.rfind(needle) else {
    panic!("direct triggerRef(shared) missing");
  };
  let arg = call + "triggerRef(".len();
  assert_eq!(
    facts.source_contracts.trigger_ref_non_ref.len(),
    1,
    "only the direct shared root must report; {:?}",
    facts.source_contracts
  );
  let Some(site) = facts.source_contracts.trigger_ref_non_ref.first() else {
    panic!("direct-use finding missing");
  };
  assert_eq!(site.span.offset, arg, "{site:?} source={source}");
  assert_eq!(site.span.length, "shared".len());
}

#[test]
fn source_contracts_vue_identity_sources() {
  let runtime =
    analyze("import { triggerRef } from '@vue/runtime-core'; triggerRef({ n: 1 });", "ts");
  assert_eq!(
    runtime.source_contracts.trigger_ref_non_ref.len(),
    1,
    "{:?}",
    runtime.source_contracts
  );
  let named_auto = analyze("import { triggerRef } from '#imports'; triggerRef({ n: 1 });", "ts");
  assert_eq!(
    named_auto.source_contracts.trigger_ref_non_ref.len(),
    1,
    "{:?}",
    named_auto.source_contracts
  );
  let ns_auto = analyze("import * as Auto from '#imports'; Auto.triggerRef({ n: 1 });", "ts");
  assert!(ns_auto.source_contracts.is_empty(), "{:?}", ns_auto.source_contracts);
  let custom = analyze("import { useMagic } from '#imports'; useMagic();", "ts");
  assert!(custom.source_contracts.is_empty(), "{:?}", custom.source_contracts);
}

#[test]
fn source_contracts_watch_api_option_and_signature_sites() {
  let equals = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n, (v) => v, { equals: (a, b) => a === b });",
    "ts",
  );
  assert_eq!(
    equals.source_contracts.watch_ignored_option.len(),
    1,
    "{:?}",
    equals.source_contracts
  );
  let method = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n, (v) => v, { equals(a, b) { return a === b; } });",
    "ts",
  );
  assert_eq!(
    method.source_contracts.watch_ignored_option.len(),
    1,
    "{:?}",
    method.source_contracts
  );
  let effect = analyze(
    "import { ref, watchEffect } from 'vue'; const n = ref(0); watchEffect(() => n.value, { once: true, immediate: true });",
    "ts",
  );
  assert_eq!(
    effect.source_contracts.watch_ignored_option.len(),
    2,
    "{:?}",
    effect.source_contracts
  );
  let trailing = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n, (v) => v, { equals: () => true }, 'extra');",
    "ts",
  );
  assert_eq!(
    trailing.source_contracts.watch_ignored_option.len(),
    1,
    "{:?}",
    trailing.source_contracts
  );
  let crlf_src = "import { ref, watch } from 'vue';\r\nconst \u{8ba1}\u{6570} = ref(0);\r\nwatch(\u{8ba1}\u{6570}, (v) => v, { equals: () => true });\r\n";
  let crlf = analyze(crlf_src, "ts");
  assert_eq!(crlf.source_contracts.watch_ignored_option.len(), 1, "{:?}", crlf.source_contracts);
  let ignored = crlf.source_contracts.watch_ignored_option.first();
  assert!(ignored.is_some_and(|site| site.span.line == 3 && site.span.length == 6), "{ignored:?}");
  if let Some(site) = ignored {
    let end = site.span.offset.saturating_add(site.span.length);
    assert_eq!(crlf_src.get(site.span.offset..end), Some("equals"));
  }
  let handler = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n, { handler() { return n.value; }, equals: () => true });",
    "ts",
  );
  assert_eq!(
    handler.source_contracts.watch_signature_mismatch.len(),
    1,
    "{:?}",
    handler.source_contracts
  );
  assert!(
    handler.source_contracts.watch_ignored_option.is_empty(),
    "signature must win over ignored-option; {:?}",
    handler.source_contracts
  );
  let array_cb = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n, [() => {}, () => {}]);",
    "ts",
  );
  assert!(
    array_cb.source_contracts.watch_signature_mismatch.is_empty(),
    "{:?}",
    array_cb.source_contracts
  );
  let zero = analyze("import { ref, watch } from 'vue'; const n = ref(0); watch(n, 0);", "ts");
  assert!(zero.source_contracts.watch_signature_mismatch.is_empty(), "{:?}", zero.source_contracts);
  for zero_bigint in ["0n", "0x0n", "0o0n", "0b0n", "0x0_0n"] {
    let source =
      format!("import {{ ref, watch }} from 'vue'; const n = ref(0); watch(n, {zero_bigint});");
    let facts = analyze(&source, "ts");
    assert!(
      facts.source_contracts.watch_signature_mismatch.is_empty(),
      "zero bigint {zero_bigint} must stay quiet; {:?}",
      facts.source_contracts
    );
  }
  let one_n = analyze("import { ref, watch } from 'vue'; const n = ref(0); watch(n, 1n);", "ts");
  assert_eq!(
    one_n.source_contracts.watch_signature_mismatch.len(),
    1,
    "{:?}",
    one_n.source_contracts
  );
  let computed_literal = analyze(
    "import { ref, watchEffect } from 'vue'; const n = ref(0); watchEffect(() => n.value, { ['once']: true });",
    "ts",
  );
  assert!(
    computed_literal.source_contracts.watch_ignored_option.is_empty(),
    "computed literal keys must stay quiet; {:?}",
    computed_literal.source_contracts
  );
  let imported_alias = analyze(
    "import { ref, watch as observe } from 'vue'; const n = ref(0); observe(n, { handler() { return n.value; } });",
    "ts",
  );
  assert_eq!(
    imported_alias.source_contracts.watch_signature_mismatch.len(),
    1,
    "{:?}",
    imported_alias.source_contracts
  );
  let shadow = analyze(
    "import { ref } from 'vue'; const n = ref(0); function watch(_a: unknown, _b: unknown) {} watch(n, { handler() {} });",
    "ts",
  );
  assert!(shadow.source_contracts.is_empty(), "{:?}", shadow.source_contracts);
  let two_fns = analyze(
    "import { ref, watchEffect } from 'vue'; const n = ref(0); watchEffect(() => n.value, (x) => x);",
    "ts",
  );
  assert_eq!(
    two_fns.source_contracts.watch_signature_mismatch.len(),
    1,
    "{:?}",
    two_fns.source_contracts
  );
  let named_opts = analyze(
    "import { ref, watchEffect } from 'vue'; const n = ref(0); function opts() {} watchEffect(() => n.value, opts);",
    "ts",
  );
  assert!(named_opts.source_contracts.is_empty(), "{:?}", named_opts.source_contracts);
  let aliased = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); const options = { equals: () => true }; watch(n, (v) => v, options);",
    "ts",
  );
  assert!(
    aliased.source_contracts.watch_ignored_option.is_empty(),
    "{:?}",
    aliased.source_contracts
  );
  let auto = analyze(
    "import { watchEffect } from '#imports'; const n = { value: 0 }; watchEffect(() => n.value, { once: true });",
    "ts",
  );
  assert_eq!(auto.source_contracts.watch_ignored_option.len(), 1, "{:?}", auto.source_contracts);
  let ns_auto =
    analyze("import * as Auto from '#imports'; Auto.watchEffect(() => 1, { once: true });", "ts");
  assert!(ns_auto.source_contracts.is_empty(), "{:?}", ns_auto.source_contracts);
  let type_only = analyze(
    "import { type watchEffect } from 'vue'; const watchEffect = (_a: unknown, _b: unknown) => {}; watchEffect(() => 1, () => 2);",
    "ts",
  );
  assert!(type_only.source_contracts.is_empty(), "{:?}", type_only.source_contracts);
}

#[test]
fn source_contracts_watch_option_sites_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [50_u64, 100, 200] {
    let mut source = String::from("import { ref, watch } from 'vue'; const n = ref(0);");
    for _ in 0..size {
      source.push_str("watch(n, (v) => v, { equals: () => true });");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.watch_ignored_option.len(), usize::try_from(size).unwrap_or(usize::MAX));
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "watch-option work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_watch_option_literal_width_stays_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [50_u64, 100, 200] {
    let mut keys = Vec::new();
    for index in 0..size {
      keys.push(format!("k{index}: 1"));
    }
    keys.push("equals: () => true".into());
    let source = format!(
      "import {{ ref, watch }} from 'vue'; const n = ref(0); watch(n, (v) => v, {{ {} }});",
      keys.join(", ")
    );
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.watch_ignored_option.len(),
      1,
      "one equals key among {size} fillers; {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "watch-option width work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_destructured_binding_reassignment_is_quiet() {
  let facts = analyze(
    "import { reactive, ref, toRefs, triggerRef, watch } from 'vue';\
     let target = 0;\
     ({ target } = { target: ref(1) });\
     triggerRef(target);\
     let data = 0;\
     [data] = [reactive({})];\
     reactive(data);\
     let plain = {};\
     ({ plain } = { plain: reactive({ count: 1 }) });\
     toRefs(plain);\
     let source = 0;\
     [source] = [ref(1)];\
     watch(source, () => {});",
    "ts",
  );
  assert!(
    facts.source_contracts.trigger_ref_non_ref.is_empty(),
    "shorthand destructure to Ref must not keep primitive triggerRef; {:?}",
    facts.source_contracts
  );
  assert!(
    facts.source_contracts.primitive_reactive_target.is_empty(),
    "array destructure to reactive must not keep primitive reactive(); {:?}",
    facts.source_contracts
  );
  assert!(
    facts.source_contracts.torefs_non_proxy.is_empty(),
    "shorthand destructure to reactive must not keep plain toRefs; {:?}",
    facts.source_contracts
  );
  assert!(
    facts.source_contracts.watch_unwrapped_source.is_empty(),
    "array destructure to ref must not keep primitive watch source; {:?}",
    facts.source_contracts
  );
}

#[test]
fn source_contracts_rest_and_default_binding_reassignment_is_quiet() {
  let facts = analyze(
    "import { reactive, ref, triggerRef } from 'vue';\
     let leftover = 0;\
     [...leftover] = [reactive({})];\
     reactive(leftover);\
     let boxed = 0;\
     ({ boxed = ref(1) } = {});\
     triggerRef(boxed);",
    "ts",
  );
  assert!(
    facts.source_contracts.primitive_reactive_target.is_empty(),
    "rest-to-array must invalidate primitive reactive(); {:?}",
    facts.source_contracts
  );
  assert!(
    facts.source_contracts.trigger_ref_non_ref.is_empty(),
    "default-to-Ref must invalidate primitive triggerRef; {:?}",
    facts.source_contracts
  );
}

#[test]
fn source_contracts_watch_then_writes_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64, 128] {
    let mut source = String::from("import { ref, watch } from 'vue'; const n = ref(0);");
    for _ in 0..size {
      source.push_str("watch(n.value, () => {});");
    }
    for _ in 0..size {
      source.push_str("n.value = 1;");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.watch_unwrapped_source.len(), usize::try_from(size).unwrap_or(usize::MAX));
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "watch-then-write work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_many_properties_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut keys = Vec::new();
    for index in 0..size {
      keys.push(format!("p{index}: {{}}"));
    }
    let mut source = format!(
      "import {{ reactive, watch }} from 'vue'; const state = reactive({{ {} }});",
      keys.join(", ")
    );
    for index in 0..size {
      source.push_str("watch(state.p");
      source.push_str(&index.to_string());
      source.push_str(", () => {}); state.p");
      source.push_str(&index.to_string());
      source.push_str(" = {};");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.watch_replaced_object_source.len(),
      usize::try_from(size).unwrap_or(usize::MAX)
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "property-object work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_early_spreads_then_explicit_props_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { toRefs } from 'vue';");
    for index in 0..size {
      source.push_str("const s");
      source.push_str(&index.to_string());
      source.push_str(" = {};");
    }
    source.push_str("toRefs({");
    for index in 0..size {
      source.push_str("...s");
      source.push_str(&index.to_string());
      source.push(',');
    }
    for index in 0..size {
      source.push('p');
      source.push_str(&index.to_string());
      source.push_str(": {},");
    }
    source.push_str("});");
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.torefs_non_proxy.len(),
      1,
      "plain object after early spreads must emit toRefs-on-non-proxy; {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "early-spread then explicit-prop work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_gated_facts_match_forced_full_for_all_sinks() {
  let cases = [
    "import { reactive, triggerRef } from 'vue'; const obj = reactive({ n: 1 }); triggerRef(obj);",
    "import { toRefs } from 'vue'; toRefs({ a: 1 });",
    "import { reactive } from 'vue'; reactive(0);",
    "import { readonly } from 'vue'; readonly(0);",
    "import { shallowReactive } from 'vue'; shallowReactive(0);",
    "import { shallowReadonly } from 'vue'; shallowReadonly(0);",
    "import { ref, watch } from 'vue'; const n = ref(0); watch((n.value) as number, () => {});",
    "import { reactive, watch } from 'vue'; const obj = reactive({ nested: { x: 1 } }); watch(obj.nested, () => {}); obj.nested = { x: 2 };",
    "import { watchEffect } from 'vue'; watchEffect(() => {}, { once: true });",
    "import { watchPostEffect } from 'vue'; watchPostEffect(() => {}, { immediate: true });",
    "import { watchSyncEffect } from 'vue'; watchSyncEffect(() => {}, { flush: 'post', once: true });",
    "import * as Vue from 'vue'; Vue.watchEffect(() => {}, { deep: true });",
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n, (v) => v, { equals: () => true });",
    "import { ref, watch } from 'vue'; const n = ref(0); function accept(_value: unknown) {} watch(n, (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });",
    "import { reactive, watch } from 'vue'; const state = reactive({ n: 1 }); function accept(_value: unknown) {} watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "import { toRef } from 'vue'; toRef(1, 'k');",
    "import { effectScope } from 'vue'; effectScope(() => {});",
    "import { customRef } from 'vue'; const count = customRef(() => ({ set() {} })); void count.value;",
    "import { effectScope } from 'vue'; const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); void result.count;",
    "import { reactive, toRefs } from 'vue'; void toRefs(reactive({ count: 1 })).missing.value;",
    "import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }));",
    "import { customRef, watch } from 'vue'; const r = customRef((_t, trigger) => { let value = 0; return { get() { return value; }, set(next: number) { value = next; trigger(); } }; }); watch(r, () => {}); r.value = 1;",
  ];
  for source in cases {
    let facts = assert_gated_matches_forced(source, ScriptKind::Setup);
    assert!(!facts.is_empty(), "sink fixture must emit a fact: {source}");
  }
  let custom_ref = assert_gated_matches_forced(
    "import { customRef } from 'vue'; const count = customRef(() => ({ set() {} })); void count.value;",
    ScriptKind::Setup,
  );
  assert_eq!(
    custom_ref.invalid_custom_ref_interface.len(),
    1,
    "lone named customRef missing-getter demand must populate its channel: {custom_ref:?}"
  );
  let stopped_scope = assert_gated_matches_forced(
    "import { effectScope } from 'vue'; const scope = effectScope(); scope.stop(); const result = scope.run(() => ({ count: 1 })); void result.count;",
    ScriptKind::Setup,
  );
  assert_eq!(
    stopped_scope.inactive_scope_result.len(),
    1,
    "lone named stopped effectScope result demand must populate its channel: {stopped_scope:?}"
  );
  let missing_key = assert_gated_matches_forced(
    "import { reactive, toRefs } from 'vue'; void toRefs(reactive({ count: 1 })).missing.value;",
    ScriptKind::Setup,
  );
  assert_eq!(
    missing_key.missing_torefs_key.len(),
    1,
    "missing toRefs key demand must populate its channel: {missing_key:?}"
  );
  let extracted = assert_gated_matches_forced(
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
    ScriptKind::Setup,
  );
  assert_eq!(
    extracted.extracted_reactive_collection_method.len(),
    1,
    "positive extracted-method channel must populate: {extracted:?}"
  );
  let native_clone = assert_gated_matches_forced(
    "import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }));",
    ScriptKind::Setup,
  );
  assert_eq!(
    native_clone.uncloneable_proxy_data.len(),
    1,
    "named clone positive must yield a clone fact; {native_clone:?}"
  );
  let trigger = assert_gated_matches_forced(
    "import { reactive, triggerRef } from 'vue'; const obj = reactive({ n: 1 }); triggerRef(obj);",
    ScriptKind::Setup,
  );
  assert_eq!(trigger.trigger_ref_non_ref.first().map(|site| site.api.as_str()), Some("triggerRef"));
  let primitive = assert_gated_matches_forced(
    "import { shallowReadonly } from 'vue'; shallowReadonly(null);",
    ScriptKind::Setup,
  );
  assert_eq!(
    primitive.primitive_reactive_target.first().map(|site| site.api.as_str()),
    Some("shallowReadonly")
  );
}

#[test]
fn source_contracts_identity_table_preserves_named_namespace_and_auto_import() {
  let runtime_sources =
    ["vue", "vue-demi", "@vue/runtime-core", "@vue/runtime-dom", "@vue/reactivity"];
  for source_mod in runtime_sources {
    let named = format!(
      "import {{ triggerRef, ref as makeRef }} from '{source_mod}'; triggerRef(makeRef(1)); triggerRef({{ n: 1 }});"
    );
    let facts = assert_gated_matches_forced(&named, ScriptKind::Setup);
    assert_eq!(facts.trigger_ref_non_ref.len(), 1, "{named}");
    let aliased = format!(
      "import {{ watch as observe, ref }} from '{source_mod}'; const n = ref(0); observe(n.value, () => {{}});"
    );
    let facts = assert_gated_matches_forced(&aliased, ScriptKind::Setup);
    assert_eq!(facts.watch_unwrapped_source.len(), 1, "{aliased}");
    let string_name = format!(
      "import {{ 'watch' as observe, ref }} from '{source_mod}'; const n = ref(0); observe(n.value, () => {{}});"
    );
    let facts = assert_gated_matches_forced(&string_name, ScriptKind::Setup);
    assert_eq!(facts.watch_unwrapped_source.len(), 1, "{string_name}");
    let namespace = format!("import * as Vue from '{source_mod}'; Vue.triggerRef({{ n: 1 }});");
    let facts = assert_gated_matches_forced(&namespace, ScriptKind::Setup);
    assert_eq!(facts.trigger_ref_non_ref.len(), 1, "{namespace}");
  }
  let named_auto = "import { triggerRef } from '#imports'; triggerRef({ n: 1 });";
  let facts = assert_gated_matches_forced(named_auto, ScriptKind::Setup);
  assert_eq!(facts.trigger_ref_non_ref.len(), 1);
  let mixed_type = "import { type ref, triggerRef } from 'vue'; triggerRef({ n: 1 });";
  let facts = assert_gated_matches_forced(mixed_type, ScriptKind::Setup);
  assert_eq!(facts.trigger_ref_non_ref.len(), 1);
}

#[test]
fn source_contracts_macros_need_a_sink_and_setup_kind() {
  let props_setup = "import { triggerRef } from 'vue'; triggerRef(defineProps());";
  let props_facts = assert_gated_matches_forced(props_setup, ScriptKind::Setup);
  assert_eq!(
    props_facts.trigger_ref_non_ref.len(),
    1,
    "defineProps is a proven proxy in setup: {props_facts:?}"
  );
  let model_setup = "import { triggerRef } from 'vue'; triggerRef(defineModel());";
  let model_facts = assert_gated_matches_forced(model_setup, ScriptKind::Setup);
  assert!(
    model_facts.trigger_ref_non_ref.is_empty(),
    "defineModel is a proven ref in setup: {model_facts:?}"
  );
  let props_script = assert_gated_matches_forced(props_setup, ScriptKind::Script);
  assert!(
    props_script.trigger_ref_non_ref.is_empty(),
    "ordinary script does not prove defineProps: {props_script:?}"
  );
  let model_script = assert_gated_matches_forced(model_setup, ScriptKind::Script);
  assert!(
    model_script.trigger_ref_non_ref.is_empty(),
    "ordinary script does not prove defineModel: {model_script:?}"
  );
}

#[test]
fn source_contracts_sink_inventory_is_the_eligibility_table() {
  assert_eq!(contract_sink("triggerRef"), Some(ContractSink::TriggerRef));
  assert_eq!(contract_sink("toRefs"), Some(ContractSink::ToRefs));
  for api in ["reactive", "readonly", "shallowReactive", "shallowReadonly"] {
    assert_eq!(contract_sink(api), Some(ContractSink::ProxyConstructor), "{api}");
  }
  assert_eq!(contract_sink("watch"), Some(ContractSink::Watch));
  for api in ["watchEffect", "watchPostEffect", "watchSyncEffect"] {
    assert_eq!(contract_sink(api), Some(ContractSink::WatchEffectFamily), "{api}");
  }
  assert_eq!(contract_sink("toRef"), Some(ContractSink::ToRef));
  assert_eq!(contract_sink("effectScope"), Some(ContractSink::EffectScope));
  assert_eq!(contract_sink("customRef"), Some(ContractSink::CustomRef));
  assert_eq!(contract_sink("computed"), Some(ContractSink::Computed));
  assert_eq!(contract_sink("syncRef"), Some(ContractSink::SyncRef));
  assert_eq!(contract_sink("computedAsync"), Some(ContractSink::ComputedAsync));
  assert_eq!(contract_sink("onMounted"), Some(ContractSink::OnMounted));
  for api in ["ref", "shallowRef", "toRaw", "nextTick"] {
    assert_eq!(contract_sink(api), None, "{api} must not gate collection");
  }
}

#[test]
fn source_contracts_effect_family_named_imports_alone_match_forced_full() {
  let watch_effect = assert_gated_matches_forced(
    "import { watchEffect } from 'vue'; watchEffect(() => {}, { once: true });",
    ScriptKind::Setup,
  );
  assert_eq!(watch_effect.watch_ignored_option.len(), 1, "{watch_effect:?}");
  let post = assert_gated_matches_forced(
    "import { watchPostEffect } from 'vue'; watchPostEffect(() => {}, { immediate: true });",
    ScriptKind::Setup,
  );
  assert_eq!(post.watch_ignored_option.len(), 1, "{post:?}");
  let sync = assert_gated_matches_forced(
    "import { watchSyncEffect } from 'vue'; watchSyncEffect(() => {}, { flush: 'post', once: true });",
    ScriptKind::Setup,
  );
  assert_eq!(
    sync.watch_ignored_option.len(),
    1,
    "named watchSyncEffect with flush:'post' must still report ignored once; {sync:?}"
  );
  assert_eq!(
    sync.watch_ignored_option.first().map(|site| site.api.as_str()),
    Some("watchSyncEffect")
  );
  let namespace = assert_gated_matches_forced(
    "import * as Vue from 'vue'; Vue.watchSyncEffect(() => {}, { once: true });",
    ScriptKind::Setup,
  );
  assert_eq!(namespace.watch_ignored_option.len(), 1, "{namespace:?}");
  let ordinary = assert_gated_matches_forced(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n, (v) => v, { equals: () => true });",
    ScriptKind::Setup,
  );
  assert_eq!(ordinary.watch_ignored_option.len(), 1, "{ordinary:?}");
  assert!(ordinary.watch_unwrapped_source.is_empty(), "{ordinary:?}");
}

#[test]
fn source_contracts_callback_named_watch_import_alone_match_forced_full() {
  let once = assert_gated_matches_forced(
    "import { ref, watch } from 'vue';     const n = ref(0);     function accept(_value: unknown) {}     watch(n, (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });",
    ScriptKind::Setup,
  );
  assert_eq!(
    once.watch_callback_contracts.len(),
    1,
    "named watch + ref (non-sink) must report once-immediate; {once:?}"
  );
  assert_eq!(
    once.watch_callback_contracts.first().map(|site| site.reason),
    Some(vue_vet_core::WatchCallbackContractReason::OnceImmediateUndefinedGuard)
  );
  let identity = assert_gated_matches_forced(
    "import { reactive, watch } from 'vue';     const state = reactive({ n: 1 });     function accept(_value: unknown) {}     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    ScriptKind::Setup,
  );
  assert_eq!(
    identity.watch_callback_contracts.len(),
    1,
    "named watch + reactive must report root identity; {identity:?}"
  );
  assert_eq!(
    identity.watch_callback_contracts.first().map(|site| site.reason),
    Some(vue_vet_core::WatchCallbackContractReason::ReactiveRootIdentityGuard)
  );
}

#[test]
fn source_contracts_normalization_named_imports_alone_match_forced_full() {
  let toref =
    assert_gated_matches_forced("import { toRef } from 'vue'; toRef(1, 'k');", ScriptKind::Setup);
  assert_eq!(
    toref.toref_ignored_key.len(),
    1,
    "named toRef alone must report ignored-key; {toref:?}"
  );
  assert_eq!(
    toref.toref_ignored_key.first().map(|site| site.reason),
    Some(ToRefIgnoredKeyReason::Primitive)
  );
  let scope = assert_gated_matches_forced(
    "import { effectScope } from 'vue'; effectScope(() => {});",
    ScriptKind::Setup,
  );
  assert_eq!(
    scope.effect_scope_callback.len(),
    1,
    "named effectScope alone must report callback argument; {scope:?}"
  );
}

#[test]
fn source_contracts_bypass_without_fact_producing_sinks() {
  assert_bypass("", ScriptKind::Setup);
  assert_bypass("const n = 1;", ScriptKind::Script);
  assert_bypass(
    "import { ref, toRaw } from 'vue'; const n = ref(0); const d = toRaw(n);",
    ScriptKind::Setup,
  );
  assert_bypass(
    "import { type triggerRef } from 'vue'; const triggerRef = (_value: unknown) => {}; triggerRef({ n: 1 });",
    ScriptKind::Setup,
  );
  assert_bypass("import Vue from 'vue'; Vue.triggerRef({ n: 1 });", ScriptKind::Setup);
  assert_bypass("import * as Auto from '#imports'; Auto.triggerRef({ n: 1 });", ScriptKind::Setup);
  assert_bypass(
    "import { triggerRef } from '@vue/toolkit'; triggerRef({ n: 1 });",
    ScriptKind::Setup,
  );
  assert_bypass("triggerRef(1); reactive(0); watch(1, () => {});", ScriptKind::Setup);
  assert_bypass("defineProps<{ title: string }>();", ScriptKind::Setup);
  assert_bypass(
    "import { ref } from 'vue'; const o = { a: { x: 1 }, b: [1] }; const r = ref(o); const alias = r; alias.value = { a: { x: 2 } };",
    ScriptKind::Setup,
  );
}

#[test]
fn source_contracts_once_immediate_and_root_identity_predicates() {
  let once = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function accept(_value: unknown) {}\
     watch(n, (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert_eq!(
    once.source_contracts.watch_callback_contracts.len(),
    1,
    "{:?}",
    once.source_contracts
  );
  assert_eq!(
    once.source_contracts.watch_callback_contracts.first().map(|site| site.reason),
    Some(vue_vet_core::WatchCallbackContractReason::OnceImmediateUndefinedGuard)
  );
  let ordered = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function accept(_value: unknown) {}\
     watch(n, (next, old) => { if (old === void 0) return; accept(next); }, { immediate: true, once: true });",
    "ts",
  );
  assert_eq!(ordered.source_contracts.watch_callback_contracts.len(), 1);
  let reactive_array = analyze(
    "import { reactive, watch } from 'vue';\
     const list = reactive([1]);\
     function accept(_value: unknown) {}\
     watch(list, (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert_eq!(reactive_array.source_contracts.watch_callback_contracts.len(), 1);
  let tuple = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function accept(_value: unknown) {}\
     watch([n], (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert!(
    tuple.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    tuple.source_contracts
  );
  let identity = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ n: 1 });\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert_eq!(
    identity.source_contracts.watch_callback_contracts.len(),
    1,
    "{:?}",
    identity.source_contracts
  );
  assert_eq!(
    identity.source_contracts.watch_callback_contracts.first().map(|site| site.reason),
    Some(vue_vet_core::WatchCallbackContractReason::ReactiveRootIdentityGuard)
  );
  let getter = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ n: 1 });\
     function accept(_value: unknown) {}\
     watch(() => state.n, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(getter.source_contracts.watch_callback_contracts.is_empty());
  let deep_ref = analyze(
    "import { ref, watch } from 'vue';\
     const source = ref({ n: 1 });\
     function accept(_value: unknown) {}\
     watch(source, (next, old) => { if (next === old) return; accept(next); }, { deep: true });",
    "ts",
  );
  assert!(deep_ref.source_contracts.watch_callback_contracts.is_empty());
  let local_undefined = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     const undefined = 1;\
     function accept(_value: unknown) {}\
     watch(n, (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert!(local_undefined.source_contracts.watch_callback_contracts.is_empty());
  let void_effect = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function accept(_value: unknown) {}\
     watch(n, (next, old) => { if (old === void accept(next)) return; accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert!(
    void_effect.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    void_effect.source_contracts
  );
  let after_return = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function accept(_value: unknown) {}\
     watch(n, (next, old) => { if (old === undefined) return; return; accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert!(
    after_return.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    after_return.source_contracts
  );
  let dead_branch = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function accept(_value: unknown) {}\
     watch(n, (next, old) => { if (old === undefined) return; if (false) accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert!(
    dead_branch.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    dead_branch.source_contracts
  );
  let tagged = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ __v_isRef: true, value: 0 });\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    tagged.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    tagged.source_contracts
  );
  let proxy_ref = analyze(
    "import { reactive, ref, watch } from 'vue';\
     const source = reactive(ref(0));\
     function accept(_value: unknown) {}\
     watch(source, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    proxy_ref.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    proxy_ref.source_contracts
  );
  let inherited = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ n: 1 });\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); }, { __proto__: { immediate: true } });",
    "ts",
  );
  assert_eq!(
    inherited.source_contracts.watch_callback_contracts.len(),
    1,
    "{:?}",
    inherited.source_contracts
  );
  let unicode = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function 接收(_value: unknown) {}\
     watch(n, (新, 旧) => { if (旧 === undefined) return; 接收(新); }, { once: true, immediate: true });",
    "ts",
  );
  assert_eq!(unicode.source_contracts.watch_callback_contracts.len(), 1);
  let crlf = analyze(
    "import { ref, watch } from 'vue';\r\nconst n = ref(0);\r\nfunction accept(_value: unknown) {}\r\nwatch(n, (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });\r\n",
    "ts",
  );
  #[expect(clippy::panic, reason = "missing CRLF span evidence must fail the regression")]
  let Some(site) = crlf.source_contracts.watch_callback_contracts.first() else {
    panic!("CRLF once-immediate fact missing");
  };
  assert!(site.guard_span.offset > 0 && site.watch_span.length > 0);
}

#[test]
fn source_contracts_second_review_false_positives_stay_quiet() {
  let nested_block = analyze(
    "import { ref, watch } from 'vue';\
     const n = ref(0);\
     function accept(_value: unknown) {}\
     watch(n, (next, old) => { if (old === undefined) return; { return } accept(next); }, { once: true, immediate: true });",
    "ts",
  );
  assert!(
    nested_block.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    nested_block.source_contracts
  );
  let later_marker = analyze(
    "import { reactive, watch } from 'vue';\
     const taggedLater = reactive({ value: 0 });\
     taggedLater.__v_isRef = true;\
     function accept(_value: unknown) {}\
     watch(taggedLater, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    later_marker.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    later_marker.source_contracts
  );
  let frozen = analyze(
    "import { reactive, watch } from 'vue';\
     const frozenTarget = { n: 0 };\
     Object.freeze(frozenTarget);\
     const frozenState = reactive(frozenTarget);\
     function accept(_value: unknown) {}\
     watch(frozenState, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    frozen.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    frozen.source_contracts
  );
  let readonly_marker = analyze(
    "import { reactive, watch } from 'vue';\
     const readonlyMarker = reactive({ __v_isReadonly: true, n: 0 });\
     function accept(_value: unknown) {}\
     watch(readonlyMarker, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    readonly_marker.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    readonly_marker.source_contracts
  );
  let raw_marker = analyze(
    "import { reactive, watch } from 'vue';\
     const rawMarker = reactive({ __v_raw: { n: 0 }, n: 0 });\
     function accept(_value: unknown) {}\
     watch(rawMarker, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    raw_marker.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    raw_marker.source_contracts
  );
  let field_mutate = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ n: 1 });\
     state.n = 2;\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert_eq!(
    field_mutate.source_contracts.watch_callback_contracts.len(),
    1,
    "{:?}",
    field_mutate.source_contracts
  );
  let pattern_field = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ n: 1 });\
     ({ n: state.n } = { n: 2 });\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert_eq!(
    pattern_field.source_contracts.watch_callback_contracts.len(),
    1,
    "{:?}",
    pattern_field.source_contracts
  );
}

#[test]
fn source_contracts_third_review_capability_roles_stay_quiet() {
  let pattern_marker = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0 });\
     ({ x: tagged.__v_isRef } = { x: true });\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    pattern_marker.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    pattern_marker.source_contracts
  );
  let nested_default_rest = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0 });\
     ({ nested: { x: tagged.__v_isRef = true } = {}, ..._rest } = { nested: {} });\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    nested_default_rest.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    nested_default_rest.source_contracts
  );
  let ts_wrapper = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0 });\
     ({ x: (tagged.__v_isRef as boolean) } = { x: true });\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    ts_wrapper.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    ts_wrapper.source_contracts
  );
  let spread_freeze = analyze(
    "import { reactive, watch } from 'vue';\
     const frozen = { n: 0 };\
     Object.freeze(...[frozen]);\
     const state = reactive(frozen);\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    spread_freeze.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    spread_freeze.source_contracts
  );
  let indexed_freeze = analyze(
    "import { reactive, watch } from 'vue';\
     const published = { n: 0 };\
     const container = [published];\
     Object.freeze(container[0]);\
     const publishedState = reactive(published);\
     function accept(_value: unknown) {}\
     watch(publishedState, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    indexed_freeze.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    indexed_freeze.source_contracts
  );
  let sequence_freeze = analyze(
    "import { reactive, watch } from 'vue';\
     const frozen = { n: 0 };\
     Object.freeze((0, frozen));\
     const state = reactive(frozen);\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    sequence_freeze.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    sequence_freeze.source_contracts
  );
}

#[test]
fn source_contracts_fourth_review_pattern_values_and_receiver() {
  let same_object_pattern = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ child: { n: 0 } });\
     watch(state.child, () => {});\
     ({ child: state.child } = { child: state.child });",
    "ts",
  );
  assert!(
    same_object_pattern.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    same_object_pattern.source_contracts
  );
  let same_array_pattern = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ child: { n: 0 } });\
     watch(state.child, () => {});\
     [state.child] = [state.child];",
    "ts",
  );
  assert!(
    same_array_pattern.source_contracts.watch_replaced_object_source.is_empty(),
    "{:?}",
    same_array_pattern.source_contracts
  );
  let direct_replace = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ child: { n: 0 } });\
     watch(state.child, () => {});\
     state.child = { n: 1 };",
    "ts",
  );
  assert_eq!(
    direct_replace.source_contracts.watch_replaced_object_source.len(),
    1,
    "{:?}",
    direct_replace.source_contracts
  );
  let receiver = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0, tag: function () { this.__v_isRef = true } });\
     tagged.tag();\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    receiver.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    receiver.source_contracts
  );
  let tagged_template = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0, tag: function () { this.__v_isRef = true } });\
     tagged.tag``;\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    tagged_template.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    tagged_template.source_contracts
  );
  let tagged_template_wrapper = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0, tag: function () { this.__v_isRef = true } });\
     ((tagged.tag as () => void)!)``;\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    tagged_template_wrapper.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    tagged_template_wrapper.source_contracts
  );
  let tagged_template_args = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0, tag: function () { this.__v_isRef = true } });\
     tagged.tag<string>``;\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    tagged_template_args.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    tagged_template_args.source_contracts
  );
  let instantiated_tagged = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0, tag: function () { this.__v_isRef = true } });\
     (tagged.tag<number>)``;\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    instantiated_tagged.source_contracts.watch_callback_contracts.is_empty(),
    "TS instantiation tagged-template receiver must abstain; {:?}",
    instantiated_tagged.source_contracts
  );
  let instantiated_call = analyze(
    "import { reactive, watch } from 'vue';\
     const tagged = reactive({ value: 0, tag: function () { this.__v_isRef = true } });\
     tagged.tag<number>();\
     function accept(_value: unknown) {}\
     watch(tagged, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    instantiated_call.source_contracts.watch_callback_contracts.is_empty(),
    "TS instantiation call receiver must abstain; {:?}",
    instantiated_call.source_contracts
  );
  let receiver_freeze = analyze(
    "import { reactive, watch } from 'vue';\
     const raw = { n: 0, lock: function () { Object.freeze(this) } };\
     raw.lock();\
     const state = reactive(raw);\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert!(
    receiver_freeze.source_contracts.watch_callback_contracts.is_empty(),
    "{:?}",
    receiver_freeze.source_contracts
  );
  let field_mutate = analyze(
    "import { reactive, watch } from 'vue';\
     const state = reactive({ n: 1 });\
     state.n = 2;\
     function accept(_value: unknown) {}\
     watch(state, (next, old) => { if (next === old) return; accept(next); });",
    "ts",
  );
  assert_eq!(
    field_mutate.source_contracts.watch_callback_contracts.len(),
    1,
    "{:?}",
    field_mutate.source_contracts
  );
}

#[test]
fn source_parent_identifier_watch_use_drops_unwrapped_payload_proof() {
  let alone =
    analyze("import { ref, watch } from 'vue'; const n = ref(0); watch(n.value, () => {});", "ts");
  assert_eq!(
    alone.source_contracts.watch_unwrapped_source.len(),
    1,
    "parent unwrapped proof for watch(n.value) must stay; {:?}",
    alone.source_contracts
  );
  let combined = analyze(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n.value, () => {}); watch(n, () => {});",
    "ts",
  );
  assert!(
    combined.source_contracts.watch_unwrapped_source.is_empty(),
    "source-parent escape/uncertain indexing: a later identifier watch(n) currently drops watch(n.value); {:?}",
    combined.source_contracts
  );
  assert!(
    combined.source_contracts.watch_callback_contracts.is_empty(),
    "empty callback must not emit callback-contract facts; {:?}",
    combined.source_contracts
  );
}

#[test]
#[expect(clippy::panic, reason = "shared-literal fixture construction must fail the test")]
fn source_contracts_shared_wide_literal_watchers_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [32_u64, 64, 128] {
    let mut fields = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    for index in 0..size {
      fields.push(format!("f{index}: {index}"));
    }
    let mut source = format!(
      "import {{ reactive, watch }} from 'vue'; function accept(_value: unknown) {{}} const raw = {{ {} }};",
      fields.join(", ")
    );
    for index in 0..size {
      write!(
        source,
        "const s{index} = reactive(raw); watch(s{index}, (next, old) => {{ if (next === old) return; accept(next); }});"
      )
      .unwrap_or_else(|error| panic!("shared-literal fixture write: {error}"));
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.watch_callback_contracts.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "shared-literal identity facts at width {size}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-literal watcher work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_watch_callback_sites_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [50_u64, 100, 200] {
    let mut source = String::from(
      "import { ref, watch } from 'vue'; const n = ref(0); function accept(_value: unknown) {}",
    );
    for _ in 0..size {
      source.push_str(
        "watch(n, (next, old) => { if (old === undefined) return; accept(next); }, { once: true, immediate: true });",
      );
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.watch_callback_contracts.len(),
      usize::try_from(size).unwrap_or(usize::MAX)
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "callback-site work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
#[expect(clippy::panic, reason = "missing span evidence must fail the regression")]
fn source_contracts_structured_clone_reports_proven_proxy() {
  let direct =
    analyze("import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }));", "ts");
  assert_eq!(
    direct.source_contracts.uncloneable_proxy_data.len(),
    1,
    "{:?}",
    direct.source_contracts
  );
  let alias = analyze(
    "import { reactive } from 'vue'; const state = reactive({ count: 1 }); const alias = state; structuredClone(alias);",
    "ts",
  );
  assert_eq!(
    alias.source_contracts.uncloneable_proxy_data.len(),
    1,
    "{:?}",
    alias.source_contracts
  );
  let mutated = analyze(
    "import { reactive } from 'vue'; const state = reactive({ count: 1 }); state.count = 2; structuredClone(state);",
    "ts",
  );
  assert_eq!(
    mutated.source_contracts.uncloneable_proxy_data.len(),
    1,
    "{:?}",
    mutated.source_contracts
  );
  let asserted = analyze(
    "import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }) as { count: number });",
    "ts",
  );
  assert_eq!(
    asserted.source_contracts.uncloneable_proxy_data.len(),
    1,
    "{:?}",
    asserted.source_contracts
  );
  let Some(site) = asserted.source_contracts.uncloneable_proxy_data.first() else {
    panic!("assertion span missing");
  };
  let source =
    "import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }) as { count: number });";
  let Some(arg) = source.find("reactive({ count: 1 }) as { count: number }") else {
    panic!("assertion argument missing");
  };
  assert_eq!(site.span.offset, arg);
  assert_eq!(site.span.length, "reactive({ count: 1 }) as { count: number }".len());
}

#[test]
fn source_contracts_structured_clone_named_imports_alone_match_forced_full() {
  let facts = assert_gated_matches_forced(
    "import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }));",
    ScriptKind::Setup,
  );
  assert_eq!(
    facts.uncloneable_proxy_data.len(),
    1,
    "named clone positive must yield a clone fact; {facts:?}"
  );
}

#[test]
fn source_contracts_structured_clone_stays_quiet_for_controls() {
  let native_only = analyze("structuredClone({ count: 1 });", "ts");
  assert!(
    native_only.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    native_only.source_contracts
  );
  let raw = analyze(
    "import { markRaw, reactive } from 'vue'; const raw = { count: 1 }; markRaw(raw); const value = reactive(raw); structuredClone(value);",
    "ts",
  );
  assert!(raw.source_contracts.uncloneable_proxy_data.is_empty(), "{:?}", raw.source_contracts);
  let to_raw = analyze(
    "import { reactive, toRaw } from 'vue'; structuredClone(toRaw(reactive({ count: 1 })));",
    "ts",
  );
  assert!(
    to_raw.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    to_raw.source_contracts
  );
  let marker = analyze(
    "import { reactive } from 'vue'; structuredClone(reactive({ __v_skip: true, count: 1 }));",
    "ts",
  );
  assert!(
    marker.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    marker.source_contracts
  );
  let shadow = analyze(
    "import { reactive } from 'vue'; function structuredClone(_value: unknown) {} structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    shadow.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    shadow.source_contracts
  );
  let two =
    analyze("import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }), {});", "ts");
  assert!(two.source_contracts.uncloneable_proxy_data.is_empty(), "{:?}", two.source_contracts);
  let nested = analyze(
    "import { reactive } from 'vue'; const state = reactive({ count: 1 }); structuredClone({ state });",
    "ts",
  );
  assert!(
    nested.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    nested.source_contracts
  );
}

#[test]
#[expect(clippy::panic, reason = "missing span evidence must fail the regression")]
fn source_contracts_structured_clone_unicode_and_crlf_span_the_data_argument() {
  let unicode = "import { reactive } from 'vue'; structuredClone(reactive({ 计数: 1 }));";
  let facts = analyze(unicode, "ts");
  let needle = "reactive({ 计数: 1 })";
  let Some(arg) = unicode.find(needle) else {
    panic!("unicode argument missing");
  };
  let Some(site) = facts.source_contracts.uncloneable_proxy_data.first() else {
    panic!("unicode finding missing: {:?}", facts.source_contracts);
  };
  assert_eq!(site.span.offset, arg);
  assert_eq!(site.span.length, needle.len());

  let crlf = "import { reactive } from 'vue';\r\nstructuredClone(reactive({ count: 1 }));";
  let facts = analyze(crlf, "ts");
  let needle = "reactive({ count: 1 })";
  let Some(arg) = crlf.find(needle) else {
    panic!("crlf argument missing");
  };
  let Some(site) = facts.source_contracts.uncloneable_proxy_data.first() else {
    panic!("crlf finding missing: {:?}", facts.source_contracts);
  };
  assert_eq!(site.span.offset, arg);
  assert_eq!(site.span.length, needle.len());
  assert_eq!(site.span.line, 2);
}

#[test]
fn source_contracts_structured_clone_sites_scale_sublinear_per_doubling() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [32_u64, 64, 128] {
    let mut source = String::from("import { reactive } from 'vue';");
    for index in 0..size {
      source.push_str("const s");
      source.push_str(&index.to_string());
      source.push_str(" = reactive({ n: 1 }); structuredClone(s");
      source.push_str(&index.to_string());
      source.push_str(");");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.uncloneable_proxy_data.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "size {size} findings; {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "structured-clone work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_structured_clone_poisons_equivalent_native_writes() {
  let computed = analyze(
    "import { reactive } from 'vue'; globalThis['structuredClone'] = ((value: unknown) => value) as typeof structuredClone; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    computed.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    computed.source_contracts
  );
  let pattern_member = analyze(
    "import { reactive } from 'vue'; ({ clone: globalThis.structuredClone } = { clone: ((value: unknown) => value) as typeof structuredClone }); structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    pattern_member.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    pattern_member.source_contracts
  );
  let pattern_global = analyze(
    "import { reactive } from 'vue'; [structuredClone] = [((value: unknown) => value) as typeof structuredClone]; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    pattern_global.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    pattern_global.source_contracts
  );
  let asserted = analyze(
    "import { reactive } from 'vue'; (globalThis.structuredClone as typeof structuredClone) = ((value: unknown) => value) as typeof structuredClone; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    asserted.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    asserted.source_contracts
  );
  let deleted = analyze(
    "import { reactive } from 'vue'; delete globalThis.structuredClone; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    deleted.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    deleted.source_contracts
  );
  let rest = analyze(
    "import { reactive } from 'vue'; ({ ...structuredClone } = { structuredClone: ((value: unknown) => value) as typeof structuredClone }); structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(rest.source_contracts.uncloneable_proxy_data.is_empty(), "{:?}", rest.source_contracts);
  let defaulted = analyze(
    "import { reactive } from 'vue'; ({ structuredClone = ((value: unknown) => value) as typeof structuredClone } = {}); structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    defaulted.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    defaulted.source_contracts
  );
  let updated = analyze(
    "import { reactive } from 'vue'; structuredClone++; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    updated.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    updated.source_contracts
  );
  let dormant = analyze(
    "import { reactive } from 'vue'; function replace() { structuredClone = ((value: unknown) => value) as typeof structuredClone; } structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    dormant.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    dormant.source_contracts
  );
  let dynamic_ident = analyze(
    "import { reactive } from 'vue'; const key = 'structuredClone'; globalThis[key] = ((value: unknown) => value) as typeof structuredClone; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    dynamic_ident.source_contracts.uncloneable_proxy_data.is_empty(),
    "unresolved computed globalThis write must poison; {:?}",
    dynamic_ident.source_contracts
  );
}

#[test]
fn source_contracts_structured_clone_poisons_loop_assignment_targets() {
  const PREFIX: &str = "import { reactive } from 'vue'; ";
  const SUFFIX: &str = " structuredClone(reactive({ count: 1 }));";
  let identity = "((value: unknown) => value) as typeof structuredClone";
  let cases = [
    (
      "for-of dynamic",
      format!("const key = 'structuredClone'; for (globalThis[key] of [{identity}]) {{}}"),
    ),
    ("for-of static", format!("for (globalThis.structuredClone of [{identity}]) {{}}")),
    ("for-of ident", format!("for (structuredClone of [{identity}]) {{}}")),
    (
      "for-of member pattern",
      format!("for ({{ clone: globalThis.structuredClone }} of [{{ clone: {identity} }}]) {{}}"),
    ),
    ("for-of default", format!("for ({{ structuredClone = {identity} }} of [{{}}]) {{}}")),
    (
      "for-of rest",
      format!("for ({{ ...structuredClone }} of [{{ structuredClone: {identity} }}]) {{}}"),
    ),
    ("for-of array", format!("for ([structuredClone] of [[{identity}]]) {{}}")),
    (
      "for-of ts",
      format!("for ((globalThis.structuredClone as typeof structuredClone) of [{identity}]) {{}}"),
    ),
    (
      "for-in dynamic",
      "const key = 'structuredClone'; for (globalThis[key] in { x: 1 }) {}".to_string(),
    ),
    ("for-in static", "for (globalThis.structuredClone in { x: 1 }) {}".to_string()),
    (
      "for-await-of",
      format!(
        "async function replace() {{ for await (globalThis.structuredClone of [{identity}]) {{}} }}"
      ),
    ),
  ];
  for (label, head) in cases {
    let source = format!("{PREFIX}{head}{SUFFIX}");
    let facts = analyze(&source, "ts");
    assert!(
      facts.source_contracts.uncloneable_proxy_data.is_empty(),
      "{label} must poison native identity; {:?}",
      facts.source_contracts
    );
  }

  let unrelated = analyze(
    "import { reactive } from 'vue'; for (globalThis['fetch'] of [((value: unknown) => value) as typeof fetch]) {} structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert_eq!(
    unrelated.source_contracts.uncloneable_proxy_data.len(),
    1,
    "unrelated loop key must not poison; {:?}",
    unrelated.source_contracts
  );
  let shadowed = analyze(
    "import { reactive } from 'vue'; const globalThis = { structuredClone: ((value: unknown) => value) as typeof structuredClone }; for (globalThis.structuredClone of [((value: unknown) => value) as typeof structuredClone]) {} structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert_eq!(
    shadowed.source_contracts.uncloneable_proxy_data.len(),
    1,
    "shadowed globalThis loop write must not poison; {:?}",
    shadowed.source_contracts
  );
  let declared = analyze(
    "import { reactive } from 'vue'; for (const structuredClone of [((value: unknown) => value) as typeof structuredClone]) {} structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert_eq!(
    declared.source_contracts.uncloneable_proxy_data.len(),
    1,
    "declaration loop heads keep binding semantics; {:?}",
    declared.source_contracts
  );
}

#[test]
fn source_contracts_structured_clone_loop_assignment_targets_count_work() {
  let base = "import { reactive } from 'vue'; structuredClone(reactive({ count: 1 }));";
  let (_, base_stats) = contract_full_stats(base);
  let mut loops = String::from("import { reactive } from 'vue';");
  for index in 0..8 {
    loops.push_str("for (globalThis['fetch'] of [0]) {} // ");
    loops.push_str(&index.to_string());
    loops.push('\n');
  }
  loops.push_str("structuredClone(reactive({ count: 1 }));");
  let (facts, stats) = contract_full_stats(&loops);
  assert_eq!(
    facts.uncloneable_proxy_data.len(),
    1,
    "unrelated loop keys must keep the clone positive; {facts:?}"
  );
  assert_eq!(
    stats.writes,
    base_stats.writes.saturating_add(8),
    "each assignment-form loop head charges one write visit; base={base_stats:?} loops={stats:?}"
  );
  assert!(
    stats.import_source_steps <= 2,
    "loop poison must not extra-walk import sources; {stats:?}"
  );
  let declared = "import { reactive } from 'vue'; for (const x of [0]) {} structuredClone(reactive({ count: 1 }));";
  let (_, declared_stats) = contract_full_stats(declared);
  assert_eq!(
    declared_stats.writes, base_stats.writes,
    "declaration loop heads must not charge assignment-target writes; {declared_stats:?}"
  );
}

#[test]
fn source_contracts_structured_clone_keeps_unrelated_static_globalthis_key_eligible() {
  let facts = analyze(
    "import { reactive } from 'vue'; globalThis['fetch'] = ((value: unknown) => value) as typeof fetch; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert_eq!(
    facts.source_contracts.uncloneable_proxy_data.len(),
    1,
    "known unrelated static key must not poison; {:?}",
    facts.source_contracts
  );
}

#[test]
fn source_contracts_structured_clone_requires_definite_call_key() {
  let dynamic_call = analyze(
    "import { reactive } from 'vue'; const key = 'structuredClone'; globalThis[key](reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    dynamic_call.source_contracts.uncloneable_proxy_data.is_empty(),
    "computed call is not a definite native intrinsic; {:?}",
    dynamic_call.source_contracts
  );
  let identity_call = analyze(
    "import { reactive } from 'vue'; const key = 'identity'; globalThis[key](reactive({ count: 1 }));",
    "ts",
  );
  assert!(
    identity_call.source_contracts.uncloneable_proxy_data.is_empty(),
    "{:?}",
    identity_call.source_contracts
  );
}

#[test]
fn source_contracts_structured_clone_keeps_shadowed_global_this_eligible() {
  let facts = analyze(
    "import { reactive } from 'vue'; const globalThis = { structuredClone: ((value: unknown) => value) as typeof structuredClone }; globalThis.structuredClone = ((value: unknown) => value) as typeof structuredClone; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert_eq!(
    facts.source_contracts.uncloneable_proxy_data.len(),
    1,
    "{:?}",
    facts.source_contracts
  );
}

#[test]
fn source_contracts_structured_clone_excludes_optional_native_calls() {
  let facts =
    analyze("import { reactive } from 'vue'; structuredClone?.(reactive({ count: 1 }));", "ts");
  assert!(facts.source_contracts.uncloneable_proxy_data.is_empty(), "{:?}", facts.source_contracts);
}

#[test]
#[expect(clippy::panic, reason = "missing span evidence must fail the regression")]
fn source_contracts_structured_clone_budget_does_not_cache_exhaustion() {
  let deep_first = "import { reactive, readonly } from 'vue'; const state = reactive({ count: 1 }); function deep() { return structuredClone(readonly(readonly(readonly(readonly(readonly(readonly(readonly(state)))))))); } function direct() { return structuredClone(state); }";
  let facts = analyze(deep_first, "ts");
  let Some(direct_at) = deep_first.rfind("structuredClone(state)") else {
    panic!("direct call missing");
  };
  let arg = direct_at + "structuredClone(".len();
  assert!(
    facts.source_contracts.uncloneable_proxy_data.iter().any(|site| site.span.offset == arg),
    "direct state argument must report after a deeper exhausted query; {:?}",
    facts.source_contracts
  );

  let direct_first = "import { reactive, readonly } from 'vue'; const state = reactive({ count: 1 }); function direct() { return structuredClone(state); } function deep() { return structuredClone(readonly(readonly(readonly(readonly(readonly(readonly(readonly(state)))))))); }";
  let facts = analyze(direct_first, "ts");
  let Some(direct_at) = direct_first.find("structuredClone(state)") else {
    panic!("direct call missing");
  };
  let arg = direct_at + "structuredClone(".len();
  assert!(
    facts.source_contracts.uncloneable_proxy_data.iter().any(|site| site.span.offset == arg),
    "direct state argument must report when declared first; {:?}",
    facts.source_contracts
  );
}

#[test]
fn source_contracts_structured_clone_requires_vue3_proxy_origin() {
  let auto =
    analyze("import { reactive } from '#imports'; structuredClone(reactive({ count: 1 }));", "ts");
  assert!(auto.source_contracts.uncloneable_proxy_data.is_empty(), "{:?}", auto.source_contracts);
  let demi =
    analyze("import { reactive } from 'vue-demi'; structuredClone(reactive({ count: 1 }));", "ts");
  assert!(demi.source_contracts.uncloneable_proxy_data.is_empty(), "{:?}", demi.source_contracts);
  let namespace =
    analyze("import * as Vue from 'vue'; structuredClone(Vue.reactive({ count: 1 }));", "ts");
  assert_eq!(
    namespace.source_contracts.uncloneable_proxy_data.len(),
    1,
    "{:?}",
    namespace.source_contracts
  );
  let runtime_core = analyze(
    "import { reactive } from '@vue/runtime-core'; structuredClone(reactive({ count: 1 }));",
    "ts",
  );
  assert_eq!(
    runtime_core.source_contracts.uncloneable_proxy_data.len(),
    1,
    "{:?}",
    runtime_core.source_contracts
  );
  let primitive_auto = analyze("import { reactive } from '#imports'; void reactive(0);", "ts");
  assert_eq!(
    primitive_auto.source_contracts.primitive_reactive_target.len(),
    1,
    "named #imports must still feed the other source-contract rules; {:?}",
    primitive_auto.source_contracts
  );
}

#[test]
fn source_contracts_structured_clone_wide_negative_cache_and_property_width() {
  let mut previous: Option<(u64, u64)> = None;
  for width in [32_u64, 64, 128] {
    let mut source = String::from("import { reactive } from 'vue'; const state = reactive({");
    for index in 0..width {
      source.push_str(" p");
      source.push_str(&index.to_string());
      source.push_str(": 1,");
    }
    source.push_str(" __v_skip: true });");
    for index in 0..width {
      source.push_str(" structuredClone(state); // q");
      source.push_str(&index.to_string());
    }
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.uncloneable_proxy_data.is_empty(),
      "ineligible wide target must stay quiet at width {width}; {contracts:?}"
    );
    if let Some((prev_width, prev_work)) = previous {
      assert_eq!(width, prev_width * 2, "widths must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "wide negative clone work grew from {prev_work} to {work} on {prev_width}->{width} (must stay <3x per doubling)"
      );
    }
    previous = Some((width, work));
  }
}

#[test]
fn source_contracts_import_source_steps_bypass_nested_local_calls() {
  fn nest(depth: u32, with_positive: bool) -> String {
    let mut source = String::from("import { reactive } from 'vue';\n");
    for index in 0..depth {
      source.push_str("function local");
      source.push_str(&index.to_string());
      source.push_str("() {\n");
    }
    source.push_str("void 0;\n");
    for index in (0..depth).rev() {
      source.push_str("}\nlocal");
      source.push_str(&index.to_string());
      source.push_str("();\n");
    }
    if with_positive {
      source.push_str("structuredClone(reactive({ count: 1 }));\n");
    }
    source
  }

  let (quiet, quiet_stats) = contract_full_stats(&nest(64, false));
  assert!(quiet.uncloneable_proxy_data.is_empty(), "{quiet:?}");
  assert!(
    quiet_stats.import_source_steps == 0,
    "nested local calls must not examine import sources; {quiet_stats:?}"
  );

  let mut previous: Option<(u32, u64, u64)> = None;
  for depth in [32_u32, 64, 128] {
    let (contracts, stats) = contract_full_stats(&nest(depth, true));
    assert_eq!(
      contracts.uncloneable_proxy_data.len(),
      1,
      "depth {depth} must keep the constructor/clone positive; {contracts:?}"
    );
    assert!(
      stats.import_source_steps <= 2,
      "only the nested Vue constructor (call node + argument record) may examine import source at depth {depth}; {stats:?}"
    );
    if let Some((prev_depth, prev_work, prev_steps)) = previous {
      assert_eq!(depth, prev_depth * 2, "depths must double");
      assert_eq!(stats.import_source_steps, prev_steps);
      assert!(
        stats.work().saturating_mul(10) < prev_work.saturating_mul(30),
        "nested-local clone work grew from {prev_work} to {} on {prev_depth}->{depth}",
        stats.work()
      );
    }
    previous = Some((depth, stats.work(), stats.import_source_steps));
  }
}

#[test]
fn source_contracts_effect_family_one_arg_and_no_args_bypass_indexes() {
  for api in ["watchEffect", "watchPostEffect", "watchSyncEffect"] {
    assert_bypass(&format!("import {{ {api} }} from 'vue'; {api}(() => {{}});"), ScriptKind::Setup);
    assert_bypass(&format!("import {{ {api} }} from 'vue'; {api}();"), ScriptKind::Setup);
    assert_bypass(
      &format!("import {{ {api} as run }} from 'vue'; run(() => {{}});"),
      ScriptKind::Setup,
    );
    assert_bypass(
      &format!("import {{ {api} }} from 'vue'; {api}?.(() => {{}});"),
      ScriptKind::Setup,
    );
    assert_bypass(
      &format!("import {{ {api} }} from 'vue'; ({api} as any)(() => {{}});"),
      ScriptKind::Setup,
    );
    assert_bypass(
      &format!("import {{ {api} }} from 'vue'; {api}!(() => {{}});"),
      ScriptKind::Setup,
    );
    assert_bypass(
      &format!("import {{ {api} }} from '#imports'; {api}(() => {{}});"),
      ScriptKind::Setup,
    );
  }
}

#[test]
fn source_contracts_effect_family_second_arg_and_spreads_match_forced_full() {
  for api in ["watchEffect", "watchPostEffect", "watchSyncEffect"] {
    let ignored = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; {api}(() => {{}}, {{ once: true }});"),
      ScriptKind::Setup,
    );
    assert_eq!(ignored.watch_ignored_option.len(), 1, "{api} ignored once");
    let valid = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; {api}(() => {{}}, {{ flush: 'pre' }});"),
      ScriptKind::Setup,
    );
    assert!(valid.is_empty(), "{api} flush-only options must stay quiet: {valid:?}");
    let invalid_immediate = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; {api}(() => {{}}, {{ immediate: true }});"),
      ScriptKind::Setup,
    );
    assert_eq!(invalid_immediate.watch_ignored_option.len(), 1, "{api} ignored immediate");
    let signature = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; {api}(() => {{}}, () => {{}});"),
      ScriptKind::Setup,
    );
    assert_eq!(signature.watch_signature_mismatch.len(), 1, "{api} function-as-options");
    let spread = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; const args = [() => {{}}]; {api}(...args);"),
      ScriptKind::Setup,
    );
    assert!(spread.is_empty(), "{api} spread must stay equivalent: {spread:?}");
    let trailing_spread = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; {api}(() => {{}}, ...[{{ once: true }}]);"),
      ScriptKind::Setup,
    );
    assert!(
      trailing_spread.is_empty(),
      "{api} trailing spread must stay equivalent: {trailing_spread:?}"
    );
    let alias = assert_gated_matches_forced(
      &format!("import {{ {api} as run }} from 'vue'; run(() => {{}}, {{ once: true }});"),
      ScriptKind::Setup,
    );
    assert_eq!(alias.watch_ignored_option.len(), 1, "{api} alias");
    let optional = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; {api}?.(() => {{}}, {{ once: true }});"),
      ScriptKind::Setup,
    );
    assert_eq!(optional.watch_ignored_option.len(), 1, "{api} optional call");
    let wrapped = assert_gated_matches_forced(
      &format!("import {{ {api} }} from 'vue'; ({api} as any)(() => {{}}, {{ once: true }});"),
      ScriptKind::Setup,
    );
    assert_eq!(wrapped.watch_ignored_option.len(), 1, "{api} ts wrapper");
  }
}

#[test]
fn source_contracts_effect_family_namespace_aliases_and_unsupported_match_forced_full() {
  let namespace_one = assert_gated_matches_forced(
    "import * as Vue from 'vue'; Vue.watchEffect(() => {});",
    ScriptKind::Setup,
  );
  assert!(namespace_one.is_empty(), "{namespace_one:?}");
  let namespace_two = assert_gated_matches_forced(
    "import * as Vue from 'vue'; Vue.watchPostEffect(() => {}, { deep: true });",
    ScriptKind::Setup,
  );
  assert_eq!(namespace_two.watch_ignored_option.len(), 1, "{namespace_two:?}");
  let namespace_sync = assert_gated_matches_forced(
    "import * as Vue from 'vue'; Vue.watchSyncEffect(() => {}, { once: true });",
    ScriptKind::Setup,
  );
  assert_eq!(namespace_sync.watch_ignored_option.len(), 1, "{namespace_sync:?}");
  let binding = assert_gated_matches_forced(
    "import { watchEffect } from 'vue'; const fx = watchEffect; fx(() => {}, { once: true });",
    ScriptKind::Setup,
  );
  assert!(binding.is_empty(), "imported binding is not followed as a call: {binding:?}");
  assert_bypass(
    "import { watchEffect } from 'vue'; function inner() { const watchEffect = (_a: unknown, _b: unknown) => {}; watchEffect(() => {}, { once: true }); }",
    ScriptKind::Setup,
  );
  let dynamic = assert_gated_matches_forced(
    "import { watchEffect } from 'vue'; watchEffect.call(null, () => {}, { once: true });",
    ScriptKind::Setup,
  );
  assert!(dynamic.is_empty(), "dynamic .call stays unproven: {dynamic:?}");
  assert_bypass(
    "import { type watchEffect } from 'vue'; const watchEffect = (_a: unknown, _b: unknown) => {}; watchEffect(() => {}, { once: true });",
    ScriptKind::Setup,
  );
}

#[test]
fn source_contracts_mixed_and_ordinary_watch_keep_full_index() {
  let mixed = assert_gated_matches_forced(
    "import { ref, watch, watchEffect } from 'vue'; const n = ref(0); watch(n, (v) => v, { equals: () => true }); watchEffect(() => {});",
    ScriptKind::Setup,
  );
  assert_eq!(mixed.watch_ignored_option.len(), 1, "{mixed:?}");
  let ordinary = assert_gated_matches_forced(
    "import { ref, watch } from 'vue'; const n = ref(0); watch(n.value as number, () => {});",
    ScriptKind::Setup,
  );
  assert_eq!(ordinary.watch_unwrapped_source.len(), 1, "{ordinary:?}");
  let unused_watch = assert_gated_matches_forced(
    "import { watch, watchEffect } from 'vue'; watchEffect(() => {});",
    ScriptKind::Setup,
  );
  assert!(unused_watch.is_empty(), "{unused_watch:?}");
}

#[test]
fn source_contracts_recommended_invalid_fixture_bypasses_indexes() {
  let sfc = include_str!("../../../../fixtures/rules/recommended/invalid.vue");
  let source = script_setup_from_sfc(sfc);
  assert!(
    source.contains("import { ref, watchEffect } from 'vue'"),
    "committed recommended-invalid fixture must import ref/watchEffect: {source}"
  );
  assert_eq!(
    source.matches("watchEffect(").count(),
    3,
    "committed fixture must keep three watchEffect calls: {source}"
  );
  assert_bypass(source, ScriptKind::Setup);
  let (facts, stats) = contract_collect(source, ScriptKind::Setup, false);
  let (forced, forced_stats) = contract_collect(source, ScriptKind::Setup, true);
  assert!(facts.is_empty(), "{facts:?}");
  assert_eq!(facts, forced);
  assert!(stats.is_import_preflight_only(), "gated={stats:?}");
  assert_eq!(stats.owners, 0, "{stats:?}");
  assert!(stats.queries >= 5, "two import-map entries plus three argument candidates: {stats:?}");
  assert!(
    stats.nodes > semantic_node_count(source),
    "ancestor hops must add nodes beyond the import pass: {stats:?}"
  );
  assert!(forced_stats.owners > 0, "{forced_stats:?}");
  assert!(
    stats.work() < forced_stats.work(),
    "bypass work {} must be below forced-full {}",
    stats.work(),
    forced_stats.work()
  );
}

#[test]
fn source_contracts_effect_only_one_arg_work_stays_preflight_while_forced_grows() {
  let mut previous: Option<(u64, u64, u64, u64)> = None;
  for size in [32_u64, 64, 128] {
    let source = effect_only_source("watchEffect", size, None);
    let (facts, stats) = contract_collect(&source, ScriptKind::Setup, false);
    let (forced, forced_stats) = contract_collect(&source, ScriptKind::Setup, true);
    assert!(facts.is_empty(), "{facts:?}");
    assert_eq!(facts, forced);
    assert!(stats.is_import_preflight_only(), "size={size} gated={stats:?}");
    assert_eq!(stats.owners, 0, "bypass must not build owners: {stats:?}");
    assert!(
      stats.queries > size,
      "one import-map entry plus {size} argument candidates: {stats:?}"
    );
    assert!(forced_stats.owners > 0, "size={size} forced={forced_stats:?}");
    assert!(
      stats.work() < forced_stats.work(),
      "size={size} gated {} vs forced {}",
      stats.work(),
      forced_stats.work()
    );
    if let Some((prev_size, prev_gated, prev_forced, prev_queries)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        forced_stats.work() > prev_forced,
        "forced-full work must grow from {prev_forced} to {} on {prev_size}->{size}",
        forced_stats.work()
      );
      assert!(
        stats.work() > prev_gated,
        "gated preflight work must grow from {prev_gated} to {} on {prev_size}->{size}",
        stats.work()
      );
      assert!(
        stats.queries > prev_queries,
        "argument-candidate queries must grow from {prev_queries} to {} on {prev_size}->{size}",
        stats.queries
      );
      assert!(
        stats.work().saturating_mul(10) < prev_gated.saturating_mul(30),
        "gated preflight work grew from {prev_gated} to {} on {prev_size}->{size}",
        stats.work()
      );
    }
    previous = Some((size, stats.work(), forced_stats.work(), stats.queries));
  }
  let emitting = effect_only_source("watchSyncEffect", 16, Some("{ once: true }"));
  let facts = assert_gated_matches_forced(&emitting, ScriptKind::Setup);
  assert_eq!(facts.watch_ignored_option.len(), 16, "{facts:?}");
}

#[test]
fn source_contracts_effect_wrapper_depth_counts_preflight_work() {
  let mut previous: Option<(u64, u64, u64)> = None;
  for depth in [0_u64, 8, 16, 32] {
    let source = effect_wrapper_source(depth);
    let (facts, stats) = contract_collect(&source, ScriptKind::Setup, false);
    let nodes = semantic_node_count(&source);
    assert!(facts.is_empty(), "depth={depth} {facts:?}");
    assert!(stats.is_import_preflight_only(), "depth={depth} {stats:?}");
    assert_eq!(stats.owners, 0, "depth={depth} {stats:?}");
    assert!(
      stats.work() > nodes,
      "depth={depth} counted work {} must include eligibility traversal beyond {nodes} import-pass nodes: {stats:?}",
      stats.work()
    );
    let extra = stats.work().saturating_sub(nodes);
    if let Some((prev_depth, prev_work, prev_extra)) = previous {
      assert!(
        stats.work() > prev_work,
        "wrapper-depth work must grow from {prev_work} to {} on {prev_depth}->{depth}: {stats:?}",
        stats.work()
      );
      assert!(
        extra > prev_extra,
        "wrapper ancestor/peel extra work must grow from {prev_extra} to {extra} on {prev_depth}->{depth}: {stats:?}"
      );
      if prev_depth > 0 {
        assert_eq!(depth, prev_depth * 2, "wrapper depths must double after the empty wrap");
        assert!(
          stats.work().saturating_mul(10) < prev_work.saturating_mul(30),
          "wrapper-depth work grew from {prev_work} to {} on {prev_depth}->{depth}",
          stats.work()
        );
      }
    }
    previous = Some((depth, stats.work(), extra));
  }
}

#[test]
fn source_contracts_effect_candidate_width_counts_preflight_work() {
  let mut previous_calls: Option<(u64, u64, u64)> = None;
  for width in [8_u64, 16, 32] {
    let source = effect_only_source("watchEffect", width, None);
    let (facts, stats) = contract_collect(&source, ScriptKind::Setup, false);
    assert!(facts.is_empty(), "calls={width} {facts:?}");
    assert!(stats.is_import_preflight_only(), "calls={width} {stats:?}");
    assert_eq!(stats.owners, 0, "calls={width} {stats:?}");
    assert!(
      stats.queries > width,
      "one import-map entry plus {width} remaining argument candidates: {stats:?}"
    );
    assert!(stats.references > width, "one specifier plus {width} resolved references: {stats:?}");
    if let Some((prev_width, prev_work, prev_queries)) = previous_calls {
      assert_eq!(width, prev_width * 2, "call widths must double");
      assert!(
        stats.work() > prev_work,
        "call-width work must grow from {prev_work} to {} on {prev_width}->{width}",
        stats.work()
      );
      assert!(
        stats.queries > prev_queries,
        "call-width queries must grow from {prev_queries} to {} on {prev_width}->{width}",
        stats.queries
      );
      assert!(
        stats.work().saturating_mul(10) < prev_work.saturating_mul(30),
        "call-width work grew from {prev_work} to {} on {prev_width}->{width}",
        stats.work()
      );
    }
    previous_calls = Some((width, stats.work(), stats.queries));
  }

  let mut previous_imports: Option<(usize, u64, u64)> = None;
  for width in [2_usize, 4, 8] {
    let source = effect_import_width_source(width);
    let (facts, stats) = contract_collect(&source, ScriptKind::Setup, false);
    assert!(facts.is_empty(), "imports={width} {facts:?}");
    assert!(stats.is_import_preflight_only(), "imports={width} {stats:?}");
    assert_eq!(stats.owners, 0, "imports={width} {stats:?}");
    let width_u64 = u64::try_from(width).unwrap_or(u64::MAX);
    assert!(
      stats.queries > width_u64,
      "{width} import-map entries plus one argument candidate: {stats:?}"
    );
    if let Some((prev_width, prev_work, prev_queries)) = previous_imports {
      assert_eq!(width, prev_width * 2, "import widths must double");
      assert!(
        stats.queries > prev_queries,
        "import-map queries must grow from {prev_queries} to {} on {prev_width}->{width}",
        stats.queries
      );
      assert!(
        stats.work() >= prev_work,
        "import-map work must not shrink from {prev_work} to {} on {prev_width}->{width}",
        stats.work()
      );
      assert!(
        stats.work().saturating_mul(10) < prev_work.saturating_mul(30).max(10),
        "import-map work grew from {prev_work} to {} on {prev_width}->{width}",
        stats.work()
      );
    }
    previous_imports = Some((width, stats.work(), stats.queries));
  }
}

#[test]
fn source_contracts_computed_only_imports_match_forced_full() {
  let source = "import { computed, ref } from 'vue';\
     const items = ref([1, 2]);\
     const doubled = computed(() => items.value.map((n: number) => n * 2));\
     const first = computed(() => doubled.value[0]);\
     void first.value;\
     items.value = [1, 2];\
     void first.value;";
  let facts = assert_gated_matches_forced(source, ScriptKind::Setup);
  assert_eq!(
    facts.stable_computed_identity.len(),
    1,
    "computed-only imports must still collect identity facts: {facts:?}"
  );
}

#[test]
fn source_contracts_stable_computed_identity_positive_and_controls() {
  let positive = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const items = ref([1, 2]);\
     const doubled = computed(() => items.value.map((n: number) => n * 2));\
     watch(doubled, (value) => { void value; });\
     items.value = [1, 2];",
    ScriptKind::Setup,
  );
  assert_eq!(positive.stable_computed_identity.len(), 1, "{positive:?}");

  let previous = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const items = ref([1, 2]);\
     const doubled = computed((previous: number[] | undefined) => {\
       const next = items.value.map((n: number) => n * 2);\
       if (previous) return previous;\
       return next;\
     });\
     watch(doubled, (value) => { void value; });\
     items.value = [1, 2];",
    ScriptKind::Setup,
  );
  assert!(previous.stable_computed_identity.is_empty(), "{previous:?}");

  let primitive = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const count = ref(2);\
     const doubled = computed(() => count.value * 2);\
     watch(doubled, (value) => { void value; });\
     count.value = 2;",
    ScriptKind::Setup,
  );
  assert!(primitive.stable_computed_identity.is_empty(), "{primitive:?}");

  let changed = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const items = ref([1, 2]);\
     const doubled = computed(() => items.value.map((n: number) => n * 2));\
     watch(doubled, (value) => { void value; });\
     items.value = [1, 3];",
    ScriptKind::Setup,
  );
  assert!(changed.stable_computed_identity.is_empty(), "{changed:?}");

  let lazy = assert_gated_matches_forced(
    "import { computed, ref } from 'vue';\
     const items = ref([1, 2]);\
     const doubled = computed(() => items.value.map((n: number) => n * 2));\
     const first = computed(() => doubled.value[0]);\
     void first.value;\
     items.value = [1, 2];",
    ScriptKind::Setup,
  );
  assert!(lazy.stable_computed_identity.is_empty(), "lazy child without later demand: {lazy:?}");

  let queued = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const items = ref([1, 2]);\
     const doubled = computed(() => items.value.map((n: number) => n * 2));\
     const stop = watch(doubled, (value) => { void value; }, { flush: 'pre' });\
     items.value = [1, 2];\
     stop();",
    ScriptKind::Setup,
  );
  assert!(
    queued.stable_computed_identity.is_empty(),
    "queued pre watch stopped after replace: {queued:?}"
  );

  let shadowed = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     function run(undefined: number) {\
       const items = ref([undefined]);\
       const copy = computed(() => items.value.map((n: number) => n));\
       watch(copy, (value) => { void value; }, { flush: 'sync' });\
       items.value = [void 0];\
     }\
     run(1);",
    ScriptKind::Setup,
  );
  assert!(shadowed.stable_computed_identity.is_empty(), "shadowed undefined: {shadowed:?}");

  let strings = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const items = ref(['a']);\
     const selected = computed(() => items.value.filter((n: string) => n < 'm'));\
     watch(selected, (value) => { void value; }, { flush: 'sync' });\
     items.value = ['b'];",
    ScriptKind::Setup,
  );
  assert!(strings.stable_computed_identity.is_empty(), "string relational filter: {strings:?}");

  let prior = assert_gated_matches_forced(
    "import { computed, ref, watch } from 'vue';\
     const items = ref([1]);\
     items.value = [2];\
     const doubled = computed(() => items.value.map((n: number) => n * 2));\
     void doubled.value;\
     watch(doubled, (value) => { void value; }, { flush: 'sync' });\
     items.value = [1];",
    ScriptKind::Setup,
  );
  assert!(prior.stable_computed_identity.is_empty(), "prior replacement baseline: {prior:?}");
}

#[test]
#[expect(clippy::panic, reason = "independent computed fixture construction must fail the test")]
fn source_contracts_many_independent_computeds_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [40_u64, 80, 160] {
    let mut source = String::from("import { computed, ref, watch } from 'vue';");
    for index in 0..size {
      write!(
        source,
        "const items{index} = ref([1, 2]);\
         const doubled{index} = computed(() => items{index}.value.map((n: number) => n * 2));\
         watch(doubled{index}, (value) => {{ void value; }});\
         items{index}.value = [1, 2];"
      )
      .unwrap_or_else(|error| panic!("independent computed fixture write: {error}"));
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.stable_computed_identity.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "size {size}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "independent computed work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
#[expect(clippy::panic, reason = "shared-source computed fixture construction must fail the test")]
fn source_contracts_shared_source_many_consumers_and_wide_projections_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [20_u64, 40, 80] {
    let mut source = String::from(
      "import { computed, ref, watch } from 'vue'; const items = ref([1, 2, 3, 4, 5, 6, 7, 8]);",
    );
    for index in 0..size {
      write!(
        source,
        "const doubled{index} = computed(() => items.value.map((n: number) => n * 2).filter((n: number) => n > 0).slice(0, 8).concat([9]));\
         watch(doubled{index}, (value) => {{ void value; }});"
      )
      .unwrap_or_else(|error| panic!("shared-source computed fixture write: {error}"));
    }
    source.push_str("items.value = [1, 2, 3, 4, 5, 6, 7, 8];");
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.stable_computed_identity.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "size {size} facts {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-source work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_projection_width_grows_inner_work() {
  let mut previous: Option<(usize, u64)> = None;
  for width in [8_usize, 16, 32] {
    let mut values = Vec::with_capacity(width);
    for index in 0..width {
      values.push(index.to_string());
    }
    let literal = values.join(", ");
    let source = format!(
      "import {{ computed, ref, watch }} from 'vue';\
       const items = ref([{literal}]);\
       const doubled = computed(() => items.value.map((n: number) => n * 2));\
       watch(doubled, (value) => {{ void value; }}, {{ flush: 'sync' }});\
       items.value = [{literal}];"
    );
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.stable_computed_identity.len(), 1, "width {width} {contracts:?}");
    if let Some((prev_width, prev_work)) = previous {
      assert_eq!(width, prev_width * 2, "widths must double");
      assert!(
        work > prev_work,
        "projection width work must grow: {prev_work} -> {work} on {prev_width}->{width}"
      );
    }
    previous = Some((width, work));
  }
}

#[test]
#[expect(clippy::panic, reason = "joint-shape fixture construction must fail the test")]
fn source_contracts_joint_producer_consumer_source_shapes_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [4_u64, 8, 16] {
    let mut source =
      String::from("import { computed, ref, watch } from 'vue'; const shared = ref([1, 2]);");
    for index in 0..size {
      write!(
        source,
        "const items{index} = ref([1, 2]);\
         const doubled{index} = computed(() => items{index}.value.map((n: number) => n * 2));\
         const shared{index} = computed(() => shared.value.map((n: number) => n * 2));\
         watch(doubled{index}, (value) => {{ void value; }}, {{ flush: 'sync' }});\
         watch(shared{index}, (value) => {{ void value; }}, {{ flush: 'sync' }});\
         items{index}.value = [1, 2];"
      )
      .unwrap_or_else(|error| panic!("joint shape write: {error}"));
    }
    source.push_str("shared.value = [1, 2];");
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size.saturating_mul(2)).unwrap_or(usize::MAX);
    assert_eq!(
      contracts.stable_computed_identity.len(),
      expected,
      "size {size} facts {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "joint shape work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
#[expect(clippy::panic, reason = "missing span evidence must fail the regression")]
fn source_contracts_define_model_literal_default_and_undefined_ref() {
  let facts = analyze(
    "import { ref, onMounted } from 'vue'\nconst model = defineModel({ default: 1 })\nconst value = ref()\nonMounted(() => { value.value.toFixed(2) })\ndefineExpose({ model })\n",
    "ts",
  );
  assert_eq!(facts.source_contracts.model_defaults.len(), 1, "{:?}", facts.source_contracts);
  let Some(default) = facts.source_contracts.model_defaults.first() else {
    panic!("default missing: {:?}", facts.source_contracts);
  };
  assert_eq!(default.model_name, "modelValue");
  assert_eq!(default.origin, vue_vet_core::ModelDefaultOrigin::LiteralPrimitive);
  assert_eq!(default.primitive, Some(vue_vet_core::ModelPrimitiveKind::Number));
  assert!(
    facts
      .source_contracts
      .ordinary_ref_inits
      .iter()
      .any(|init| { init.binding == "value" && init.kind == vue_vet_core::RefInitKind::Undefined }),
    "{:?}",
    facts.source_contracts.ordinary_ref_inits
  );
  assert_eq!(
    facts.source_contracts.mounted_member_demands.len(),
    1,
    "{:?}",
    facts.source_contracts.mounted_member_demands
  );
  assert_eq!(
    facts.source_contracts.mounted_member_demands.first().map(|demand| demand.member.as_str()),
    Some("toFixed")
  );
  assert!(
    facts
      .source_contracts
      .define_expose
      .iter()
      .any(|expose| expose.names.contains(&"model".into()))
  );
}

#[test]
fn source_contracts_shared_factory_and_fresh_factory_and_instance_chains() {
  let shared = analyze(
    "const shared = { n: 1 }\nconst model = defineModel({ default: () => shared })\ndefineExpose({ model })\n",
    "ts",
  );
  assert!(
    shared.source_contracts.model_defaults.iter().any(|model| {
      model.origin == vue_vet_core::ModelDefaultOrigin::SharedObjectFactory
        && model.shared_binding.as_deref() == Some("shared")
    }),
    "{:?}",
    shared.source_contracts.model_defaults
  );
  assert!(
    shared.source_contracts.shared_object_bindings.iter().any(|binding| {
      binding.binding == "shared"
        && binding
          .own_paths
          .iter()
          .any(|(path, kind)| path == "n" && *kind == vue_vet_core::ModelPrimitiveKind::Number)
    }),
    "{:?}",
    shared.source_contracts.shared_object_bindings
  );
  let fresh = analyze("const model = defineModel({ default: () => ({ n: 1 }) })\n", "ts");
  assert!(
    fresh
      .source_contracts
      .model_defaults
      .iter()
      .any(|model| { model.origin == vue_vet_core::ModelDefaultOrigin::FreshObjectFactory }),
    "{:?}",
    fresh.source_contracts.model_defaults
  );
  let literal = analyze("const model = defineModel({ default: { n: 1 } })\n", "ts");
  assert!(
    literal.source_contracts.model_defaults.iter().any(|model| {
      model.origin == vue_vet_core::ModelDefaultOrigin::SharedObjectLiteral
        && model
          .own_paths
          .iter()
          .any(|(path, kind)| path == "n" && *kind == vue_vet_core::ModelPrimitiveKind::Number)
    }),
    "{:?}",
    literal.source_contracts.model_defaults
  );
  let array = analyze("const model = defineModel({ default: [] })\n", "ts");
  assert!(
    array
      .source_contracts
      .model_defaults
      .iter()
      .any(|model| model.origin == vue_vet_core::ModelDefaultOrigin::SharedObjectLiteral),
    "{:?}",
    array.source_contracts.model_defaults
  );
  let chain = analyze(
    "import { ref, onMounted } from 'vue'\nconst left = ref(null)\nconst right = ref(null)\nonMounted(() => { left.value.model.n = 'text'; right.value.model.n.toFixed(2) })\n",
    "ts",
  );
  assert!(
    chain.source_contracts.instance_path_writes.iter().any(|write| {
      write.instance == "left"
        && write.path == ["model", "n"]
        && write.rhs_kind == vue_vet_core::RefInitKind::String
    }),
    "{:?}",
    chain.source_contracts.instance_path_writes
  );
  assert!(
    chain.source_contracts.instance_member_demands.iter().any(|demand| {
      demand.instance == "right"
        && demand.path == ["model", "n"]
        && demand.member == "toFixed"
        && !demand.optional
        && !demand.guarded
    }),
    "{:?}",
    chain.source_contracts.instance_member_demands
  );
}

#[test]
#[expect(clippy::panic, reason = "missing span evidence must fail the regression")]
fn source_contracts_model_demand_unicode_and_crlf_span_the_call() {
  let unicode = "import { ref, onMounted } from 'vue'\nconst 值 = ref()\nonMounted(() => { 值.value.toFixed(2) })\n";
  let facts = analyze(unicode, "ts");
  let needle = "值.value.toFixed(2)";
  let Some(offset) = unicode.find(needle) else {
    panic!("unicode demand missing");
  };
  let Some(demand) = facts.source_contracts.mounted_member_demands.first() else {
    panic!("unicode demand fact missing: {:?}", facts.source_contracts);
  };
  assert_eq!(demand.span.offset, offset);
  assert_eq!(demand.span.length, needle.len());

  let crlf = "import { ref, onMounted } from 'vue';\r\nconst value = ref();\r\nonMounted(() => { value.value.toFixed(2) })\r\n";
  let facts = analyze(crlf, "ts");
  let needle = "value.value.toFixed(2)";
  let Some(offset) = crlf.find(needle) else {
    panic!("crlf demand missing");
  };
  let Some(demand) = facts.source_contracts.mounted_member_demands.first() else {
    panic!("crlf demand fact missing: {:?}", facts.source_contracts);
  };
  assert_eq!(demand.span.offset, offset);
  assert_eq!(demand.span.length, needle.len());
}

#[test]
fn source_contracts_model_facts_scale_with_combined_fanout() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [8_u64, 16, 32] {
    let mut source = String::from("import { ref, onMounted } from 'vue'\n");
    for index in 0..size {
      source.push_str("const s");
      source.push_str(&index.to_string());
      source.push_str(" = { n: 1 }\nconst m");
      source.push_str(&index.to_string());
      source.push_str(" = defineModel({ default: () => s");
      source.push_str(&index.to_string());
      source.push_str(" })\nconst r");
      source.push_str(&index.to_string());
      source.push_str(" = ref()\n");
    }
    source.push_str("onMounted(() => {\n");
    for index in 0..size {
      source.push('r');
      source.push_str(&index.to_string());
      source.push_str(".value.toFixed(2)");
      source.push('\n');
    }
    source.push_str("})\n");
    let (contracts, work) = contract_stats(&source);
    let count = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.model_defaults.len(), count, "size {size} defaults; {contracts:?}");
    assert_eq!(contracts.mounted_member_demands.len(), count, "size {size} demands; {contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(work > 0, "model-fact collection must count work");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "model-fact work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn source_contracts_model_write_compares_literal_value_on_model_binding() {
  let same = analyze(
    "const model = defineModel({ default: 1 })\nconst label = ref('a')\nmodel.value = 1\nlabel.value = 'b'\n",
    "ts",
  );
  assert!(
    same.source_contracts.model_value_writes.iter().any(|write| {
      write.binding == "model" && write.unchanged_default && write.rhs_text.as_deref() == Some("1")
    }),
    "{:?}",
    same.source_contracts.model_value_writes
  );
  assert!(
    same.source_contracts.model_value_writes.iter().all(|write| write.binding != "label"),
    "unrelated ref writes must not be model writes: {:?}",
    same.source_contracts.model_value_writes
  );
  let changed = analyze("const model = defineModel({ default: 1 })\nmodel.value = 2\n", "ts");
  assert!(
    changed
      .source_contracts
      .model_value_writes
      .iter()
      .any(|write| write.binding == "model" && !write.unchanged_default),
    "{:?}",
    changed.source_contracts.model_value_writes
  );
}

#[test]
fn source_contracts_early_return_marks_mounted_demand_guarded() {
  let facts = analyze(
    "import { ref, onMounted } from 'vue'\nconst value = ref()\nonMounted(() => { if (value.value === undefined) return; value.value.toFixed(2) })\n",
    "ts",
  );
  assert!(
    facts.source_contracts.mounted_member_demands.iter().any(|demand| demand.guarded),
    "{:?}",
    facts.source_contracts.mounted_member_demands
  );
}

#[test]
fn source_contracts_ordinary_script_records_shared_object_without_model_surface() {
  let (facts, _) =
    contract_collect("const shared = { n: 1 }\n", vue_vet_core::ScriptKind::Script, true);
  assert!(
    facts.shared_object_bindings.iter().any(|binding| {
      binding.binding == "shared"
        && binding
          .own_paths
          .iter()
          .any(|(path, kind)| path == "n" && *kind == vue_vet_core::ModelPrimitiveKind::Number)
    }),
    "{:?}",
    facts.shared_object_bindings
  );
}

#[test]
fn source_contracts_model_preflight_equals_forced_full_without_surface() {
  let source = "const n = 1\n";
  let (relative, _) = contract_collect(source, vue_vet_core::ScriptKind::Setup, false);
  let (full, _) = contract_collect(source, vue_vet_core::ScriptKind::Setup, true);
  assert_eq!(relative.model_defaults, full.model_defaults);
  assert_eq!(relative.mounted_member_demands, full.mounted_member_demands);
}
