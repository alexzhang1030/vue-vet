# Engineering conventions

## Rule contract

- Built-in IDs use `vue-vet/<category>/<name>` and are treated as user-facing stable identifiers.
- Every rule declares category, default severity, confidence, and a documentation key.
- Every built-in rule keeps its metadata and `Rule` implementation in one
  dedicated file under `vue-vet-rules/src/rules`; the registry module only
  assembles rules and must not become a behavior dispatcher.
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

## Dependency boundaries

Vize and Oxc types remain inside their adapters. Stable downstream code consumes Vue Vet-owned facts. Dependency upgrades are reviewed as behavior changes and include compatibility evidence rather than blind snapshot replacement.

## Testing and completion

Use `just` as the canonical task interface and inspect recipes with `just --list`; keep local and CI commands behind the same recipes. Rust work is not complete until `just roll-rust` passes, including format, the workspace's Rolldown-derived and Vue Vet-tightened Clippy policy with warnings denied, workspace tests with the lockfile, and relevant fixture/integration tests. Do not add a lint exception without a narrow reason tied to code or an upstream dependency constraint. Use `prek` to manage hooks from `.pre-commit-config.yaml`. When local execution is unavailable, state that limitation and use CI as the evidence. Never claim a check passed when it was not run.

CodeRabbit review is advisory and never replaces repository CI or human
ownership. autofix.ci may run only deterministic repository-owned fix recipes
from a `pull_request` workflow with read-only GitHub Actions permissions; the
autofix.ci GitHub App is the sole writer. Never expose a write token to
pull-request code or use `pull_request_target` to execute untrusted changes.

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
