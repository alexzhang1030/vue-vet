# Prefer VueUse `useEventListener` when listeners lack cleanup

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

Registering `addEventListener` inside a setup lifecycle hook (`onMounted` / `onBeforeMount` / `onActivated`) without a matching `removeEventListener` often leaks handlers across remounts. VueUse `useEventListener` pairs registration with automatic cleanup.

## Bad

```vue
<script setup>
import { onMounted } from 'vue'

onMounted(() => {
  window.addEventListener('resize', () => {})
})
</script>
```

## Good

```vue
<script setup>
import { useEventListener } from '@vueuse/core'

useEventListener(window, 'resize', () => {})
</script>
```

## Limitations

Reports only when a setup lifecycle hook and an `addEventListener` (including `window.` / `document.` members) appear in the same block with no `removeEventListener`. Bare module-level listeners and explicit add/remove pairs stay quiet. Already importing or calling `useEventListener` is a safe pattern. Test files are skipped.

## Remediation

Optional dependency: install `@vueuse/core` when you want the helper, then replace the manual listener with `useEventListener`.
