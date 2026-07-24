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
| `pause-tracking-window` | `pauseTracking`/`enableTracking` window drops mid-window reads |
| `props-reactive-object` | `props.count` style reactive object (defineProps stand-in) |
| `reactive-member` | `reactive({ count }).count` member track |
| `sync-every-hof` | sync Array#every callback must track `threshold` |
| `sync-filter-hof` | sync Array#filter callback must track `query` |
| `sync-find-hof` | sync Array#find callback must track `target` |
| `sync-flatMap-hof` | sync Array#flatMap callback must track nested reads |
| `sync-forEach-hof` | sync Array#forEach callback must track `factor` |
| `sync-map-hof` | sync Array#map callback must track `factor` |
| `sync-reduce-hof` | sync Array#reduce callback must track `factor` |
| `sync-some-hof` | sync Array#some callback must track `threshold` |
| `use-route-like` | reactive route object member (`route.path`) |
| `watch-effect-ref` | `watchEffect` tracks `ref.value` |
| `watch-effect-await` | post-await read is **not** runtime-tracked (boundary) |
| `watch-source-array` | `watch([a, b])` tracks each ref `.value` |
| `watch-source-getter` | `watch(() => value.value)` source getter |
| `watch-source-ref` | `watch(ref)` tracks `.value` (not property-less) |
| `runner-run-no-track` | arbitrary `.run` invents nothing at runtime |

Bare `watch(reactiveObj)` is **static-only quiet**: runtime deep-tracks many keys
(`Object iterate`, each property). Emitting a property-less static dep would
fail under-approx identity.

Static-only (no oracle JSON): `storeToRefs` from `pinia` — unit-tested; runtime
`toRefs` tracks the **underlying store object**, so key identity differs from
local ref names and is not a fair under-approx pair without alias mapping.

`pause-tracking-window` exercises `@vue/reactivity`'s `pauseTracking` /
`enableTracking` (not re-exported from the public `vue` package in 3.5.x). The
static source still names them under `from 'vue'` to match docs / re-exports.

## Refresh expected JSON

```bash
cd crates/vue-vet-reactivity/oracle
pnpm install
pnpm oracle:write
```

Committed `expected/*.json` are the CI source of truth so Rust tests do not
require Node at test time.

## Rust comparison

`cargo test -p vue-vet-reactivity --lib oracle` loads each expected file, runs
`trace_reactivity` on `source`, and asserts under-approx + prints recall.
