# `vue-vet/reactivity/no-trigger-ref-on-non-ref`

Category: reactivity  
Default severity: warning  
Confidence: high

`triggerRef` only notifies `ref.dep`. Passing a proven non-ref (ordinary
reactive proxy or primitive) is a silent no-op.

## Bad

```vue
<script setup lang="ts">
import { reactive, triggerRef, watchEffect } from 'vue'
const obj = reactive({ n: 1 })
watchEffect(() => {
  void obj.n
})
triggerRef(obj)
</script>
```

## Good

```vue
<script setup lang="ts">
import { shallowRef, triggerRef, watchEffect } from 'vue'
const r = shallowRef({ n: 1 })
watchEffect(() => {
  void r.value.n
})
triggerRef(r)
</script>
```

`reactive(ref(...))` stays ref-like and is quiet. Import aliases, `import * as
Vue`, and local shadowing use Oxc symbol identity. Type-only imports, default
imports, `@vue/toolkit`, unresolved globals, and `reactive(unknown)` stay quiet.

## Detection

Fact-driven via Vue Vet source-contract facts (Oxc symbol identity + argument
provenance). Proven Vue APIs are named/namespace imports from `vue`,
`vue-demi`, `@vue/runtime-core`, `@vue/runtime-dom`, and `@vue/reactivity`,
plus named `#imports` specifiers that re-export those APIs. Type-only
imports, default imports, namespace `#imports`, and unknown auto-import names
stay quiet.

## Remediation

Pass a ref / `shallowRef` / `customRef`, or mutate the reactive object.

## Fixtures

- Invalid: `fixtures/rules/no-trigger-ref-on-non-ref/invalid/`
- Valid: `fixtures/rules/no-trigger-ref-on-non-ref/valid/`
