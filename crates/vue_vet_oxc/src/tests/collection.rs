use super::support::*;

#[test]
fn collection_lookup_emits_raw_proxy_map_key_and_keyed_foreach() {
  let (mismatch, _) = contract_stats(
    "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count;",
  );
  assert_eq!(mismatch.raw_proxy_map_key.len(), 1, "{mismatch:?}");
  let (same, _) = contract_stats(
    "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(raw).count;",
  );
  assert!(same.raw_proxy_map_key.is_empty(), "{same:?}");
  let (normalized, _) = contract_stats(
    "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = reactive(new Map([[raw, { count: 1 }]])); void map.get(proxy).count;",
  );
  assert!(normalized.raw_proxy_map_key.is_empty(), "{normalized:?}");
  let (keyed, _) = contract_stats(
    "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 1], ['other', 2]])); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;",
  );
  assert_eq!(keyed.keyed_map_dependency.len(), 1, "{keyed:?}");
  let (get, _) = contract_stats(
    "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 1]])); const selected = computed(() => keyed.get('selected')); void selected;",
  );
  assert!(get.keyed_map_dependency.is_empty(), "{get:?}");
}

#[test]
#[expect(clippy::panic, reason = "missing TypeScript demand span must fail the regression")]
fn collection_lookup_ts_unicode_crlf_and_unknown_controls() {
  let ts = include_str!("../../../../fixtures/rules/no-raw-proxy-map-key/invalid/basic.ts");
  let (facts, _) = contract_stats(ts);
  assert_eq!(facts.raw_proxy_map_key.len(), 1, "{facts:?}");
  let Some(site) = facts.raw_proxy_map_key.first() else {
    panic!("ts demand");
  };
  let ts_demand = "map.get(proxy).count";
  let Some(ts_offset) = ts.find(ts_demand) else {
    panic!("ts demand text");
  };
  assert_eq!(site.demand_span.offset, ts_offset, "{site:?}");
  assert_eq!(site.demand_span.length, ts_demand.len(), "{site:?}");
  assert_eq!(ts.get(ts_offset..ts_offset + ts_demand.len()), Some(ts_demand));
  let crlf = "import { reactive } from 'vue';\r\nconst raw = {};\r\nconst proxy = reactive(raw);\r\nconst map = new Map([[raw, { count: 1 }]]);\r\nvoid map.get(proxy).count;\r\n";
  assert!(crlf.contains("\r\n"), "fixture must be true CRLF");
  let (crlf_facts, _) = contract_stats(crlf);
  assert_eq!(crlf_facts.raw_proxy_map_key.len(), 1, "{crlf_facts:?}");
  let Some(crlf_site) = crlf_facts.raw_proxy_map_key.first() else {
    panic!("crlf demand");
  };
  let crlf_demand = "map.get(proxy).count";
  let Some(crlf_offset) = crlf.find(crlf_demand) else {
    panic!("crlf demand text");
  };
  assert_eq!(crlf_site.demand_span.offset, crlf_offset, "{crlf_site:?}");
  assert_eq!(crlf_site.demand_span.length, crlf_demand.len(), "{crlf_site:?}");
  assert_eq!(crlf_site.demand_span.line, 5, "{crlf_site:?}");
  assert_eq!(
    crlf.as_bytes().get(crlf_offset..crlf_offset + crlf_demand.len()),
    Some(crlf_demand.as_bytes())
  );
  let crlf_bytes =
    include_bytes!("../../../../fixtures/rules/no-raw-proxy-map-key/invalid/crlf.vue");
  assert!(
    crlf_bytes.windows(2).any(|window| window == b"\r\n"),
    "committed crlf.vue must keep CR bytes"
  );
  let unicode = "import { reactive } from 'vue'; const 原 = {}; const 代理 = reactive(原); const map = new Map([[原, { count: 1 }]]); void map.get(代理).count;";
  let (unicode_facts, _) = contract_stats(unicode);
  assert_eq!(unicode_facts.raw_proxy_map_key.len(), 1, "{unicode_facts:?}");
  let Some(unicode_site) = unicode_facts.raw_proxy_map_key.first() else {
    panic!("unicode demand");
  };
  let unicode_demand = "map.get(代理).count";
  let Some(unicode_offset) = unicode.find(unicode_demand) else {
    panic!("unicode demand text");
  };
  assert_eq!(unicode_site.demand_span.offset, unicode_offset, "{unicode_site:?}");
  assert_eq!(unicode_site.demand_span.length, unicode_demand.len(), "{unicode_site:?}");
  let demi = "import { reactive } from 'vue-demi'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count;";
  let (demi_facts, _) = contract_stats(demi);
  assert!(demi_facts.raw_proxy_map_key.is_empty(), "{demi_facts:?}");
  let auto = "import { reactive } from '#imports'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count;";
  let (auto_facts, _) = contract_stats(auto);
  assert!(auto_facts.raw_proxy_map_key.is_empty(), "{auto_facts:?}");
  let guarded = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy)?.count;";
  let (guarded_facts, _) = contract_stats(guarded);
  assert!(guarded_facts.raw_proxy_map_key.is_empty(), "{guarded_facts:?}");
}

