# Prefer `Map.get` over `forEach` to select one key

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

A reactive Map's `forEach` tracks iteration of the whole collection. Selecting
one stable string key with `forEach((value, key) => { if (key === 'selected') result = value })`
inside `computed` or `watchEffect` therefore reruns when an unrelated entry
changes. `map.get('selected')` tracks only that key and yields the same selected
value, including `undefined` after the key is removed.

The first slice is a canonical block getter: a fresh local scratch initialized
to `undefined`, assigned only by that `forEach` callback, then returned (computed)
or used (`watchEffect`). The callback contains only the strict-equality
selection assignment. Broader filters, projections, default accumulators,
multiple matches, object keys, side effects, and using `forEach`'s return stay
quiet. Native (non-reactive) Maps stay quiet.

## Bad

```vue
<script setup lang="ts">
import { computed, reactive } from 'vue'
const keyed = reactive(new Map([['selected', 1], ['other', 2]]))
const selected = computed(() => {
  let value
  keyed.forEach((entry, key) => {
    if (key === 'selected') value = entry
  })
  return value
})
void selected
</script>
```

## Good

```vue
<script setup lang="ts">
import { computed, reactive } from 'vue'
const keyed = reactive(new Map([['selected', 1], ['other', 2]]))
const selected = computed(() => keyed.get('selected'))
void selected
</script>
```

## Limitations

Requires a proven `reactive` / `shallowReactive` Map from `vue` / `@vue/runtime-*`
/ `@vue/reactivity` whose invoked `forEach` and proposed `get` are still the
instrumented methods, including when the wrapper is `reactive(inner)` of a
raw Map. Own, aliased, helper, computed-member, object/array/return storage,
assignment, and prototype replacements on the wrapper or the underlying
allocation stay quiet. Ordinary entry `set` /
`delete` do not. `vue-demi` and `#imports` stay unproven. Aliases of the getter
are not reported until they have the same proof.

## Remediation

Replace the iteration with `map.get('literal')` when one stable string key is
the whole query.

## Fixtures

- Invalid: `fixtures/rules/prefer-keyed-map-dependency/invalid/`
- Valid: `fixtures/rules/prefer-keyed-map-dependency/valid/`
