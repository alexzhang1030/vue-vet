# Prefer `useSlots` / `useAttrs` over `getCurrentInstance`

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

`getCurrentInstance` is documented as an advanced escape hatch. Reading `instance.slots` or `instance.attrs` from it can usually be replaced with the dedicated Composition API helpers `useSlots()` and `useAttrs()`, which work identically inside `<script setup>` without exposing the whole internal instance.

## Bad

```vue
<script setup>
import { getCurrentInstance } from 'vue'

const instance = getCurrentInstance()
console.log(instance.slots.default)
</script>
```

## Good

```vue
<script setup>
import { useSlots } from 'vue'

const slots = useSlots()
console.log(slots.default)
</script>
```

## Limitations

Fires on any `getCurrentInstance` call resolved from `vue` (or a bare auto-imported call with no local binding); it does not verify that `.slots` / `.attrs` are actually read off the result, since other legitimate uses of the instance exist.

## Remediation

Use `useSlots()` for slot access and `useAttrs()` for attribute access.
