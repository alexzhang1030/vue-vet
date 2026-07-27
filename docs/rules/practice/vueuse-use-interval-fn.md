# Prefer VueUse `useIntervalFn` when intervals lack cleanup

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

Starting `setInterval` inside a setup lifecycle hook (`onMounted` / `onBeforeMount` / `onActivated`) without `clearInterval` often leaks timers across remounts. VueUse `useIntervalFn` pairs the interval with pause/resume and automatic cleanup.

## Bad

```vue
<script setup>
import { onMounted } from 'vue'

onMounted(() => {
  setInterval(() => {
    console.log('tick')
  }, 1000)
})
</script>
```

## Good

```vue
<script setup>
import { useIntervalFn } from '@vueuse/core'

useIntervalFn(() => {
  console.log('tick')
}, 1000)
</script>
```

## Limitations

Reports only when a setup lifecycle hook and a `setInterval` (including `window.setInterval`) appear in the same block with no `clearInterval`. Module-level intervals and explicit set/clear pairs stay quiet. Already importing or calling `useIntervalFn` is a safe pattern. Test files are skipped.

## Remediation

Optional dependency: install `@vueuse/core` when you want the helper, then replace the manual interval with `useIntervalFn`.
