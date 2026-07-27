# Prefer VueUse `useDebounceFn` for timer debounce wrappers

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

When a script assigns `setTimeout` to a binding and later `clearTimeout`s **that same binding**, the pattern usually implements a hand-rolled debounce. VueUse provides `useDebounceFn` with the same intent and less lifecycle bookkeeping.

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

Requires an assigned `setTimeout` (including `window.setTimeout`) whose binding appears as an identifier argument to `clearTimeout` / `window.clearTimeout` in the same script block. Unrelated timers, plain one-shot `setTimeout`, and already importing/calling `useDebounceFn` stay quiet. Test files (`.test.` / `.spec.` / `__tests__`) are skipped.

## Remediation

Optional dependency: install `@vueuse/core` when you want the helper, then replace the timer wrapper with `useDebounceFn`.
