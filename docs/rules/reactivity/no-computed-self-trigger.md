# `vue-vet/reactivity/no-computed-self-trigger`

Category: reactivity  
Default severity: warning  
Confidence: high

A `computed` getter that writes a dependency it also reads is impure. The write **can invalidate its cached value**, so a later read of the computed may re-run the getter instead of returning a stable cache. This is a purity / cache issue, not a proven infinite loop. A tautological write such as `count.value = count.value` is still impure; do not treat it as necessarily cache-unstable on every access.

`no-side-effects-in-computed` still reports the write as a side effect. Keep both.

## Bad

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(() => {
  count.value++
  return count.value
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(() => count.value)
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Keep computed getters pure; move the write to `watch` or an event handler.

## Fixtures

- Invalid: `fixtures/rules/no-computed-self-trigger/invalid/`
- Valid: `fixtures/rules/no-computed-self-trigger/valid/`
