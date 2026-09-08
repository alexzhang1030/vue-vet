# `vue-vet/reactivity/no-inactive-scope-result`

Category: reactivity  
Default severity: warning  
Confidence: high

`effectScope.run` returns `undefined` after `scope.stop()`. Using that result
as an object, member, or call throws at runtime.

## Bad

```vue
<script setup lang="ts">
import { effectScope } from 'vue'
const scope = effectScope()
scope.stop()
const result = scope.run(() => ({ count: 1 }))
void result.count
</script>
```

## Good

```vue
<script setup lang="ts">
import { effectScope } from 'vue'
const scope = effectScope()
const result = scope.run(() => ({ count: 1 }))
void result.count
scope.stop()
</script>
```

Ignored `run` results, optional/guarded consumers, live scopes, conditional
`stop`, unknown owners, escaped or mutated scopes, prior return/throw/unknown
regions between `run` and the consumer, and uninvoked nested-function
consumers stay quiet. Returning the scope handle is legitimate. Same-region
straight-line `stop` then `run` then object use still reports.

## Detection

Fact-driven via Vue Vet source-contract facts. Requires a proven local
`effectScope`, an unconditional `stop()` before `run()` on the same
straight-line execution, and a demanded object use of the result.

This is distinct from late `onScopeDispose` and orphaned watcher ownership.

## Remediation

Run the callback before stopping, or do not dereference the result.

## Fixtures

- Invalid: `fixtures/rules/no-inactive-scope-result/invalid/`
- Valid: `fixtures/rules/no-inactive-scope-result/valid/`
