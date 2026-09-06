# `vue-vet/reactivity/no-late-watcher-cleanup`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

`onWatcherCleanup` must run while a watcher callback is the active effect.
After a straight-line `await`, or inside a known deferred callback
(`nextTick`, `then`, `setTimeout`, …) of that watcher, Vue has no active
watcher to associate. Await and deferred forms share one rule id: the
registration contract is one premise.

Callback-bound `onCleanup` remains valid after `await` until the watcher is
invalidated, so those calls stay quiet.

## Bad

```vue
<script setup lang="ts">
import { onWatcherCleanup, ref, watchEffect } from 'vue'

const source = ref(0)
watchEffect(async () => {
  source.value
  await Promise.resolve()
  onWatcherCleanup(() => {})
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watchEffect } from 'vue'

const source = ref(0)
watchEffect(async (onCleanup) => {
  source.value
  await Promise.resolve()
  onCleanup(() => {})
})
</script>
```

## Detection

Proven Vue `onWatcherCleanup` import (including aliases) inside an inline
watch/watchEffect callback after a straight-line `await`, or inside an inline
deferred callback of that watcher. Deferred proof is limited to global
`setTimeout` / `setInterval` / `queueMicrotask` / `requestAnimationFrame`
(callback slot 0), `Promise.then` slots 0/1 and `catch`/`finally` slot 0 on a
proven native `Promise`, and semantically resolved Vue `nextTick`. Arbitrary
`.then` objects and extra `setTimeout` data arguments stay quiet.

An explicit third argument (watcher owner) or a spread stays quiet. Vue 3.5.40
associates cleanup after `await` only when the owner was captured **before**
the await:

```ts
watchEffect(async () => {
  const owner = getCurrentWatcher()
  await Promise.resolve()
  onWatcherCleanup(cleanup, false, owner)
})
```

Calling `getCurrentWatcher()` after `await` does not restore an owner
(`cleanups: 0`, runtime warning). This rule still abstains on any explicit
third argument because owner identity is not reconstructed. Literal
`failSilently: true` (second argument `true`) is treated as deliberate warning
suppression and stays quiet.

Uncertain control-flow, nested unproven callbacks, and same-name locals that
are not the Vue import stay quiet. `<script setup>` top-level await restoration
does not restore watcher-callback context across that callback's own `await`.

## Applicability

Plain TypeScript and SFC. Nested synchronous callback semantics are not
inferred without proof.

## Remediation

Register `onWatcherCleanup` synchronously, or use the `onCleanup` argument
passed into the watcher callback.
