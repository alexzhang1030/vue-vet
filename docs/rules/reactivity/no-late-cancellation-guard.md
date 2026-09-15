# `vue-vet/reactivity/no-late-cancellation-guard`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

A run-local cancellation flag that is registered on the callback-bound
`onCleanup` **after** a source-dependent `await` cannot observe the
invalidation that already happened during that await. Vue still runs the
cleanup for *later* invalidation, which is why this is not
`no-late-watcher-cleanup` (that rule is `onWatcherCleanup` owner loss).

Registering the same flag synchronously before `await` closes the window.

## Bad

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'

const source = ref('one')
const result = ref<string | null>(null)
watch(source, async (value, _previous, onCleanup) => {
  const data = await Promise.resolve(value)
  let cancelled = false
  onCleanup(() => {
    cancelled = true
  })
  if (!cancelled) result.value = data
})
</script>
```

If a later run finishes first, the earlier run still writes its stale result
because `cancelled` was still `false` when that await resolved.

## Good

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'

const source = ref('one')
const result = ref<string | null>(null)
watch(source, async (value, _previous, onCleanup) => {
  let cancelled = false
  onCleanup(() => {
    cancelled = true
  })
  const data = await Promise.resolve(value)
  if (!cancelled) result.value = data
})
</script>
```

Outer generation tokens, current-source equality, keyed caches, `AbortController`,
and serialized task queues are independent valid contracts and stay quiet.

## Detection

Proven Vue `watch` / `watchEffect` / `watchPostEffect` / `watchSyncEffect`
(named imports and aliases) with an async callback. The callback is
straight-line: one source-dependent `await`, a run-local `let`/`var` flag
initialized to `false`, a bound `onCleanup` (or const alias of that parameter)
whose closure assigns the flag to `true` **after** that await, and
`if (!cancelled)` writing the awaited result to a shared `ref` / `shallowRef`
`.value`. Every `true`-assigner of the flag must be a registrar cleanup
(bound `onCleanup` or Vue `onWatcherCleanup`); a sync registrar plus a
redundant late bound registration stays quiet. One analysis per callback.
The primary span is the late registration; the diagnostic also names the
await, guarded write, and flag. Closed `once: true` (and non-closed options)
do not fire — see Applicability.

## Applicability

Plain TypeScript modules and Vue SFCs. `async` callbacks with only synchronous
writes stay quiet. `watch(..., { once: true })` stays quiet: Vue wraps the
callback as `_cb(...args); watchHandle()`, so `effect.stop()` runs every
cleanup registered so far before the first `await` settles. There is never a
competing run, and moving the registration before `await` (the usual
remediation) makes `stop()` set the flag first and drop the only result.
Options that are not a closed object literal (`opts`, a spread, a computed
key, a non-literal `once`) stay Unknown — the collector cannot prove `once`
is absent. Loops, `try`/`finally`, extra `if` branches, multiple `await`s,
helper-delegated registrars, escaped flags, and uncertain source/result flow
stay Unknown. `onWatcherCleanup` after `await` remains
`no-late-watcher-cleanup`. VueUse async-state APIs are later coverage of this
owner, not extra IDs.

## Not detected

The proof is a straight-line `if (!cancelled) { sink.value = awaited }` after
the single source-dependent `await`. These shapes stay silent even when a
stale settlement can write at runtime:

- `if (cancelled) return` then an unguarded write
- more than one `await`, `try`/`finally`, loops, extra `if` branches
- array / getter-tuple sources the collector does not treat as the watched ref
- writing `source.value` (or another value) instead of the awaited result
- writing a `reactive` object field instead of a shared `ref` / `shallowRef`

## Remediation

Call the bound `onCleanup` that sets the cancellation flag before `await`, then
write the result only when the flag is still clear.