#[test]
#[expect(clippy::panic, reason = "missing review-probe spans must fail the regression")]
fn collection_lookup_review_probes_stay_on_runtime_evidence() {
  let source5 = "import { reactive, watch } from 'vue'; const state = reactive({ child: 1 }); const helper = { get(target) { target.child = () => 7; } }; helper.get(state); watch(state.child, () => {});";
  let (source5_facts, _) = contract_stats(source5);
  assert!(
    source5_facts.watch_unwrapped_source.is_empty(),
    "helper.get first-arg spelling must not grant a source5 exemption: {source5_facts:?}"
  );
  let skip = "import { reactive } from 'vue'; const raw = { __v_skip: true }; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (skip_facts, _) = contract_stats(skip);
  assert!(skip_facts.raw_proxy_map_key.is_empty(), "{skip_facts:?}");
  let frozen = "import { reactive } from 'vue'; const raw = {}; Object.preventExtensions(raw); const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (frozen_facts, _) = contract_stats(frozen);
  assert!(frozen_facts.raw_proxy_map_key.is_empty(), "{frozen_facts:?}");
  let unknown = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const extra = ((value) => value)(proxy); const map = new Map([[raw, { count: 1 }], [extra, { count: 7 }]]); void map.get(proxy).count;";
  let (unknown_facts, _) = contract_stats(unknown);
  assert!(unknown_facts.raw_proxy_map_key.is_empty(), "{unknown_facts:?}");
  let dead = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); false && map.delete(proxy); void map.get(proxy).count;";
  let (dead_facts, _) = contract_stats(dead);
  assert!(dead_facts.raw_proxy_map_key.is_empty(), "{dead_facts:?}");
  let late = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); function read() { return map.get(proxy).count; } map.set(proxy, { count: 7 }); void read();";
  let (late_facts, _) = contract_stats(late);
  assert!(late_facts.raw_proxy_map_key.is_empty(), "{late_facts:?}");
  let property = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const answer = { undefined: 7 }; void answer[map.get(proxy)];";
  let (property_facts, _) = contract_stats(property);
  assert!(property_facts.raw_proxy_map_key.is_empty(), "{property_facts:?}");
  let proto = "import { reactive, computed } from 'vue'; Map.prototype.get = function get(key) { return key; }; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count; const keyed = reactive(new Map([['selected', 1]])); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (proto_facts, _) = contract_stats(proto);
  assert!(proto_facts.raw_proxy_map_key.is_empty(), "{proto_facts:?}");
  assert!(proto_facts.keyed_map_dependency.is_empty(), "{proto_facts:?}");
  let global = "import { computed, reactive } from 'vue'; globalThis.Map = class extends Map {}; const keyed = reactive(new Map([['selected', 1]])); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (global_facts, _) = contract_stats(global);
  assert!(global_facts.keyed_map_dependency.is_empty(), "{global_facts:?}");
  let generator = "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 7]])); const selected = computed(() => { let value; keyed.forEach(function* (entry, key) { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (generator_facts, _) = contract_stats(generator);
  assert!(generator_facts.keyed_map_dependency.is_empty(), "{generator_facts:?}");
  let deferred = "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 7]])); const selected = computed(() => { let value; queueMicrotask(() => keyed.forEach((entry, key) => { if (key === 'selected') value = entry })); return value; }); void selected;";
  let (deferred_facts, _) = contract_stats(deferred);
  assert!(deferred_facts.keyed_map_dependency.is_empty(), "{deferred_facts:?}");
  let wrappers = "import { reactive, shallowReactive } from 'vue'; const raw = {}; const first = reactive(raw); const second = shallowReactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(first).count;";
  let (wrapper_facts, _) = contract_stats(wrappers);
  assert_eq!(wrapper_facts.raw_proxy_map_key.len(), 1, "{wrapper_facts:?}");
  let Some(wrapper_site) = wrapper_facts.raw_proxy_map_key.first() else {
    return;
  };
  let first_wrapper = "reactive(raw)";
  let Some(wrapper_offset) = wrappers.find(first_wrapper) else {
    panic!("first wrapper");
  };
  assert_eq!(wrapper_site.wrapper_span.offset, wrapper_offset, "{wrapper_site:?}");
  assert_eq!(wrapper_site.wrapper_span.length, first_wrapper.len(), "{wrapper_site:?}");
}

