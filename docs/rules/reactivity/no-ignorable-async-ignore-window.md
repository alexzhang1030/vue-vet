# `vue-vet/reactivity/no-ignorable-async-ignore-window`

Category: reactivity  
Default severity: warning  
Confidence: high

`watchIgnorable` / `ignorableWatch` (`@vueuse/core` and `@vueuse/shared`)
open a **synchronous** ignore window. `ignoreUpdates(updater)` runs
`updater()` without awaiting it. A changed write to the watched source after
a reachable `await` therefore notifies an active callback.

## Bad

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { watchIgnorable } from '@vueuse/core'
const source = ref(0)
const seen: number[] = []
const { ignoreUpdates } = watchIgnorable(source, (value) => {
  seen.push(value)
}, { flush: 'sync' })
void ignoreUpdates(async () => {
  await Promise.resolve()
  source.value = 2
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { watchIgnorable } from '@vueuse/core'
const source = ref(0)
const seen: number[] = []
const { ignoreUpdates } = watchIgnorable(source, (value) => {
  seen.push(value)
}, { flush: 'sync' })
void ignoreUpdates(async () => {
  await Promise.resolve()
  ignoreUpdates(() => {
    source.value = 2
  })
})
</script>
```

## Safe (stays quiet)

Named aliases, `import * as VueUse`, `ignorableWatch`, and `@vueuse/shared`
use the same owner. Same-value writes, writes to a different source, a
custom `eventFilter`, a nested `ignoreUpdates(() => { ... })` around the
write, and a straight `stop()` before the ignore call or before the
post-await write stay quiet. `{ flush: 'sync', once: true, immediate: true }`
is already consumed at registration. An outside, compound, non-literal, or
other-callback write that makes the post-await assignment unproven also
stays quiet.

## Not claimed by this owner (quiet, still leaks at runtime)

Default (`pre`) flush — the common `watchIgnorable` usage — ends the ignore
window synchronously the same way `flush: 'sync'` does. So do `post` flush,
reactive getter sources, a non-inline updater (including a same-file async
helper passed by reference), and a post-await write inside `forEach` or
another nested callable. Template writes such as `v-model="source"` are
invisible to the script index. These are detection limits, not safety
proofs.

## Detection

Fact-driven via Vue Vet source-contract facts. The source must be a local Vue
`ref` / `shallowRef` of a known primitive, the callback must consume its
value, and options must be a closed object whose only keys are `flush:
'sync'` plus optional `immediate` / `deep` / `once`. The previous value must
be a proven literal from the last straight-line write in that updater (or
the ref initializer when no such write exists); any write to the same ref
outside that updater abstains. Reports the first proven leaking write per
`watchIgnorable` / `ignorableWatch` wrapper. The main span is the post-await
write; related spans identify `ignoreUpdates` and the suspension.

## Remediation

Await the data first, then call `ignoreUpdates(() => { ... })` around the
write that should stay ignored.

## Fixtures

- Invalid: `fixtures/rules/no-ignorable-async-ignore-window/invalid/`
- Valid: `fixtures/rules/no-ignorable-async-ignore-window/valid/`
