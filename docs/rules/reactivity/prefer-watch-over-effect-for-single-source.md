# `vue-vet/reactivity/prefer-watch-over-effect-for-single-source`

Category: reactivity  
Default severity: warning  
Confidence: high

An assignment-only `watchEffect` / `watchPostEffect` / `watchSyncEffect` that tracks a single unconditional source is clearer as `watch` with an explicit source.

Default `watch` is **lazy**. If `source` starts at a non-zero value, the callback does not run until `source` changes. `{ immediate: true }` runs that first callback **synchronously on setup**, including when `flush` is `'post'`. `watchPostEffect` still defers its first run until after the current tick. Vue 3.5.40 event order for `watch(source, cb, { immediate: true, flush: 'post' })` beside `watchPostEffect`: `beforeTick = [watch]`, `afterTick = [watch, effect]`. Later invalidations share `'post'` flush; only the first-run timing differs. Keep `watchPostEffect` when that first run must stay post-flush.

`watchEffect` first-runs in the current flush (`'pre'`). `{ immediate: true }` (default `'pre'`) preserves that first run. `watchSyncEffect` first-runs synchronously; `{ immediate: true, flush: 'sync' }` preserves that first-run timing.

Do not convert a self-write. `watch(source, cb, { immediate: true, flush: 'sync' })` that writes `source` retriggers (Vue 3.5.40 control: bounded `count++` hits three runs). The rule stays quiet when the tracked source is also written, including `const alias = count` then `alias.value = count.value + 1`, chained aliases, and the same pattern when ordinary `<script>` and `<script setup>` both bind `count`. When follow coverage is incomplete (`unknown_calls` / `uncertain_accesses` / truncated follow), the rule abstains and does not claim the effect “only tracks” one source. Help always uses a getter: `watch(() => count.value, …)` — never `watch(count.value, …)`.

## Bad

```vue
<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const source = ref(0)
const out = ref(0)
watchEffect(() => {
  out.value = source.value
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref(0)
const out = ref(0)
watch(source, (value) => {
  out.value = value
}, { immediate: true })
</script>
```

A `watchPostEffect` conversion that must keep the first run after the current tick stays as `watchPostEffect`. `watchSyncEffect` conversions that should keep sync first-run use `{ immediate: true, flush: 'sync' }`.

Quiet (must not report): `fixtures/rules/prefer-watch-over-effect-for-single-source/valid/self-write.vue`, `…/valid/alias-self-write.vue`, `…/valid/dual-script-alias.vue`, and `…/valid/incomplete-external.vue`. True positive: `…/invalid/single-source.vue` (`target.value = count.value`).

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Prefer `watch(() => source.value, …, { immediate: true })` for `watchEffect`. Keep `watchPostEffect` when the first run must stay post-flush. Use `{ immediate: true, flush: 'sync' }` when replacing `watchSyncEffect`.

## Fixtures

- Invalid: `fixtures/rules/prefer-watch-over-effect-for-single-source/invalid/`
- Valid: `fixtures/rules/prefer-watch-over-effect-for-single-source/valid/`
