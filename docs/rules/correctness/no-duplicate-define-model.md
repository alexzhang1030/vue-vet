# `vue-vet/correctness/no-duplicate-define-model`

Category: correctness  
Default severity: warning  
Confidence: high

Calling `defineModel` twice for the same model name is invalid.

## Bad

```vue
<script setup lang="ts">
const model = defineModel<string>()
const again = defineModel<string>()
</script>
```

## Good

```vue
<script setup lang="ts">
const model = defineModel<string>()
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Keep one `defineModel` per model name.

## Fixtures

- Invalid: `fixtures/rules/no-duplicate-define-model/invalid/`
- Valid: `fixtures/rules/no-duplicate-define-model/valid/`
