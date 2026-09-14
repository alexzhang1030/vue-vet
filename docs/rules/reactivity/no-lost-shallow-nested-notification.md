# `vue-vet/reactivity/no-lost-shallow-nested-notification`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

A nested write into a proven plain payload of `shallowRef` or `shallowReactive`
does not notify through the shallow tracking frontier. Vue Vet reports this when
an already-running synchronous `watchSyncEffect` or `watchEffect` in the same
execution region reads that nested path through the container.

Computed consumers stay quiet in this slice: a getter may remain unevaluated
until after the write. Conditional or unactivated watchers, async reads after
`await`, stopped handles, unknown `watchEffect` flush options (duplicate keys,
spreads), payload replacement through assignment patterns or mutable aliases,
spread/duplicate-key object overwrites, templates, imported factories, collection
mutations, dynamic keys, `markRaw`, and `triggerRef` of the same `shallowRef`
are also quiet. A `shallowRef` argument that already carries `__v_isRef` (Vue
identity normalization) is not a proven new shallow box. Marker-key presence
(`__v_skip`, `__v_isReadonly`, `__v_isRef`, `__v_raw`, `__proto__`) keeps the
payload unproven; known-false marker values are not distinguished in this slice.

## Bad

```vue
<script setup lang="ts">
import { shallowRef, watchSyncEffect } from 'vue'

const state = shallowRef({ count: 1 })
watchSyncEffect(() => {
  void state.value.count
})
state.value.count = 2
</script>
```

## Good

```vue
<script setup lang="ts">
import { shallowRef, watchSyncEffect } from 'vue'

const state = shallowRef({ count: 1 })
watchSyncEffect(() => {
  void state.value.count
})
state.value = { count: 2 }
</script>
```

## Detection

Fact-driven: shared source/view/path facts. The write must cross a proven plain
payload branch past the shallow frontier while a matching unconditional consumer
is already active in the same region.

## Remediation

Replace the tracked slot (`state.value = …` / `state.details = …`), keep the
nested object reactive, or call `triggerRef` on a matching `shallowRef` when
batching is intentional.
