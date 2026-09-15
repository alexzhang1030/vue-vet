# `vue-vet/reactivity/no-pre-flush-template-ref-demand`

Category: reactivity

Default severity: warning

Confidence: high

A Vue `watch` with effective `flush: 'pre'` (the default) or `'sync'` runs
before the owner component patches. A `v-if` that starts false leaves a
local native template ref at `null`. Demanding a capability on that ref in
the watch callback throws `TypeError` even though the same patch creates
the node. A `'post'` watcher observes the created element.

Vue 3.5.40 compiled SFC mounts confirm: pre throws, post reads `'ready'`.
Reading an already-rendered node's previous text is a valid snapshot.

## Bad

```vue
<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
const visible = ref(false)
const node = ref(null)
watch(visible, () => {
  node.value.textContent
}, { flush: 'pre' })
onMounted(() => {
  visible.value = true
})
</script>
<template>
  <span v-if="visible" ref="node">ready</span>
</template>
```

## Good

```vue
<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from 'vue'
const visible = ref(false)
const node = ref(null)
watch(visible, () => {
  node.value.textContent
}, { flush: 'post' })
onMounted(() => {
  visible.value = true
})
</script>
<template>
  <span v-if="visible" ref="node">ready</span>
</template>
```

Quiet also includes `v-show` and already-mounted nodes, optional/guarded
refs (`?.`, `if (node.value)`, `node.value && …`, sibling
`if (!node.value) return` / `if (node.value == null) return`), any `await`
before the demand (not only `nextTick`), stopped or paused watchers,
callback refs, child component refs, unrelated sources, transitions,
async/SSR owners, a same-name `v-else` or unconditional sibling ref, and
incomplete mounted-owner proof. `<Suspense>` wrappers and default-slot
native children of a local component still fire when the other premises
hold. Memo-blocked creation after a tick belongs to
`vue-vet/reactivity/no-v-memo-blocked-ref-demand`; when both apply to the
same ref, only the earlier pre-flush failure is reported.

The primary span is the failed capability demand. Related evidence is the
condition write, watcher, and template allocation. `flush: 'post'` is
recommended only when this ordinary native node is created in that patch.

## Detection

Fact-driven via `TemplateRefDemandFacts.pre_flush`, joined from Vize
allocation relations and Oxc watch/owner/value/demand facts. Template
expressions are parsed with Oxc. Element start-tag spans do not prove
descendant ownership.

## Remediation

Schedule the demand with `flush: 'post'`, await `nextTick` before reading
the ref, or guard the capability (`node.value?.textContent`).

## Limitations

Unknown effects, helper writers, escaped refs or conditions, dynamic
`:ref`, and incomplete mounted-owner proof stay quiet. Getter sources
(`watch(() => visible.value, …)`), `useTemplateRef`, the `watchEffect` /
`watchPostEffect` / `watchSyncEffect` family, a non-literal enabling write
(`visible.value = !visible.value`), and Options-API `<script>` (no
`<script setup>`) are Unknown even when the runtime throws.

## Fixtures

- Invalid: `fixtures/rules/no-pre-flush-template-ref-demand/invalid/`
- Valid: `fixtures/rules/no-pre-flush-template-ref-demand/valid/`
