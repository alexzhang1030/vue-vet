# `vue-vet/reactivity/no-inject-same-instance-provide`

Category: reactivity  
Default severity: warning  
Confidence: high

`provide` in a Vue component setup supplies descendants. `inject` in that same
setup reads the ancestor and app chain, not the current instance. A fresh
native `Symbol()` created in that setup cannot exist on an ancestor, so
`inject` resolves to its fallback. An unguarded native method demand that the
fallback lacks and the locally provided value supplies throws at runtime.

## Bad

```vue
<script setup lang="ts">
import { inject, provide } from 'vue'
const key = Symbol('count')
provide(key, 7)
const count = inject(key, 'missing')
count.toFixed(2)
count!.toFixed(2)
;(count as number).toFixed(2)
count['toFixed'](2)
</script>
```

`count?.toFixed(2)` is also a finding when the fallback is a non-nullish
primitive: optional chaining only short-circuits `null` / `undefined`.

## Good

```vue
<script setup lang="ts">
import { inject, provide } from 'vue'
const key = Symbol('count')
provide(key, 7)
const count = inject(key, 0)
count.toFixed(2)
const missing = inject(key)
missing?.toFixed(2)
const local = 7
local.toFixed(2)
</script>
```

Safe in the first slice: a numeric (or otherwise same-capability) fallback, a
string / imported / shared / `Symbol.for` key that an ancestor or the app may
provide, a real type/logical/ternary/`try` guard, `count?.toFixed?.(2)` (optional
*call*), and `count?.member` only when the fallback is absent, `undefined`, or
`null`.

## Limitations

Out of first-slice scope — runtime can still throw, but this slice stays quiet:

- Nested functions called from setup (`inner()` still uses `currentInstance`)
- Composable / helper `provide` (`useProvide()`)
- `console.log(count)` (or other result escape) before the demand
- Factory default without a proven `true` flag (`inject(key, () => 'x')`)
- Escaped or module-exported keys, mutable or conditional `provide`

## Detection

Fact-driven via Vue Vet source-contract facts. The first slice is a direct
straight-line `<script setup>`: a const native `Symbol()`, one resolved Vue
`provide`/`inject` pair on that key, a known fallback (including unary numeric
literals such as `-1`) or third-argument default factory, and a later unguarded
native callable-member demand (static or computed string-literal, including
TypeScript `!` / `as` / `satisfies` wrappers). Optional member access is a
guard only for a proven nullish fallback. The primary span is the failing
demand. Related spans identify the key, provide, and inject.

## Remediation

Use the local value in this setup, or inject a key that an ancestor or the app
actually provides.

## Fixtures

- Invalid: `fixtures/rules/no-inject-same-instance-provide/invalid/`
- Valid: `fixtures/rules/no-inject-same-instance-provide/valid/`
