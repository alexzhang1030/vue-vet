use super::support::*;

#[test]
fn extracted_collection_methods_report_bare_calls() {
  let map_get = extracted_methods(
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert_eq!(map_get.extracted_reactive_collection_method.len(), 1, "{map_get:?}");
  let site = first_extracted(&map_get);
  assert_eq!(site.method, "get");
  assert_eq!(site.collection, "Map");
  assert_eq!(site.api, "reactive");
  assert_eq!(site.object, "map");
  let map_set = extracted_methods(
    "import { reactive } from 'vue'; const map = reactive(new Map()); const set = map.set; set('a', 1);",
  );
  assert_eq!(first_extracted(&map_set).method, "set");
  let map_has = extracted_methods(
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { has } = map; has('a');",
  );
  assert_eq!(first_extracted(&map_has).method, "has");
  let set_add = extracted_methods(
    "import { reactive } from 'vue'; const items = reactive(new Set()); const { add } = items; add(1);",
  );
  assert_eq!(first_extracted(&set_add).collection, "Set");
  let set_has = extracted_methods(
    "import { reactive } from 'vue'; const items = reactive(new Set([1])); const has = items.has; has(1);",
  );
  assert_eq!(first_extracted(&set_has).method, "has");
  let array_map = extracted_methods(
    "import { reactive } from 'vue'; const items = reactive([1, 2]); const { map } = items; map((n) => n);",
  );
  assert_eq!(first_extracted(&array_map).collection, "Array");
  let array_includes = extracted_methods(
    "import { reactive } from 'vue'; const items = reactive([1, 2]); const { includes } = items; includes(1);",
  );
  assert_eq!(first_extracted(&array_includes).method, "includes");
  let array_push = extracted_methods(
    "import { reactive } from 'vue'; const items = reactive([1, 2]); const push = items.push; push(3);",
  );
  assert_eq!(first_extracted(&array_push).method, "push");
}

#[test]
fn extracted_collection_methods_follow_aliases_namespaces_and_ts_wrappers() {
  let aliased = extracted_methods(
    "import { reactive as rx } from 'vue'; const map = rx(new Map([['a', 1]])); const alias = map; const get = alias.get; get('a');",
  );
  assert_eq!(aliased.extracted_reactive_collection_method.len(), 1, "{aliased:?}");
  let namespace = extracted_methods(
    "import * as Vue from 'vue'; const map = Vue.reactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert_eq!(namespace.extracted_reactive_collection_method.len(), 1, "{namespace:?}");
  let wrapped = extracted_methods(
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]]) as Map<string, number>); const get = (map as Map<string, number>).get; get('a' as const);",
  );
  assert_eq!(wrapped.extracted_reactive_collection_method.len(), 1, "{wrapped:?}");
  let reactivity = extracted_methods(
    "import { reactive } from '@vue/reactivity'; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert_eq!(reactivity.extracted_reactive_collection_method.len(), 1, "{reactivity:?}");
  let runtime = extracted_methods(
    "import { shallowReactive } from '@vue/runtime-core'; const map = shallowReactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert_eq!(first_extracted(&runtime).api, "shallowReactive");
  let raw = extracted_methods(
    "import { reactive } from 'vue'; const raw = new Map([['a', 1]]); const map = reactive(raw); const { get } = map; get('a');",
  );
  assert_eq!(raw.extracted_reactive_collection_method.len(), 1, "{raw:?}");
}

#[test]
fn extracted_collection_methods_reuse_counted_actual_proxy_origin() {
  let (named, named_stats) = contract_full_stats(
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert_eq!(named.extracted_reactive_collection_method.len(), 1, "{named:?}");
  assert!(
    named_stats.import_source_steps <= 2,
    "named constructor origin is the counted VueImport lookup; {named_stats:?}"
  );
  let (namespace, namespace_stats) = contract_full_stats(
    "import * as Vue from 'vue'; const map = Vue.reactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert_eq!(namespace.extracted_reactive_collection_method.len(), 1, "{namespace:?}");
  assert!(
    namespace_stats.import_source_steps <= 2,
    "namespace constructor origin is the counted VueImport lookup; {namespace_stats:?}"
  );
  let (local, local_stats) = contract_full_stats(
    "function reactive<T>(value: T): T { return value; } const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert!(local.extracted_reactive_collection_method.is_empty(), "{local:?}");
  assert!(
    local_stats.import_source_steps == 0,
    "local constructors must not examine VueImport; {local_stats:?}"
  );
  let (type_only, type_only_stats) = contract_full_stats(
    "import type { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
  );
  assert!(type_only.extracted_reactive_collection_method.is_empty(), "{type_only:?}");
  assert!(
    type_only_stats.import_source_steps == 0,
    "type-only sources must not examine VueImport; {type_only_stats:?}"
  );
}

#[test]
fn extracted_collection_methods_stay_quiet_for_safe_and_unknown_controls() {
  let sources = [
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); map.get('a');",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; get.call(map, 'a');",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; get.apply(map, ['a']);",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; Reflect.apply(get, map, ['a']);",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const bound = map.get.bind(map); bound('a');",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); const { get } = map; void get;",
    "import { reactive } from 'vue'; const object = reactive({ get: () => 1 }); const { get } = object; get();",
    "function reactive<T>(value: T): T { return value; } const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
    "import { reactive } from 'vue'; class Map { get(_key: string) { return 1; } } const map = reactive(new Map()); const { get } = map; get('a');",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); map.get = ((_key: string) => 1) as typeof map.get; const { get } = map; get('a');",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); (map as { __v_skip?: boolean }).__v_skip = true; const { get } = map; get('a');",
    "import { reactive } from 'vue'; function tag(value: object) { void value; } const map = reactive(new Map([['a', 1]])); tag(map); const { get } = map; get('a');",
    "import { reactive } from 'vue-demi'; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
    "import { reactive } from '#imports'; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
    "import { reactive } from 'vue'; function createMap() { return new Map([['a', 1]]); } const map = reactive(createMap()); const { get } = map; get('a');",
    "import { reactive } from 'vue'; Map.prototype.get = function get() { return 1; }; const map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
    "import { reactive } from 'vue'; const key = 'get'; const map = reactive(new Map([['a', 1]])); const get = map[key]; get('a');",
    "import { reactive } from 'vue'; let map = reactive(new Map([['a', 1]])); const { get } = map; get('a');",
    "import { readonly } from 'vue'; const map = readonly(new Map([['a', 1]])); const { get } = map; get('a');",
    "import { reactive } from 'vue'; class Box { constructor(public value: object) {} } const map = reactive(new Map([['a', 1]])); void new Box(map); const { get } = map; get('a');",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); let get: typeof map.get; ({ get } = map); get('a');",
    "import { reactive } from 'vue'; const map = reactive(new WeakMap()); const { get } = map as { get: (key: object) => unknown }; get({});",
    "import { reactive } from 'vue'; const items = reactive(new Array(2)); const { map } = items as { map: (fn: (n: unknown) => unknown) => unknown[] }; map((n) => n);",
    "import { reactive } from 'vue'; const map = reactive(new Map([['a', 1]])); (map as { tag?: () => void }).tag = () => {}; (map as { tag: () => void }).tag(); const { get } = map; get('a');",
  ];
  for source in sources {
    let facts = extracted_methods(source);
    assert!(
      facts.extracted_reactive_collection_method.is_empty(),
      "must stay quiet: {source} -> {facts:?}"
    );
  }
}

#[test]
#[expect(clippy::panic, reason = "missing span evidence must fail the regression")]
fn extracted_collection_methods_unicode_and_crlf_use_exact_bytes() {
  let unicode = "import { reactive } from 'vue'; const 表 = reactive(new Map([['键', 1]])); const { get } = 表; get('键');";
  let facts = extracted_methods(unicode);
  assert_eq!(facts.extracted_reactive_collection_method.len(), 1, "{facts:?}");
  let site = first_extracted(&facts);
  let Some(call) = unicode.find("get('键')") else {
    panic!("unicode call");
  };
  assert_eq!(site.call_span.offset, call);
  assert_eq!(site.call_span.length, "get('键')".len());
  assert_eq!(unicode.as_bytes().get(call).copied(), Some(b'g'));
  let crlf = "import { reactive } from 'vue';\r\nconst map = reactive(new Map([['a', 1]]));\r\nconst { get } = map;\r\nget('a');\r\n";
  let crlf_facts = extracted_methods(crlf);
  assert_eq!(crlf_facts.extracted_reactive_collection_method.len(), 1, "{crlf_facts:?}");
  let crlf_site = first_extracted(&crlf_facts);
  let Some(crlf_call) = crlf.find("get('a')") else {
    panic!("crlf call");
  };
  assert_eq!(crlf_site.call_span.offset, crlf_call);
  assert_eq!(crlf_site.call_span.length, "get('a')".len());
  assert!(crlf.contains('\r'), "fixture must keep CR bytes");
}

#[test]
fn extracted_collection_methods_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { reactive } from 'vue';");
    for index in 0..size {
      source.push_str("const m");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(new Map([['a', 1]])); const { get: g");
      source.push_str(&index.to_string());
      source.push_str(" } = m");
      source.push_str(&index.to_string());
      source.push_str("; g");
      source.push_str(&index.to_string());
      source.push_str("('a');");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(
      contracts.extracted_reactive_collection_method.len(),
      usize::try_from(size).unwrap_or(usize::MAX),
      "size={size} work={work}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "extracted-method work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn extracted_collection_methods_stay_quiet_for_review_capability_roles() {
  let sources = [
    "import { reactive } from 'vue'; const raw = [1]; ({ skip: raw.__v_skip, map: raw.map } = { skip: true, map: () => [7] }); const items = reactive(raw); const { map } = items; map();",
    "import { reactive } from 'vue'; const raw = [1]; const marker = '__v_skip'; raw[marker] = true; raw.map = () => [7]; const items = reactive(raw); const { map } = items; map();",
    "import { reactive } from 'vue'; function configure(target: { __v_skip?: boolean; map?: () => number[] }) { target.__v_skip = true; target.map = () => [7]; } const raw = [1]; configure(true && raw); const items = reactive(raw); const { map } = items; map();",
    "import { reactive } from 'vue'; const raw = [1] as { configure?: (value: TemplateStringsArray) => void; map?: () => number[]; __v_skip?: boolean }; raw.configure = function (this: typeof raw) { this.__v_skip = true; this.map = () => [7]; }; raw.configure`change`; const items = reactive(raw); const { map } = items; map();",
    "import { reactive } from 'vue'; const raw = [1] as { configure?: () => void; map?: () => number[]; __v_skip?: boolean }; raw.configure = function (this: typeof raw) { this.__v_skip = true; this.map = () => [7]; }; const method = 'configure'; raw[method](); const items = reactive(raw); const { map } = items; map();",
    "import { reactive } from 'vue'; const raw = [1] as { valueOf?: () => number[]; map?: () => number[]; __v_skip?: boolean }; raw.valueOf = function (this: typeof raw) { this.__v_skip = true; this.map = () => [7]; }; raw.valueOf(); const items = reactive(raw); const { map } = items; map();",
    "import { reactive } from 'vue'; const raw = [1] as { map?: () => number[]; __v_skip?: boolean }; function configure() { alias.__v_skip = true; alias.map = () => [7]; } const alias = raw; configure(); const items = reactive(raw); const { map } = items; map();",
    "import { reactive } from 'vue'; const original = Map; try { (globalThis as { Map: typeof Map }).Map = class { get() { return 7; } } as unknown as MapConstructor; const items = reactive(new Map()); const { get } = items as { get: () => number }; get(); } finally { (globalThis as { Map: typeof Map }).Map = original; }",
    "import { reactive } from 'vue'; const items = reactive(new Map()); const { get } = items; false && get('key');",
    "import { reactive } from 'vue'; function run() { const items = reactive(new Map()); const { get } = items; return 7; get('key'); } run();",
    "import { reactive } from 'vue'; const raw = [1]; delete raw.map; const items = reactive(raw); const { map } = items as { map: () => number[] }; map();",
    "import { reactive } from 'vue'; const raw = [1] as { map?: () => number[] }; for (raw.map of [() => [7]]) {} const items = reactive(raw); const { map } = items; map();",
  ];
  for source in sources {
    let facts = extracted_methods(source);
    assert!(
      facts.extracted_reactive_collection_method.is_empty(),
      "must stay quiet: {source} -> {facts:?}"
    );
  }
}

#[test]
fn extracted_collection_methods_dense_negatives_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { reactive } from 'vue';");
    for index in 0..size {
      source.push_str("const r");
      source.push_str(&index.to_string());
      source.push_str(" = [1]; const k");
      source.push_str(&index.to_string());
      source.push_str(" = '__v_skip'; r");
      source.push_str(&index.to_string());
      source.push_str("[k");
      source.push_str(&index.to_string());
      source.push_str("] = true; const i");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(r");
      source.push_str(&index.to_string());
      source.push_str("); const { map: m");
      source.push_str(&index.to_string());
      source.push_str(" } = i");
      source.push_str(&index.to_string());
      source.push_str("; m");
      source.push_str(&index.to_string());
      source.push_str("();");
    }
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.extracted_reactive_collection_method.is_empty(),
      "dense computed-marker writes must stay quiet size={size} work={work} {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "dense-negative work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn extracted_collection_methods_shared_alias_negatives_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { reactive } from 'vue'; const raw = [1];");
    for index in 0..size {
      source.push_str("const a");
      source.push_str(&index.to_string());
      source.push_str(" = raw; a");
      source.push_str(&index.to_string());
      source.push_str(".__v_skip = true;");
    }
    source.push_str("const items = reactive(raw); const { map } = items; map();");
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.extracted_reactive_collection_method.is_empty(),
      "shared-alias writes must stay quiet size={size} work={work} {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-alias-negative work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn extracted_collection_methods_stay_quiet_for_budget_boundary_escapes() {
  let sources = [
    escape_helper_source(&nest_true_and("raw", 8)),
    escape_helper_source(&nest_true_and("raw", 9)),
    escape_helper_source(&nest_true_and("raw", 16)),
    escape_helper_source(&nest_ternary("raw", 9)),
    escape_helper_source(&nest_sequence("raw", 9)),
    format!(
      "import {{ reactive }} from 'vue'; function configure(target: unknown) {{ const value = (target as {{ __v_skip?: boolean; map?: () => number[] }}[][][][][][][][][])[0][0][0][0][0][0][0][0][0]; value.__v_skip = true; value.map = () => [7]; }} const raw = [1] as {{ __v_skip?: boolean; map?: () => number[] }}; configure({}); const items = reactive(raw); const {{ map }} = items; map();",
      nest_array("raw", 9)
    ),
  ];
  for source in sources {
    let facts = extracted_methods(&source);
    assert!(
      facts.extracted_reactive_collection_method.is_empty(),
      "must stay quiet: {source} -> {facts:?}"
    );
  }
}

#[test]
fn extracted_collection_methods_keep_sibling_positive_across_unresolved_escape() {
  let source = format!(
    "{} const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
    escape_helper_source(&nest_true_and("raw", 9))
  );
  let facts = extracted_methods(&source);
  assert_eq!(facts.extracted_reactive_collection_method.len(), 1, "{facts:?}");
  assert_eq!(first_extracted(&facts).method, "get");
  assert_eq!(first_extracted(&facts).object, "kept");
}

#[test]
fn extracted_collection_methods_deeper_escape_negatives_scale_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; function configure(target: { __v_skip?: boolean; map?: () => number[] } | unknown) { void target; }",
    );
    for index in 0..size {
      let name = format!("r{index}");
      let escaped = match index % 4 {
        0 => nest_true_and(&name, 9),
        1 => nest_ternary(&name, 9),
        2 => nest_array(&name, 9),
        _ => nest_sequence(&name, 9),
      };
      source.push_str("const ");
      source.push_str(&name);
      source.push_str(" = [1]; configure(");
      source.push_str(&escaped);
      source.push_str("); const i");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(");
      source.push_str(&name);
      source.push_str("); const { map: m");
      source.push_str(&index.to_string());
      source.push_str(" } = i");
      source.push_str(&index.to_string());
      source.push_str("; m");
      source.push_str(&index.to_string());
      source.push_str("();");
    }
    source.push_str("const deep = [1]; configure(");
    source.push_str(&nest_true_and("deep", u32::try_from(size).unwrap_or(u32::MAX)));
    source.push_str(
      "); const deepItems = reactive(deep); const { map: deepMap } = deepItems; deepMap();",
    );
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.extracted_reactive_collection_method.is_empty(),
      "deeper escape negatives must stay quiet size={size} work={work} {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "deeper-escape-negative work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn extracted_collection_methods_stay_quiet_for_native_global_escapes() {
  let sources = [
    native_ctor_escape_source(&nest_true_and("Array", 8)),
    native_ctor_escape_source(&nest_true_and("Array", 9)),
    native_ctor_escape_source(&nest_true_and("Array", 16)),
    native_prototype_escape_source(&nest_true_and("Array.prototype", 8)),
    native_prototype_escape_source(&nest_true_and("Array.prototype", 9)),
    native_prototype_escape_source(&nest_true_and("Array.prototype", 16)),
    global_alias_escape_source("world"),
    global_alias_escape_source(&nest_true_and("world", 8)),
    global_alias_escape_source(&nest_true_and("world", 9)),
    global_alias_escape_source(&nest_true_and("world", 16)),
  ];
  for source in sources {
    let facts = extracted_methods(&source);
    assert!(
      facts.extracted_reactive_collection_method.is_empty(),
      "must stay quiet: {source} -> {facts:?}"
    );
  }
}

#[test]
fn extracted_collection_methods_keep_map_positive_when_only_array_capability_is_unresolved() {
  let source = format!(
    "{} const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
    native_ctor_escape_source(&nest_true_and("Array", 9))
  );
  let facts = extracted_methods(&source);
  assert_eq!(facts.extracted_reactive_collection_method.len(), 1, "{facts:?}");
  assert_eq!(first_extracted(&facts).method, "get");
  assert_eq!(first_extracted(&facts).object, "kept");
}

#[test]
fn extracted_collection_methods_keep_positive_when_shadowed_globals_escape() {
  let shadowed_array = format!(
    "import {{ reactive }} from 'vue'; function configure(ctor: unknown) {{ void ctor; }} const Array = class {{}}; configure({}); const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
    nest_true_and("Array", 9)
  );
  let shadowed_global_this = format!(
    "import {{ reactive }} from 'vue'; function configure(world: unknown) {{ void world; }} const globalThis = {{ Map: class {{ get() {{ return 7; }} }} }}; configure({}); const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
    nest_true_and("globalThis", 9)
  );
  for source in [shadowed_array, shadowed_global_this] {
    let facts = extracted_methods(&source);
    assert_eq!(facts.extracted_reactive_collection_method.len(), 1, "{source} -> {facts:?}");
    assert_eq!(first_extracted(&facts).method, "get");
    assert_eq!(first_extracted(&facts).object, "kept");
  }
}

#[test]
fn extracted_collection_methods_unresolved_global_escape_work_scales_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; function configure(target: unknown) { void target; } const world = globalThis;",
    );
    for index in 0..size {
      let name = format!("r{index}");
      let escaped = match index % 4 {
        0 => nest_true_and("Array", 9),
        1 => nest_true_and("Array.prototype", 9),
        2 => nest_true_and("world", 9),
        _ => nest_true_and(&name, 9),
      };
      source.push_str("const ");
      source.push_str(&name);
      source.push_str(" = [1]; configure(");
      source.push_str(&escaped);
      source.push_str("); const i");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(");
      source.push_str(&name);
      source.push_str("); const { map: m");
      source.push_str(&index.to_string());
      source.push_str(" } = i");
      source.push_str(&index.to_string());
      source.push_str("; m");
      source.push_str(&index.to_string());
      source.push_str("();");
    }
    source.push_str("const deep = [1]; configure(");
    source.push_str(&nest_true_and("Array", u32::try_from(size).unwrap_or(u32::MAX)));
    source.push_str(
      "); const deepItems = reactive(deep); const { map: deepMap } = deepItems; deepMap();",
    );
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.extracted_reactive_collection_method.is_empty(),
      "unresolved global-escape work must stay quiet size={size} work={work} {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "unresolved-global-escape work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn extracted_collection_methods_stay_quiet_for_native_ctor_aliases() {
  let sources = [
    native_ctor_alias_escape_source("const capability = Array;", "capability"),
    native_ctor_alias_escape_source("const capability = Array;", &nest_true_and("capability", 8)),
    native_ctor_alias_escape_source("const capability = Array;", &nest_true_and("capability", 9)),
    native_ctor_alias_escape_source("const capability = Array;", &nest_true_and("capability", 16)),
    native_prototype_alias_escape_source("const capability = Array.prototype;", "capability"),
    native_prototype_alias_escape_source(
      "const capability = Array.prototype;",
      &nest_true_and("capability", 8),
    ),
    native_prototype_alias_escape_source(
      "const capability = Array.prototype;",
      &nest_true_and("capability", 9),
    ),
    native_prototype_alias_escape_source(
      "const capability = Array.prototype;",
      &nest_true_and("capability", 16),
    ),
    native_ctor_alias_escape_source("const ctor = Array; const capability = ctor;", "capability"),
    native_prototype_alias_escape_source(
      "const proto = Array.prototype; const capability = proto;",
      "capability",
    ),
    native_prototype_alias_escape_source(
      "const ctor = Array; const capability = ctor.prototype;",
      "capability",
    ),
  ];
  for source in sources {
    let facts = extracted_methods(&source);
    assert!(
      facts.extracted_reactive_collection_method.is_empty(),
      "must stay quiet: {source} -> {facts:?}"
    );
  }
}

#[test]
fn extracted_collection_methods_keep_map_positive_when_only_array_alias_escapes() {
  let sources = [
    format!(
      "{} const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
      native_ctor_alias_escape_source("const capability = Array;", "capability")
    ),
    format!(
      "{} const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
      native_ctor_alias_escape_source("const capability = Array;", &nest_true_and("capability", 9))
    ),
    format!(
      "{} const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
      native_prototype_alias_escape_source("const capability = Array.prototype;", "capability")
    ),
    format!(
      "{} const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
      native_prototype_alias_escape_source(
        "const capability = Array.prototype;",
        &nest_true_and("capability", 9)
      )
    ),
  ];
  for source in sources {
    let facts = extracted_methods(&source);
    assert_eq!(facts.extracted_reactive_collection_method.len(), 1, "{source} -> {facts:?}");
    assert_eq!(first_extracted(&facts).method, "get");
    assert_eq!(first_extracted(&facts).object, "kept");
  }
}

#[test]
fn extracted_collection_methods_keep_positive_when_shadowed_ctor_aliases_escape() {
  let shadowed_constructor = format!(
    "import {{ reactive }} from 'vue'; function configure(ctor: unknown) {{ void ctor; }} const Array = class {{}}; const capability = Array; configure({}); const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
    nest_true_and("capability", 9)
  );
  let shadowed_prototype = format!(
    "import {{ reactive }} from 'vue'; function configure(prototype: unknown) {{ void prototype; }} const Array = {{ prototype: {{}} }}; const capability = Array.prototype; configure({}); const kept = reactive(new Map([['a', 1]])); const {{ get }} = kept; get('a');",
    nest_true_and("capability", 9)
  );
  let shadowed_direct = "import { reactive } from 'vue'; function configure(ctor: unknown) { void ctor; } const Array = class {}; const capability = Array; configure(capability); const kept = reactive(new Map([['a', 1]])); const { get } = kept; get('a');";
  for source in [shadowed_constructor, shadowed_prototype, shadowed_direct.to_string()] {
    let facts = extracted_methods(&source);
    assert_eq!(facts.extracted_reactive_collection_method.len(), 1, "{source} -> {facts:?}");
    assert_eq!(first_extracted(&facts).method, "get");
    assert_eq!(first_extracted(&facts).object, "kept");
  }
}

#[test]
fn extracted_collection_methods_native_ctor_alias_work_scales_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; function configure(target: unknown) { void target; }",
    );
    for index in 0..size {
      let escaped = match index % 4 {
        0 => {
          source.push_str("const c");
          source.push_str(&index.to_string());
          source.push_str(" = Array; ");
          format!("c{index}")
        }
        1 => {
          source.push_str("const p");
          source.push_str(&index.to_string());
          source.push_str(" = Array.prototype; ");
          format!("p{index}")
        }
        2 => {
          source.push_str("const a");
          source.push_str(&index.to_string());
          source.push_str(" = Array; const b");
          source.push_str(&index.to_string());
          source.push_str(" = a");
          source.push_str(&index.to_string());
          source.push_str("; ");
          format!("b{index}")
        }
        _ => {
          source.push_str("const d");
          source.push_str(&index.to_string());
          source.push_str(" = Array; const e");
          source.push_str(&index.to_string());
          source.push_str(" = d");
          source.push_str(&index.to_string());
          source.push_str(".prototype; ");
          format!("e{index}")
        }
      };
      let nested = if index % 2 == 0 { nest_true_and(&escaped, 9) } else { escaped };
      source.push_str("configure(");
      source.push_str(&nested);
      source.push_str("); const i");
      source.push_str(&index.to_string());
      source.push_str(" = reactive([1]); const { map: m");
      source.push_str(&index.to_string());
      source.push_str(" } = i");
      source.push_str(&index.to_string());
      source.push_str("; m");
      source.push_str(&index.to_string());
      source.push_str("();");
    }
    let (contracts, work) = contract_stats(&source);
    assert!(
      contracts.extracted_reactive_collection_method.is_empty(),
      "native ctor-alias work must stay quiet size={size} work={work} {contracts:?}"
    );
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2, "fixture sizes must double");
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "native-ctor-alias work grew from {prev_work} to {work} on {prev_size}->{size} (must stay <3x per doubling)"
      );
    }
    previous = Some((size, work));
  }
}
