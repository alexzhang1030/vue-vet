# `vue-vet/reactivity/no-cancelled-filter-promise-demand`

Category: reactivity  
Default severity: warning  
Confidence: high

`useDebounceFn` from `@vueuse/core` / `@vueuse/shared` **13.9.0** returns a
Promise for every wrapper call. A later call in the same synchronous region
cancels the pending timer. With default `rejectOnCancel: false`, the earlier
promise **fulfills** `undefined`. Demanding a native method that the callback
result would support throws.

## Bad

```vue
<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn((value: string) => value.toUpperCase(), 50)
const first = run('aa')
const latest = run('bb')
void latest
;(await first).slice(0, 1)
</script>
```

## Good

```vue
<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn((value: string) => value.toUpperCase(), 50)
const first = run('aa')
const latest = run('bb')
void (await first)
;(await latest).slice(0, 1)
</script>
```

Proven-safe patterns stay quiet: guarded or optional demands,
`{ rejectOnCancel: true }` with an explicit rejection path, zero delay,
`maxWait: 0`, sequential `await` of each call, demanding only the latest
default-debounce result, escaped wrappers/promises/options, and an `await` or
`yield` between the two wrapper calls (the first timer may fire during that
gap).

Quiet is **not** the same as safe for several shapes this slice does not
detect. Runtime (VueUse 13.9.0) still fulfills the cancelled promise with
`undefined` for `maxWait > 0` (both promises), trailing or `leading: false`
throttle, and `.then` handlers (unhandled rejection). Also not yet detected:
dynamic or missing delay, uninvoked or deferred call sites, producer at setup
with calls inside an invoked handler, loop bodies, `const delay = 50`, the
VueUse default delay 200, named-function callbacks, `Promise.all` destructure,
property reads such as `(await first).length`, and file-wide silencers
(`Object.defineProperty` anywhere, or any symbol named `String` / `Number` /
`Boolean` / `BigInt` / `Object`, including type-only declarations). Those
fixtures live under `valid/unproven-*.vue`.

Each superseded first-call promise that has an unguarded native demand is
reported (not only the earliest site per `useDebounceFn` producer).

## Detection

Fact-driven via Vue Vet source-contract facts. The producer must be proven
`useDebounceFn` from `@vueuse/core` or `@vueuse/shared` with a finite positive
literal delay, absent `maxWait`, and effective `rejectOnCancel: false`. Two
same-wrapper calls must share one reachable synchronous region with **no**
`await`/`yield` between them so the second call supersedes the first before a
timer task can run. The earlier result promise is awaited; an unguarded native
method demand then runs on the fulfilled `undefined`. Own-key / duplicate /
default option semantics follow the last own key. Relative wall-clock guesses
are not used. A `const` binding initialized to a primitive literal and never
reassigned keeps that kind when passed as an argument.

## Remediation

Demand the latest call's result, wait for each call to settle before the next
invocation, or set `rejectOnCancel: true` and handle rejection. Latest-result
advice applies only to this default-debounce option set.

## Fixtures

- Invalid: `fixtures/rules/no-cancelled-filter-promise-demand/invalid/`
- Valid: `fixtures/rules/no-cancelled-filter-promise-demand/valid/`
- Unproven quiet controls: `fixtures/rules/no-cancelled-filter-promise-demand/valid/unproven-*.vue`
