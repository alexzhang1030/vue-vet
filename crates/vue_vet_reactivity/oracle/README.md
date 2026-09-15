# Reactivity runtime oracle

Ground truth for static under-approximation checks.

Vue's `onTrack` (on `computed` / `watchEffect` / `watch`) records the real
dependency set during synchronous tracking. The static tracer must satisfy:

```text
tracer_reads ⊆ runtime_deps   (no invented edges)
```

and we report **recall** `|intersection| / |runtime_deps|` as a measured
completeness number — not a 280-case syntax matrix.

## Cases

| id | Intent |
| --- | --- |
| `baseline-ref-computed` | happy path ref → computed |
| `computed-object-get` | `computed({ get, set })` tracks getter reads |
| `computed-fn-ref` | `computed(load)` tracks the referenced getter body |
| `computed-helper-call` | `computed(() => load())` tracks the same helper body as `computed(load)` |
| `computed-helper-ternary` | `computed(() => cond ? load() : 0)` tracks cond + helper reads |
| `watch-source-fn-ref` | `watch(load)` treats a local function as a source getter |
| `pause-tracking-window` | `pauseTracking`/`enableTracking` window drops mid-window reads |
| `pause-tracking-helper` | `pauseTracking` inside `load()` drops the helper read; later `enableTracking` still tracks |
| `reset-tracking-window` | `pauseTracking`/`resetTracking` window drops mid-window reads |
| `props-reactive-object` | `props.count` style reactive object (defineProps stand-in) |
| `reactive-member` | `reactive({ count }).count` member track |
| `sync-every-hof` | sync Array#every callback must track `threshold` |
| `sync-filter-hof` | sync Array#filter callback must track `query` |
| `sync-find-hof` | sync Array#find callback must track `target` |
| `sync-findIndex-hof` | sync Array#findIndex callback must track `target` |
| `sync-findLast-hof` | sync Array#findLast callback must track `target` |
| `sync-findLastIndex-hof` | sync Array#findLastIndex callback must track `target` |
| `sync-flatMap-hof` | sync Array#flatMap callback must track nested reads |
| `sync-forEach-hof` | sync Array#forEach callback must track `factor` |
| `sync-map-hof` | sync Array#map callback must track `factor` |
| `sync-reduce-hof` | sync Array#reduce callback must track `factor` |
| `sync-reduceRight-hof` | sync Array#reduceRight callback must track `factor` |
| `sync-some-hof` | sync Array#some callback must track `threshold` |
| `array-from-mapfn` | `Array.from(iter, mapFn)` tracks mapFn body |
| `json-parse-reviver` | `JSON.parse(text, reviver)` tracks reviver body |
| `sort-hof` | Array#sort comparator tracks nested reactive reads |
| `string-replace-hof` | String#replace replacer tracks nested reactive reads |
| `string-replaceAll-hof` | String#replaceAll replacer tracks nested reactive reads |
| `toSorted-hof` | Array#toSorted comparator tracks nested reactive reads |
| `to-value-getter` | `toValue(() => count.value)` tracks getter body |
| `use-route-like` | reactive route object member (`route.path`) |
| `lvalue-object` | nested `draft.value.params.x = time.value` gets draft.value |
| `lvalue-index-write` | `target[key.value] = source.value` tracks key and source, not target |
| `watch-effect-ref` | `watchEffect` tracks `ref.value` |
| `watch-effect-await` | post-await read is **not** runtime-tracked (boundary) |
| `watch-source-array` | `watch([a, b])` tracks each ref `.value` |
| `watch-source-array-getters` | `watch([() => a.value, () => b.value])` each getter body |
| `watch-source-getter` | `watch(() => value.value)` source getter |
| `watch-source-parens` | `watch((ref))` tracks the same `.value` as `watch(ref)` |
| `watch-source-ref` | `watch(ref)` tracks `.value` (not property-less) |
| `watch-source-reactive-deep` | `watch(reactive)` → static deep-root `*`; runtime has many keys |
| `runner-run-no-track` | arbitrary `.run` invents nothing at runtime |

Bare `watch(reactiveObj)` emits static `property: "*"` (deep/iterate root). The
oracle treats `*` as under-approx when the binding appears in any runtime dep —
never invent concrete nested keys.