#[test]
fn collection_lookup_growth_stays_subquadratic_and_wide_negatives_stay_quiet() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { computed, reactive } from 'vue';");
    for index in 0..size {
      source.push_str("const raw");
      source.push_str(&index.to_string());
      source.push_str(" = {}; const proxy");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(raw");
      source.push_str(&index.to_string());
      source.push_str("); const map");
      source.push_str(&index.to_string());
      source.push_str(" = new Map([[raw");
      source.push_str(&index.to_string());
      source.push_str(", { count: 1 }]]); void map");
      source.push_str(&index.to_string());
      source.push_str(".get(proxy");
      source.push_str(&index.to_string());
      source.push_str(").count;");
      source.push_str("const keyed");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(new Map([['selected', 1], ['other', ");
      source.push_str(&index.to_string());
      source.push_str("]])); const selected");
      source.push_str(&index.to_string());
      source.push_str(" = computed(() => { let value; keyed");
      source.push_str(&index.to_string());
      source.push_str(".forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected");
      source.push_str(&index.to_string());
      source.push(';');
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.raw_proxy_map_key.len(), expected, "{contracts:?}");
    assert_eq!(contracts.keyed_map_dependency.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "collection-lookup work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
  let mut negative = String::from("import { computed, reactive } from 'vue';");
  for index in 0..64 {
    negative.push_str("const n");
    negative.push_str(&index.to_string());
    negative.push_str(" = reactive(new Map([['k', ");
    negative.push_str(&index.to_string());
    negative.push_str("]])); void n");
    negative.push_str(&index.to_string());
    negative.push_str(".get('k'); const c");
    negative.push_str(&index.to_string());
    negative.push_str(" = computed(() => n");
    negative.push_str(&index.to_string());
    negative.push_str(".get('k')); void c");
    negative.push_str(&index.to_string());
    negative.push(';');
  }
  let (quiet, _) = contract_stats(&negative);
  assert!(quiet.raw_proxy_map_key.is_empty(), "{quiet:?}");
  assert!(quiet.keyed_map_dependency.is_empty(), "{quiet:?}");
  let nested = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(reactive(reactive(reactive(reactive(reactive(reactive(reactive(reactive(raw))))))))); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count;";
  let (exhausted, _) = contract_stats(nested);
  assert!(
    exhausted.raw_proxy_map_key.is_empty(),
    "MAX_DEPTH exhaustion must stay unknown: {exhausted:?}"
  );
}

#[test]
fn collection_lookup_growth_shared_root_wide_initializer_and_negatives() {
  let mut previous_shared: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]);",
    );
    for index in 0..size {
      source.push_str("void map.get(proxy).count; const _r");
      source.push_str(&index.to_string());
      source.push_str(" = 0;");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(
      contracts.raw_proxy_map_key.len(),
      expected,
      "shared-root reads {size}: {contracts:?}"
    );
    assert!(work > 0, "shared-root work must count inner visits: {work}");
    if let Some((prev_size, prev_work)) = previous_shared {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-root work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous_shared = Some((size, work));
  }
  let mut previous_wide: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([",
    );
    for index in 0..size {
      source.push_str("['k");
      source.push_str(&index.to_string());
      source.push_str("', ");
      source.push_str(&index.to_string());
      source.push_str("], ");
    }
    source.push_str("[raw, { count: 1 }]]); void map.get(proxy).count;");
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.raw_proxy_map_key.len(), 1, "wide initializer {size}: {contracts:?}");
    assert!(work > 0, "wide initializer work must count entries: {work}");
    if let Some((prev_size, prev_work)) = previous_wide {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "wide initializer work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous_wide = Some((size, work));
  }
  let mut negative = String::from("import { computed, reactive } from 'vue';");
  for index in 0..64 {
    negative.push_str("const raw");
    negative.push_str(&index.to_string());
    negative.push_str(" = { __v_skip: true }; const proxy");
    negative.push_str(&index.to_string());
    negative.push_str(" = reactive(raw");
    negative.push_str(&index.to_string());
    negative.push_str("); const map");
    negative.push_str(&index.to_string());
    negative.push_str(" = new Map([[raw");
    negative.push_str(&index.to_string());
    negative.push_str(", { count: 1 }]]); void map");
    negative.push_str(&index.to_string());
    negative.push_str(".get(proxy");
    negative.push_str(&index.to_string());
    negative.push_str(").count;");
  }
  let (quiet, work) = contract_stats(&negative);
  assert!(quiet.raw_proxy_map_key.is_empty(), "{quiet:?}");
  assert!(work > 0, "negative skip-marker work must still count: {work}");
}

#[test]
fn collection_lookup_second_review_boundaries_stay_quiet_or_report() {
  let helper = "import { reactive } from 'vue'; const helper = { get(target) { Object.preventExtensions(target) } }; const raw = {}; helper.get(raw); const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (helper_facts, _) = contract_stats(helper);
  assert!(helper_facts.raw_proxy_map_key.is_empty(), "{helper_facts:?}");
  let alias_helper = "import { reactive } from 'vue'; const helper = { get(target) { Object.preventExtensions(target) } }; const api = helper; const raw = {}; api.get(raw); const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (alias_helper_facts, _) = contract_stats(alias_helper);
  assert!(alias_helper_facts.raw_proxy_map_key.is_empty(), "{alias_helper_facts:?}");
  let native_key = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map(); map.set(raw, { count: 1 }); void map.get(proxy).count;";
  let (native_key_facts, _) = contract_stats(native_key);
  assert_eq!(native_key_facts.raw_proxy_map_key.len(), 1, "{native_key_facts:?}");
  let repeated = "import { reactive } from 'vue'; const raw = {}; const first = reactive(raw); const second = reactive(raw); const map = new Map([[raw, { count: 1 }], [first, { count: 7 }]]); void map.get(second).count;";
  let (repeated_facts, _) = contract_stats(repeated);
  assert!(repeated_facts.raw_proxy_map_key.is_empty(), "{repeated_facts:?}");
  let repeated_only = "import { reactive } from 'vue'; const raw = {}; const first = reactive(raw); const second = reactive(raw); const map = new Map([[first, { count: 7 }]]); void map.get(second).count;";
  let (repeated_only_facts, _) = contract_stats(repeated_only);
  assert!(repeated_only_facts.raw_proxy_map_key.is_empty(), "{repeated_only_facts:?}");
  let repeated_shallow = "import { shallowReactive } from 'vue'; const raw = {}; const first = shallowReactive(raw); const second = shallowReactive(raw); const map = new Map([[first, { count: 7 }]]); void map.get(second).count;";
  let (repeated_shallow_facts, _) = contract_stats(repeated_shallow);
  assert!(repeated_shallow_facts.raw_proxy_map_key.is_empty(), "{repeated_shallow_facts:?}");
  let alias_wrap = "import { reactive } from 'vue'; const raw = {}; const first = reactive(raw); const alias = first; const map = new Map([[raw, { count: 1 }], [first, { count: 7 }]]); void map.get(alias).count;";
  let (alias_wrap_facts, _) = contract_stats(alias_wrap);
  assert!(alias_wrap_facts.raw_proxy_map_key.is_empty(), "{alias_wrap_facts:?}");
  let inline_same = "import { reactive } from 'vue'; const raw = {}; const first = reactive(raw); const map = new Map([[first, { count: 7 }]]); void map.get(reactive(raw)).count;";
  let (inline_same_facts, _) = contract_stats(inline_same);
  assert!(inline_same_facts.raw_proxy_map_key.is_empty(), "{inline_same_facts:?}");
  let distinct = "import { reactive, shallowReactive } from 'vue'; const raw = {}; const deep = reactive(raw); const shallow = shallowReactive(raw); const map = new Map([[raw, { count: 1 }], [deep, { count: 7 }]]); void map.get(shallow).count;";
  let (distinct_facts, _) = contract_stats(distinct);
  assert_eq!(distinct_facts.raw_proxy_map_key.len(), 1, "{distinct_facts:?}");
  let wrappers = "import { reactive, shallowReactive } from 'vue'; const raw = {}; const first = reactive(raw); const second = shallowReactive(raw); const map = new Map([[raw, { count: 1 }]]); void second; void map.get(first).count;";
  let (wrapper_facts, _) = contract_stats(wrappers);
  assert_eq!(wrapper_facts.raw_proxy_map_key.len(), 1, "{wrapper_facts:?}");
  let zero_delete = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); 0 ?? map.delete(proxy); void map.get(proxy).count;";
  let (zero_delete_facts, _) = contract_stats(zero_delete);
  assert!(zero_delete_facts.raw_proxy_map_key.is_empty(), "{zero_delete_facts:?}");
  let false_delete = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); false ?? map.delete(proxy); void map.get(proxy).count;";
  let (false_delete_facts, _) = contract_stats(false_delete);
  assert!(false_delete_facts.raw_proxy_map_key.is_empty(), "{false_delete_facts:?}");
  let empty_delete = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); '' ?? map.delete(proxy); void map.get(proxy).count;";
  let (empty_delete_facts, _) = contract_stats(empty_delete);
  assert!(empty_delete_facts.raw_proxy_map_key.is_empty(), "{empty_delete_facts:?}");
  let null_delete = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); null ?? map.delete(proxy); void map.get(proxy).count;";
  let (null_delete_facts, _) = contract_stats(null_delete);
  assert_eq!(null_delete_facts.raw_proxy_map_key.len(), 1, "{null_delete_facts:?}");
  let undef_delete = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); undefined ?? map.delete(proxy); void map.get(proxy).count;";
  let (undef_delete_facts, _) = contract_stats(undef_delete);
  assert_eq!(undef_delete_facts.raw_proxy_map_key.len(), 1, "{undef_delete_facts:?}");
  let shadowed = "import { reactive } from 'vue'; const undefined = 0; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); undefined ?? map.delete(proxy); void map.get(proxy).count;";
  let (shadowed_facts, _) = contract_stats(shadowed);
  assert!(shadowed_facts.raw_proxy_map_key.is_empty(), "{shadowed_facts:?}");
  let unknown_delete = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); maybe ?? map.delete(proxy); void map.get(proxy).count;";
  let (unknown_delete_facts, _) = contract_stats(unknown_delete);
  assert!(unknown_delete_facts.raw_proxy_map_key.is_empty(), "{unknown_delete_facts:?}");
  let zero_get = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void (0 ?? map.get(proxy).count);";
  let (zero_get_facts, _) = contract_stats(zero_get);
  assert!(zero_get_facts.raw_proxy_map_key.is_empty(), "{zero_get_facts:?}");
  let false_and = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); false && map.delete(proxy); void map.get(proxy).count;";
  let (false_and_facts, _) = contract_stats(false_and);
  assert!(false_and_facts.raw_proxy_map_key.is_empty(), "{false_and_facts:?}");
  let own_get = "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 7]])); keyed.get = () => 9; const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (own_get_facts, _) = contract_stats(own_get);
  assert!(own_get_facts.keyed_map_dependency.is_empty(), "{own_get_facts:?}");
  let own_foreach = "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 7]])); keyed.forEach = () => {}; const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (own_foreach_facts, _) = contract_stats(own_foreach);
  assert!(own_foreach_facts.keyed_map_dependency.is_empty(), "{own_foreach_facts:?}");
  let alias_override = "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 7]])); const alias = keyed; alias.get = () => 9; const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (alias_override_facts, _) = contract_stats(alias_override);
  assert!(alias_override_facts.keyed_map_dependency.is_empty(), "{alias_override_facts:?}");
  let helper_escape = "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 7]])); const touch = (value) => value; touch(keyed); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (helper_escape_facts, _) = contract_stats(helper_escape);
  assert!(helper_escape_facts.keyed_map_dependency.is_empty(), "{helper_escape_facts:?}");
  let wrapped_ident = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (wrapped_ident_facts, _) = contract_stats(wrapped_ident);
  assert_eq!(wrapped_ident_facts.keyed_map_dependency.len(), 1, "{wrapped_ident_facts:?}");
  let ordinary_set = "import { computed, reactive } from 'vue'; const keyed = reactive(new Map([['selected', 1], ['other', 2]])); keyed.set('other', 3); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (ordinary_set_facts, _) = contract_stats(ordinary_set);
  assert_eq!(ordinary_set_facts.keyed_map_dependency.len(), 1, "{ordinary_set_facts:?}");
  let early_return = "import { reactive } from 'vue'; function read() { const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); return 7; void result.count; } read();";
  let (early_return_facts, _) = contract_stats(early_return);
  assert!(early_return_facts.raw_proxy_map_key.is_empty(), "{early_return_facts:?}");
  let early_throw = "import { reactive } from 'vue'; function read() { const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); throw 7; void result.count; }";
  let (early_throw_facts, _) = contract_stats(early_throw);
  assert!(early_throw_facts.raw_proxy_map_key.is_empty(), "{early_throw_facts:?}");
  let guarded_return = "import { reactive } from 'vue'; function read(flag: boolean) { const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); if (flag) return 7; void result.count; } read(true);";
  let (guarded_return_facts, _) = contract_stats(guarded_return);
  assert!(guarded_return_facts.raw_proxy_map_key.is_empty(), "{guarded_return_facts:?}");
  let chained_after = "import { reactive } from 'vue'; function read() { const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); return 7; void map.get(proxy).count; } read();";
  let (chained_after_facts, _) = contract_stats(chained_after);
  assert!(chained_after_facts.raw_proxy_map_key.is_empty(), "{chained_after_facts:?}");
  let await_demand = "import { reactive } from 'vue'; async function read() { const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); await Promise.resolve(); void result.count; }";
  let (await_demand_facts, _) = contract_stats(await_demand);
  assert!(await_demand_facts.raw_proxy_map_key.is_empty(), "{await_demand_facts:?}");
  let reachable = "import { reactive } from 'vue'; function read() { const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); void result.count; return 7; } read();";
  let (reachable_facts, _) = contract_stats(reachable);
  assert_eq!(reachable_facts.raw_proxy_map_key.len(), 1, "{reachable_facts:?}");
}

