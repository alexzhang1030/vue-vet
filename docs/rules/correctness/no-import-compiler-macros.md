# `vue-vet/correctness/no-import-compiler-macros`

Category: correctness  
Default severity: warning  
Confidence: high

Compiler macros (`defineProps`, `defineEmits`, …) are compiler-injected and must not be imported.

## Bad

```vue
<script setup lang="ts">
import { defineProps } from 'vue'
const props = defineProps<{ title: string }>()
</script>
```

## Good

```vue
<script setup lang="ts">
const props = defineProps<{ title: string }>()
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Delete the import; call the macro directly in `<script setup>`.

## Fixtures

- Invalid: `fixtures/rules/no-import-compiler-macros/invalid/`
- Valid: `fixtures/rules/no-import-compiler-macros/valid/`
