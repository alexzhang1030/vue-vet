# `vue-vet/correctness/require-v-for-key`

Category: correctness  
Default severity: warning  
Confidence: high

Vue Vet rule `vue-vet/correctness/require-v-for-key` reports a fact-driven correctness or reactivity issue. Prefer the Bad/Good examples below; fixtures under `fixtures/rules/require-v-for-key/` are the executable corpus.

## Bad

```vue
<script setup lang="ts">
// See fixtures/rules for the executable invalid corpus.
</script>
```

## Good

```vue
<script setup lang="ts">
// See fixtures/rules for the executable valid corpus.
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).

## Remediation

Follow the Good pattern, or suppress with a narrow inline disable when reviewed.

## Fixtures

- Invalid: `fixtures/rules/require-v-for-key/invalid/`
- Valid: `fixtures/rules/require-v-for-key/valid/`
