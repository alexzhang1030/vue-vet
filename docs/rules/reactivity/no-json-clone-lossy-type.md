# `vue-vet/reactivity/no-json-clone-lossy-type`

Category: reactivity  
Default severity: warning  
Confidence: high

Default `useCloned` JSON cloning turns a nested `Date` into an ISO string.
Calling `.getTime()` or `.getUTCFullYear()` on that output throws.

A string consumer of the same path stays quiet. Custom clones, delayed
initialization, helpers, optional or guarded demand, identity or nested-path
repair, result-alias repair, executing watcher hooks, `toJSON` bearers, and
mutated `Date` / `String` / `JSON` intrinsics stay unknown.

## Bad

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { useCloned } from '@vueuse/core'
const { cloned } = useCloned(ref({ when: new Date('2020-01-01') }))
cloned.value.when.getTime()
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { useCloned } from '@vueuse/core'
const { cloned } = useCloned(ref({ when: new Date('2020-01-01') }), {
  clone: (value) => ({ when: new Date(value.when) }),
})
cloned.value.when.getTime()
</script>
```

Named aliases, `import * as VueUse` from `@vueuse/core`, and TypeScript
wrappers use Oxc symbol identity. `@vueuse/shared` does not export
`useCloned`. Map/Set loss is not claimed until each native output and
demand is proven separately.

## Detection

Fact-driven via Vue Vet source-contract facts. The clone must be the default
JSON intrinsic (`JSON.parse(JSON.stringify(...))`), the source a local Vue
`ref` of a fresh own-data object with a native `Date`, and a reachable
unguarded Date method must run after initialization with no identity,
nested-path, or result-alias repair. Present-but-unknown `clone` options,
executing watcher hooks, custom `toJSON`, and mutated `String.prototype` /
`JSON.parse` stay unproven.

## Remediation

Use a domain-appropriate clone that preserves `Date` methods. JSON.stringify
converts `Date` to a string, so date methods are not functions on the result.

## Fixtures

- Invalid: `fixtures/rules/no-json-clone-lossy-type/invalid/`
- Valid: `fixtures/rules/no-json-clone-lossy-type/valid/`
