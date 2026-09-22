use super::support::*;

#[test]
fn filter_settlement_emits_cancelled_demand_and_stays_quiet_without_await() {
  let (positive, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(positive.cancelled_filter_promise_demand.len(), 1, "{positive:?}");
  let Some(site) = positive.cancelled_filter_promise_demand.first() else {
    return;
  };
  assert_eq!(site.member, "slice", "{site:?}");
  assert_eq!(site.api, "useDebounceFn", "{site:?}");
  let (quiet, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); void first;",
  );
  assert!(quiet.cancelled_filter_promise_demand.is_empty(), "{quiet:?}");
}

#[test]
fn filter_settlement_shared_alias_namespace_and_awaited_binding() {
  let (shared, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/shared'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(shared.cancelled_filter_promise_demand.len(), 1, "{shared:?}");
  let (alias, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const alias = run; const first = alias('aa'); alias('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(alias.cancelled_filter_promise_demand.len(), 1, "{alias:?}");
  let (namespace, _) = contract_stats(
    "import * as VueUse from '@vueuse/core'; const run = VueUse.useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(namespace.cancelled_filter_promise_demand.len(), 1, "{namespace:?}");
  let (bound, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); const cancelled = await first; cancelled.slice(0, 1);",
  );
  assert_eq!(bound.cancelled_filter_promise_demand.len(), 1, "{bound:?}");
  let (literal_arg, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const query = 'aa'; const first = run(query); run('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(literal_arg.cancelled_filter_promise_demand.len(), 1, "{literal_arg:?}");
  let (siblings, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const a = run('aa'); const b = run('bb'); run('cc'); void (await a).slice(0, 1); void (await b).slice(0, 1);",
  );
  assert_eq!(siblings.cancelled_filter_promise_demand.len(), 2, "{siblings:?}");
}

#[test]
#[expect(clippy::panic, reason = "unicode CRLF fixture must contain the demand")]
fn filter_settlement_unicode_crlf_and_options() {
  let source = "import { useDebounceFn } from '@vueuse/core';\r\nconst 运行 = useDebounceFn((value: string) => value.toUpperCase(), 50);\r\nconst 先前 = 运行('aa');\r\n运行('bb');\r\nconst 结果 = (await 先前).slice(0, 1);\r\n";
  let demand = "(await 先前).slice(0, 1)";
  let Some(offset) = source.find(demand) else {
    panic!("unicode CRLF fixture must contain the demand");
  };
  let (line, column) = vue_vet_core::LineIndex::new(source).byte_to_line_column(offset);
  let (facts, _) = contract_stats(source);
  assert_eq!(facts.cancelled_filter_promise_demand.len(), 1, "{facts:?}");
  if let Some(site) = facts.cancelled_filter_promise_demand.first() {
    assert_eq!(site.demand_span.offset, offset, "{site:?}");
    assert_eq!(site.demand_span.length, demand.len(), "{site:?}");
    assert_eq!(site.demand_span.line, line, "{site:?}");
    assert_eq!(site.demand_span.column, column, "{site:?}");
  }
  let (empty, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, {}); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(empty.cancelled_filter_promise_demand.len(), 1, "{empty:?}");
  let (reject_false, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, { rejectOnCancel: false }); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(reject_false.cancelled_filter_promise_demand.len(), 1, "{reject_false:?}");
  let (duplicate, _) = contract_stats(
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, { rejectOnCancel: true, rejectOnCancel: false }); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
  );
  assert_eq!(duplicate.cancelled_filter_promise_demand.len(), 1, "{duplicate:?}");
}

#[test]
fn filter_settlement_safe_controls_stay_quiet() {
  for source in [
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); const cancelled = await first; if (cancelled) cancelled.slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); void (await first)?.slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, { rejectOnCancel: true }); const first = run('aa'); run('bb'); void (await first);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 0); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = await run('aa'); const latest = await run('bb'); first.slice(0, 1); latest.slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); const latest = run('bb'); void (await first); void (await latest).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; function hold(fn: (value: string) => Promise<string>) { void fn; } const run = useDebounceFn((value: string) => value.toUpperCase(), 50); hold(run); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; function hold(value: Promise<string>) { void value; } const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); hold(first); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const options = { rejectOnCancel: false }; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, options); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; String.prototype.slice = function slice() { return ''; }; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); async function later() { const first = run('aa'); run('bb'); void (await first).slice(0, 1); } void later;",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, { maxWait: 0 }); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, { maxWait: 5 }); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useThrottleFn } from '@vueuse/core'; const run = useThrottleFn((value: string) => value.toUpperCase(), 20, true, true); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); first.then((value: string) => value.slice(0, 1));",
    "import { ref } from 'vue'; import { useDebounceFn } from '@vueuse/core'; const delay = ref(50); const run = useDebounceFn((value: string) => value.toUpperCase(), delay); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn(async (value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const extra = { maxWait: 0 }; const run = useDebounceFn((value: string) => value.toUpperCase(), 50, { ...extra }); const first = run('aa'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 20); const first = run('aa'); await new Promise((resolve) => setTimeout(resolve, 100)); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 20); const slow = useDebounceFn((value: string) => value, 80); const first = run('aa'); await slow('x'); run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 20); const first = run('aa'); await first; run('bb'); void (await first).slice(0, 1);",
    "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 20); const first = run('aa'); void (await first).slice(0, 1); run('bb'); void (await first).slice(1, 2);",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.cancelled_filter_promise_demand.is_empty(),
      "safe probe must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn filter_settlement_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from("import { useDebounceFn } from '@vueuse/core';");
    for index in 0..size {
      source.push_str("const r");
      source.push_str(&index.to_string());
      source.push_str(" = useDebounceFn((value: string) => value.toUpperCase(), 50); const f");
      source.push_str(&index.to_string());
      source.push_str(" = r");
      source.push_str(&index.to_string());
      source.push_str("('aa'); r");
      source.push_str(&index.to_string());
      source.push_str("('bb'); void (await f");
      source.push_str(&index.to_string());
      source.push_str(").slice(0, 1);");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.cancelled_filter_promise_demand.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "filter-settlement work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn filter_settlement_shared_wrapper_and_alias_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const a0 = run;",
    );
    for index in 1..size {
      source.push_str("const a");
      source.push_str(&index.to_string());
      source.push_str(" = a");
      source.push_str(&(index - 1).to_string());
      source.push(';');
    }
    source.push_str("const first = a");
    source.push_str(&(size - 1).to_string());
    source.push_str("('aa'); a");
    source.push_str(&(size - 1).to_string());
    source.push_str("('bb'); void (await first).slice(0, 1);");
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.cancelled_filter_promise_demand.len(), 1, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-wrapper alias work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn filter_settlement_shared_calls_and_fanout_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [20_u64, 40, 80] {
    let mut source = String::from(
      "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50);",
    );
    for index in 0..size {
      source.push_str("const f");
      source.push_str(&index.to_string());
      source.push_str(" = run('a");
      source.push_str(&index.to_string());
      source.push_str("');");
    }
    for index in 0..size {
      source.push_str("void (await f");
      source.push_str(&index.to_string());
      source.push_str(").slice(0, 1);");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size.saturating_sub(1)).unwrap_or(usize::MAX);
    assert_eq!(contracts.cancelled_filter_promise_demand.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-calls work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
  previous = None;
  for size in [20_u64, 40, 80] {
    let mut source = String::from(
      "import { useDebounceFn } from '@vueuse/core'; const run = useDebounceFn((value: string) => value.toUpperCase(), 50); const first = run('aa'); run('bb');",
    );
    for _ in 0..size {
      source.push_str("void (await first).slice(0, 1);");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.cancelled_filter_promise_demand.len(), 1, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "fanout await/demand work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}
