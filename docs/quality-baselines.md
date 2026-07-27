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
| `reactivity-rules` | 3 | 3 |

Expected findings count only `true_positive` / `known_limitation` pairs. FP pins
must remain absent. Changing either set requires updating the precision JSON and
explaining the behavior change in the PR.

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

## Offline real-repo spot checks (not CI inputs)

External showcase apps are reviewed offline only (licenses + mutable trees). Do
not add them to `fixtures/quality/manifest.json` unless they become checksummed,
project-owned fixtures.

Captured 2026-07-27 with `vue-vet` at `CONVENTIONS_VERSION` 4 (post a11y /
RouterLink work). Re-run offline after major a11y or project-graph changes.

| Repo | License | Setup | Observed findings (informational) |
| --- | --- | --- | --- |
| [antfu/vitesse-lite](https://github.com/antfu/vitesse-lite) @ tip | MIT | `pnpm install` | No crash. Icon-only footer controls: `anchor-has-content` / `button-has-content` (static `title` may carry safe `aria-label` insert). |
| [nuxt/starter](https://github.com/nuxt/starter) `v4` | MIT | `pnpm install` | No crash; **0** findings on the minimal app. |
| [antfu/vitesse](https://github.com/antfu/vitesse) @ tip | MIT | `pnpm install` | No crash; **5** on `TheFooter.vue` — icon-only `RouterLink`×2, `<button>`, `<a>`×2 (`anchor-has-content` / `button-has-content`; GitHub static `title` has safe `aria-label` insert). |

Corpus coverage that pins the same classes of issue in CI:

- Icon-only / named controls → `a11y-forms` (`IconLink`, `EmptyButton`, `SafeNamedControls`)
- RouterLink accessible name → rule fixtures under `fixtures/rules/anchor-has-content`
- Prop under-approx quiet gaps → `prop-flow` (`SpreadChild` whole-object `v-bind`; computed/bracket/call expressions)
- Unique-key provide/inject → `provide-inject`
- Core reactivity lint TPs → `reactivity-rules` (+ after-await in `module-seeds`)

Still expected quiet outside CI corpus: Vite-only aliases not in tsconfig,
dynamic imports, App Tree provide/inject beyond the unique-key index.

## Publishing with a Beta tag

Release notes for Beta+ should link:

1. This file
2. `docs/quality-gates.md`
3. The CodSpeed report for the release commit
4. A short note if precision expectations changed since the previous tag
