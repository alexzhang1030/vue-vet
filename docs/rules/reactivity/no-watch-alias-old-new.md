# `vue-vet/reactivity/no-watch-alias-old-new`

Category: reactivity  
Default severity: warning  
Confidence: high

`watch` on a proven `reactive` / `shallowReactive` root reuses the same
proxy for `new` and `old` after the initial run. A top-level
`if (next === old) return` therefore skips every later call or write that
consumes `new`.

Ref sources with `deep: true` use that comparison to filter root
replacement and stay quiet. Getter sources, `immediate: true` with
`once: true`, logging-only comparisons, member/`NaN` comparisons, defaults,
reassignment, and shadowing stay quiet. `reactive(ref(…))` and
`reactive({ __v_isRef: true, … })` follow Vue's `isRef` before `isReactive`
check, so `new`/`old` can differ; those stay quiet. Writing `__v_isRef` after
construction (including assignment-pattern / destructuring targets),
method receivers that mutate `this` (for example `tag() { this.__v_isRef = true }`,
including when that method is a tagged-template tag),
`Object.freeze` of constructor input (direct, spread, sequence, or
aggregate storage), and `__v_isReadonly` /
`__v_raw` keys stay quiet. Spread, accessor, prototype, or other Vue marker
keys on the reactive target stay unknown. Ordinary nested field writes such
as `state.n = 2` remain a reactive root. Both this rule and
`no-deep-watch-on-reactive-root` can fire on the same watch: the latter
reports source-wide tracking, and this rule reports callback identity.

## Bad

```vue
<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ n: 1 })
function accept(_value: unknown) {}
watch(state, (next, old) => {
  if (next === old) return
  accept(next)
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
const state = reactive({ n: 1 })
const source = ref({ n: 1 })
function accept(_value: unknown) {}
watch(
  () => state.n,
  (next, old) => {
    if (next === old) return
    accept(next)
  },
)
watch(
  source,
  (next, old) => {
    if (next === old) return
    accept(next)
  },
  { deep: true },
)
</script>
```

## Detection

Fact-driven via Vue Vet source-contract facts from Oxc symbol identity.
Requires actual reactive-root provenance, two unchanged simple parameters,
default or literal `immediate: false`, and a dominating identity return.

## Remediation

Watch a getter or a field, or drop the identity early-return.

## Fixtures

- Invalid: `fixtures/rules/no-watch-alias-old-new/invalid/`
- Valid: `fixtures/rules/no-watch-alias-old-new/valid/`
