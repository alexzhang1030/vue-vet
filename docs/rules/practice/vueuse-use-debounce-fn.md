# Prefer VueUse `useDebounceFn` for timer debounce wrappers

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

When a script both assigns `setTimeout` and calls `clearTimeout`, the pattern usually implements a hand-rolled debounce. VueUse provides `useDebounceFn` with the same intent and less lifecycle bookkeeping.

## Bad

```vue
<script setup>
let timer
function search(query) {
  clearTimeout(timer)
  timer = setTimeout(() => console.log(query), 200)
}
</script>
```

## Good

```vue
<script setup>
import { useDebounceFn } from '@vueuse/core'

const search = useDebounceFn((query) => {
  console.log(query)
}, 200)
</script>
```

## Limitations

Requires both an assigned `setTimeout` and a `clearTimeout` call in the same script block. Plain one-shot timers stay quiet. Already importing or calling `useDebounceFn` is a safe pattern. Test files (`.test.` / `.spec.` / `__tests__`) are skipped.

## Remediation

Optional dependency: install `@vueuse/core` when you want the helper, then replace the timer wrapper with `useDebounceFn`.
