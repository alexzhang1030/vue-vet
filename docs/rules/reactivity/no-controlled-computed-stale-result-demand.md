# `vue-vet/reactivity/no-controlled-computed-stale-result-demand`

Category: reactivity  
Default severity: warning  
Confidence: high

`computedWithControl` / `controlledComputed` from `@vueuse/shared` or
`@vueuse/core` samples unlisted inputs only when a listed source notifies or
`trigger()` runs. A populated cache can retain an earlier primitive while an
unlisted input changes kind. Demanding a native method that the cached kind
lacks throws.

## Bad

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { computedWithControl } from '@vueuse/shared'
const revision = ref(0)
const source = ref(1)
const value = computedWithControl(revision, () => source.value)
void value.value
source.value = 'text'
value.value.toUpperCase()
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { computedWithControl } from '@vueuse/shared'
const revision = ref(0)
const source = ref(1)
const value = computedWithControl([revision, source], () => source.value)
void value.value
source.value = 'text'
value.value.toUpperCase()
</script>
```

The first result read after the write, a listed-source update, an explicit
`trigger()`, a chosen commit revision change, a same-kind write, a supported
cached method, a guarded demand, a retained snapshot, and a later cache hit
that still holds the first filled kind stay quiet. Object setters, option
overrides, escaped trigger aliases, shadowed imports, constructor shadows,
native prototype repairs, and unknown helper effects stay unknown. Outer
tracking consumers need their own execution proof and stay outside this slice.

## Detection

Fact-driven via Vue Vet source-contract facts. The producer must be a proven
shared/core `computedWithControl` or `controlledComputed` with a local ref
invalidation source, a direct inline getter returning another native local
ref's primitive `.value`, and default options. Findings require a cache-filling
read, a later unlisted kind-changing write, and an unguarded native method
demand on the retained result.

## Remediation

List every reactive input, call `trigger()` at the commit that should publish,
or use Vue `computed` when each input owns invalidation.

## Fixtures

- Invalid: `fixtures/rules/no-controlled-computed-stale-result-demand/invalid/`
- Valid: `fixtures/rules/no-controlled-computed-stale-result-demand/valid/`
