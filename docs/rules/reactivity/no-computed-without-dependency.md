# `vue-vet/reactivity/no-computed-without-dependency`

Category: reactivity  
Default severity: warning  
Confidence: high

A `computed` that never reads reactive state is just a static value.

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

When the getter has `.value` / `unref` / `toValue` on names that could not be
classified, the finding is still reported and marked `(maybe: …)` — analysis ran,
but those accesses were not proven reactive (under-approx miss, not silence).

## Remediation

Return a plain value, or read reactive state inside the getter.

## Fixtures

- Invalid: `fixtures/rules/no-computed-without-dependency/invalid/`
- Valid: `fixtures/rules/no-computed-without-dependency/valid/`
