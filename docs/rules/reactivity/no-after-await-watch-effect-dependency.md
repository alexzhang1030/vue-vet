# `vue-vet/reactivity/no-after-await-watch-effect-dependency`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

`watchEffect()` only tracks reactive reads reached during its **synchronous**
execution. After `await`, Vue stops collecting dependencies for that run.

Deferred callbacks (`nextTick` / `then` / `pauseTracking`) are covered by
sibling tracer rules, not this one.

## Bad

```vue
<script setup lang="ts">
import { ref, watchEffect } from 'vue'

const result = ref(0)
watchEffect(async () => {
  await Promise.resolve()
  console.log(result.value)
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watchEffect } from 'vue'

const result = ref(0)
watchEffect(async () => {
  const current = result.value
  await Promise.resolve()
  console.log(current)
})
</script>
```

## Detection

Fact-driven: effect-family reads classified `AfterAwait`.

## Remediation

Read required dependencies before the first `await`, or switch to explicit
`watch` sources when async work must re-run on those inputs.
