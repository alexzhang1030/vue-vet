# `vue-vet/reactivity/no-empty-watch-sources`

Category: reactivity  
Default severity: warning  
Confidence: high

`watch` with an empty source list never runs usefully.

## Bad

```vue
<script setup lang="ts">
import { watch } from 'vue'
watch([], () => {})
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
watch(count, () => {})
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Pass at least one source.

## Fixtures

- Invalid: `fixtures/rules/no-empty-watch-sources/invalid/`
- Valid: `fixtures/rules/no-empty-watch-sources/valid/`
