# `vue-vet/reactivity/no-extracted-reactive-collection-method`

Category: reactivity

Default severity: warning

Confidence: high

Vue instruments `Map` / `Set` / array methods on `reactive` and
`shallowReactive` proxies. Extracting a method (`const { get } = map` or
`const get = map.get`) and calling it bare drops the proxy receiver. Vue 3.5.40
throws `TypeError`. Extracted `Array.push` can also skip tracking-batch
cleanup.

One ID covers the shared receiver-loss premise. First coverage is Map
`get` / `set` / `has`, Set `has` / `add`, and Array `map` / `includes` /
`push`.

Applicability requires a proven Vue runtime `reactive` / `shallowReactive`
(`vue`, `@vue/runtime-core`, `@vue/runtime-dom`, `@vue/reactivity`) wrapping a
fresh intrinsic `new Map` / `new Set` or array literal, a local **const**
extraction of a covered method, and a later reachable bare call. Import aliases,
namespace imports, const aliases, and TypeScript wrappers are followed.
Collection identity uses a dedicated capability query (static/computed/pattern
writes, update/delete, loop assignment targets, helper/`new`/tagged receivers,
and bounded logical/conditional/sequence/aggregate escapes) with precomputed
alias roots. On escape-depth exhaustion, identifier roots referenced
inside a leftover nested-expression span stay
unknown, and unresolved native `Map` / `Set` / `Array` (including
`.prototype`) identifiers in that span taint constructor identity. Const
aliases of those constructors and `.prototype` objects share that native-kind
identity on ordinary and leftover escapes. A true
`globalThis` alias escape taints every supported intrinsic; local shadows of
those names stay distinct. Native `Map` / `Set` / `Array` provenance is unknown after
unresolved `globalThis` constructor writes. Short-circuit and post-return calls
stay quiet. `vue-demi`, `#imports`, unresolved API spelling, collection factories,
prototype mutation, overwritten methods, escaped collections, dynamic keys,
and mutated bindings stay quiet. Generic source5 `uncertain` / `escaped` is
unchanged.

The primary span is the bare call. Help cites the constructor and extraction
sites and recommends keeping the receiver. No autofix.

## Bad

```vue
<script setup lang="ts">
import { reactive } from 'vue'
const map = reactive(new Map([['a', 1]]))
const { get } = map
get('a')
</script>
```

## Good

```vue
<script setup lang="ts">
import { reactive } from 'vue'
const map = reactive(new Map([['a', 1]]))
map.get('a')
</script>
```

Receiver-preserving alternatives that stay quiet: `get.call(map, key)`,
`get.apply(map, [key])`, `Reflect.apply(get, map, [key])`, and
`map.get.bind(map)`.

## Detection

Detection uses Oxc symbol identity and Vue Vet source-contract facts.
Native `Map` / `Set` globals must be unshadowed.

## Remediation

Keep the collection as the call receiver, or bind / `call` / `apply` the
extracted function with that receiver.

## Fixtures

- Invalid: `fixtures/rules/no-extracted-reactive-collection-method/invalid/`
- Valid: `fixtures/rules/no-extracted-reactive-collection-method/valid/`