#[test]
fn collection_lookup_third_review_boundaries_stay_quiet() {
  let native_override = "import { reactive } from 'vue'; const helper = new Map(); helper.get = target => Object.preventExtensions(target); const raw = {}; helper.get(raw); const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (native_override_facts, _) = contract_stats(native_override);
  assert!(native_override_facts.raw_proxy_map_key.is_empty(), "{native_override_facts:?}");
  let later_override = "import { reactive } from 'vue'; const helper = new Map(); const raw = {}; helper.get(raw); helper.get = target => Object.preventExtensions(target); const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (later_override_facts, _) = contract_stats(later_override);
  assert!(later_override_facts.raw_proxy_map_key.is_empty(), "{later_override_facts:?}");
  let alias_override = "import { reactive } from 'vue'; const helper = new Map(); const api = helper; api.get = target => Object.preventExtensions(target); const raw = {}; helper.get(raw); const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (alias_override_facts, _) = contract_stats(alias_override);
  assert!(alias_override_facts.raw_proxy_map_key.is_empty(), "{alias_override_facts:?}");
  let wrapped_helper = "import { computed, reactive } from 'vue'; function alter(target) { target.get = () => 9 } const inner = new Map([['selected', 7]]); alter(inner); const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (wrapped_helper_facts, _) = contract_stats(wrapped_helper);
  assert!(wrapped_helper_facts.keyed_map_dependency.is_empty(), "{wrapped_helper_facts:?}");
  let wrapped_computed = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); inner['get'] = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (wrapped_computed_facts, _) = contract_stats(wrapped_computed);
  assert!(wrapped_computed_facts.keyed_map_dependency.is_empty(), "{wrapped_computed_facts:?}");
  let wrapped_ident = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (wrapped_ident_facts, _) = contract_stats(wrapped_ident);
  assert_eq!(wrapped_ident_facts.keyed_map_dependency.len(), 1, "{wrapped_ident_facts:?}");
  let return_before = "import { reactive } from 'vue'; function read() { return 7; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count; } read();";
  let (return_before_facts, _) = contract_stats(return_before);
  assert!(return_before_facts.raw_proxy_map_key.is_empty(), "{return_before_facts:?}");
  let throw_before = "import { reactive } from 'vue'; function read() { throw 7; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count; }";
  let (throw_before_facts, _) = contract_stats(throw_before);
  assert!(throw_before_facts.raw_proxy_map_key.is_empty(), "{throw_before_facts:?}");
  let guarded_before = "import { reactive } from 'vue'; function read(flag: boolean) { if (flag) return 7; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); void map.get(proxy).count; } read(true);";
  let (guarded_before_facts, _) = contract_stats(guarded_before);
  assert!(guarded_before_facts.raw_proxy_map_key.is_empty(), "{guarded_before_facts:?}");
  let skipped_and = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); false && result.count;";
  let (skipped_and_facts, _) = contract_stats(skipped_and);
  assert!(skipped_and_facts.raw_proxy_map_key.is_empty(), "{skipped_and_facts:?}");
  let skipped_or = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); true || result.count;";
  let (skipped_or_facts, _) = contract_stats(skipped_or);
  assert!(skipped_or_facts.raw_proxy_map_key.is_empty(), "{skipped_or_facts:?}");
  let skipped_nullish = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); const result = map.get(proxy); 0 ?? result.count;";
  let (skipped_nullish_facts, _) = contract_stats(skipped_nullish);
  assert!(skipped_nullish_facts.raw_proxy_map_key.is_empty(), "{skipped_nullish_facts:?}");
  let tagged = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]); tag`${map.get(proxy).count}`;";
  let (tagged_facts, _) = contract_stats(tagged);
  assert!(tagged_facts.raw_proxy_map_key.is_empty(), "{tagged_facts:?}");
  let native_set = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map(); map.set(raw, { count: 1 }); void map.get(proxy).count;";
  let (native_set_facts, _) = contract_stats(native_set);
  assert_eq!(native_set_facts.raw_proxy_map_key.len(), 1, "{native_set_facts:?}");
}

