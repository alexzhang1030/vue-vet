# Prefer `toValue` over `unref` on Vue 3.3 and newer

Default severity: **info**. Category: **practice** (excluded from score and default CI exit).

Vue 3.3 adds `toValue()`, which unwraps refs like `unref` **and invokes function
payloads**. `unref` returns a function argument unchanged. Prefer `toValue` only
when getter-or-ref normalization is intended.

## Bad

```vue
<script setup>
import { ref, unref } from 'vue'

const count = ref(0)
const n = unref(() => count.value)
</script>
```

Nuxt / auto-import projects may call bare `unref` without an import; a function
payload is still suggested:

```vue
<script setup>
const count = ref(0)
const n = unref(() => count.value)
</script>
```

## Good

```vue
<script setup>
import { ref, toValue } from 'vue'

const count = ref(0)
const n = toValue(() => count.value)
</script>
```

Ordinary `unref(ref)` / `MaybeRef<number>` unwrapping stays quiet:

```vue
<script setup>
import { ref, unref } from 'vue'
const input = ref(2)
const formatted = unref(input).toFixed(2)
</script>
```

## Limitations

Requires Vue 3.3+ from the nearest `package.json`. Matches Vue `unref` (including `#imports` / auto-import) only when there is getter-relevant evidence: a function/arrow argument (`unref(() => …)`), including parenthesized wrappers.

A known Ref identifier is **not** getter evidence. `unref` on a correct `MaybeRef<number>` parameter stays quiet. The quality corpus `PreferToValue.vue` is that routine numeric/ref unwrap and must stay quiet; `PreferToValueGetter.vue` is the getter-payload true positive. Local lookalike functions named `unref` stay quiet. This is a documented low-noise policy: `toValue` is suggested where getters are in play, not for every successful `unref`.

## Remediation

Import `toValue` from `vue` (or the project's auto-import equivalent) and replace getter `unref(...)` call sites. Keep `unref` when the function itself is the value.
