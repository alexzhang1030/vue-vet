use super::support::*;

#[test]
fn private_receiver_emits_method_and_getter_and_stays_quiet_without_demand() {
  let (method, _) = contract_stats(
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
  );
  assert_eq!(method.reactive_private_field_access.len(), 1, "{method:?}");
  let (getter, _) = contract_stats(
    "import { shallowReactive } from 'vue'; class Counter { #n = 1; get value() { return this.#n } } const proxy = shallowReactive(new Counter()); void proxy.value;",
  );
  assert_eq!(getter.reactive_private_field_access.len(), 1, "{getter:?}");
  assert!(
    getter.reactive_private_field_access.first().is_some_and(|site| site.getter),
    "{getter:?}"
  );
  let (unused, _) = contract_stats(
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } void reactive(new Counter());",
  );
  assert!(unused.reactive_private_field_access.is_empty(), "{unused:?}");
  let (class_only, _) = contract_stats("class Counter { #n = 1 }");
  assert!(class_only.reactive_private_field_access.is_empty(), "{class_only:?}");
}

#[test]
fn private_receiver_source_alias_and_chained_and_this_alias() {
  let (wrap, _) = contract_stats(
    "import { reactive } from 'vue'; const wrap = reactive; class Counter { #n = 1; read() { return this.#n } } const proxy = wrap(new Counter()); void proxy.read();",
  );
  assert_eq!(wrap.reactive_private_field_access.len(), 1, "{wrap:?}");
  let (chained, _) = contract_stats(
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } void reactive(new Counter()).read();",
  );
  assert_eq!(chained.reactive_private_field_access.len(), 1, "{chained:?}");
  let (alias, _) = contract_stats(
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { const self = this; return self.#n } } const proxy = reactive(new Counter()); void proxy.read();",
  );
  assert_eq!(alias.reactive_private_field_access.len(), 1, "{alias:?}");
}

#[test]
fn private_receiver_safe_controls_stay_quiet() {
  for source in [
    "import { reactive } from 'vue'; class Counter { #n = 1; read = () => this.#n } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; constructor() { this.read = this.read.bind(this) } read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { n = 1; read() { return this.n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { private n = 1; read() { return this.n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { static #n = 1; read() { return Counter.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; readOther(other: Counter) { return other.#n } } const raw = new Counter(); const proxy = reactive(new Counter()); void proxy.readOther(raw);",
    "import { reactive } from 'vue'; class Counter { #n = 1; guarded() { return #n in this ? this.#n : 0 } } const proxy = reactive(new Counter()); void proxy.guarded();",
    "import { reactive, toRaw } from 'vue'; class Counter { #n = 1; read() { return toRaw(this).#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { markRaw, reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = reactive(markRaw(new Counter())); void proxy.read();",
    "import { reactive } from 'vue'; class Replacement { #n = 3; constructor() { return { read: () => 4 } } read() { return this.#n } } const proxy = reactive(new Replacement()); void proxy.read();",
    "import { reactive } from 'vue'; class Base { #n = 1; read() { return this.#n } } class Child extends Base {} const proxy = reactive(new Child()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } Counter.prototype.read = function read() { return 0 }; const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } function retain(ctor: typeof Counter) { void ctor } retain(Counter); const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; function helper() {} class Counter { #n = 1; read() { helper(); return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = reactive(new Counter()); false && proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; constructor() { void new.target } read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = reactive(new Counter()); proxy.read = () => 0; void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = reactive(new Counter()); const fn = proxy.read; void fn();",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.reactive_private_field_access.is_empty(),
      "private-receiver safe probe must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn private_receiver_readonly_family_and_return_demand_report() {
  let (readonly_api, _) = contract_stats(
    "import { readonly } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = readonly(new Counter()); void proxy.read();",
  );
  assert_eq!(readonly_api.reactive_private_field_access.len(), 1, "{readonly_api:?}");
  assert_eq!(
    readonly_api.reactive_private_field_access.first().map(|site| site.api.as_str()),
    Some("readonly"),
    "{readonly_api:?}"
  );
  let (shallow_ro, _) = contract_stats(
    "import { shallowReadonly } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = shallowReadonly(new Counter()); void proxy.read();",
  );
  assert_eq!(shallow_ro.reactive_private_field_access.len(), 1, "{shallow_ro:?}");
  let (returned, _) = contract_stats(
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } function setup() { const proxy = reactive(new Counter()); return proxy.read() } setup();",
  );
  assert_eq!(returned.reactive_private_field_access.len(), 1, "{returned:?}");
}