Static-only (no oracle JSON): `storeToRefs` from `pinia` — unit-tested; runtime
`toRefs` tracks the **underlying store object**, so key identity differs from
local ref names and is not a fair under-approx pair without alias mapping.

`pause-tracking-window` / `reset-tracking-window` exercise `@vue/reactivity`'s
`pauseTracking` / `enableTracking` / `resetTracking` (not re-exported from the
public `vue` package in 3.5.x). The static source still names them under
`from 'vue'` to match docs / re-exports.

Runtime contract oracles (Node, frozen Vue 3.5.40 lock) live beside this
onTrack suite: `lifetime-runs.mjs`, `source-contracts.mjs`,
`lost-notification-runs.mjs`, and `cleanup-identity-runs.mjs`
(`just oracle-cleanup-identity`). They are not onTrack JSON.

## Refresh expected JSON

```bash
cd crates/vue_vet_reactivity/oracle
pnpm install
pnpm oracle:write
```

Committed `expected/*.json` are the CI source of truth so Rust tests do not
require Node at test time.

## Gate (Evidence complete)

`just oracle` (or `cargo test -p vue_vet_reactivity --lib oracle`) loads each
committed expected file, runs `trace_reactivity` on `source`, and asserts:

- **under-approx:** `tracer ⊆ runtime` (no invented concrete keys; deep root
  `*` is allowed when the binding appears in any runtime dep)
- **recall:** ≥99% on this **representative** case set

This is a recall gate on committed cases — not a claim that every SFC in the
universe is covered. Static-only joins (e.g. parent `:foo` → child props) stay
in Rust unit/project tests.

## Contract oracles (not onTrack JSON)

These Node scripts pin Vue / VueUse runtime premises for source-contract rules.
They use this package's locked `node_modules` and are separate from `just oracle`.

| Recipe | Script | Pin |
| --- | --- | --- |
| `just oracle-source-contracts` | `source-contracts.mjs` | Vue 3.5.40 |
| `just oracle-value-contracts` | `value-contracts.mjs` | Vue 3.5.40 |
| `just oracle-filter-settlement` | `filter-settlement-contracts.mjs` | Vue 3.5.40 + `@vueuse/core` / `@vueuse/shared` 13.9.0 |
| `just oracle-self-trigger` | `self-trigger-runs.mjs` | Vue 3.5.40 run counts |
| `just oracle-lifetime` | `lifetime-runs.mjs` | Vue 3.5.40 |
| `just oracle-model-demand` | `model-demand.mjs` | Vue 3.5.40 + `@vue/compiler-sfc` / `@vue/server-renderer` 3.5.40 |

`filter-settlement-contracts.mjs` asserts `useDebounceFn` same-turn supersession
fulfills `undefined`, sequential/zero-delay/`maxWait: 0` preserves both results,
an await longer than the delay between calls preserves the first result,
`maxWait > 0` fulfills **both** promises `undefined`, trailing / `leading: false`
throttle cancel a pending promise, and a `.then` handler on the cancelled
promise is an unhandled `TypeError`.

## Run counts and template host

`just oracle-custom-ref-notification` (`custom-ref-notification-runs.mjs`)
asserts Vue 3.5.40 `customRef` run counts for lost `track`, lost `trigger`,
track-in-setter, standard, backing ref/reactive, deferred trigger, `triggerRef`,
same-value write, no consumer, stop/pause, helper/unknown factories, post-flush
first runs, guarded registration, `once`+`immediate`, unread effect paths,
prior same-value writes, constant setters, coercing `==`, member/IIFE
capability forwarding, and later accessor replacement.

`just oracle-scheduling-practice` (`scheduling-practice.mjs`) pins Vue 3.5.40
and VueUse core 13.9.0 for queued `watch` flush, attached child `effectScope`,
and lazy `computedAsync` startup. Independent round-7 probes remain read-only.

`just oracle-until-demand` (`until-demand.mjs`) pins Vue 3.5.40 and VueUse
13.9.0 for `until(ref).toBe` timeout unmatched-demand: timeout fulfills the
current unmatched value, expected-kind demand throws, current-kind demand is
valid, an intervening write matches, `throwOnTimeout` rejects, and already
matched / zero-timeout / optional-chain / compound-assign controls.

