# `vue-vet/reactivity/no-toraw-write-of-tracked-state`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

Writing a path through `toRaw` of a `reactive` root does not notify a consumer
that already reads that path on the proxy. Vue Vet reports the bypassing write
when a same-region `watchSyncEffect` or synchronous `watchEffect` is already
subscribed through the proxy.

`toRaw` reads, clones, and equality stay quiet. A consumer that only reads
`toRaw(state).n` is not a proxy consumer. `readonly(plain)` and
`reactive(markRaw(...))` stay quiet: they are not proven notifying proxies.
Objects that carry Vue internal marker keys (`__v_skip`, `__v_isReadonly`,
`__v_isRef`, `__v_raw`, `__proto__`) stay quiet, including numeric/string
truthy flags and unknown or accessor values. Known-false marker values are not
distinguished from presence in this slice.

## Bad

```vue
<script setup lang="ts">
import { reactive, toRaw, watchSyncEffect } from 'vue'

const state = reactive({ count: 1 })
watchSyncEffect(() => {
  void state.count
})
const raw = toRaw(state)
raw.count = 2
</script>
```

## Good

```vue
<script setup lang="ts">
import { reactive, watchSyncEffect } from 'vue'

const state = reactive({ count: 1 })
watchSyncEffect(() => {
  void state.count
})
state.count = 2
</script>
```

## Detection

Fact-driven: a raw view of a proven `reactive` root plus an exact matching
proxy consumer path and a later direct assignment/update.

## Remediation

Write through the reactive proxy so Vue can notify the subscriber.
