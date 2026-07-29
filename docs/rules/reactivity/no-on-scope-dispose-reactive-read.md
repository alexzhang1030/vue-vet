# `vue-vet/reactivity/no-on-scope-dispose-reactive-read`

Category: reactivity  
Default severity: warning  
Confidence: high

`onScopeDispose` runs when an effect scope is disposed. Reactive reads there do
not establish ongoing tracking the way `computed` / `watchEffect` do, and they
often indicate accidental dependency use during teardown.

## Bad

```vue
<script setup lang="ts">
import { effectScope, onScopeDispose, ref } from 'vue'

const count = ref(0)
const scope = effectScope()
scope.run(() => {
  onScopeDispose(() => {
    console.log(count.value)
  })
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { effectScope, onScopeDispose, ref } from 'vue'

const count = ref(0)
const scope = effectScope()
scope.run(() => {
  onScopeDispose(() => {
    // teardown only — no reactive reads
    console.log('disposed')
  })
})
void count
</script>
```

## Detection

Fact-driven via `vue_vet_reactivity` tracking scopes (`OnScopeDispose` reads).

## Remediation

Keep dispose callbacks free of reactive dependency reads. Capture plain values
earlier if you need them for cleanup logging.
