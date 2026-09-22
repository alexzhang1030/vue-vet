use super::support::*;

const IGNORABLE_POSITIVE: &str = "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core';\
const source = ref(0); const seen: number[] = [];\
const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' });\
void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });";

const SHARED_POSITIVE: &str = "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core';\
const useValue = createSharedComposable((value: string | number) => ref(value));\
const first = useValue(1); const second = useValue('text');\
void second.value.toUpperCase();";

#[test]
fn vueuse_ignorable_async_write_emits_and_controls_stay_quiet() {
  let (positive, _) = contract_stats(IGNORABLE_POSITIVE);
  assert_eq!(positive.ignorable_async_ignore_window.len(), 1, "{positive:?}");
  let (nested, _) = contract_stats(
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core';\
     const source = ref(0); const seen: number[] = [];\
     const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' });\
     void ignoreUpdates(async () => { await Promise.resolve(); ignoreUpdates(() => { source.value = 2; }); });",
  );
  assert!(nested.ignorable_async_ignore_window.is_empty(), "{nested:?}");
  let (same, _) = contract_stats(
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core';\
     const source = ref(0); const seen: number[] = [];\
     const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' });\
     void ignoreUpdates(async () => { await Promise.resolve(); source.value = 0; });",
  );
  assert!(same.ignorable_async_ignore_window.is_empty(), "{same:?}");
  let (immediate, _) = contract_stats(
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core';\
     const source = ref(0); const seen: number[] = [];\
     const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync', immediate: true });\
     void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
  );
  assert_eq!(immediate.ignorable_async_ignore_window.len(), 1, "{immediate:?}");
  let (once, _) = contract_stats(
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core';\
     const source = ref(0); const seen: number[] = [];\
     const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync', once: true });\
     void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
  );
  assert_eq!(once.ignorable_async_ignore_window.len(), 1, "{once:?}");
}

#[test]
fn vueuse_shared_incompatible_demand_emits_and_numeric_seeds_stay_quiet() {
  let (positive, _) = contract_stats(SHARED_POSITIVE);
  assert_eq!(positive.shared_composable_first_instance_args.len(), 1, "{positive:?}");
  let (numbers, _) = contract_stats(
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core';\
     const useValue = createSharedComposable((value: number) => ref(value));\
     const first = useValue(1); const second = useValue(2); first.value = 3;\
     void second.value.toFixed(0);",
  );
  assert!(numbers.shared_composable_first_instance_args.is_empty(), "{numbers:?}");
  let (global, _) = contract_stats(
    "import { ref } from 'vue'; import { createGlobalState } from '@vueuse/core';\
     const useValue = createGlobalState((value: string | number) => ref(value));\
     const first = useValue(1); const second = useValue('text');\
     void second.value.toUpperCase();",
  );
  assert_eq!(global.shared_composable_first_instance_args.len(), 1, "{global:?}");
  let (nullish, _) = contract_stats(
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core';\
     const useValue = createSharedComposable((value: string | null) => ref(value));\
     const first = useValue(null); const second = useValue('text');\
     void second.value.toUpperCase();",
  );
  assert_eq!(nullish.shared_composable_first_instance_args.len(), 1, "{nullish:?}");
}

#[test]
fn vueuse_aliases_namespace_and_shared_package() {
  let (alias, _) = contract_stats(
    "import { ref } from 'vue'; import { watchIgnorable as ignorable } from '@vueuse/core';\
     const source = ref(0); const seen: number[] = [];\
     const { ignoreUpdates } = ignorable(source, (value) => { seen.push(value); }, { flush: 'sync' });\
     void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
  );
  assert_eq!(alias.ignorable_async_ignore_window.len(), 1, "{alias:?}");
  let (shared_pkg, _) = contract_stats(
    "import { ref } from 'vue'; import { ignorableWatch } from '@vueuse/shared';\
     const source = ref(0); const seen: number[] = [];\
     const { ignoreUpdates } = ignorableWatch(source, (value) => { seen.push(value); }, { flush: 'sync' });\
     void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
  );
  assert_eq!(shared_pkg.ignorable_async_ignore_window.len(), 1, "{shared_pkg:?}");
  let (namespace, _) = contract_stats(
    "import { ref } from 'vue'; import * as VueUse from '@vueuse/core';\
     const useValue = VueUse.createSharedComposable((value: string | number) => ref(value));\
     const first = useValue(1); const second = useValue('text');\
     void second.value.toUpperCase();",
  );
  assert_eq!(namespace.shared_composable_first_instance_args.len(), 1, "{namespace:?}");
}