`just oracle-injection-demand` (`injection-demand-contracts.mjs`) pins Vue
3.5.40 for same-instance `provide`/`inject` on a fresh native `Symbol()`
key: the fallback lacks a native callable the local provide would supply.

`just oracle-snapshot-demand` (`snapshot-demand.mjs`) installs this package
with `--frozen-lockfile` and resolves `vue@3.5.40` / `@vueuse/core@13.9.0` /
`@vueuse/shared@13.9.0` from the oracle `package.json` only. It pins:

- default `useCloned` JSON clone turns a nested `Date` into a string, so
  `.getTime()` / `.getUTCFullYear()` throw while string consumers succeed
- custom `clone` functions keep `Date`
- nested `cloned.value.when = new Date(...)` repairs the method
- `useManualRefHistory` identity dump/parse aliases `source.value`
- a nested write mutates retained records; `undo` / `reset` restore the
  edited object
- `{ clone: true }` and function `clone` restore the recorded value
- root replacement, latest-rebase, write-restored-before-demand, drained
  undo, and `clear` / `capacity` follow VueUse 13.9.0 stack semantics

`just oracle-self-trigger` (`self-trigger-runs.mjs`) is separate from onTrack
JSON. It asserts Vue 3.5.40 execution counts for self-write effects, one-shot
versus repeating `requestAnimationFrame`, and template host behavior:

- object-form slot `v-bind="{ key: item.id }"` produces VNode keys `first` /
  `second`
- `<Transition>` with `css: false` forwards `enter` / `leave` into a child
  component whose root toggles

Those cases use Vue's `createRenderer` custom host, so they do not need a DOM
package.

`just oracle-model-demand` (`model-demand.mjs`) compiles every SFC under
`fixtures/projects/model-demand/` and the two model-demand rule fixture trees
with `@vue/compiler-sfc` 3.5.40, then mounts the shipped parent/child pairs
(createRenderer so `onMounted` runs) for the unsynced-parent and shared-default
premises.

`just oracle-source-contracts` (`source-contracts.mjs`) is the Vue 3.5.40
runtime pin for issue #224 API contracts, including `toRef` ignored-key
overloads (immutable ref vs live `__v_isRef` marker, including pattern
assignment, constructor arguments, and call / tagged-template receivers,
including TypeScript instantiation wrappers) and
`effectScope` constructor callbacks.

`just oracle-computed-identity` (`computed-identity.mjs`) proves Vue 3.5.40
original vs previous-value-reused computed identity, equal projected values,
reduced downstream watch/computed work, and the same-value / changed content /
activation / stop / NaN / signed-zero controls.

`just oracle-lifetime` (`lifetime-runs.mjs` plus `lifetime-ownership-runs.mjs`)
pins Vue 3.5.40 and includes discarded nested-watch / detached-scope ownership:
outer callbacks fire at least twice after the original `scope.run` returns, the
owner is stopped, the inner source is mutated, and residual callbacks are
asserted. Returned disposers and explicit `scope.run` re-entry stay owned.
Defined object/array assignment defaults skip the initializer;
`ref(customRef(...))` preserves the custom getter; a mutual computed cycle
evaluates to `undefined` and terminates; synchronous conditional
`getCurrentScope()` capture can retain the owner. Bounded hits are not
infinite-execution claims.

## VueUse demand premises

`just oracle-vueuse-demand` (`vueuse-demand.mjs`) is a Node runtime gate for
the two VueUse source-contract rules. It installs this package's lock
(`vue` 3.5.40, `@vueuse/core` / `@vueuse/shared` 13.9.0) and asserts:

- `watchIgnorable` + `flush: 'sync'`: the ignore window is synchronous; a
  changed write after `await` reaches the callback; a nested
  `ignoreUpdates(() => { ... })` write stays ignored; a same-value write
  does not notify
- `createSharedComposable` / `createGlobalState`: the first live
  initializer is retained; an incompatible later demand throws; same-kind
  seeds share updates; disposing the last shared owner allows a new
  string instance
