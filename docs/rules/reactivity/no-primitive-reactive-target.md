# `vue-vet/reactivity/no-primitive-reactive-target`

Category: reactivity  
Default severity: warning  
Confidence: high

`reactive`, `readonly`, `shallowReactive`, and `shallowReadonly` cannot wrap
primitive or `null` values. Vue returns the input and warns. One rule covers
the whole proxy family.

## Bad

```vue
<script setup lang="ts">
import { reactive, readonly } from 'vue'
const n = reactive(0)
const s = readonly('x')
void n
void s
</script>
```

## Good

```vue
<script setup lang="ts">
import { reactive, readonly, ref } from 'vue'
const n = ref(0)
const state = reactive({ n: 0 })
const view = readonly({ n: 0 })
void n
void state
void view
</script>
```

Object / array / `Map` / `Set` / ref objects stay quiet. Unknown arguments stay
quiet. Evidence is a literal or an immutable local with proven primitive
provenance.

## Detection

Fact-driven via Vue Vet source-contract facts. The reported span is the
primitive argument.

## Remediation

Pass a non-null object, or use `ref` for primitive state.

## Fixtures

- Invalid: `fixtures/rules/no-primitive-reactive-target/invalid/`
- Valid: `fixtures/rules/no-primitive-reactive-target/valid/`
