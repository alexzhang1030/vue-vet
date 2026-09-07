# `vue-vet/reactivity/no-watch-replaced-object-source`

Category: reactivity  
Default severity: warning  
Confidence: high

`watch(state.nested)` subscribes to the *current* object identity. A later
same-property replacement (`state.nested = { … }`) does not retarget that
subscription.

Applicability is restricted to **direct expression statements in the same
straight-line block** (function body or module top-level). The replacement must
be a simple `=` of a fresh object/array/`new` collection. Compound assigns
(`||=`), `reactive(existing)` (cached proxy), `state.p = state.p`, branched
control flow, `const stop = watch(…); stop()`, readonly/shallow roots, spreads,
and unknown aliases stay quiet. Mutation *before* subscription and nested-only
writes (`state.nested.x = …`) are safe. Getter sources stay quiet.

## Bad

```vue
<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ nested: { x: 1 } })
watch(state.nested, () => {})
state.nested = { x: 9 }
</script>
```

## Good

```vue
<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ nested: { x: 1 } })
watch(() => state.nested, () => {}, { deep: true })
state.nested = { x: 9 }
</script>
```

The primary span is the watch source. Help text cites the replacement
line/column.

## Detection

Fact-driven via Vue Vet source-contract facts (Oxc symbol identity + lexical
order). Not generic call syntax.

## Remediation

Watch a getter, and use `{ deep: true }` when nested fields must be observed
across replacement.

## Fixtures

- Invalid: `fixtures/rules/no-watch-replaced-object-source/invalid/`
- Valid: `fixtures/rules/no-watch-replaced-object-source/valid/`
