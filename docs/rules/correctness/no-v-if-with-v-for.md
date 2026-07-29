# `vue-vet/correctness/no-v-if-with-v-for`

Category: correctness  
Default severity: warning  
Confidence: high

Vue Vet rule `vue-vet/correctness/no-v-if-with-v-for` reports a fact-driven correctness or reactivity issue. Prefer the Bad/Good examples below; fixtures under `fixtures/rules/no-v-if-with-v-for/` are the executable corpus.

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

- Invalid: `fixtures/rules/no-v-if-with-v-for/invalid/`
- Valid: `fixtures/rules/no-v-if-with-v-for/valid/`
