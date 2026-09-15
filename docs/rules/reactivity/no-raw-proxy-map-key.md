# `vue-vet/reactivity/no-raw-proxy-map-key`

Category: reactivity  
Default severity: warning  
Confidence: high

A native `Map` compares keys by JavaScript identity. A fresh plain object and
the Vue `reactive` / `shallowReactive` proxy of that object are distinct keys, so
`map.get(proxy)` is `undefined` when the entry was stored under `raw` (and the
other direction too). Unguarded member or call demand on that result throws.

Applicability is a **demand slice**: the Map is a local `const` of `new Map`
whose constructor is the unresolved global (not poisoned by `Map` /
`globalThis.Map` reassignment, shadowing, or dynamic writes). Entries are a
closed literal initializer or straight-line `.set`. The mismatched `get` uses
the opposite identity of a proven raw/proxy pair from `vue` / `@vue/runtime-*` /
`@vue/reactivity`. Direct demand and a local `const` result alias are reported
when unguarded.

`vue-demi` and `#imports` constructors stay unproven. Vue `reactive(new Map)`
and `shallowReactive(new Map)` instrument `get`/`set` (lookup uses `toRaw`)
and stay quiet. Same-identity get, optional / nullish / `if`
guards, default fallbacks, bare `has`/`get` without a failing consumer, unknown
maps, escaped receivers, and proven storage of both identities stay quiet.
Own `.get` / `.set` / `.has` uses on a proven native `Map` do not poison a
native positive only while that method remains the native one; an own, aliased,
or escaped override of the invoked method poisons the key argument. Same-spelling
helper methods do. Mutating fields of the raw object keeps the same identity.
Repeated `reactive(raw)` / `shallowReactive(raw)` share identity within one
flavor. `??` uses nullishness (`0` skips the RHS); `&&` / `||` use truthiness.
Demand requires execution from the callable region entry through the Map
declaration, get, and use. A `return` / `throw` / `await` before the declaration
or a skipped `&&` / `||` / `??` / logical-assignment / optional-call argument
or optional computed key stays quiet.

The primary span is the unguarded demand. Help cites the stored key and the
wrapper that produced the proxy identity used in the mismatch, and it is
directional: wrapping repairs raw-stored keys, not proxy-stored keys.

## Bad

```vue
<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const map = new Map([[raw, { count: 1 }]])
void map.get(proxy).count
</script>
```

## Good

```vue
<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const map = reactive(new Map([[raw, { count: 1 }]]))
void map.get(proxy).count
</script>
```

## Detection

Fact-driven via Vue Vet source-contract facts (Oxc symbol identity). Not generic
call syntax.

## Remediation

When the stored key is the raw object, look up that raw identity, or wrap the
Map with `reactive` / `shallowReactive` so later proxy lookups are normalized.
When the stored key is already the proxy, look up that proxy identity; wrapping
the Map does not rewrite keys that were stored as proxies.

## Fixtures

- Invalid: `fixtures/rules/no-raw-proxy-map-key/invalid/`
- Valid: `fixtures/rules/no-raw-proxy-map-key/valid/`
