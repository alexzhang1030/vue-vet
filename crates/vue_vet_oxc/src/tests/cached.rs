use super::support::*;

#[test]
#[expect(clippy::panic, reason = "missing memoize fact must fail the unit test")]
fn cached_result_memoize_and_controlled_emit_stale_demand() {
  let (memo, _) = contract_stats(
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve().toUpperCase();",
  );
  assert_eq!(memo.memoize_stale_result_demand.len(), 1, "{memo:?}");
  let Some(site) = memo.memoize_stale_result_demand.first() else {
    panic!("memoize stale demand missing");
  };
  assert_eq!(site.member, "toUpperCase", "{memo:?}");
  assert_eq!(site.cached_kind, vue_vet_core::PrimitiveValueKind::Number, "{memo:?}");
  assert_eq!(site.current_kind, vue_vet_core::PrimitiveValueKind::String, "{memo:?}");
  let (controlled, _) = contract_stats(
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; const revision = ref(0); const source = ref(1); const value = computedWithControl(revision, () => source.value); void value.value; source.value = 'text'; value.value.toUpperCase();",
  );
  assert_eq!(controlled.controlled_computed_stale_result_demand.len(), 1, "{controlled:?}");
  let (wrapped, _) = contract_stats(
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; (resolve() as string).toUpperCase();",
  );
  assert_eq!(wrapped.memoize_stale_result_demand.len(), 1, "{wrapped:?}");
  let (alias, _) = contract_stats(
    "import { ref } from 'vue'; import { controlledComputed } from '@vueuse/core'; const revision = ref(0); const source = ref(1); const value = controlledComputed(revision, () => source.value); void value.value; source.value = 'text'; value.value.toUpperCase();",
  );
  assert_eq!(alias.controlled_computed_stale_result_demand.len(), 1, "{alias:?}");
}

#[test]
fn cached_result_required_controls_stay_quiet() {
  for source in [
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); source.value = 'text'; resolve().toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve(1).toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 2; resolve().toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve().toString();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve()?.toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve.load(); resolve().toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve.delete(); resolve().toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve.clear(); resolve().toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value, { getKey: () => source.value }); resolve(); source.value = 'text'; resolve().toUpperCase();",
    "import { ref } from 'vue'; const useMemoize = (resolver: () => unknown) => resolver; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve().toUpperCase();",
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; const revision = ref(0); const source = ref(1); const value = computedWithControl(revision, () => source.value); source.value = 'text'; value.value.toUpperCase();",
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; const revision = ref(0); const source = ref(1); const value = computedWithControl([revision, source], () => source.value); void value.value; source.value = 'text'; value.value.toUpperCase();",
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; const revision = ref(0); const source = ref(1); const value = computedWithControl(revision, () => source.value); void value.value; source.value = 'text'; value.trigger(); value.value.toUpperCase();",
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; const revision = ref(0); const source = ref(1); const value = computedWithControl(revision, () => source.value); void value.value; source.value = 'text'; revision.value++; value.value.toUpperCase();",
    "import { computed, ref } from 'vue'; const source = ref(1); const value = computed(() => source.value); void value.value; source.value = 'text'; value.value.toUpperCase();",
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; const revision = ref(0); const source = ref(1); const value = computedWithControl(revision, () => source.value, { flush: 'sync' }); void value.value; source.value = 'text'; value.value.toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/shared'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text'; resolve().toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref('initial'); const resolve = useMemoize(() => source.value); resolve(); source.value = 1; resolve(); source.value = 'latest'; resolve().toUpperCase();",
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; const revision = ref(0); const source = ref('initial'); const value = computedWithControl(revision, () => source.value); void value.value; source.value = 1; void value.value; source.value = 'latest'; value.value.toUpperCase();",
    "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; Number.prototype.toUpperCase = function () { return 'supported' }; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'latest'; resolve().toUpperCase();",
    "import { ref } from 'vue'; import { computedWithControl } from '@vueuse/shared'; Number['prototype']['toUpperCase'] = function () { return 'supported' }; const revision = ref(0); const source = ref(1); const value = computedWithControl(revision, () => source.value); void value.value; source.value = 'latest'; value.value.toUpperCase();",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.memoize_stale_result_demand.is_empty()
        && facts.controlled_computed_stale_result_demand.is_empty(),
      "cached-result control must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn cached_result_independent_producers_grow_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [32_u64, 64, 128] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; import { computedWithControl } from '@vueuse/shared';",
    );
    for index in 0..size {
      source.push_str("const s");
      source.push_str(&index.to_string());
      source.push_str(" = ref(1); const r");
      source.push_str(&index.to_string());
      source.push_str(" = useMemoize(() => s");
      source.push_str(&index.to_string());
      source.push_str(".value); r");
      source.push_str(&index.to_string());
      source.push_str("(); s");
      source.push_str(&index.to_string());
      source.push_str(".value = 'text'; r");
      source.push_str(&index.to_string());
      source.push_str("().toUpperCase();");
      source.push_str("const rev");
      source.push_str(&index.to_string());
      source.push_str(" = ref(0); const cs");
      source.push_str(&index.to_string());
      source.push_str(" = ref(1); const v");
      source.push_str(&index.to_string());
      source.push_str(" = computedWithControl(rev");
      source.push_str(&index.to_string());
      source.push_str(", () => cs");
      source.push_str(&index.to_string());
      source.push_str(".value); void v");
      source.push_str(&index.to_string());
      source.push_str(".value; cs");
      source.push_str(&index.to_string());
      source.push_str(".value = 'text'; v");
      source.push_str(&index.to_string());
      source.push_str(".value.toUpperCase();");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.memoize_stale_result_demand.len(), expected, "{contracts:?}");
    assert_eq!(contracts.controlled_computed_stale_result_demand.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "independent cached-result work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn cached_result_shared_source_producers_grow_subquadratically() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [32_u64, 64, 128] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1);",
    );
    for index in 0..size {
      source.push_str("const r");
      source.push_str(&index.to_string());
      source.push_str(" = useMemoize(() => source.value); r");
      source.push_str(&index.to_string());
      source.push_str("();");
    }
    source.push_str("source.value = 'text';");
    for index in 0..size {
      source.push('r');
      source.push_str(&index.to_string());
      source.push_str("().toUpperCase();");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.memoize_stale_result_demand.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "shared-source cached-result work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn cached_result_many_demands_on_one_cache_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [32_u64, 64, 128] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text';",
    );
    for _ in 0..size {
      source.push_str("resolve().toUpperCase();");
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.memoize_stale_result_demand.len(), 1, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "many-demand cached-result work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn cached_result_deep_wrapper_chains_stay_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { useMemoize } from '@vueuse/core'; const source = ref(1); const resolve = useMemoize(() => source.value); resolve(); source.value = 'text';",
    );
    source.push_str("const w0 = resolve;");
    for index in 1..size {
      source.push_str("const w");
      source.push_str(&index.to_string());
      source.push_str(" = w");
      source.push_str(&(index - 1).to_string());
      source.push(';');
    }
    source.push('w');
    source.push_str(&(size - 1).to_string());
    source.push_str("().toUpperCase();");
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.memoize_stale_result_demand.len(), 1, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "wrapper-chain cached-result work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}
