# Prefer VueUse `useRafFn` when rAF loops lack cleanup

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

Scheduling `requestAnimationFrame` inside a setup lifecycle hook (`onMounted` / `onBeforeMount` / `onActivated`) without `cancelAnimationFrame` often keeps frames running after unmount. VueUse `useRafFn` pairs the loop with pause/resume and automatic cleanup.

## Bad

```vue
<script setup>
import { onMounted } from 'vue'

onMounted(() => {
  const loop = () => {
    // paint
    requestAnimationFrame(loop)
  }
  requestAnimationFrame(loop)
})
</script>
```

## Good

```vue
<script setup>
import { useRafFn } from '@vueuse/core'

useRafFn(() => {
  // paint
})
</script>
```

## Limitations

Reports only when a setup lifecycle hook and a `requestAnimationFrame` (including `window.requestAnimationFrame`) appear in the same block with no `cancelAnimationFrame`. Module-level rAF and explicit request/cancel pairs stay quiet. Already importing or calling `useRafFn` is a safe pattern. Test files are skipped.

## Remediation

Optional dependency: install `@vueuse/core` when you want the helper, then replace the manual rAF loop with `useRafFn`.
