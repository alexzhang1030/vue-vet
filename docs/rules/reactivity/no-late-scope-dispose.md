# `vue-vet/reactivity/no-late-scope-dispose`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

`onScopeDispose` records a hook on the **currently active** effect scope.
After `await` inside a proven async `effectScope().run` callback, that scope is
no longer active, so the hook is dropped and Vue warns at runtime.

## Bad

```vue
<script setup lang="ts">
import { effectScope, onScopeDispose } from 'vue'

const scope = effectScope()
await scope.run(async () => {
  await Promise.resolve()
  onScopeDispose(() => {})
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { effectScope, onScopeDispose } from 'vue'

const scope = effectScope()
await scope.run(async () => {
  onScopeDispose(() => {})
  await Promise.resolve()
})
</script>
```

## Detection

Proven Vue `onScopeDispose` import inside a proven async `effectScope().run`
callback (inline or named) after a straight-line `await`. Registration before
`await` stays quiet. An explicit inner synchronous `scope.run` stays quiet.
`scope.on()` / `scope.off()` in that callback stays quiet: Vue 3.5.40
`scope.on()` after `await` restores the active scope. Passing the scope value
to an unknown helper, or assigning `scope.run`, stays quiet.

Runtime evidence (Vue 3.5.40): `onScopeDispose(fn, true)` (`failSilently`)
suppresses Vue's own warning and still does not attach the hook. This rule
stays quiet when the second argument is the boolean literal `true`, matching
that explicit suppression. Other second arguments are not treated as
suppression.

## Applicability

Plain TypeScript and SFC. Unknown scope values and unproven control-flow stay
quiet.

## Remediation

Register `onScopeDispose` before `await`, or re-enter `scope.run` synchronously.
Use `failSilently: true` only when skipping the missing-scope warning is
intentional.
