# `vue-vet/reactivity/no-nested-watch-without-cleanup`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

A repeating `watch` / `watchEffect` callback that creates another watcher and
discards the inner stop handle leaves that inner subscription alive after the
outer callback (and its original active scope) has ended. Vue does not stop
the inner watcher when the outer one stops. One rule id covers watcher API
variants.

Immediate first-run of `watchEffect` or `{ immediate: true }` may still be
owned by the surrounding component or `effectScope`. Later callbacks are the
leak.

## Bad

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'

const outer = ref(0)
const inner = ref(0)
watch(outer, () => {
  watch(inner, () => {}, { flush: 'sync' })
}, { flush: 'sync' })
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'

const outer = ref(0)
const inner = ref(0)
watch(outer, (_value, _old, onCleanup) => {
  const stop = watch(inner, () => {}, { flush: 'sync' })
  onCleanup(stop)
}, { flush: 'sync' })
</script>
```

## Detection

Oxc-backed lifetime facts shared with `no-orphaned-scope-watcher`. The inner
call must be a local expression-statement or `void` discard. Outer callback
repeatability and inner subscription retention are separate facts. The outer
must be a proven Vue watcher whose source *result* can still change (a `ref` /
proxy, a computed whose getter is not a proven constant, or a getter that
returns a changing value — including `() => source.value` when `source` is a
constant computed, which stays stable). `once` / `scheduler` options are known
and repeatable under the exact API (`once` is ignored on the effect family).
Stop proofs require same-block later `stop()` through only literals or proven
fresh plain Vue `ref.value` reads. `ref(existingRef)` and `ref(customRef(...))`
preserve the existing getter, so they cannot prove an early stop. Arbitrary
getters, proxies, and coercions also cannot. The inner source must still
subscribe after that callback: classified tracked operand roles (assignment
RHS, *activated* defaults, and computed keys are reads; a proven defined
object/array value skips its default; unknown default activation leaves the
read unproven; `delete inner.value` is a delete; compound assignment / update
still read) on an actual execution prefix (a preceding `return` is dead).
Unknown evaluation forms stay unknown. A getter result is Changing only after
an eligible synchronous subscribing read; after-await / async Promise results
stay Unknown. Computed getter graphs mark a node Visiting before following an
edge: a cycle is Unknown, completed results stay memoized, and acyclic chains
are depth-bounded. A constant `computed(() => 1)` used as an *inner* source
still retains subscribers under Vue 3.5.40 (`inner.dep.sc === 2`) even when
residual callback counts stay 0.

Scope ownership is per invocation: a function may run under `scope.run` and
later as a repeating watcher callback. Proven `scope.on()` / `scope.off()`
intervals, reachable `scope.run` re-entry, retained / returned / stored /
passed handles, escaped `getCurrentScope()` from a proven synchronous `run`
(a synchronous conditional capture is `MayEscape` and leaves owner proof
incomplete; after-await `getCurrentScope()` is `undefined` and stays with
`no-orphaned-scope-watcher`),
unknown helpers, unknown options, exhausted ordinary `watch(..., { once: true,
immediate: true })` inners, one-time factories, branched or unreachable
creation, write-only / guarded / type-only / after-await / dead reads, and
local sources stay quiet. Inherited option properties (`{ __proto__: { once:
true } }`) do not exhaust. `return watch(...)` from a watch callback stays
with `no-returned-watcher-cleanup`. After-await orphans stay with
`no-orphaned-scope-watcher`. Nested watchers inside a reported detached
`effectScope(true)` stay with `no-detached-effect-scope-without-stop`.

Runtime evidence: Vue 3.5.40 (`just oracle-all`). Tests discard the inner
handle, fire the outer callback at least twice after the original `scope.run`
returns, stop that owner, mutate the inner source, and assert residual
callbacks. Bounded runs are not infinite-execution claims.

## Applicability

Plain TypeScript and SFC.

## Remediation

Keep the inner stop handle and register it with `onCleanup` / `onWatcherCleanup`,
or create the inner watcher inside a live `scope.run` that you still own.