#[test]
fn collection_lookup_fourth_review_boundaries_stay_quiet_or_report() {
  let and_alias = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); let chosen = false; chosen &&= result.count;";
  let (and_alias_facts, _) = contract_stats(and_alias);
  assert!(and_alias_facts.raw_proxy_map_key.is_empty(), "{and_alias_facts:?}");
  let or_alias = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); let chosen = true; chosen ||= result.count;";
  let (or_alias_facts, _) = contract_stats(or_alias);
  assert!(or_alias_facts.raw_proxy_map_key.is_empty(), "{or_alias_facts:?}");
  let nullish_alias = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); let chosen = 0; chosen ??= result.count;";
  let (nullish_alias_facts, _) = contract_stats(nullish_alias);
  assert!(nullish_alias_facts.raw_proxy_map_key.is_empty(), "{nullish_alias_facts:?}");
  let and_direct = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); let chosen = false; chosen &&= map.get(proxy).count;";
  let (and_direct_facts, _) = contract_stats(and_direct);
  assert!(and_direct_facts.raw_proxy_map_key.is_empty(), "{and_direct_facts:?}");
  let skipped_mut = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]]); let chosen = false; chosen &&= map.delete(proxy); void map.get(proxy).count;";
  let (skipped_mut_facts, _) = contract_stats(skipped_mut);
  assert!(skipped_mut_facts.raw_proxy_map_key.is_empty(), "{skipped_mut_facts:?}");
  let opt_call_alias = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); const sink = null; sink?.(result.count);";
  let (opt_call_alias_facts, _) = contract_stats(opt_call_alias);
  assert!(opt_call_alias_facts.raw_proxy_map_key.is_empty(), "{opt_call_alias_facts:?}");
  let opt_call_direct = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const sink = null; sink?.(map.get(proxy).count);";
  let (opt_call_direct_facts, _) = contract_stats(opt_call_direct);
  assert!(opt_call_direct_facts.raw_proxy_map_key.is_empty(), "{opt_call_direct_facts:?}");
  let opt_computed_alias = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); const sink = null; void sink?.[result.count];";
  let (opt_computed_alias_facts, _) = contract_stats(opt_computed_alias);
  assert!(opt_computed_alias_facts.raw_proxy_map_key.is_empty(), "{opt_computed_alias_facts:?}");
  let opt_computed_direct = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const sink = null; void sink?.[map.get(proxy).count];";
  let (opt_computed_direct_facts, _) = contract_stats(opt_computed_direct);
  assert!(opt_computed_direct_facts.raw_proxy_map_key.is_empty(), "{opt_computed_direct_facts:?}");
  let holder = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); const holder = { inner }; holder.inner.get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (holder_facts, _) = contract_stats(holder);
  assert!(holder_facts.keyed_map_dependency.is_empty(), "{holder_facts:?}");
  let array = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); const holder = [inner]; holder[0].get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (array_facts, _) = contract_stats(array);
  assert!(array_facts.keyed_map_dependency.is_empty(), "{array_facts:?}");
  let returned = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); function expose() { return inner; } expose().get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (returned_facts, _) = contract_stats(returned);
  assert!(returned_facts.keyed_map_dependency.is_empty(), "{returned_facts:?}");
  let wrapped_ident = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (wrapped_ident_facts, _) = contract_stats(wrapped_ident);
  assert_eq!(wrapped_ident_facts.keyed_map_dependency.len(), 1, "{wrapped_ident_facts:?}");
  let taken = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); true && result.count;";
  let (taken_facts, _) = contract_stats(taken);
  assert_eq!(taken_facts.raw_proxy_map_key.len(), 1, "{taken_facts:?}");
  let ordinary = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (ordinary_facts, _) = contract_stats(ordinary);
  assert_eq!(ordinary_facts.raw_proxy_map_key.len(), 1, "{ordinary_facts:?}");
  let native_set = "import { reactive } from 'vue'; const helper = new Map(); const raw = {}; helper.set(raw, 1); const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); void map.get(proxy).count;";
  let (native_set_facts, _) = contract_stats(native_set);
  assert_eq!(native_set_facts.raw_proxy_map_key.len(), 1, "{native_set_facts:?}");
}

