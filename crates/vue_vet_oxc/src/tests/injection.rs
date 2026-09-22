use super::support::*;

const INJECT_BASIC: &str = "import { inject, provide } from 'vue'; const key = Symbol('count'); provide(key, 7); const count = inject(key, 'missing'); count.toFixed(2);";

#[test]
fn injection_demand_reports_same_instance_symbol_fallback() {
  let (facts, _) = contract_stats(INJECT_BASIC);
  assert_eq!(facts.inject_same_instance_provide.len(), 1, "{facts:?}");
  let Some(site) = facts.inject_same_instance_provide.first() else {
    return;
  };
  assert_eq!(site.member, "toFixed");
  assert_eq!(site.fallback_kind, vue_vet_core::PrimitiveValueKind::String);
  assert_eq!(site.provided_kind, vue_vet_core::PrimitiveValueKind::Number);
  assert_eq!(site.demand_span.length, "count.toFixed(2)".len(), "{site:?}");
}

#[test]
fn injection_demand_survives_later_await() {
  let (facts, _) = contract_stats(
    "import { inject, provide } from 'vue'; const key = Symbol('count'); provide(key, 7); const count = inject(key, 'missing'); await Promise.resolve(); count.toFixed(2);",
  );
  assert_eq!(facts.inject_same_instance_provide.len(), 1, "{facts:?}");
}

#[test]
fn injection_demand_chained_and_factory_and_absent_default() {
  let (chained, _) = contract_stats(
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); inject(key, 'missing').toFixed(2);",
  );
  assert_eq!(chained.inject_same_instance_provide.len(), 1, "{chained:?}");
  let (factory, _) = contract_stats(
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, () => 'missing', true); count.toFixed(2);",
  );
  assert_eq!(factory.inject_same_instance_provide.len(), 1, "{factory:?}");
  let (absent, _) = contract_stats(
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key); count.toFixed(2);",
  );
  assert_eq!(absent.inject_same_instance_provide.len(), 1, "{absent:?}");
  let Some(site) = absent.inject_same_instance_provide.first() else {
    return;
  };
  assert_eq!(site.fallback_kind, vue_vet_core::PrimitiveValueKind::Nullish);
  assert!(site.default_absent, "{site:?}");
}

#[test]
fn injection_demand_typescript_wrappers_unary_computed_and_optional() {
  for (source, demand) in [
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); count!.toFixed(2);",
      "count!.toFixed(2)",
    ),
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); (count as number).toFixed(2);",
      "(count as number).toFixed(2)",
    ),
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); (count satisfies unknown as number).toFixed(2);",
      "(count satisfies unknown as number).toFixed(2)",
    ),
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key); count!.toFixed(2);",
      "count!.toFixed(2)",
    ),
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject<number>(key); count!.toFixed(2);",
      "count!.toFixed(2)",
    ),
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 'text'); const label = inject(key, -1); label.toUpperCase();",
      "label.toUpperCase()",
    ),
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); count['toFixed'](2);",
      "count['toFixed'](2)",
    ),
    (
      "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); count?.toFixed(2);",
      "count?.toFixed(2)",
    ),
  ] {
    let (facts, _) = contract_stats(source);
    assert_eq!(facts.inject_same_instance_provide.len(), 1, "{source} => {facts:?}");
    let Some(site) = facts.inject_same_instance_provide.first() else {
      continue;
    };
    let Some(offset) = source.find(demand) else {
      continue;
    };
    assert_eq!(site.demand_span.offset, offset, "{demand} {site:?}");
    assert_eq!(site.demand_span.length, demand.len(), "{demand} {site:?}");
  }
}

#[test]
fn injection_demand_unicode_crlf_and_namespace() {
  let source = "import * as Vue from 'vue';\r\nconst 键 = Symbol('count');\r\nVue.provide(键, 7);\r\nconst 计数 = Vue.inject(键, 'missing');\r\n计数.toFixed(2);\r\n";
  let (facts, _) = contract_stats(source);
  assert_eq!(facts.inject_same_instance_provide.len(), 1, "{facts:?}");
  let Some(site) = facts.inject_same_instance_provide.first() else {
    return;
  };
  let demand = "计数.toFixed(2)";
  let offset = source.find(demand).unwrap_or(usize::MAX);
  let (line, column) = vue_vet_core::LineIndex::new(source).byte_to_line_column(offset);
  assert_eq!(site.demand_span.offset, offset, "{site:?}");
  assert_eq!(site.demand_span.length, demand.len(), "{site:?}");
  assert_eq!(site.demand_span.line, line, "{site:?}");
  assert_eq!(site.demand_span.column, column, "{site:?}");
  assert_eq!(site.demand_span.line, 5, "{site:?}");
  assert_eq!(site.demand_span.column, 1, "{site:?}");
}

