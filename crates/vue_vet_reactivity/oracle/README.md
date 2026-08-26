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