#[test]
fn collection_lookup_fifth_review_chain_and_wrapper_propagation() {
  let inherited_call = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); const sink = null; sink?.accept(result.count);";
  let (inherited_call_facts, _) = contract_stats(inherited_call);
  assert!(inherited_call_facts.raw_proxy_map_key.is_empty(), "{inherited_call_facts:?}");
  let inherited_computed = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); const sink = null; void sink?.field[result.count];";
  let (inherited_computed_facts, _) = contract_stats(inherited_computed);
  assert!(inherited_computed_facts.raw_proxy_map_key.is_empty(), "{inherited_computed_facts:?}");
  let grouped = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); const sink = {}; (sink?.accept)(result.count);";
  let (grouped_facts, _) = contract_stats(grouped);
  assert_eq!(grouped_facts.raw_proxy_map_key.len(), 1, "{grouped_facts:?}");
  let alias_storage = "import { computed, reactive } from 'vue'; const inner = new Map([['selected', 7]]); const alias = inner; const holder = { alias }; holder.alias.get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (alias_storage_facts, _) = contract_stats(alias_storage);
  assert!(alias_storage_facts.keyed_map_dependency.is_empty(), "{alias_storage_facts:?}");
  let secondary_write = "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]); const alternate = shallowReactive(inner); alternate.get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (secondary_write_facts, _) = contract_stats(secondary_write);
  assert!(secondary_write_facts.keyed_map_dependency.is_empty(), "{secondary_write_facts:?}");
  let paired_positive = "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]); const alternate = shallowReactive(inner); const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected; void alternate;";
  let (paired_positive_facts, _) = contract_stats(paired_positive);
  assert_eq!(paired_positive_facts.keyed_map_dependency.len(), 1, "{paired_positive_facts:?}");
  let middle_chain = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); const sink = null; void sink?.accept(result.count).next;";
  let (middle_chain_facts, _) = contract_stats(middle_chain);
  assert!(middle_chain_facts.raw_proxy_map_key.is_empty(), "{middle_chain_facts:?}");
  let nested_write = "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]); const middle = reactive(inner); const alternate = shallowReactive(middle); alternate.get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (nested_write_facts, _) = contract_stats(nested_write);
  assert!(nested_write_facts.keyed_map_dependency.is_empty(), "{nested_write_facts:?}");
  let inline_write = "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]); shallowReactive(inner).get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (inline_write_facts, _) = contract_stats(inline_write);
  assert!(inline_write_facts.keyed_map_dependency.is_empty(), "{inline_write_facts:?}");
  let nested_inline = "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]); shallowReactive(reactive(inner)).get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;";
  let (nested_inline_facts, _) = contract_stats(nested_inline);
  assert!(nested_inline_facts.keyed_map_dependency.is_empty(), "{nested_inline_facts:?}");
  let mut hop = String::from(
    "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]); const wrapper0 = reactive(inner);",
  );
  for index in 1..=10 {
    hop.push_str("const wrapper");
    hop.push_str(&index.to_string());
    hop.push_str(" = shallowReactive(wrapper");
    hop.push_str(&(index - 1).to_string());
    hop.push_str(");");
  }
  hop.push_str("wrapper10.get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;");
  let (hop_facts, _) = contract_stats(&hop);
  assert!(hop_facts.keyed_map_dependency.is_empty(), "{hop_facts:?}");
  let grouped_suffix = "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const result = map.get(proxy); const sink = null; (sink?.accept)(result.count)?.next;";
  let (grouped_suffix_facts, _) = contract_stats(grouped_suffix);
  assert_eq!(grouped_suffix_facts.raw_proxy_map_key.len(), 1, "{grouped_suffix_facts:?}");
}

