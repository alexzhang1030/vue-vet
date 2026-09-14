# Prefer lazy `computedAsync` when startup work is unused

Default severity: **info**. Category: **practice** (excluded from score and default CI exit). Group: **derivation**.

VueUse `computedAsync` is eager by default and starts evaluating before any
consumer reads the result. When the producer is a local pure async function
that synchronously captures a primitive source, a later source write supersedes
that startup work, and the only consumer is a read-only loading-tolerant
observer, `{ lazy: true }` skips the unused startup evaluations. The first read
then sees `initialState`, and the returned value is readonly.

## Bad

```vue
<script setup>
import { ref, watch } from 'vue'
import { computedAsync } from '@vueuse/core'

const source = ref(1)
const sink = ref(0)
const value = computedAsync(async () => source.value * 10, -1)
source.value = 3
watch(value, (current) => {
  if (current !== -1) sink.value = current
}, { immediate: true })
</script>
```

## Good

```vue
<script setup>
import { ref, watch } from 'vue'
import { computedAsync } from '@vueuse/core'

const source = ref(1)
const sink = ref(0)
const value = computedAsync(async () => source.value * 10, -1, { lazy: true })
source.value = 3
watch(value, (current) => {
  if (current !== -1) sink.value = current
}, { immediate: true })
</script>
```

Keep eager mode when a caller writes the result, needs the settled value
before first read, or relies on custom `evaluating` / `onError` / cancel
behavior. Nonliteral `lazy` options, `once` or stopped consumers, replaced
`.value` getters, and unexecuted source writes stay quiet.

## Limitations

Requires exact `@vueuse/core` `computedAsync`, known eager defaults, a locally
analyzable async producer whose result depends on a synchronously captured
primitive source, a reachable pre-demand source change, and a private
read-only loading-tolerant consumer that accepts `initialState` and then the
current result. Returned, exported, or mutable results, prefetch or capability
demand, custom `evaluating` / `onError` / cancel, unknown helper effects, and
incomplete execution provenance stay quiet. The lazy output is readonly and
remains started after its first demand. The `computedAsync` signature is
preserved. No automatic rewrite.

## Remediation

Pass `{ lazy: true }` as the third argument of `computedAsync`.

## Fixtures

- Invalid: `fixtures/rules/prefer-lazy-computed-async/invalid/`
- Valid: `fixtures/rules/prefer-lazy-computed-async/valid/`
