# `vue-vet/reactivity/no-watch-unwrapped-source`

Category: reactivity  
Default severity: warning  
Confidence: high

`watch(ref.value)` or `watch(state.n)` with a proven primitive payload passes
a snapshot into `watch`. Vue warns that the source is invalid, and later
writes never run the callback.

## Bad

```vue
<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
const n = ref(0)
const state = reactive({ n: 0 })
watch(n.value, () => {})
watch(state.n, () => {})
</script>
```

## Good

```vue
<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
const n = ref(0)
const state = reactive({ n: 0 })
watch(n, () => {})
watch(() => state.n, () => {})
</script>
```

`watch(ref)`, getters, functions held in a ref, arrays of valid sources, and
known reactive *object* members stay quiet. A ref initialized to a primitive
can later hold an object: any unknown write, `let`/boxed alias, assignment
pattern, helper call, spread argument, or write in another function keeps this
rule quiet. When this rule reports, `no-empty-watch-sources` is not also
reported on the same `watch` call.

## Detection

Fact-driven via Vue Vet source-contract facts. Top-level source-array entries
are checked. Named callbacks are supported.

## Remediation

Pass the ref, or wrap the member in a getter.

## Fixtures

- Invalid: `fixtures/rules/no-watch-unwrapped-source/invalid/`
- Valid: `fixtures/rules/no-watch-unwrapped-source/valid/`
