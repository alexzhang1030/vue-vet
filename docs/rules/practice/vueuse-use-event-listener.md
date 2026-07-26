# Prefer VueUse `useEventListener` when listeners lack cleanup

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

Registering `addEventListener` without a matching `removeEventListener` in the same script block often leaks handlers across remounts. VueUse `useEventListener` pairs registration with automatic cleanup.

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

Reports when an `addEventListener` (including `window.` / `document.` members) appears without any `removeEventListener` in the same block. Explicit add/remove pairs stay quiet even when verbose. Already importing or calling `useEventListener` is a safe pattern. Test files are skipped.

## Remediation

Optional dependency: install `@vueuse/core` when you want the helper, then replace the manual listener with `useEventListener`.
