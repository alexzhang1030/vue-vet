# `vue-vet/reactivity/no-empty-watch-sources`

Category: reactivity  
Default severity: warning  
Confidence: high

`watch` with an empty source list never runs usefully.

## Bad

```vue
<script setup lang="ts">
import { watch } from 'vue'
watch([], () => {})
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
watch(count, () => {})
watch((count), () => {})
watch(count as any, () => {})
</script>
```

Composable object bags (including `.d.ts` `{ width: Ref; height: Ref }` shapes
such as VueUse `useElementSize`) seed destructured locals, so renamed sources
stay known:

```vue
<script setup lang="ts">
import { watch, type Ref } from 'vue'
declare function useElementSize(): {
  width: Ref<number>
  height: Ref<number>
}
const { width: hostWidth, height: hostHeight } = useElementSize()
watch([hostWidth, hostHeight], () => {})
</script>
```

## Detection

Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts (not a parallel regex pattern engine).
Incomplete coverage (`unknown_calls`, `follow_truncated`, or `uncertain_accesses`)
suppresses this finding entirely — the rule does not emit a confident or
`(maybe: …)` empty-source verdict when analysis is incomplete.
Unclassified static or computed member sources (`watch(sources['active'])`,
`watch(() => bag.current)`, and the same access reached through a same-file
zero-arg helper) mark coverage incomplete, so this rule and Explain abstain.
Constant getters (`watch(() => 42)`) and empty arrays still report.
Known refs, peeled parens / `as` wrappers, and destructured `.d.ts` bag fields
stay classified.

## Remediation

Pass at least one source.

## Fixtures

- Invalid: `fixtures/rules/no-empty-watch-sources/invalid/`
  (`constant-getter.vue`, `empty-array.vue`)
- Valid: `fixtures/rules/no-empty-watch-sources/valid/`
  (`safe.vue`, `parens-ref.vue`, `destructure-dts-bag.vue`,
  `computed-member.vue`, `property-source.vue`, `helper-call.vue`,
  `deferred-helper.vue`)
