# Prefer VueUse `useRafFn` when rAF loops lack cleanup

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

A **repeating** `requestAnimationFrame` inside a setup lifecycle hook (`onMounted` / `onBeforeMount` / `onActivated`) without `cancelAnimationFrame` often keeps frames running after unmount. VueUse `useRafFn` defaults to a repeating loop with pause/resume and automatic cleanup.

Loop evidence is Oxc symbol-resolved self-scheduling: the named callback schedules itself from the nearest function boundary. A one-shot `requestAnimationFrame(() => { … })`, a named one-shot `requestAnimationFrame(render)`, a finite two-frame delay `requestAnimationFrame(() => requestAnimationFrame(render))`, and a deferred nested `later()` that is never called stay quiet. Callback shadowing is preserved: an inner `tick` that does not reschedule itself is not the outer recursive `tick`.

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

Reports only when `requestAnimationFrame` is nested under a setup lifecycle hook **and** the callback reschedules itself, with no `cancelAnimationFrame`. Nested rAF that is not the same symbol is not a loop. Block-level co-presence of `onMounted` and a one-shot rAF is not enough. Module-level rAF and explicit request/cancel pairs stay quiet. Already importing or calling `useRafFn` is a safe pattern. Test files are skipped.

## Remediation

Optional dependency: install `@vueuse/core` when you want the helper, then replace the manual rAF loop with `useRafFn`.
