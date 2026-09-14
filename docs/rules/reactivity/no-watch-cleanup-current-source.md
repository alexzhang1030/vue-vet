# `vue-vet/reactivity/no-watch-cleanup-current-source`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

Vue runs the previous watcher cleanup **after** the source already holds the
new value and **before** the next callback. A cleanup that calls
`removeEventListener` on `source.value` therefore targets the replacement
EventTarget, and the listener acquired in that run stays on the old one.

One rule id covers `watch` with `flush: 'pre'` (default) or `'sync'`, and both
callback-bound `onCleanup` and synchronous `onWatcherCleanup`.

The default pre watcher coalesces synchronous assignments into one callback, so
two batched replacements without a scheduler boundary are **not** this defect.

## Bad

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'

const handler = () => {}
const source = ref(new EventTarget())
watch(
  source,
  (target, _prev, onCleanup) => {
    target.addEventListener('click', handler)
    onCleanup(() => {
      source.value.removeEventListener('click', handler)
    })
  },
  { immediate: true, flush: 'sync' },
)
source.value = new EventTarget()
</script>
```

A stable `const` alias of a distinct allocation is the same defect:

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'

const handler = () => {}
const initial = new EventTarget()
const source = ref(initial)
const replacement = new EventTarget()
watch(
  source,
  (target, _prev, onCleanup) => {
    target.addEventListener('click', handler)
    onCleanup(() => {
      source.value.removeEventListener('click', handler)
    })
  },
  { immediate: true, flush: 'sync' },
)
source.value = replacement
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'

const handler = () => {}
const source = ref(new EventTarget())
watch(
  source,
  (target, _prev, onCleanup) => {
    target.addEventListener('click', handler)
    onCleanup(() => {
      target.removeEventListener('click', handler)
    })
  },
  { immediate: true, flush: 'sync' },
)
source.value = new EventTarget()
</script>
```

## Detection

Oxc-backed lifetime facts. The watched source is a local `ref` / `shallowRef`
whose payload is a proven native `EventTarget` allocation (`new EventTarget()`
as a global constructor). The callback's first parameter is the run-local
target and receives native `addEventListener`. Cleanup is registered with the
bound `onCleanup` parameter or synchronous `onWatcherCleanup`. That cleanup
rereads the same source's `.value` for `removeEventListener` with the same
event, handler identity, and capture option.

A later distinct EventTarget **allocation** must execute while the watcher is
still active, after a proven acquisition. Source value transitions and callback
acquisitions are separate events:

- `{ immediate: true }` with `flush: 'sync'`: acquisition is the value at the
  watch call; the first later distinct write replaces it.
- `{ immediate: true }` with queued `pre` / `post`: acquisition is still the
  watch-call value. A proven `await nextTick()` uses the settled identity at
  that boundary, so `initial → fresh → initial` in one batch is not a
  replacement. Without a boundary, one later distinct write still replaces;
  extra unsettled writes stay unknown.
- `flush: 'sync'` without immediate: only a write that changes the registered
  value acquires; the next distinct write replaces it. Writing the original
  allocation back first is not an acquisition.
- `flush: 'pre'` / `'post'` without immediate: acquisition is a settled value
  different from the registered identity at a proven `await nextTick()`
  boundary; a later distinct write after that boundary replaces it.

Watch creation must be execution-proven in its owner lane. Const handle aliases
are canonicalized before `stop` / `pause` / `resume` / escape queries.
Conditional, deferred, reentered, or mutated handles stay unknown. An earlier
conditional or otherwise uncertain stop is an ordered lifetime boundary: a
later definite `stop()` / `.stop()` does not prove the watcher stayed active
across that use. Written payload aliases are Unknown: Oxc semantic write roles are indexed
before allocation and source-write collection, covering plain, logical, and
compound assignment, destructuring, and every execution owner. Stable `const`
aliases keep allocation provenance, so `const replacement = new EventTarget();
source.value = replacement` still reports. Method mutation or generic escape through a written
receiver alias uses the native-capability Unknown boundary. Native-payload
seeds and identifier-flow edges are collected from declarations and
assignments, then escapes are resolved after the identity index is complete,
so assignment, copy, and later-declaration forms share one proof. Native method and escape checks apply to the
**acquired** allocation, not only the initializer. `null` / `undefined`
listener arguments create no listener; unknown objects stay quiet; proven
functions and `handleEvent` objects remain supported. Release aggregation
builds each source symbol's write summary once from semantic reference roles
and reuses it.

The diagnostic span is the wrong release receiver. Help points at the
acquisition and the replacement.

Native `EventTarget` is a baseline intrinsic: an unresolved global constructor
used as `new EventTarget()`. Import names, local bindings, global/prototype
replacement, method mutation, helper/constructor/tag/pattern/dynamic escapes,
and `addEventListener` / `removeEventListener` mutation make identity
unproven. A matching captured-target `removeEventListener` in registered
cleanup discharges that acquisition. A source read alone (logging, state
checks) stays quiet.

## Applicability

Vue SFCs, ordinary script, and plain JS/TS, plus const aliases, only where
semantic evidence covers them. Shadowed Vue imports, unknown helpers, unknown
watch options, escaped or paused handles, `once: true`, spreads, computed keys,
control-flow ordering, `once` / `AbortSignal` listener options, shared-let
cleanup of the preceding target, same-target writeback, uninvoked writes,
batched default-pre assignments, same-value first sync writes, queued
round-trip restoration, inactive watch creation, const stop-handle aliases,
mutated later acquisitions, null/undefined listeners, written payload aliases
(including rebound, logical, later-owner, conditional, uninvoked, and
destructured assignments), method mutation or generic escape through a written
receiver alias, assignment or copy of a native payload that later escapes,
escape observed before a later declaration, and earlier conditional stops
before a later definite stop stay quiet. A distinct `const`
EventTarget alias, including a const chain, assigned into the source still
reports.

## Remediation

Capture the run-local target (the callback argument or a local copy) and call
`removeEventListener` on that identity. Do not reread the watched source in
cleanup.
