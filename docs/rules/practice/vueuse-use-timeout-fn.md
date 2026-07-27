# Prefer VueUse `useTimeoutFn` when timeouts lack cleanup

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

Starting `setTimeout` inside a setup lifecycle hook (`onMounted` / `onBeforeMount` / `onActivated`) without `clearTimeout` often races with unmount. VueUse `useTimeoutFn` pairs the delay with start/stop controls and automatic cleanup.

## Bad

```vue
<script setup>
import { onMounted } from 'vue'

onMounted(() => {
  setTimeout(() => {
    console.log('ready')
  }, 1000)
})
</script>
```

## Good

```vue
<script setup>
import { useTimeoutFn } from '@vueuse/core'

useTimeoutFn(() => {
  console.log('ready')
}, 1000)
</script>
```

## Limitations

Reports only when a setup lifecycle hook and a `setTimeout` (including `window.setTimeout`) appear in the same block with no `clearTimeout`. Module-level timeouts, fire-and-forget timers outside lifecycle hooks, and clear+set debounce patterns stay quiet (the latter may be suggested as `useDebounceFn` instead). Already importing or calling `useTimeoutFn` is a safe pattern. Test files are skipped.

## Remediation

Optional dependency: install `@vueuse/core` when you want the helper, then replace the manual timeout with `useTimeoutFn`.
