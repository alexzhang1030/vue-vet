# `vue-vet/reactivity/no-watch-signature-mismatch`

Category: reactivity  
Default severity: warning  
Confidence: high

Composition `watch` requires a function callback. A `{ handler }` object (or
another non-function such as a string) warns that the old `watch(fn, options?)`
signature moved; the handler never runs. `watchEffect(fn, fn2)` treats `fn2` as
options, so `fn2` never runs. `watchEffect(ref, cb)` is the same slot mistake.

Callback **arrays** are invoked by Vue despite the signature warning and stay
quiet here. Falsy `0` / `null` / `undefined` / missing callbacks use effect-form
behavior and are excluded. Named or aliased function options can carry
`flush: 'sync'` and must stay quiet.

## Bad

```vue
<script setup lang="ts">
import { ref, watch, watchEffect } from 'vue'
const count = ref(0)
watch(count, { handler() { void count.value }, immediate: true })
watchEffect(() => count.value, (n) => void n)
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watch, watchEffect } from 'vue'
const count = ref(0)
watch(count, (n) => void n)
watchEffect(() => void count.value)
</script>
```

Spread arguments, unknown callees, and Options API `this.$watch` stay quiet.
Imported Vue aliases (`import { watch as observe }`) and runtime namespace
imports (`import * as Vue from 'vue'`) are supported. Type-only imports,
local shadows, `#imports` namespace specifiers, and unknown callback/options
aliases stay quiet because those identities are unproven.

## Detection

Fact-driven via Vue Vet source-contract facts. The second argument of proven
`watch` is a fresh plain object or a proven truthy primitive. The second
argument of proven `watch*Effect` is a fresh function/arrow when the first
argument is an inline effect or a proven ref.

## Remediation

Use `watch(source, callback, options)` or `watchEffect(callback, options)`.

## Fixtures

- Invalid: `fixtures/rules/no-watch-signature-mismatch/invalid/`
- Valid: `fixtures/rules/no-watch-signature-mismatch/valid/`