#[test]
fn injection_demand_safe_probes_stay_quiet() {
  for source in [
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 0); count.toFixed(2);",
    "import { inject, provide } from 'vue'; provide('count', 7); const count = inject('count', 'missing'); count.toFixed(2);",
    "import { inject, provide } from 'vue'; const key = Symbol.for('count'); provide(key, 7); const count = inject(key, 'missing'); count.toFixed(2);",
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key); count?.toFixed(2);",
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); count?.toFixed?.(2);",
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); if (typeof count === 'number') count.toFixed(2);",
    "import { inject, provide } from 'vue'; function inner() { const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); count.toFixed(2); } inner();",
    "import { inject, provide } from 'vue'; function leak(key: symbol) { void key } const key = Symbol(); leak(key); provide(key, 7); const count = inject(key, 'missing'); count.toFixed(2);",
    "import { inject, provide } from 'vue'; Number.prototype.toFixed = String.prototype.toUpperCase; const key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); count.toFixed(2);",
    "import { inject, provide } from 'vue'; const Symbol = () => 'x'; const key = Symbol(); provide(key as never, 7); const count = inject(key as never, 'missing'); count.toFixed(2);",
    "import { inject, provide } from 'vue'; const flag = true; const key = Symbol(); provide(key, 7); const count = inject(key, () => 'missing', flag); count.toFixed(2);",
    "import { inject, provide } from 'vue'; let key = Symbol(); provide(key, 7); const count = inject(key, 'missing'); count.toFixed(2);",
    "import { inject } from 'vue'; const key = Symbol(); const count = inject(key, 'missing'); count.toFixed(2);",
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7); provide(key, 'text'); const count = inject(key, 'missing'); count.toFixed(2);",
    "import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 'text'); const count = inject(key, 'missing'); count.toUpperCase();",
    "import Vue from 'vue'; const key = Symbol(); Vue.provide(key, 7); const count = Vue.inject(key, 'missing'); count.toFixed(2);",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.inject_same_instance_provide.is_empty(),
      "safe probe must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn injection_demand_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [20_u64, 80] {
    let mut source = String::from("import { inject, provide } from 'vue';");
    for index in 0..size {
      source.push_str("const k");
      source.push_str(&index.to_string());
      source.push_str(" = Symbol(); provide(k");
      source.push_str(&index.to_string());
      source.push_str(", 7); const c");
      source.push_str(&index.to_string());
      source.push_str(" = inject(k");
      source.push_str(&index.to_string());
      source.push_str(", 'missing'); c");
      source.push_str(&index.to_string());
      source.push_str(".toFixed(2);");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.inject_same_instance_provide.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 4);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(50),
        "injection work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn injection_demand_many_ops_under_one_setup_and_alias_chain() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [20_u64, 80] {
    let mut source =
      String::from("import { inject, provide } from 'vue'; const key = Symbol(); provide(key, 7);");
    for index in 0..size {
      source.push_str("const c");
      source.push_str(&index.to_string());
      source.push_str(" = inject(key, 'missing'); c");
      source.push_str(&index.to_string());
      source.push_str(".toFixed(2);");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.inject_same_instance_provide.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 4);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(50),
        "shared-key injection work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
  let mut source = String::from("import { inject, provide } from 'vue'; const k0 = Symbol();");
  for index in 1..16 {
    source.push_str("const k");
    source.push_str(&index.to_string());
    source.push_str(" = k");
    source.push_str(&(index - 1).to_string());
    source.push(';');
  }
  source.push_str("provide(k15, 7); const count = inject(k0, 'missing'); count.toFixed(2);");
  let (facts, _) = contract_stats(&source);
  assert_eq!(facts.inject_same_instance_provide.len(), 1, "{facts:?}");
}

#[test]
fn injection_demand_order_is_deterministic() {
  let first = contract_stats(INJECT_BASIC);
  let second = contract_stats(INJECT_BASIC);
  assert_eq!(first.0.inject_same_instance_provide, second.0.inject_same_instance_provide);
}
