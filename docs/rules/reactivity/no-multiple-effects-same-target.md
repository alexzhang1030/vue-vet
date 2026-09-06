# `vue-vet/reactivity/no-multiple-effects-same-target`

Category: reactivity  
Default severity: warning  
Confidence: high

Multiple independent effects writing the same resolved target can overwrite
each other. Several assignments inside one effect are a single writer.

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
Writers are distinct tracking scopes, keyed by Oxc binding identity (same-name
locals in different functions stay distinct). Alias writers join the
Oxc-resolved root span (`alias_of_span`), not the nearest same-name
declaration. Several write sites in one effect count as one writer. The
diagnostic does not claim a data race.

## Remediation

Keep a single writer, or write distinct targets.

## Fixtures

- Invalid: `fixtures/rules/no-multiple-effects-same-target/invalid/` (`two-effects.vue`, `shadowed-alias.vue`)
- Valid: `fixtures/rules/no-multiple-effects-same-target/valid/` (`single-writer.vue`, `same-name-functions.vue`)
