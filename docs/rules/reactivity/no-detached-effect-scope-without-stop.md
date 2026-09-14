# `vue-vet/reactivity/no-detached-effect-scope-without-stop`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

`effectScope(true)` is detached from the current owner. If a local `const`
scope runs a synchronous `watch` / `watchEffect` on a source that outlives
the factory, and the scope is discarded without `stop`, those subscriptions
keep running after the original owner stops.

A function declaration alone does not prove the factory repeats or leaks.
The first version requires a proven repeating outer watcher callback.

## Bad

```vue
<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'

const outer = ref(0)
const inner = ref(0)
watch(outer, () => {
  const scope = effectScope(true)
  scope.run(() => {
    watch(inner, () => {}, { flush: 'sync' })
  })
}, { flush: 'sync' })
</script>
```

## Good

```vue
<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'

const inner = ref(0)
function listen() {
  const scope = effectScope(true)
  scope.run(() => {
    watch(inner, () => {}, { flush: 'sync' })
  })
  return () => scope.stop()
}
const dispose = listen()
dispose()
</script>
```

## Detection

Literal `effectScope(true)` assigned to a local `const`, a reachable
synchronous `.run` that creates an unused `watch` / `watchEffect` with a live
tracked subscription on an externally alive reactive source, and no retained
ownership of the scope. Direct `scope.stop()`, returned disposers / scopes /
handles, container stores, constructor arguments, tagged-template values,
unknown helpers, registered cleanup, capability mutation (including computed
keys, `delete`, destructuring, and loop assignment), and `getCurrentScope()`
escaping from that specific synchronous `run` (named Vue import or alias,
returned value, container capture, or a synchronous conditional capture that
only *may* escape) unprove ownership. Proven after-await lookups remain
absent and do not transfer ownership. Known unreachable lookups stay inert.
Unknown factories, empty scopes, unreachable `if (false)` runs, exhausted
ordinary `watch(..., { once: true, immediate: true })` inners, and pure
`computed` / snapshot work stay quiet.

The same discarded detached scope suppresses nested-watch findings for
watchers created inside that `run`. Independent after-await orphans stay with
`no-orphaned-scope-watcher`.

Runtime evidence: Vue 3.5.40 (`just oracle-lifetime`). Tests discard the
scope, fire the outer callback at least twice after the original owner `run`
returns, stop that owner, mutate the inner source, and assert residual
callbacks. Bounded runs are not infinite-execution claims.

## Applicability

Plain TypeScript and SFC.

## Remediation

Return a disposer that calls `scope.stop()`, store the scope and stop it from
owner cleanup, or avoid creating a detached scope per repeating callback.
