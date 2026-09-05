# Quality baselines (published measurements)

Companion to [quality-gates.md](./quality-gates.md). These are the measurable
signals Vue Vet publishes for Beta readiness. Numbers that depend on wall-clock
hardware are informational; CodSpeed remains the PR regression comparator.

## Precision baselines (versioned expectations)

Exact `(rule_id, file)` sets for the quality corpus are committed under
[`fixtures/quality/precision/`](../fixtures/quality/precision/). CI fails on drift
via `just quality-gates`.

| Project | Expected findings (TP / known_limitation) | FP pins |
| --- | ---: | ---: |
| `basic` | 1 | 0 |
| `configured` | 1 | 0 |
| `nuxt-graph` | 2 | 0 |
| `vue-3.4` | 1 | 0 |
| `vue-3.5` | 2 | 0 |
| `prop-flow` | 0 | 2 |
| `practice-vueuse` | 9 | 0 |
| `a11y-forms` | 14 | 4 |
| `suppressed` | 0 | 1 |
| `module-seeds` | 1 | 0 |
| `provide-inject` | 0 | 2 |
| `reactivity-rules` | 2 | 7 |

Expected findings count only `true_positive` / `known_limitation` pairs. FP pins
must remain absent. Changing either set requires updating the precision JSON and
explaining the behavior change in the PR. `reactivity-rules` counts (2 TP / 7 FP)
match `fixtures/quality/precision/reactivity-rules.json`. Two former TPs on
`ConditionalWatch.vue` — `no-conditional-watch-effect-dependency` and
`prefer-explicit-sources-for-conditional-deps` — are now FP pins: Vue tracks
dynamic dependencies behind a reactive guard, those rule IDs are withdrawn, and
the findings must stay absent. Remaining TPs are unused-binding and
`prefer-computed`. The other five FP pins on `SafePatterns.vue` are unchanged.

## Performance baselines (CodSpeed suite names)

Stable benchmark names (do not rename without a new baseline rationale):

| Suite | Benchmark | Mode |
| --- | --- | --- |
| `vue_vet_vize` / `analyze_sfc` | `analyze_recommended_valid` | SFC micro |
| `vue_vet_vize` / `analyze_sfc` | `analyze_recommended_invalid` | SFC micro |
| `vue_vet_session` / `scan_modes` | `scan_cold_nuxt_graph` | Cold project scan |
| `vue_vet_session` / `scan_modes` | `scan_warm_nuxt_graph` | Warm cache scan |
| `vue_vet_session` / `scan_modes` | `scan_overlay_nuxt_graph` | Incremental overlay |
| `vue_vet_session` / `scan_modes` | `scan_diff_filter_nuxt_graph` | Diff filter (`filter_diff` only; analyze and cache teardown are unmeasured) |
| `vue_vet_reactivity` / `module_scaling` | `trace_1k_modules` | Cold one-shot (`persist_linking_cache` forced off). No-regression, not the locality win. |
| `vue_vet_reactivity` / `module_scaling` | `trace_warm_leaf_edit_1k_modules` | Warm `ModuleTraceState` + one independent leaf body edit |
| `vue_vet_session` / `whole_project` | `scan_cold_mixed_1k` | Cold analyze of mixed Vue+TS. Setup generates the tree outside the measured closure. |
| `vue_vet_session` / `whole_project` | `scan_warm_mixed_1k` | New session after a primed on-disk cache (CLI warm). |
| `vue_vet_session` / `whole_project` | `scan_script_edit_mixed_1k` | Incremental `analyze_affected` after Parent.vue script overlay. |
| `vue_vet_session` / `whole_project` | `scan_dependency_edit_mixed_1k` | Incremental `analyze_affected` after `useCounter.ts` overlay. |
| `vue_vet_session` / `whole_project` | `json_render_mixed_1k` | `render(..., Json)` of a completed 1k mixed snapshot. |
| `vue_vet_session` / `whole_project` | `scan_template_edit_mixed_5k` | Incremental template overlay on a 5k-file mixed tree. |