#[test]
fn collection_lookup_paired_wrappers_and_nested_chains_grow_linearly() {
  let mut previous_wrappers: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [8_u64, 16, 32] {
    let mut source = String::from(
      "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]);",
    );
    for index in 0..size {
      source.push_str("const w");
      source.push_str(&index.to_string());
      source.push_str(" = shallowReactive(inner); void w");
      source.push_str(&index.to_string());
      source.push(';');
    }
    source.push_str("const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;");
    let (facts, stats) = contract_full_stats(&source);
    assert_eq!(facts.keyed_map_dependency.len(), 1, "paired wrappers {size}");
    if let Some((prev_size, prev)) = previous_wrappers {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.queries <= prev.queries.saturating_mul(2).saturating_add(prev_size.saturating_mul(8)),
        "paired-wrapper queries {prev:?} -> {stats:?}"
      );
    }
    previous_wrappers = Some((size, stats));
  }
  let mut previous_chain: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [8_u64, 16, 32] {
    let mut source = String::from(
      "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 7 }]]); const sink = null; void sink",
    );
    for index in 0..size {
      source.push_str("?.n");
      source.push_str(&index.to_string());
    }
    source.push_str("?.[map.get(proxy).count];");
    let (facts, stats) = contract_full_stats(&source);
    assert!(facts.raw_proxy_map_key.is_empty(), "nested chain {size}");
    if let Some((prev_size, prev)) = previous_chain {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.queries <= prev.queries.saturating_mul(2).saturating_add(prev_size.saturating_mul(8)),
        "nested-chain queries {prev:?} -> {stats:?}"
      );
    }
    previous_chain = Some((size, stats));
  }
}

#[test]
fn collection_lookup_shared_allocation_wc_grows_linearly() {
  let mut previous: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [8_u64, 16, 32] {
    let mut source = String::from(
      "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]);",
    );
    for index in 0..size {
      source.push_str("const w");
      source.push_str(&index.to_string());
      source.push_str(" = shallowReactive(inner); void w");
      source.push_str(&index.to_string());
      source.push(';');
    }
    source.push_str("const keyed = reactive(inner);");
    for index in 0..size {
      source.push_str("const selected");
      source.push_str(&index.to_string());
      source.push_str(" = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected");
      source.push_str(&index.to_string());
      source.push(';');
    }
    let (facts, stats) = contract_full_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(facts.keyed_map_dependency.len(), expected, "W=C facts {size}");
    if let Some((prev_size, prev)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.queries
          <= prev.queries.saturating_mul(2).saturating_add(prev_size.saturating_mul(16)),
        "W=C queries {prev:?} -> {stats:?}"
      );
      assert!(
        stats.queries
          < prev.queries.saturating_mul(size / prev_size).saturating_mul(size / prev_size),
        "W=C must not be quadratic {prev:?} -> {stats:?}"
      );
    }
    previous = Some((size, stats));
  }
}

#[test]
fn collection_lookup_nested_origin_depth_grows_linearly() {
  let mut previous: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [8_u64, 16, 32] {
    let mut source = String::from(
      "import { computed, reactive, shallowReactive } from 'vue'; const inner = new Map([['selected', 7]]); const w0 = reactive(inner);",
    );
    for index in 1..size {
      source.push_str("const w");
      source.push_str(&index.to_string());
      source.push_str(" = shallowReactive(w");
      source.push_str(&(index - 1).to_string());
      source.push_str(");");
    }
    source.push('w');
    source.push_str(&(size - 1).to_string());
    source.push_str(".get = () => 9; const keyed = reactive(inner); const selected = computed(() => { let value; keyed.forEach((entry, key) => { if (key === 'selected') value = entry }); return value; }); void selected;");
    let (facts, stats) = contract_full_stats(&source);
    assert!(facts.keyed_map_dependency.is_empty(), "nested origin {size}");
    if let Some((prev_size, prev)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.queries
          <= prev.queries.saturating_mul(2).saturating_add(prev_size.saturating_mul(16)),
        "nested-origin queries {prev:?} -> {stats:?}"
      );
    }
    previous = Some((size, stats));
  }
}

