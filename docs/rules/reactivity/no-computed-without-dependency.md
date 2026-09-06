# `vue-vet/reactivity/no-computed-without-dependency`

Category: **practice** (excluded from score and default CI exit)

Default severity: info

Confidence: high

The stable rule id remains `vue-vet/reactivity/no-computed-without-dependency` for configuration
and suppression compatibility.

A `computed` that never reads reactive state is a static wrapper. That is often
intentional: a `Ref` contract argument, an SSR branch, or a placeholder getter.
The finding is an API-style preference. Proven empty
getters still report so callers can replace them with plain values. Unclassified
member provenance still abstains.

## Bad

```vue
<script setup lang="ts">
import { computed } from 'vue'
const label = computed(() => 'static')
</script>
```

## Good

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(() => String(count.value))
</script>
```

Factory returns count too — including composables that `return ref(...)` and
external packages whose `.d.ts` declares `(): Ref<T>` (for example VueUse
`useMediaQuery`):

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
function useFlag() {
  const flag = ref(false)
  return flag
}
const isCoarsePointer = useFlag()
const hint = computed(() => (isCoarsePointer.value ? 'a' : 'b'))
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).
The tracer classifies call-return kinds (`Factory(Ref)` from body analysis or
declared `.d.ts` return types) so unknown ecosystem callees are not mistaken for
static computeds when their return is a proven ref.

Unclassified member provenance (`state.current.size` on an unknown factory
result), including the same access through a same-file zero-arg helper or
identifier getter, marks tracking coverage incomplete. Absence rules abstain;
Explain reports unclassified accesses. Proven reactive reads and plain
constant getters (`computed(() => 42)`) stay distinguishable.

## Remediation

Return a plain value, or read reactive state inside the getter.

## Fixtures

- Invalid: `fixtures/rules/no-computed-without-dependency/invalid/`
  (`placeholder.vue` static getter; `ref-contract.vue` / `ssr-ref-contract.vue`
  still report as practice when the wrapper satisfies a `Ref` argument)
- Valid: `fixtures/rules/no-computed-without-dependency/valid/`
  (`unknown-member.vue`, helper/ident-getter variants, `helper-uncertain.vue`)
