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

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Return a plain value, or read reactive state inside the getter.

## Fixtures

- Invalid: `fixtures/rules/no-computed-without-dependency/invalid/`
- Valid: `fixtures/rules/no-computed-without-dependency/valid/`
