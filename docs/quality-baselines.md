# Quality baselines (published measurements)

Companion to [quality-gates.md](./quality-gates.md): the measurable signals
Vue Vet publishes for Beta readiness. Wall-clock numbers are informational;
CodSpeed is the PR regression comparator. Re-pin history is in `git log`.

## Precision baselines

Exact `(rule_id, file)` sets for the quality corpus are committed under
[`fixtures/quality/precision/`](../fixtures/quality/precision/) (one JSON per
manifest project); `just quality-gates` fails on drift. Expected findings
count only `true_positive` / `known_limitation` pairs; `false_positive` pins
must remain absent. Changing either set means updating the JSON and
explaining the behavior change in the PR.

## Native binary size budget

The `pkg.pr.new` matrix measures the stripped `vue-vet` from
`cargo build --release --target` and compares `file_bytes` plus a gzip-9 proxy
(mtime 0, no filename — not the release archive or npm tarball) to
[`fixtures/quality/native-size-budget.json`](../fixtures/quality/native-size-budget.json);
maxima are `ceil(candidate × 1.03)` per target and the `baseline` rows are a
published reference only. Reproduce with
`just native-size-check <binary> <rust-triple>`.

Standing decision (@alexzhang1030, 2026-09-15): binary growth from built-in
rules is expected and the 3 % regression budget is the only enforced guard.
When a rule lane crosses the line, re-pin `candidate` to that PR's own matrix
run (commit + run id in the JSON). Do **not** re-pin for dependency or profile
changes without the protocol below.

Profile overrides (documented beside `[profile.release]` in `Cargo.toml`) are
accepted only with exact CLI and cache output equality plus same-tree median
gates within 5 %: 5k no-cache CLI, 5k fresh-cache CLI (serialize, write,
rename, observe a hit), and `whole_project::scan_cold_mixed_1k` /
`scan_warm_mixed_1k` / `json_render_mixed_1k` (Divan `--exact` needs the full
path). LSP-only runtime crates additionally need a real LSP workflow
(initialize → `didOpen` → `didChange` → two `--explain-scope` hovers →
`shutdown`; 8 ABBA cycles; median ≤ 105 %) and byte-identical MCP
`initialize` / `tools/list` / `vue_vet_scan` output. Benchmark builds are
unwind while the shipped CLI is `panic = "abort"`; time the CLI separately.

## Performance baselines (CodSpeed suite names)

Stable benchmark names (do not rename without a new baseline rationale):

| Suite | Benchmark | Mode |
| --- | --- | --- |
| `vue_vet_vize` / `analyze_sfc` | `analyze_recommended_valid`, `analyze_recommended_invalid` | SFC micro |
| `vue_vet_session` / `scan_modes` | `scan_cold_nuxt_graph`, `scan_warm_nuxt_graph`, `scan_overlay_nuxt_graph` | Project scan modes |
| `vue_vet_session` / `scan_modes` | `scan_diff_filter_nuxt_graph` | One `filter_diff`; analyze and teardown unmeasured |
| `vue_vet_session` / `scan_modes` | `scan_incremental_edits_nuxt_graph`, `scan_noop_analyze_affected`, `scan_independent_leaf_edit_1k_modules`, `scan_incremental_root_edit_1k_modules` | Incremental session |
| `vue_vet_reactivity` / `module_scaling` | `trace_1k_modules`, `trace_5k_modules`, `trace_1k_reexport_chain` | Cold one-shot (`persist_linking_cache` off); no-regression only |
| `vue_vet_reactivity` / `module_scaling` | `trace_warm_leaf_edit_1k_modules` | Warm `ModuleTraceState` + one leaf edit — the locality signal |
| `vue_vet_session` / `whole_project` | `scan_cold_mixed_1k`, `scan_warm_mixed_1k`, `scan_script_edit_mixed_1k`, `scan_dependency_edit_mixed_1k`, `json_render_mixed_1k`, `scan_template_edit_mixed_5k` | Mixed Vue+TS tree; generation and teardown outside the closure |

The `whole_project` fixture asserts its shape in the bench
(`assert_mixed_semantics`). Each package's suites must be built in one
CodSpeed invocation ([gotchas](../.agents/docs/gotchas.md#benchmarks)).
Commands: `just bench`, `just bench-codspeed-build`, `just bench-codspeed-run`.

## Compatibility, crash-free, and offline spot checks

Pinned analysis-stack versions are machine-checked in
[`fixtures/quality/compat-matrix.json`](../fixtures/quality/compat-matrix.json)
via `just compat-matrix` ([upgrade procedure](./vize-compatibility.md)).
`reference_fixture_corpus_never_crashes` (CLI tests) walks the full
`fixtures/` corpus; releases also run `just quality-gates` and `just oracle`.

External showcase apps (`antfu/vitesse`, `antfu/vitesse-lite`, `nuxt/starter`)
are reviewed offline only — licenses and mutable trees keep them out of
`fixtures/quality/manifest.json`. Run with dependencies installed so package
resolution is External, and re-run after major a11y, project-graph, or tracer
changes. Each issue class they surfaced is pinned in the CI corpus or rule
fixtures. Still expected quiet: Vite-only aliases not in tsconfig, dynamic
imports, App Tree provide/inject beyond the unique-key index.

## Beta cut checklist

Do not tag Beta while any of these is red, from a clean `main` tip:

1. `just roll-rust`, `just compat-matrix`, `just quality-gates`, `just oracle`.
2. CodSpeed on the release commit shows no unexplained regression versus the
   previous published train; Codecov project/patch thresholds hold.
3. Native release matrix targets in `release.yml` match [install](./install.md).
4. Release notes link this file, `quality-gates.md`, and the CodSpeed report,
   and state the version train and any precision expectation delta.
5. Tag and publish only after the release workflow's gate jobs pass.

Post-Beta work that does not block the tag: [ROADMAP.md](../ROADMAP.md).