#[test]
fn collection_lookup_replay_counts_constructor_mutations_and_lookups_once() {
  let mut previous_shared: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]);",
    );
    for index in 0..size {
      source.push_str("void map.get(proxy).count; const _r");
      source.push_str(&index.to_string());
      source.push_str(" = 0;");
    }
    let (contracts, stats) = contract_full_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.raw_proxy_map_key.len(), expected, "shared-root reads {size}");
    if let Some((prev_size, prev)) = previous_shared {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.queries <= prev.queries.saturating_mul(2).saturating_add(prev_size),
        "shared-root queries {prev:?} -> {stats:?}"
      );
      assert!(
        stats.object_entries <= prev.object_entries.saturating_add(size),
        "shared-root constructor visits {prev:?} -> {stats:?}"
      );
    }
    previous_shared = Some((size, stats));
  }
  let mut previous_wide: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([",
    );
    for index in 0..size {
      source.push_str("['k");
      source.push_str(&index.to_string());
      source.push_str("', ");
      source.push_str(&index.to_string());
      source.push_str("], ");
    }
    source.push_str("[raw, { count: 1 }]]);");
    for index in 0..size {
      source.push_str("void map.get(proxy).count; const _w");
      source.push_str(&index.to_string());
      source.push_str(" = 0;");
    }
    let (contracts, stats) = contract_full_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.raw_proxy_map_key.len(), expected, "wide+reads {size}");
    if let Some((prev_size, prev)) = previous_wide {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.queries.saturating_mul(10) < prev.queries.saturating_mul(30),
        "wide+reads queries {prev:?} -> {stats:?}"
      );
      assert!(
        stats.object_entries <= prev.object_entries.saturating_mul(2).saturating_add(size),
        "wide+reads constructor visits {prev:?} -> {stats:?}"
      );
    }
    previous_wide = Some((size, stats));
  }
  let mut previous_raw: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { reactive } from 'vue';");
    for index in 0..size {
      source.push_str("const raw");
      source.push_str(&index.to_string());
      source.push_str(" = {}; const proxy");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(raw");
      source.push_str(&index.to_string());
      source.push_str(");");
    }
    source.push_str("const map = new Map([");
    for index in 0..size {
      source.push_str("[raw");
      source.push_str(&index.to_string());
      source.push_str(", { count: 1 }], ");
    }
    source.push_str("]);");
    for index in 0..size {
      source.push_str("void map.get(proxy");
      source.push_str(&index.to_string());
      source.push_str(").count;");
    }
    let (contracts, stats) = contract_full_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.raw_proxy_map_key.len(), expected, "distinct raw keys {size}");
    if let Some((prev_size, prev)) = previous_raw {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.queries.saturating_mul(10) < prev.queries.saturating_mul(30),
        "distinct raw queries {prev:?} -> {stats:?}"
      );
    }
    previous_raw = Some((size, stats));
  }
  let mut previous_mut: Option<(u64, crate::source_contracts::SourceContractStats)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map();",
    );
    for index in 0..size {
      source.push_str("map.set(raw, { count: ");
      source.push_str(&index.to_string());
      source.push_str(" });");
    }
    for _ in 0..size {
      source.push_str("void map.get(proxy).count;");
    }
    let (contracts, stats) = contract_full_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.raw_proxy_map_key.len(), expected, "mutation-heavy {size}");
    if let Some((prev_size, prev)) = previous_mut {
      assert_eq!(size, prev_size * 2);
      assert!(
        stats.writes <= prev.writes.saturating_mul(2).saturating_add(size),
        "mutation visits {prev:?} -> {stats:?}"
      );
    }
    previous_mut = Some((size, stats));
  }
  let mut unknown = String::from(
    "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]);",
  );
  for index in 0..32 {
    unknown.push_str("map.set(maybe");
    unknown.push_str(&index.to_string());
    unknown.push_str(", 1); void map.get(proxy).count;");
  }
  let (quiet, stats) = contract_full_stats(&unknown);
  assert!(quiet.raw_proxy_map_key.is_empty(), "{quiet:?}");
  assert!(stats.queries > 0 && stats.writes > 0, "unknown-key work: {stats:?}");
}

#[test]
fn collection_lookup_replay_pins_exact_work_for_size_16() {
  let mut shared = String::from(
    "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([[raw, { count: 1 }]]);",
  );
  for index in 0..16 {
    shared.push_str("void map.get(proxy).count; const _r");
    shared.push_str(&index.to_string());
    shared.push_str(" = 0;");
  }
  let (shared_facts, shared_stats) = contract_full_stats(&shared);
  assert_eq!(shared_facts.raw_proxy_map_key.len(), 16, "{shared_facts:?}");
  let mut wide = String::from(
    "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map([",
  );
  for index in 0..16 {
    wide.push_str("['k");
    wide.push_str(&index.to_string());
    wide.push_str("', ");
    wide.push_str(&index.to_string());
    wide.push_str("], ");
  }
  wide.push_str("[raw, { count: 1 }]]);");
  for index in 0..16 {
    wide.push_str("void map.get(proxy).count; const _w");
    wide.push_str(&index.to_string());
    wide.push_str(" = 0;");
  }
  let (wide_facts, wide_stats) = contract_full_stats(&wide);
  assert_eq!(wide_facts.raw_proxy_map_key.len(), 16, "{wide_facts:?}");
  let mut distinct = String::from("import { reactive } from 'vue';");
  for index in 0..16 {
    distinct.push_str("const raw");
    distinct.push_str(&index.to_string());
    distinct.push_str(" = {}; const proxy");
    distinct.push_str(&index.to_string());
    distinct.push_str(" = reactive(raw");
    distinct.push_str(&index.to_string());
    distinct.push_str(");");
  }
  distinct.push_str("const map = new Map([");
  for index in 0..16 {
    distinct.push_str("[raw");
    distinct.push_str(&index.to_string());
    distinct.push_str(", { count: 1 }], ");
  }
  distinct.push_str("]);");
  for index in 0..16 {
    distinct.push_str("void map.get(proxy");
    distinct.push_str(&index.to_string());
    distinct.push_str(").count;");
  }
  let (distinct_facts, _) = contract_full_stats(&distinct);
  assert_eq!(distinct_facts.raw_proxy_map_key.len(), 16, "{distinct_facts:?}");
  let mut mutated = String::from(
    "import { reactive } from 'vue'; const raw = {}; const proxy = reactive(raw); const map = new Map();",
  );
  for index in 0..16 {
    mutated.push_str("map.set(raw, { count: ");
    mutated.push_str(&index.to_string());
    mutated.push_str(" });");
  }
  for _ in 0..16 {
    mutated.push_str("void map.get(proxy).count;");
  }
  let (mutated_facts, mutated_stats) = contract_full_stats(&mutated);
  assert_eq!(mutated_facts.raw_proxy_map_key.len(), 16, "{mutated_facts:?}");
  assert_eq!(shared_stats.writes, 0, "{shared_stats:?}");
  assert_eq!(wide_stats.writes, 0, "{wide_stats:?}");
  assert!(mutated_stats.writes <= 16, "{mutated_stats:?}");
  assert!(
    shared_stats.queries <= wide_stats.queries.saturating_mul(4),
    "shared-root queries stay within 4x the wide read: {shared_stats:?} {wide_stats:?}"
  );
}
