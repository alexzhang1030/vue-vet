# Prefer an attached child `effectScope` under parent pause

Default severity: **info**. Category: **practice** (excluded from score and default CI exit). Group: **lifetime**.

`effectScope(true)` creates a detached child that keeps running while the
parent is paused. When the child is constructed inside a live `parent.run`,
owns a synchronous last-value watcher, and is stopped from
`onScopeDispose(() => child.stop())`, an attached child
(`effectScope()` without `true`) joins parent pause, coalesces pending work,
and still stops with the parent.

## Bad

```vue
<script setup>
import { effectScope, onScopeDispose, ref, watch } from 'vue'

const source = ref(0)
const sink = ref(0)
const parent = effectScope()
parent.run(() => {
  const child = effectScope(true)
  child.run(() => {
    watch(source, (value) => {
      sink.value = value
    }, { flush: 'sync' })
  })
  onScopeDispose(() => child.stop())
})
parent.pause()
source.value = 2
source.value = 3
parent.resume()
void sink.value
</script>
```

## Good

```vue
<script setup>
import { effectScope, onScopeDispose, ref, watch } from 'vue'

const source = ref(0)
const sink = ref(0)
const parent = effectScope()
parent.run(() => {
  const child = effectScope()
  child.run(() => {
    watch(source, (value) => {
      sink.value = value
    }, { flush: 'sync' })
  })
  onScopeDispose(() => child.stop())
})
parent.pause()
source.value = 2
source.value = 3
parent.resume()
void sink.value
</script>
```

Keep a detached child when it must pause independently of the parent.

## Limitations

Requires `effectScope(true)` created while a known parent is dynamically
current, a real synchronous watcher, valid `onScopeDispose(() => child.stop())`
ownership, an actual `parent.pause`, changed source writes while paused,
`parent.resume`, and a private primitive sink read after resume. Explicit child
pause or resume, independent or escaped children, unknown current scope,
effects or logging, paused-period sink demand, live `watchSyncEffect`
subscribers, return-to-baseline batches that never deliver, and unexecuted
pause stay quiet. Dropped management handles stay with the detached lifetime
warning. No automatic rewrite.

## Remediation

Create the child with `effectScope()`. `onScopeDispose(() => child.stop())`
still owns disposal.

## Fixtures

- Invalid: `fixtures/rules/prefer-attached-effect-scope/invalid/`
- Valid: `fixtures/rules/prefer-attached-effect-scope/valid/`
