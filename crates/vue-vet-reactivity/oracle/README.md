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
| `props-reactive-object` | `props.count` style reactive object (defineProps stand-in) |
| `sync-filter-hof` | sync Array#filter callback must track `query` |
| `sync-map-hof` | sync Array#map callback must track `factor` |
| `use-route-like` | reactive route object member (`route.path`) |
| `watch-effect-await` | post-await read is **not** runtime-tracked (boundary) |
| `runner-run-no-track` | arbitrary `.run` invents nothing at runtime |

Static-only (no oracle JSON): `storeToRefs` from `pinia` — unit-tested; runtime
`toRefs` tracks the **underlying store object**, so key identity differs from
local ref names and is not a fair under-approx pair without alias mapping.

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
