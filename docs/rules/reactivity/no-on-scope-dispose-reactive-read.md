# `vue-vet/reactivity/no-on-scope-dispose-reactive-read`

Category: reactivity  
Default severity: warning  
Confidence: high

Vue Vet rule `vue-vet/reactivity/no-on-scope-dispose-reactive-read` reports a fact-driven correctness or reactivity issue. Prefer the Bad/Good examples below; fixtures under `fixtures/rules/no-on-scope-dispose-reactive-read/` are the executable corpus.

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

- Invalid: `fixtures/rules/no-on-scope-dispose-reactive-read/invalid/`
- Valid: `fixtures/rules/no-on-scope-dispose-reactive-read/valid/`