#[test]
fn private_receiver_class_shape_repairs_stay_precise() {
  for source in [
    "import { reactive } from 'vue'; class Counter { #n = 1; __v_skip = true; read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; __v_raw = {}; read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; [Symbol.toStringTag] = 'Counter'; read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; get [Symbol.toStringTag]() { return 'Counter' } read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; patched = (this.read = () => 0); read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; class Counter { #n = 1; async read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "import { reactive } from 'vue'; const proxy = reactive(new Counter()); void proxy.read(); class Counter { #n = 1; read() { return this.#n } }",
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return this.#n } } const proxy = reactive(new Counter()); proxy.other = 1; void proxy.read();",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.reactive_private_field_access.is_empty(),
      "private-receiver repair quiet probe must stay silent: {source} => {facts:?}"
    );
  }
  let shadowed = analyze(
    "import { reactive } from 'vue'; class Counter { #n = 1; read = function () { return 1 }; read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
    "js",
  );
  assert!(
    shadowed.source_contracts.reactive_private_field_access.is_empty(),
    "JS own-field shadow must stay silent: {shadowed:?}"
  );
  let (ctor, _) = contract_stats(
    "import { reactive } from 'vue'; class Counter { #n; constructor() { this.#n = 5; this.items = [] } read() { return this.#n } } const proxy = reactive(new Counter()); void proxy.read();",
  );
  assert_eq!(ctor.reactive_private_field_access.len(), 1, "{ctor:?}");
  let (in_arg, _) = contract_stats(
    "import { reactive } from 'vue'; class Counter { #n = 1; read() { return String(this.#n) } } const proxy = reactive(new Counter()); void proxy.read();",
  );
  assert_eq!(in_arg.reactive_private_field_access.len(), 1, "{in_arg:?}");
}

#[test]
fn private_receiver_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { reactive } from 'vue';");
    for index in 0..size {
      source.push_str("class C");
      source.push_str(&index.to_string());
      source.push_str(" { #n = 1; read() { return this.#n } } const p");
      source.push_str(&index.to_string());
      source.push_str(" = reactive(new C");
      source.push_str(&index.to_string());
      source.push_str("()); void p");
      source.push_str(&index.to_string());
      source.push_str(".read();");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.reactive_private_field_access.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "private-receiver work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn private_receiver_shared_class_and_alias_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { reactive } from 'vue'; class Shared { #n = 1; read() { return this.#n } }",
    );
    for index in 0..size {
      source.push_str("const wrap");
      source.push_str(&index.to_string());
      source.push_str(" = reactive; const p");
      source.push_str(&index.to_string());
      source.push_str(" = wrap");
      source.push_str(&index.to_string());
      source.push_str("(new Shared()); const a");
      source.push_str(&index.to_string());
      source.push_str(" = p");
      source.push_str(&index.to_string());
      source.push_str("; void a");
      source.push_str(&index.to_string());
      source.push_str(".read();");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.reactive_private_field_access.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-class private-receiver work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn private_receiver_deeper_method_bodies_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { reactive } from 'vue'; class Deep { #n = 1; read() {");
    for index in 0..size {
      source.push_str("const v");
      source.push_str(&index.to_string());
      source.push_str(" = ");
      source.push_str(&index.to_string());
      source.push(';');
    }
    source.push_str(" return this.#n } } const proxy = reactive(new Deep()); void proxy.read();");
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.reactive_private_field_access.len(), 1, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "deep-method private-receiver work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}
