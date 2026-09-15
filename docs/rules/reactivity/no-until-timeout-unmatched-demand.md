# `vue-vet/reactivity/no-until-timeout-unmatched-demand`

Category: reactivity  
Default severity: warning  
Confidence: high

`until(source).toBe(expected, { timeout })` races a watcher against
`promiseTimeout`. VueUse treats any finite `timeout` including `0` and
negatives as a real timer (`timeout != null`). Default `throwOnTimeout:
false` fulfills the **current unmatched value** when the timer wins. A
later method that belongs to the expected kind then throws.

## Bad

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { until } from '@vueuse/core'
const source = ref(0)
const value = await until(source).toBe('ready', { timeout: 5 })
value.toUpperCase()
value?.toUpperCase()
</script>
```

`0?.toUpperCase()` still throws: optional chaining only skips
`null`/`undefined`, not other primitives.

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { until } from '@vueuse/core'
const source = ref(0)
const value = await until(source).toBe('ready', { timeout: 5 })
value.toFixed(1)
const matched = ref('ready')
;(await until(matched).toBe('ready', { timeout: 5 })).toUpperCase()
await until(ref(0)).toBe('ready', { timeout: 5, throwOnTimeout: true })
const nullish = ref(null)
;(await until(nullish).toBe('ready', { timeout: 5 }))?.toUpperCase()
</script>
```

Initial or intervening same-block match, supported current-value
consumers, result guards (`typeof`, truthiness, `||` fallback, early
return / throw), optional chains **only when the unmatched value is
provably `null`/`undefined`**, `throwOnTimeout: true`, a `try`/`catch`
that handles the demand `TypeError`, source escape / helper / timer
writers, callbacks with unknown timing, dynamic comparison / duration /
options, custom wrappers, and a different execution owner stay quiet.

A write inside `if` / `for` before `until` does not prove the current
value. Compound, update, and destructuring writes to `.value` (`||=`,
`+=`, `++`, `[source.value] = …`) leave the source unknown. Reassigning
the await result before the demand also stays quiet.

## Detection

Fact-driven via Vue Vet source-contract facts. Requires exact
`@vueuse/core` or `@vueuse/shared` `until(ref).toBe(expected)` (named
import; VueUse `until` is not auto-imported), a closed ordinary local
primitive or nullish `ref` / `shallowRef`, a literal unmatched expected
value, a finite timeout (`0` and negatives included), effective
`throwOnTimeout: false`, and a proved unmatched source value throughout
the await. Only straight-line writes in the same block as `until` prove
that value. The fulfilled timeout result then reaches an unguarded method
demand valid for expected and invalid for the timeout kind. Main span is
the failing demand.

## Remediation

Consume the timeout kind, omit the timeout, or set `throwOnTimeout: true`
and handle rejection. Assigning the expected value before the timer is a
successful wait, not a timeout fallback.

## Limitations

Out of scope for now (runtime can still throw; the rule stays quiet):

- `toBeTruthy` (including a `ref(null)` object followed by a member demand)
- `.not`, `changed` / `changedTimes`, `toMatch`, `toContains`
- getter / `computed` / `reactive` getter sources
- `flush: 'pre'`, `deep: true`
- `??` fallback (`0 ?? 'x'` is still `0`)
- `substr` and other methods missing from the native-kind lists
- VueUse `until` via `unplugin-auto-import` (no named import)

`try { await until(...); demand } catch {}` is **handled**, not ignored:
the demand `TypeError` is caught, so silence is correct.

## Fixtures

- Invalid: `fixtures/rules/no-until-timeout-unmatched-demand/invalid/`
- Valid: `fixtures/rules/no-until-timeout-unmatched-demand/valid/`
