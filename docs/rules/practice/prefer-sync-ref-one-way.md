# Prefer one-way `syncRef` for a closed sink

Default severity: **info**. Category: **practice** (excluded from score and default CI exit). Group: **derivation**.

VueUse `syncRef(left, right)` defaults to `direction: "both"` and installs two
watchers. When `right` is a locally allocated ordinary primitive ref or
`shallowRef` that only `syncRef` writes, a changed left-side write plus an
ordinary right-side read keep the same observations with `{ direction: 'ltr' }`,
which removes the reverse watcher.

## Bad

```vue
<script setup>
import { ref } from 'vue'
import { syncRef } from '@vueuse/core'

const source = ref(1)
const display = ref(0)
syncRef(source, display)
source.value = 5
void display.value
</script>
```

## Good

```vue
<script setup>
import { ref } from 'vue'
import { syncRef } from '@vueuse/core'

const source = ref(1)
const display = ref(0)
syncRef(source, display, { direction: 'ltr' })
source.value = 5
void display.value
</script>
```

Keep two-way `syncRef` when the right side is written, transformed, or escaped.

## Limitations

Requires a resolved `@vueuse/shared` or `@vueuse/core` `syncRef`, two distinct
closed local `ref` / `shallowRef` primitives, and known default options
(`direction: "both"`, identity transforms, `deep: false`, `flush: "sync"`,
`immediate: true`). Parameters, v-model or event writers (including aliases of the sink),
returns, exports, containers, mutable transforms or options, foreign helpers,
custom / model / computed setters, object payloads, unknown lifecycle,
right-side writes, shared alias identity, concatenated or otherwise unknown
`direction` values, unexecuted candidates, and a returned stop handle invoked
before the left write stay quiet. No automatic rewrite.

## Remediation

Pass `{ direction: 'ltr' }`. The stop handle still owns disposal.

## Fixtures

- Invalid: `fixtures/rules/prefer-sync-ref-one-way/invalid/`
- Valid: `fixtures/rules/prefer-sync-ref-one-way/valid/`
