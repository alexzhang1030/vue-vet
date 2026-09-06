# `vue-vet/reactivity/no-proxy-structured-clone`

Category: reactivity  
Default severity: warning  
Confidence: high

Native `structuredClone` throws `DataCloneError` when it reaches an actual
ECMAScript Proxy. Vue `reactive` / `readonly` / `shallowReactive` /
`shallowReadonly` allocate a Proxy for a fresh object or array. This rule
reports when that proven Proxy is the single data argument of unshadowed
`structuredClone`.

Existing `DeepProxy` / `ShallowProxy` / `ReadonlyProxy` shape labels are not
enough: Vue returns the raw target for marked, frozen, or non-extensible
input.

## Bad

```vue
<script setup lang="ts">
import { reactive, readonly, shallowReactive, shallowReadonly } from 'vue'
structuredClone(reactive({ count: 1 }))
const state = shallowReactive({ count: 1 })
const alias = state
structuredClone(alias)
structuredClone(readonly({ count: 1 }))
structuredClone(shallowReadonly({ count: 1 }))
</script>
```

## Good

```vue
<script setup lang="ts">
import { markRaw, reactive, readonly, shallowRef, toRaw } from 'vue'
structuredClone({ count: 1 })
structuredClone(reactive(markRaw({ count: 1 })))
const frozen = Object.freeze({ count: 1 })
structuredClone(readonly(frozen))
structuredClone(toRaw(reactive({ count: 1 })))
structuredClone(shallowRef({ count: 1 }).value)
function structuredClone(_value: unknown) {}
structuredClone(reactive({ count: 1 }))
</script>
```

Quiet also includes Vue marker keys (`__v_skip`, `__v_isReadonly`, `__v_isRef`,
`__v_raw`, `__proto__`), spreads, computed keys, accessors, raw bindings passed
into a constructor (`const raw = {}; reactive(raw)`), two-argument calls,
optional `structuredClone?.(...)`, constructors imported from `#imports` or
`vue-demi`, `window.structuredClone`, and writes that replace the unresolved
global or unshadowed `globalThis.structuredClone` (computed string keys,
unresolved computed keys that could name that intrinsic, destructuring
default/rest, TypeScript wrappers, `delete`, updates, and `for...in` /
`for...of` assignment heads). Variable-declaration loop heads are not
mutations. A computed *call* `globalThis[key](...)` is not a definite native
intrinsic. Known unrelated static keys (`globalThis['fetch'] = ...`) do not
poison. Nested
object/ref payloads and MessagePort receivers are out of this slice (same ID
later).

The reported span is the original data argument, including TypeScript
assertions used only as syntax. Ordinary later mutations of a proven Proxy
keep Proxy identity. There is no universal `toRaw(...)` autofix.

## Detection

Fact-driven via `SourceContractFacts.uncloneable_proxy_data`. The callee must
be an Oxc unresolved global `structuredClone` (or unshadowed
`globalThis.structuredClone`) with no known writes to that intrinsic in the
module. Native identity is not a Vue API whitelist entry.

File-rule eligibility currently also runs when any script call exists, so a
native-only module with no Vue imports is analyzed and stays quiet. A future
native-only sink must not require a Vue `info.api` to enter the call walk.

## Remediation

Build a plain snapshot with explicitly selected cloneable fields. Do not wrap
the value with `toRaw` as a universal fix: `toRaw` is shallow, and nested
stored proxies or other uncloneable data can remain.

## Fixtures

- Invalid: `fixtures/rules/no-proxy-structured-clone/invalid/`
- Valid: `fixtures/rules/no-proxy-structured-clone/valid/`

Runtime evidence: pinned Vue 3.5.40 + Node `structuredClone`
(`just oracle-source-contracts`) asserts exception name `DataCloneError` for
the promised positives and successful clone of the safe controls.
