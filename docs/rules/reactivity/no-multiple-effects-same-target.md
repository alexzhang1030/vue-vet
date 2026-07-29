# `vue-vet/reactivity/no-multiple-effects-same-target`

Category: reactivity  
Default severity: warning  
Confidence: high

Multiple effects writing the same target race updates.

## Bad

```vue
<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const a = ref(1)
const b = ref(2)
const out = ref(0)
watchEffect(() => { out.value = a.value })
watchEffect(() => { out.value = b.value })
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const a = ref(1)
const out = ref(0)
watchEffect(() => { out.value = a.value })
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Keep a single writer, or write distinct targets.

## Fixtures

- Invalid: `fixtures/rules/no-multiple-effects-same-target/invalid/`
- Valid: `fixtures/rules/no-multiple-effects-same-target/valid/`
