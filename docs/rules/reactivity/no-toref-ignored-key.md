# `vue-vet/reactivity/no-toref-ignored-key`

Category: reactivity

Default severity: warning

Confidence: high

`toRef` checks existing-ref identity, then callable getter normalization, then
the object-key overload. A static key is ignored on a proven ref, function, or
primitive/nullish source.

`toRef(existingRef, 'value')` is quiet: Vue returns the same ref, so writes
still go through. Function sources ignore `'value'` and return a getter ref.

## Bad

```vue
<script setup lang="ts">
import { ref, toRef } from 'vue'
const count = ref(0)
const aliased = toRef(count, 'n')
const getter = toRef(() => 1, 'value')
void aliased
void getter
</script>
```

## Good

```vue
<script setup lang="ts">
import { reactive, ref, toRef } from 'vue'
const count = ref(0)
const same = toRef(count, 'value')
const missing = toRef(count, undefined)
const state = reactive({ n: 1 })
const n = toRef(state, 'n')
const later = toRef(state, 'later')
void same
void missing
void n
void later
</script>
```

Ordinary plain, reactive, readonly, and props objects stay on the object-key
overload, including an absent property that becomes a live binding. Unknown
keys, spreads, shadowed imports, and uncertain sources stay quiet. Type
assertions preserve classification from the proven runtime source.

Vue's `toRef` overload follows the live `__v_isRef` marker.
After `state.__v_isRef = false` or
`delete state.__v_isRef`, `toRef(state, 'count')` is the object-property
overload. This rule reports only when the marker is an immutable Vue-ref
capability: writes or deletes of `__v_isRef`, helper-argument escape, method
receivers, call and tagged-template receivers (including TypeScript
instantiation wrappers such as `obj.method<T>()` and a parenthesized
`obj.method<T>` tagged template), constructor arguments, pattern assignment
to the marker (static / computed / default / rest / TS wrappers), spreads,
reassignment, and aliases through non-const bindings abstain. Ordinary
`.value` / data writes keep proof. Literal `{ __v_isRef: true }` objects
retain unknown Vue-ref provenance.

## Detection

Fact-driven via Vue Vet source-contract facts (Oxc symbol identity + argument
provenance). Proven Vue APIs are named/namespace imports from `vue`,
`vue-demi`, `@vue/runtime-core`, `@vue/runtime-dom`, and `@vue/reactivity`,
plus named `#imports` specifiers that re-export those APIs.

## Remediation

Drop the ignored key, use `toRef(ref, 'value')` when writeback to the same ref
is intended, or pass an object if a property binding is intended.

## Fixtures

- Invalid: `fixtures/rules/no-toref-ignored-key/invalid/`
- Valid: `fixtures/rules/no-toref-ignored-key/valid/`
