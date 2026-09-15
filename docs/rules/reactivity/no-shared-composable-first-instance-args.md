# `vue-vet/reactivity/no-shared-composable-first-instance-args`

Category: reactivity  
Default severity: warning  
Confidence: high

`createSharedComposable` / `createGlobalState` (`@vueuse/core` and
`@vueuse/shared`) run the factory only for the first live instance. Later
arguments are discarded. A later unguarded demand that the retained first
value cannot satisfy throws, even when the discarded argument would have
worked.

## Bad

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { createSharedComposable } from '@vueuse/core'
const useValue = createSharedComposable((value: string | number) => ref(value))
const first = useValue(1)
const second = useValue('text')
void second.value.toUpperCase()
</script>
```

## Good

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { createSharedComposable } from '@vueuse/core'
const useValue = createSharedComposable((value: number) => ref(value))
const first = useValue(1)
const second = useValue(2)
first.value = 3
void second.value.toFixed(0)
</script>
```

## Safe (stays quiet)

`createGlobalState` uses the same first-instance argument rule with process
lifetime. Distinct numeric seeds with shared updates stay quiet. Stopping
the last `createSharedComposable` owner before a new string instance stays
quiet. A `typeof` guard before the demand stays quiet. A write through any
alias of the live shared ref before the demand repairs the retained value.
The first call in program order owns the retained state: an earlier
unproven argument, spread, setup call before `effectScope.run`, or
`createGlobalState` call in another callable poisons that wrapper.

## Not claimed by this owner (quiet, still leaks at runtime)

Two live owner scopes, a factory passed by reference, and a callable
escaped into a helper still keep the first initializer at runtime. Optional
chaining is not a type guard — `second.value?.toUpperCase()` still throws
on a retained number. Local shadows of the VueUse export are a different
binding and are out of scope.

## Detection

Fact-driven via Vue Vet source-contract facts. The factory must be an inline
synchronous function that returns Vue `ref` / `shallowRef` of an unchanged
parameter. The first `useValue(...)` call after the wrapper in source order
is the retained initializer; later calls are compared only against that
site. Both calls must share one live setup or `effectScope.run` owner
(`createGlobalState` needs only the process lifetime). Reports the first
proven leaking demand per wrapper. The main span is the failing demand;
related spans identify the first call and the discarded later argument.

## Remediation

Use independent composable instances, or an explicitly keyed factory/cache,
when each argument should own its own state.

## Fixtures

- Invalid: `fixtures/rules/no-shared-composable-first-instance-args/invalid/`
- Valid: `fixtures/rules/no-shared-composable-first-instance-args/valid/`
