# `vue-vet/reactivity/no-torefs-on-non-proxy`

Category: reactivity  
Default severity: warning  
Confidence: high

`toRefs` on a proven plain object or array returns refs that do not subscribe
to later mutations of the original value.

## Bad

```vue
<script setup lang="ts">
import { toRefs } from 'vue'
const { a } = toRefs({ a: 1 })
void a
</script>
```

## Good

```vue
<script setup lang="ts">
import { reactive, toRefs } from 'vue'
const { a } = toRefs(reactive({ a: 1 }))
void a
</script>
```

Known `reactive` / `readonly` / `shallow*` proxies and `defineProps` stay
quiet. Unknown arguments stay quiet. The reported span is the `toRefs`
argument.

## Detection

Fact-driven via Vue Vet source-contract facts. Immutable locals initialized
from object/array literals report when provenance cannot change.

## Remediation

Wrap with `reactive` / `readonly` first, or use `toRef` on a real proxy.

## Fixtures

- Invalid: `fixtures/rules/no-torefs-on-non-proxy/invalid/`
- Valid: `fixtures/rules/no-torefs-on-non-proxy/valid/`
