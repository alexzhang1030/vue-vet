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

## Good

```vue
<script setup>
import { ref, toValue } from 'vue'

const count = ref(0)
const n = toValue(count)
</script>
```

## Limitations

Requires Vue 3.3+ from the nearest `package.json` and a call that resolves to Vue's `unref`
(including aliases). Local lookalike functions named `unref` stay quiet. Bare auto-imported
`unref` without a resolvable import binding is not reported yet.

## Remediation

Import `toValue` from `vue` and replace `unref(...)` call sites.