`whole_project` 1k mixed fixture (250 groups, Parent `:value` + `:label="String(doubled)"`,
Child `label` prop and `{{ label }}`) must keep the measured baseline: 1000 files,
≥1000 module graphs, `no-v-html` diagnostics, `ComponentUsage` edges; module-graph
edges 250 Prop / 750 Computed / 500 Effect / 750 Template; scopes 500 computed /
500 `watchEffect`; `template_reads` length 750. Generation and cache-directory
teardown stay outside the timed closure. CodSpeed numbers are filled after the
first instrumented run.

Commands: `just bench`, `just bench-codspeed-build`, `just bench-codspeed-run`.

`scan_diff_filter_nuxt_graph` times one `filter_diff` of the `nuxt-graph`
summary. Analyze and cache-directory teardown stay outside the measured
closure. Putting filesystem teardown back in establishes a new baseline.

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

## Offline real-repo spot checks (not CI inputs)

External showcase apps are reviewed offline only (licenses + mutable trees). Do
not add them to `fixtures/quality/manifest.json` unless they become checksummed,
project-owned fixtures.

Captured 2026-08-10 with `vue-vet` at tip (post `defineModels` ModelRef seeds).
Re-run offline after major a11y, project-graph, or tracer binding changes.
Require `pnpm install` (or equivalent) so package resolution is External rather
than unresolved-import noise.

| Repo | License | Setup | Observed findings (informational) |
| --- | --- | --- | --- |
| [antfu/vitesse-lite](https://github.com/antfu/vitesse-lite) @ tip | MIT | `pnpm install` | No crash. Icon-only footer: `button-has-content` + `anchor-has-content` (static `title="GitHub"` carries safe `aria-label` insert). `TheInput` `form-control-has-label` (reusable control with `id` + `$attrs`). Standard `defineModel()` stays quiet. |
| [nuxt/starter](https://github.com/nuxt/starter) `v4` | MIT | `pnpm install` | No crash; **0** findings on the minimal app. |
| [antfu/vitesse](https://github.com/antfu/vitesse) @ tip | MIT | `pnpm install` | No crash. Footer icon controls: `anchor-has-content`×4 + `button-has-content` (bound `:title` stays report-only; static GitHub `title` has safe edit). `TheInput` `form-control-has-label`. Vue Macros `defineModels` destructure is **quiet** for `v-model` (was a false positive before ModelRef seeds). |

Corpus coverage that pins the same classes of issue in CI:

- Icon-only / named controls → `a11y-forms` (`IconLink`, `EmptyButton`, `SafeNamedControls`)
- RouterLink accessible name → rule fixtures under `fixtures/rules/anchor-has-content`
- Prop under-approx quiet gaps → `prop-flow` (`SpreadChild` whole-object `v-bind`; computed/bracket/call expressions)
- Unique-key provide/inject → `provide-inject`
- Core reactivity lint TPs → `reactivity-rules` (+ after-await in `module-seeds`)
- `defineModel` / `defineModels` quiet for `no-v-model-nonreactive-source` → rule fixtures under `fixtures/rules/no-v-model-nonreactive-source`

Still expected quiet outside CI corpus: Vite-only aliases not in tsconfig,
dynamic imports, App Tree provide/inject beyond the unique-key index.

## Publishing with a Beta tag

Release notes for Beta+ should link:

1. This file
2. `docs/quality-gates.md`
3. The CodSpeed report for the release commit
4. A short note if precision expectations changed since the previous tag

### Beta cut checklist

Do not tag Beta while any of these is red. Run from a clean `main` tip:

1. `just roll-rust`
2. `just compat-matrix`
3. `just quality-gates`
4. `just oracle`
5. Confirm CodSpeed on the release commit has no unexplained regression vs the
   previous published train
6. Confirm Codecov project/patch thresholds still hold on the release PR or tip
7. Confirm the native release matrix targets in `release.yml` still match
   [install docs](./install.md)
8. Draft release notes that include the four links above plus:
   - version train (`vX.Y.Z` crates.io / npm / GitHub archives)
   - one-paragraph precision summary (corpus size, TP count, FP pins)
   - known quiet gaps (this file’s offline spot-check section)
9. Tag and publish only after the release workflow’s gate jobs pass

Post-Beta engineering that does **not** block the tag: real-repo offline FP
passes, oracle-backed long-tail Factory / `.d.ts` seeds, extra single-file safe
fix producers, multi-file fix transactions.