#[test]
fn vueuse_review_safe_probes_stay_quiet() {
  for source in [
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const other = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); void ignoreUpdates(async () => { await Promise.resolve(); other.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'pre' }); void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }); void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync', eventFilter: (invoke) => invoke() }); void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); void ignoreUpdates(async () => { await Promise.resolve(); const later = () => { source.value = 2; }; void later; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates, stop } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); stop(); void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const first = useValue(1); const second = useValue('text'); if (typeof second.value === 'string') void second.value.toUpperCase();",
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const first = useValue(1); const second = useValue('text'); void second.value?.toUpperCase();",
    "import { effectScope, ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const firstOwner = effectScope(); firstOwner.run(() => useValue(1)); firstOwner.stop(); const secondOwner = effectScope(); const second = secondOwner.run(() => useValue('text')); void second?.value.toUpperCase();",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); source.value = 2; void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); source.value += 2; void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); void ignoreUpdates(async () => { source.value = Number('2'); await Promise.resolve(); source.value = 2; });",
    "import { ref, watch } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const trigger = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); watch(trigger, () => { source.value = 2; }, { flush: 'sync' }); trigger.value = 1; void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync', immediate: true, once: true }); void ignoreUpdates(async () => { await Promise.resolve(); source.value = 2; });",
    "import { ref } from 'vue'; import { watchIgnorable } from '@vueuse/core'; const source = ref(0); const seen: number[] = []; const { ignoreUpdates, stop } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' }); void ignoreUpdates(async () => { await Promise.resolve(); stop(); source.value = 2; });",
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const seed = String(Date.now()); const zero = useValue(seed); const first = useValue(1); const second = useValue('text'); void second.value.toUpperCase();",
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const args: [string] = ['seed']; const zero = useValue(...args); const first = useValue(1); const second = useValue('text'); void second.value.toUpperCase();",
    "import { effectScope, ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const seeded = useValue('seed'); const owner = effectScope(); owner.run(() => { const first = useValue(1); const second = useValue('text'); void second.value.toUpperCase(); });",
    "import { ref } from 'vue'; import { createGlobalState } from '@vueuse/core'; const useValue = createGlobalState((value: string | number) => ref(value)); const seeded = useValue('seed'); function load() { const first = useValue(1); const second = useValue('text'); void second.value.toUpperCase(); } load();",
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const first = useValue('text'); const second = useValue(1); const third = useValue('text'); void third.value.toUpperCase();",
    "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core'; const useValue = createSharedComposable((value: string | number) => ref(value)); const first = useValue(1); const second = useValue('text'); const third = useValue(2); third.value = 'fixed'; void second.value.toUpperCase();",
  ] {
    let (facts, _) = contract_stats(source);
    assert!(
      facts.ignorable_async_ignore_window.is_empty()
        && facts.shared_composable_first_instance_args.is_empty(),
      "safe probe must stay quiet: {source} => {facts:?}"
    );
  }
}

#[test]
fn vueuse_demand_growth_stays_subquadratic() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [16_u64, 32, 64] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { watchIgnorable, createSharedComposable } from '@vueuse/core';",
    );
    source.push_str("const source = ref(0); const seen: number[] = [];");
    for index in 0..size {
      source.push_str("const { ignoreUpdates: skip");
      source.push_str(&index.to_string());
      source.push_str(
        " } = watchIgnorable(source, (value) => { seen.push(value); }, { flush: 'sync' });",
      );
      source.push_str("void skip");
      source.push_str(&index.to_string());
      source.push_str("(async () => { await Promise.resolve(); source.value = ");
      source.push_str(&(index + 1).to_string());
      source.push_str("; });");
      source.push_str("const use");
      source.push_str(&index.to_string());
      source.push_str(" = createSharedComposable((value: string | number) => ref(value)); const a");
      source.push_str(&index.to_string());
      source.push_str(" = use");
      source.push_str(&index.to_string());
      source.push_str("(1); const b");
      source.push_str(&index.to_string());
      source.push_str(" = use");
      source.push_str(&index.to_string());
      source.push_str("('text'); void b");
      source.push_str(&index.to_string());
      source.push_str(".value.toUpperCase();");
    }
    let (contracts, work) = contract_stats(&source);
    let expected = usize::try_from(size).unwrap_or(usize::MAX);
    assert_eq!(contracts.ignorable_async_ignore_window.len(), expected, "{contracts:?}");
    assert_eq!(contracts.shared_composable_first_instance_args.len(), expected, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 2);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(30),
        "vueuse demand work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}

#[test]
fn vueuse_demand_one_wrapper_growth_stays_linear() {
  let mut previous: Option<(u64, u64)> = None;
  for size in [20_u64, 80] {
    let mut source = String::from(
      "import { ref } from 'vue'; import { createSharedComposable } from '@vueuse/core';\
       const useValue = createSharedComposable((value: string | number) => ref(value));",
    );
    for index in 0..size {
      source.push_str("const item");
      source.push_str(&index.to_string());
      if index % 2 == 0 {
        source.push_str(" = useValue(1); void item");
        source.push_str(&index.to_string());
        source.push_str(".value.toFixed(0);");
      } else {
        source.push_str(" = useValue('text'); void item");
        source.push_str(&index.to_string());
        source.push_str(".value.toUpperCase();");
      }
    }
    let (contracts, work) = contract_stats(&source);
    assert_eq!(contracts.shared_composable_first_instance_args.len(), 1, "{contracts:?}");
    if let Some((prev_size, prev_work)) = previous {
      assert_eq!(size, prev_size * 4);
      assert!(
        work.saturating_mul(10) < prev_work.saturating_mul(90),
        "one-wrapper vueuse demand work grew from {prev_work} to {work} on {prev_size}->{size}"
      );
    }
    previous = Some((size, work));
  }
}
