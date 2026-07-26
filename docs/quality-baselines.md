# Quality baselines (published measurements)

Companion to [quality-gates.md](./quality-gates.md). These are the measurable
signals Vue Vet publishes for Beta readiness. Numbers that depend on wall-clock
hardware are informational; CodSpeed remains the PR regression comparator.

## Precision baselines (versioned expectations)

Exact `(rule_id, file)` sets for the quality corpus are committed under
[`fixtures/quality/precision/`](../fixtures/quality/precision/). CI fails on drift
via `just quality-gates`.

| Project | Expected findings |
| --- | ---: |
| `basic` | 1 |
| `configured` | 1 |
| `nuxt-graph` | 2 |
| `vue-3.4` | 1 |
| `vue-3.5` | 2 |

Changing a count requires updating the precision JSON and explaining the behavior
change in the PR.

## Performance baselines (CodSpeed suite names)

Stable benchmark names (do not rename without a new baseline rationale):

| Suite | Benchmark | Mode |
| --- | --- | --- |
| `vue_vet_vize` / `analyze_sfc` | `analyze_recommended_valid` | SFC micro |
| `vue_vet_vize` / `analyze_sfc` | `analyze_recommended_invalid` | SFC micro |
| `vue_vet_session` / `scan_modes` | `scan_cold_nuxt_graph` | Cold project scan |
| `vue_vet_session` / `scan_modes` | `scan_warm_nuxt_graph` | Warm cache scan |
| `vue_vet_session` / `scan_modes` | `scan_overlay_nuxt_graph` | Incremental overlay |
| `vue_vet_session` / `scan_modes` | `scan_diff_filter_nuxt_graph` | Diff filter |

Commands: `just bench`, `just bench-codspeed-build`, `just bench-codspeed-run`.

### Developer-machine spot check (informational)

Captured 2026-07-27 on the maintainer macOS host while landing #13 (not a release
budget):

| Mode | Corpus | Approx. wall time |
| --- | --- | --- |
| Cold CLI scan | `nuxt-graph` | ~0.8–2.9 ms session-analyze band in Divan; CLI process ~0.3–0.8 s including startup |
| Warm CLI scan | `nuxt-graph` | Divan median ~1.0 ms; CLI process ~0.3 s |
| Cold/warm identity | `nuxt-graph` | Same diagnostic ids (enforced in tests) |

Prefer CodSpeed deltas over these wall times when judging regressions.

## Compatibility baselines

Pinned analysis stack versions are machine-checked in
[`fixtures/quality/compat-matrix.json`](../fixtures/quality/compat-matrix.json)
via `just compat-matrix`. Upgrade procedure:
[vize-compatibility.md](./vize-compatibility.md).

## Crash-free baseline

`reference_fixture_corpus_never_crashes` in the CLI test suite walks the full
`fixtures/` source corpus. Releases also run `just quality-gates` and
`just oracle` before building binaries.

## Publishing with a Beta tag

Release notes for Beta+ should link:

1. This file
2. `docs/quality-gates.md`
3. The CodSpeed report for the release commit
4. A short note if precision expectations changed since the previous tag
