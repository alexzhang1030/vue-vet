# `vue-vet/reactivity/no-proxy-structured-clone`

Category: reactivity

Default severity: warning

Confidence: high

Native `structuredClone` throws `DataCloneError` when it reaches an actual
ECMAScript Proxy. Vue `reactive` / `readonly` / `shallowReactive` /
`shallowReadonly` allocate a Proxy for a fresh object or array. This rule
reports when that proven Proxy is the single data argument of unshadowed
`structuredClone`.

Actual Proxy proof accounts for Vue returning the raw target for marked,
frozen, or non-extensible input. `DeepProxy` / `ShallowProxy` /
`ReadonlyProxy` describe the constructor's result kind.

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
`for...of` assignment heads). Variable-declaration loop heads retain binding
semantics. Dynamic computed calls retain unknown native identity. Known
unrelated static keys (`globalThis['fetch'] = ...`) preserve clone identity. Nested
object/ref payloads and MessagePort receivers are out of this slice (same ID
later).

The reported span is the original data argument, including TypeScript
assertions used only as syntax. Ordinary later mutations of a proven Proxy
keep Proxy identity. Snapshot construction requires an explicit choice of fields.

## Detection

Fact-driven via `SourceContractFacts.uncloneable_proxy_data`. The callee must
be an Oxc unresolved global `structuredClone` (or unshadowed
`globalThis.structuredClone`) with no known writes to that intrinsic in the
module. A dedicated native-capability fact records clone identity.

File-rule eligibility currently also runs when any script call exists, so a
native-only module with no Vue imports is analyzed and stays quiet. Native
sinks enter the call walk through their own capability classification.

## Remediation

Build a plain snapshot with explicitly selected cloneable fields. `toRaw` is
shallow, so nested stored proxies and other uncloneable data require separate
handling.

## Fixtures

- Invalid: `fixtures/rules/no-proxy-structured-clone/invalid/`
- Valid: `fixtures/rules/no-proxy-structured-clone/valid/`

Runtime evidence: pinned Vue 3.5.40 + Node `structuredClone`
(`just oracle-source-contracts`) asserts exception name `DataCloneError` for
the promised positives and successful clone of the safe controls.
