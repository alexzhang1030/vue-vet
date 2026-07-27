# Engineering conventions

## Rule contract

- Built-in IDs use `vue-vet/<category>/<name>` and are treated as user-facing stable identifiers.
- Every rule declares category, default severity, confidence, and a documentation key.
- Prefer the practice channel (`category: practice`) when the pattern remains correct and
  the finding only recommends a newer/ecosystem API — for example `prefer-use-template-ref`
  and VueUse recipes. Reserve Warning for real risk, unused waste, or likely bugs. Lint
  severity weights still feed the density score (Info 1 / Warning 3 / Error 10); practice
  findings do not.
- Every built-in lint rule keeps its metadata and `Rule` implementation in one
  dedicated file under `vue_vet_rules/src/rules`; the registry module only
  assembles rules and must not become a behavior dispatcher. Practice
  suggestions live in `vue_vet_practice` with the same per-rule module shape,
  `category: "practice"`, and an optional `recommendation` payload; they must
  not affect score or default CI exit.   Prefer high-precision fact links (shared
  timer bindings, lifecycle + missing cleanup, resolved Vue `unref` imports)
  over broad call presence. Recipe metadata may declare `min_vue` /
  `confidence`; matching stays in thin `Rule` code. `practice = "off"` in
  `vue-vet.toml` drops the whole channel.
- Rules use the pass API: declare `fact_kinds`, implement `run_on` for per-fact
  checks, and use `run_once` only for true multi-fact aggregation. Prefer
  immediate `report` inside the visitor. Do not filter the whole fact set into a
  temporary `Vec` and then iterate it again.
- A rule lands with rationale, bad/good examples, limitations, positive fixtures, common safe patterns, false-positive regressions, exact-span assertions, and reporter snapshots.
- Low-confidence heuristics are opt-in and never enter the default preset merely to increase rule count.

## Source locations

Internal locations are byte offsets into the original SFC source. User-facing line and column values are derived explicitly. Span changes require ASCII, Unicode, multiline, and relevant CRLF fixtures. Never assume a byte offset is a character index.

## Deterministic output

Sort diagnostics by normalized repository-relative path, byte offset, and rule ID. Do not expose platform path separators or hash-map iteration order in snapshots, JSON, baselines, or cache identities.

Machine-readable finding IDs are opaque and deterministic. Their readable
prefix uses normalized path, line/column, and rule ID; their digest changes with
user-visible severity or message changes. Exact scan coverage and an explicit
completeness flag accompany findings so empty output is never ambiguous.

## Edit contracts

Text edits use byte offsets into the original file, carry explicit safe/unsafe
applicability and rule provenance, and are sorted by normalized path and range
before any consumer sees a plan. Reject overflowing ranges and all
order-dependent overlap before touching disk. Two non-empty half-open ranges may
meet at a boundary, but insertions at replacement boundaries conflict because
their application order could change the result. Core planning and reporters
must never mutate files.

Attach an edit candidate to the diagnostic that authorizes it; rule overrides
and suppressions must remove both together. Normalize target paths relative to
the scan root before reporting or applying them. Safe application additionally
validates scan-scope containment, file bounds, and UTF-8 boundaries, applies
later byte ranges first, commits through atomic replacement, and reports a fresh
post-fix scan. Fix modes never consume a cached edit plan. A rule may advertise a
safe edit only for syntax it can replace completely; keep the diagnostic but
omit the edit when source coverage is incomplete.

## Crate and directory names

Workspace crates use **snake_case** for both the Cargo package name and the
directory under `crates/` (for example `vue_vet_reactivity`), matching the Oxc /
Rolldown layout. The CLI package remains `vue-vet` so the installed binary stays
`vue-vet`. User-facing rule IDs keep the `vue-vet/<category>/<name>` form.

## npm launcher boundary

JavaScript under `npm/` may only select a native binary and forward process
I/O. Do not move analysis, parsing, or rule logic into Node. Prefer repository
`just` recipes (`npm-test`, `pack-platform`, `npm-smoke`) for launcher work.

## Dependency boundaries

Vize and Oxc types remain inside their adapters. Stable downstream code consumes Vue Vet-owned facts. Dependency upgrades are reviewed as behavior changes and include compatibility evidence rather than blind snapshot replacement.

## Testing and completion

Use `just` as the canonical task interface and inspect recipes with `just --list`; keep local and CI commands behind the same recipes. Rust work is not complete until `just roll-rust` passes, including format, the workspace's Rolldown-derived and Vue Vet-tightened Clippy policy with warnings denied, workspace tests with the lockfile, and relevant fixture/integration tests. Do not add a lint exception without a narrow reason tied to code or an upstream dependency constraint. Use `prek` to manage hooks from `.pre-commit-config.yaml`. When local execution is unavailable, state that limitation and use CI as the evidence. Never claim a check passed when it was not run.

autofix.ci may run only deterministic repository-owned fix recipes from a
`pull_request` workflow with read-only GitHub Actions permissions; the
autofix.ci GitHub App is the sole writer. Never expose a write token to
pull-request code or use `pull_request_target` to execute untrusted changes.
Do not reintroduce CodeRabbit or other slow third-party review bots.

## Performance regression checks

CodSpeed's simulated-CPU results are the canonical pull-request performance
comparison. Benchmarks use committed, representative fixtures and stable names
so a result remains comparable across revisions. Keep benchmark inputs outside
the measured closure, pin the CodSpeed compatibility layer and CLI, and run the
same repository-owned recipes locally and in CI. Renaming a benchmark or
materially changing its fixture establishes a new baseline and requires an
explicit rationale in the pull request. Performance checks complement rather
than replace correctness tests. CodSpeed builds use the dedicated `codspeed`
profile because its instrumentation does not link Oxc reliably under thin LTO;
the release profile remains the source of truth for shipped artifacts.

Project-level cold / warm / overlay / diff-filter benches live in
`vue_vet_session` (`scan_modes`) on the quality corpus. SFC micro-benchmarks
remain in `vue_vet_vize`. Methodology and release checklists:
[quality gates](../../docs/quality-gates.md).

Codecov is the canonical coverage comparison. Project coverage may fall by at
most one percentage point relative to the base commit, while changed lines must
retain at least 80% line coverage. CI and local runs generate the same LCOV
artifact through `just coverage-lcov`; coverage status supplements the full
cross-platform test matrix and never substitutes for behavior-focused tests.

## Commits and pull requests

Commit messages follow Conventional Commits: `type(scope): imperative summary`. Use `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, or `revert`; use `!` and a `BREAKING CHANGE:` footer when a stable contract breaks. The scope names the affected product or crate boundary when that improves retrieval, for example `feat(rules): add stable v-for key diagnostic`.

Normal development happens on a focused branch and is reviewed through a pull request linked to its GitHub issue. Keep the PR draft while acceptance criteria or checks remain incomplete. Direct commits to `main` are reserved for an explicit maintainer request or a documented emergency; convenience or missing local tooling is not sufficient reason to bypass review.

## Planning and records

GitHub issues hold live implementation tasks and checklists. [ROADMAP.md](../../ROADMAP.md) holds milestone intent and release gates. PCR records hold durable rationale, architecture, conventions, and traps. Update the appropriate layer instead of duplicating the same plan in all three.
