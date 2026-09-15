# `vue-vet/reactivity/no-v-memo-blocked-ref-demand`

Category: reactivity

Default severity: warning

Confidence: high

`v-memo` caches a subtree from the first render. A first render with a
false `v-if` stores the empty branch. If later the condition becomes true
while memo dependencies keep their values, Vue skips creating the node.
A template ref stays `null`, so a capability demand after `await nextTick()`
still throws. Including the condition in the memo tuple, or invalidating a
dependency, creates the node.

Vue 3.5.40 compiled SFC mounts confirm omitted-condition after-tick throws,
complete deps create the node, and an explicit revision write repairs the
same demand. Empty-array memo participates only when this failed creation
demand is proven. Pure interpolation checkpoints stay quiet.

## Bad

```vue
<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue'
const revision = ref(0)
const visible = ref(false)
const node = ref(null)
onMounted(async () => {
  visible.value = true
  await nextTick()
  node.value.textContent
})
</script>
<template>
  <div v-memo="[revision]">
    <span v-if="visible" ref="node">ready</span>
  </div>
</template>
```

## Good

```vue
<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue'
const revision = ref(0)
const visible = ref(false)
const node = ref(null)
onMounted(async () => {
  visible.value = true
  await nextTick()
  node.value.textContent
})
</script>
<template>
  <div v-memo="[revision, visible]">
    <span v-if="visible" ref="node">ready</span>
  </div>
</template>
```

Quiet also includes complete memo deps, explicit revision invalidation,
intentional text/checkpoint snapshots, guarded refs (`?.`, `if (node.value)`,
sibling `if (!node.value) return`), a same-name `v-else` owner, `v-if` on
the memo element itself (Vue evaluates `v-if` outside `withMemo`, so the
node is created), a memo dependency that is a `computed` / derived getter
over the condition, and syntax-only `v-memo` with no incompatible demand.
The rule targets blocked *creation* (a null ref), not stale text of an
always-present element inside a memo. `v-for`, slots, nested memo, dynamic
dependencies, SSR/async/transition children, callback refs, escaped
ref/condition/memo, and incomplete render ownership stay unknown.

The primary span is the failed ref demand. Related evidence is the memo
tuple, condition, ref, source change, and completed render boundary.

## Detection

Fact-driven via `TemplateRefDemandFacts.memo_blocked`. Vize records parent,
memo, condition, and ref relations while the template tree is available.
Oxc proves the mounted-owner write, awaited `nextTick`, and unguarded
demand. A syntax omission alone yields zero findings. Pre-flush demand of
the same ref is reported by
`vue-vet/reactivity/no-pre-flush-template-ref-demand` instead.

## Remediation

Include the enabling condition in the `v-memo` dependency tuple, or write
a memo dependency (for example a revision counter) before the demand.

## Limitations

Dynamic memo expressions, nested memo, `v-for`/slot/transition children,
and unknown writers stay quiet. Any static writer of a memo dependency
anywhere in the file makes that dependency Unknown. A `computed` (or other
derived getter) of the condition is not a proven-stable local `ref` /
literal and stays quiet. Getter sources, `useTemplateRef`, the
`watchEffect` family, a non-literal enabling write, and Options-API
`<script>` stay Unknown.

## Fixtures

- Invalid: `fixtures/rules/no-v-memo-blocked-ref-demand/invalid/`
- Valid: `fixtures/rules/no-v-memo-blocked-ref-demand/valid/`
