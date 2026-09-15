# `vue-vet/reactivity/no-ref-history-snapshot-alias`

Category: reactivity  
Default severity: warning  
Confidence: high

`useManualRefHistory` defaults to identity dump/parse. A later in-place nested
write mutates the retained snapshot, so `undo` / `reset` / a history read
restores the edited object.

Root replacement, `{ clone: true }`, custom or unknown `clone` expressions,
latest-rebase, write-restored-before-demand, drained undo, and a write with
no later historical-value consumption stay quiet. Custom dump/parse/setSource,
missing undo depth, `clear` before undo, primitives, escapes, and
`useRefHistory` without separate scheduling evidence stay unknown.

## Bad

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { useManualRefHistory } from '@vueuse/core'
const source = ref({ n: 1 })
const { commit, undo } = useManualRefHistory(source)
commit()
source.value.n = 2
undo()
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { useManualRefHistory } from '@vueuse/core'
const source = ref({ n: 1 })
const { commit, undo } = useManualRefHistory(source, { clone: true })
commit()
source.value.n = 2
undo()
</script>
```

Named aliases, namespace imports from `@vueuse/core`, `shallowRef`, and
TypeScript wrappers use Oxc symbol identity. JSON cloning is suitable only
for JSON-compatible values; pick a clone that copies nested identity when
capabilities matter.

## Detection

Fact-driven via Vue Vet source-contract facts. Requires a local Vue
`ref`/`shallowRef` of a fresh own-data object, identity dump (`clone` absent,
`undefined`, or `false`), a changed nested write on the exact retained
record, and a later consumption of that historical value (`source` property
after undo/reset, or `history`/`last` snapshot property). Present-but-unknown
options abstain.

## Remediation

Replace the root instead of mutating nested fields, or pass `{ clone: true }` /
a clone function that copies the nested value.

## Fixtures

- Invalid: `fixtures/rules/no-ref-history-snapshot-alias/invalid/`
- Valid: `fixtures/rules/no-ref-history-snapshot-alias/valid/`
