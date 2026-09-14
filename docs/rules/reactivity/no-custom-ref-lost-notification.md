# `vue-vet/reactivity/no-custom-ref-lost-notification`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

A `customRef` with a valid get/set interface and a plain local primitive slot
only notifies consumers when `get` actually calls `track` (or reads backing
reactive state) and `set` actually calls `trigger` (or writes backing reactive
state, or later invokes a saved trigger / `triggerRef`). Mentioning those
parameter names is not enough.

Vue Vet reports a missing getter-tracking or setter-notification path when a
local `watch` / `watchEffect` consumer is already subscribed and a later write
stores a different known primitive. `.value` reads or writes by themselves are
legitimate synchronous snapshots and stay quiet without that consumer + changed
write pair.

Invalid `customRef` interfaces (missing or non-callable get/set) belong to a
separate pending contract. This rule requires a valid interface and keeps
primitive-slot cases separate from backing-ref bridges.

## Bad

```vue
<script setup lang="ts">
import { customRef, watch } from 'vue'

const count = customRef((_track, trigger) => {
  let value = 0
  return {
    get() {
      return value
    },
    set(next) {
      value = next
      trigger()
    },
  }
})
watch(count, () => {})
count.value = 1
</script>
```

## Good

```vue
<script setup lang="ts">
import { customRef, watch } from 'vue'

const count = customRef((track, trigger) => {
  let value = 0
  return {
    get() {
      track()
      return value
    },
    set(next) {
      value = next
      trigger()
    },
  }
})
watch(count, () => {})
count.value = 1
</script>
```

Backing `ref` / `reactive` bridges, deferred/saved `trigger`, external
`triggerRef`, same-value writes, stopped or paused consumers, helper
delegation, member/IIFE/holder-alias capability forwarding, non-inline factories, unknown
aliases, post-flush first runs (`watchPostEffect`, `watchEffect({ flush: 'post' })`),
guarded registration, `once` + `immediate` watchers, unread effect paths,
constant setters, compound or mutated setter transfers, getter/factory extra
storage writes (including executed object keys/values and proven
conditional/logical arms), factory destructuring whose default does not run, a present `undefined`
value that does run the default, a nested default object that supplies inner
properties, inherited or standard-prototype keys that are not proven missing,
property-order key and default evaluation,
shadowed `undefined`, object-valued class evaluation, generator or async
accessors, coercing `==` skips, later accessor replacement, and effect reads
after an earlier `await`/`yield` or a nested block `return` stay quiet.
Unsupported evaluation shapes are Unknown until modeled. Language `undefined`
is the unbound identifier, not a local of that name.

## Detection

Fact-driven via Vue Vet source-contract facts. Proven Vue `customRef` is a named
or namespace import from `vue`, `vue-demi`, `@vue/runtime-core`,
`@vue/runtime-dom`, or `@vue/reactivity`, plus named `#imports` specifiers.
The factory must be an inline synchronous function that returns a fresh plain
object with callable `get` and `set`. Track/trigger use Oxc symbol identity:
actual calls and const aliases, not mentions. One rule ID; the reason enum
distinguishes getter tracking from setter notification.

## Remediation

Call `track()` on the executed get path and `trigger()` on the executed set
path, or keep the value in a backing `ref` / `reactive` so Vue's own
dependency system notifies.

## Fixtures

- Invalid: `fixtures/rules/no-custom-ref-lost-notification/invalid/`
- Valid: `fixtures/rules/no-custom-ref-lost-notification/valid/`
