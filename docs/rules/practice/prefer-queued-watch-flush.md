# Prefer queued `watch` flush for last-value sinks

Default severity: **info**. Category: **practice** (excluded from score and default CI exit). Group: **derivation**.

Vue `watch` with `flush: 'sync'` runs the callback on every write in the same
tick. When the callback only copies the latest primitive into a private sink
and every local read of that sink happens after `await nextTick()`, default
`pre` flush still delivers the same later value and runs the callback once.

## Bad

```vue
<script setup>
import { nextTick, ref, watch } from 'vue'

const source = ref(0)
const sink = ref(0)
watch(source, (value) => {
  sink.value = value
}, { flush: 'sync' })
source.value = 1
source.value = 2
await nextTick()
void sink.value
</script>
```

## Good

```vue
<script setup>
import { nextTick, ref, watch } from 'vue'

const source = ref(0)
const sink = ref(0)
watch(source, (value) => {
  sink.value = value
})
source.value = 1
source.value = 2
await nextTick()
void sink.value
</script>
```

Keep `flush: 'sync'` when a same-tick read, a synchronous subscriber, or
callback cleanup must observe intermediate writes.

## Limitations

Requires a resolved Vue runtime-core `watch`, effective `flush: 'sync'`, at
least two proven changed writes in one executed synchronous region, and a
private ordinary primitive sink whose local observations occur after
`await nextTick()`. The watcher must stay active through that flush. Same-tick
reads, sync subscribers, callback cleanup or side effects, custom setters,
`once`, stop or pause, source/sink feedback, escaped sinks, return-to-baseline
batches that never deliver, assignment RHS demand before flush, unexecuted
`nextTick` regions, and enclosing-scope stop stay quiet. Immediate first-run
semantics are preserved. No automatic rewrite.

## Remediation

Drop `flush: 'sync'` so Vue uses the default `pre` queue. Keep `immediate` when
the original watcher already requested it.

## Fixtures

- Invalid: `fixtures/rules/prefer-queued-watch-flush/invalid/`
- Valid: `fixtures/rules/prefer-queued-watch-flush/valid/`
