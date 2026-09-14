# `vue-vet/reactivity/no-invalid-custom-ref-interface`

Category: reactivity  
Default severity: warning  
Confidence: high

A proven `customRef` factory that returns a fresh plain object without a
callable `get` or `set` throws `TypeError` when that capability is demanded
through `.value`.

A read requires `get`. A write requires `set`. Unused refs and getter-only
reads with a valid `get` stay quiet.

## Bad

```vue
<script setup lang="ts">
import { customRef } from 'vue'
const count = customRef(() => ({
  set() {},
}))
void count.value
</script>
```

## Good

```vue
<script setup lang="ts">
import { customRef } from 'vue'
const count = customRef((track, trigger) => {
  let value = 0
  return {
    get() {
      track()
      return value
    },
    set(next: number) {
      value = next
      trigger()
    },
  }
})
count.value = 1
void count.value
</script>
```

Named aliases, `import * as Vue`, and TypeScript wrappers use Oxc symbol
identity. Assignment-RHS `.value` is a getter read; `delete` is not. Global
`undefined` is the unresolved identifier; a local of that name stays unknown.
Dynamic keys, accessors, spreads, mutated or escaped refs, non-inline
factories, and getter/setter bodies that mutate or escape the impl receiver
before a later demanded capability stay quiet. Computed object keys and
destructuring default initializers execute during construction and use the
same receiver-effect walk; a getter that assigns `this._set` from a computed
key is Uncertain. A closed getter followed by a write still reports a missing
`set`. A write-only demand still reports when an uncertain getter never ran.

## Detection

Fact-driven via Vue Vet source-contract facts. The factory must be an inline
function with one unambiguous returned object (concise arrow or a single
return, no conditional body). Findings require a same-file `.value` read or
write for the missing capability.

## Remediation

Return `{ get, set }` callables, or drop the unused `.value` demand.

## Fixtures

- Invalid: `fixtures/rules/no-invalid-custom-ref-interface/invalid/`
- Valid: `fixtures/rules/no-invalid-custom-ref-interface/valid/`
