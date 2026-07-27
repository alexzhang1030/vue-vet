# Prefer `toValue` over `unref` on Vue 3.3 and newer

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

Vue 3.3 adds `toValue()`, which unwraps refs like `unref` and also accepts getters. Prefer it
when migrating composition utilities on Vue 3.3+.

## Bad

```vue
<script setup>
import { ref, unref } from 'vue'

const count = ref(0)
const n = unref(count)
</script>
```

Nuxt / auto-import projects may call bare `unref` without an import; that is also suggested:

```vue
<script setup>
const count = ref(0)
const n = unref(count)
</script>
```

## Good

```vue
<script setup>
import { ref, toValue } from 'vue'

const count = ref(0)
const n = toValue(count)
</script>
```

## Limitations

Requires Vue 3.3+ from the nearest `package.json`. Matches:

- `unref` resolved from `vue`, `vue-demi`, `#imports`, or `@vue/*`
- bare `unref(...)` with no local binding or import named `unref` (Nuxt / unplugin-auto-import)

Local lookalike functions named `unref` stay quiet.

## Remediation

Import `toValue` from `vue` (or the project's auto-import equivalent) and replace `unref(...)` call sites.
