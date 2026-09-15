# `vue-vet/reactivity/no-memoize-stale-result-demand`

Category: reactivity  
Default severity: warning  
Confidence: high

`useMemoize` from `@vueuse/core` keeps the resolver result at an equal key.
After a later source write changes the primitive kind, a same-key call can
return the cached value. Demanding a native method that the cached kind lacks
throws, even though the current source value would supply that method.

## Bad

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { useMemoize } from '@vueuse/core'
const source = ref(1)
const resolve = useMemoize(() => source.value)
resolve()
source.value = 'text'
resolve().toUpperCase()
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { useMemoize } from '@vueuse/core'
const source = ref(1)
const resolve = useMemoize(() => source.value)
resolve()
source.value = 'text'
resolve.delete()
resolve().toUpperCase()
</script>
```

The first call after a source write, a distinct key, a same-kind write, a
method the cached value already supports, a guarded or optional demand, a
captured snapshot, and a later cache hit that still holds the first filled
kind stay quiet. `load`, `delete`, `clear`, and a proven public cache update
repair the entry. Custom `getKey` options, mutable option objects, escaped
results, shadowed imports, constructor shadows, native prototype repairs, and
unknown helper effects stay unknown.

## Detection

Fact-driven via Vue Vet source-contract facts. The producer must be a proven
`@vueuse/core` `useMemoize` with a stable local zero-argument inline resolver
that reads one native local Vue `ref` / `shallowRef`, default cache and key.
Findings require an earlier reachable cache fill, a later kind-changing source
write, and an unguarded native method demand on the same-key result.

## Remediation

Include the reactive input in the memo key, or refresh the entry with `load` /
`delete` / `clear` at the write that owns invalidation.

## Fixtures

- Invalid: `fixtures/rules/no-memoize-stale-result-demand/invalid/`
- Valid: `fixtures/rules/no-memoize-stale-result-demand/valid/`
