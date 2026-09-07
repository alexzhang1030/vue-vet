# `vue-vet/reactivity/no-once-immediate-discard`

Category: reactivity  
Default severity: warning  
Confidence: high

`watch` with effective `{ once: true, immediate: true }` runs the callback
exactly once during creation. Vue passes `undefined` as `old` for a proven
single source. A top-level `if (old === undefined) return` (or `void 0`)
therefore exits before any later call or write that uses `new`.

Reactive arrays are classified as a single reactive root. A multi-source
tuple is a plain array of sources; its initial `old` is `[]`, so
`old === undefined` is a normal first run and stays quiet.

## Bad

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'
const n = ref(0)
function accept(_value: unknown) {}
watch(
  n,
  (next, old) => {
    if (old === undefined) return
    accept(next)
  },
  { once: true, immediate: true },
)
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'
const n = ref(0)
function accept(_value: unknown) {}
watch([n], (next, old) => {
  if (old === undefined) return
  accept(next)
}, { once: true, immediate: true })
watch(
  n,
  (next) => {
    accept(next)
  },
  { immediate: true },
)
</script>
```

Once-only, immediate-only, initial work inside the guard, `try/finally`,
rest/default/destructured parameters, imported callbacks, unknown or
duplicate/spread options, a local `undefined` binding, `void` of a call
(side-effecting), unreachable work after `return`, and later work only
inside an uncertain/`if (false)` branch or nested `{ return }` block stay
quiet. Supported single
sources are a ref, a getter, or an actually-reactive root. Plain tuples
stay quiet.

## Detection

Fact-driven via Vue Vet source-contract facts from Oxc symbol identity.

## Remediation

Drop `once`, drop the initial-undefined guard, or perform the useful work on
the initial invocation.

## Fixtures

- Invalid: `fixtures/rules/no-once-immediate-discard/invalid/`
- Valid: `fixtures/rules/no-once-immediate-discard/valid/`
