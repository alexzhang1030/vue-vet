# Prefer a stable computed identity when primitive contents stay equal

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

A `computed` getter that returns a fresh object or array is a new identity even when every selected primitive is Object.is-equal to the last result. Vue notifies identity consumers with Object.is on the getter result, so a later `watch` or already-activated computed still repeats work.

Vue 3.4+ `computed` getters receive the previous value. Every reactive read still happens first; only the returned identity can be reused when the projected primitive contents match.

## Bad

```vue
<script setup>
import { computed, ref, watch } from 'vue'

const items = ref([1, 2])
const doubled = computed(() => items.value.map((n) => n * 2))
watch(doubled, (value) => {
  render(value)
})
items.value = [1, 2]
</script>
```

Replacing `[1, 2]` with another `[1, 2]` keeps the projected contents `[2, 4]` but allocates a new array. The watcher callback runs again.

## Good

```vue
<script setup>
import { computed, ref, watch } from 'vue'

const items = ref([1, 2])
const doubled = computed((previous) => {
  const next = items.value.map((n) => n * 2)
  if (
    previous
    && previous.length === next.length
    && previous.every((value, index) => Object.is(value, next[index]))
  ) {
    return previous
  }
  return next
})
watch(doubled, (value) => {
  render(value)
})
items.value = [1, 2]
</script>
```

Returning a primitive from `computed` is already identity-stable for Object.is, so it stays quiet.

## Detection

Reports only when every part of the opportunity is proven:

1. A resolved Vue `computed` bound to a local `const`
2. The getter returns a fresh own-data array or object of primitive projections, using a closed grammar (`map` / `filter` / `slice` / `concat` / `toSorted`, or an object literal of `source.value.field` reads)
3. Actual local primitive values at the source in force when the consumer is populated (last proven same-owner write before that site, not the declaration payload alone) and at a later same-owner fresh replacement, with equivalent projected contents under Object.is (including `NaN` and signed zero). JavaScript `===` is not Object.is; unsupported string relational filters stay Unknown.
4. An established identity consumer — `watch(derived)` / `watch(() => derived.value)`, or another computed that reads `derived.value` — before that replacement, in the same straight-line owner. A downstream computed also needs a later reachable demand after the replacement; a `pre`/`post` watcher that is stopped or paused after the replacement before delivery stays quiet. Unknown delivery boundaries stay quiet.

Arbitrary object-literal getters without a proven equal-content source replacement stay quiet.

## Limitations

This is a measured work-reduction opportunity, not a correctness warning and not an automatic rewrite. A consumer that needs every new identity (forcing a `key`, logging allocations, comparing old/new by reference) can keep that behavior.

The getter still runs on the source change; previous-value reuse only stabilizes the result identity after those reactive reads. Downstream **value** outputs can stay equal while compute/callback work still repeats — extra watch callbacks are not the same claim as “the projected values changed.”

Quiet or unknown when any of these hold: Vue older than 3.4 or an unset package version; previous-value reuse already present; primitive computed outputs; same-value / same-reference source writes; changed projected contents; no established consumer; consumer created after the replacement; a lazy downstream computed with no later demand after the replacement; a `pre`/`post` watcher stopped or paused after the replacement before delivery; `deep` / `once` / custom `equals`; getter side effects, early return, `await`, or generator; source or result mutation/escape; a prior source replacement that changes the populated baseline; local bindings that shadow `undefined` / `NaN` / `Infinity`; string relational filters; readonly or native-capability uncertainty; unsupported operations or shapes.

`flush: 'sync' | 'pre' | 'post'` is coverage of the same identity contract, not a separate reason.

## Remediation

After collecting every reactive dependency, compare the new primitive projection with the previous computed value using Object.is (or an equivalent length + per-element check) and return the previous identity when contents match. Do not apply a mechanical rewrite; keep a new identity when the consumer actually needs it.

## Fixtures

- Invalid: `fixtures/rules/prefer-stable-computed-identity/invalid/`
- Valid: `fixtures/rules/prefer-stable-computed-identity/valid/`
