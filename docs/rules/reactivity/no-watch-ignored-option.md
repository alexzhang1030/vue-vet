# `vue-vet/reactivity/no-watch-ignored-option`

Category: reactivity  
Default severity: warning  
Confidence: high

`flush` configures scheduling for `watch` and `watchEffect`. `watchPostEffect`
always uses post scheduling, and `watchSyncEffect` always uses sync scheduling;
these wrappers override a supplied `flush`. This rule currently diagnoses
`equals` and effect-only `immediate` / `deep` / `once` keys. `onTrack` and
`onTrigger` remain supported.

## Bad

```vue
<script setup lang="ts">
import { ref, watch, watchEffect } from 'vue'
const count = ref(0)
watch(count, (n) => void n, { equals: (a, b) => a === b })
watchEffect(() => void count.value, { once: true })
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watch, watchEffect } from 'vue'
const count = ref(0)
watch(count, (n) => void n, { flush: 'sync' })
watchEffect(() => void count.value, { flush: 'sync' })
</script>
```

The options slot is the third argument of `watch` and the second argument of
`watch*Effect`, including when extra arguments follow. Only a fresh object
literal with unique static own data/method keys is inspected. Spreads,
computed keys (including `{ ['once']: true }`), accessors, and `__proto__`
stay quiet. `equals` reports when the own value is an inline arrow, function,
or method. Effect `immediate` / `deep` / `once` report when the own value is
a proven non-undefined literal; `undefined` stays quiet. Aliased options
objects and unknown callees stay quiet. Imported Vue aliases and runtime
namespace imports are supported. A signature-mismatch on the same call is
not also reported here.

## Detection

Fact-driven via Vue Vet source-contract facts from proven Vue / `#imports`
named `watch` / `watchEffect` / `watchPostEffect` / `watchSyncEffect`.

## Remediation

Drop the ignored key. For equality skip, watch a primitive or getter
projection, or compare inside the callback against a cloned snapshot.

## Fixtures

- Invalid: `fixtures/rules/no-watch-ignored-option/invalid/`
- Valid: `fixtures/rules/no-watch-ignored-option/valid/`
