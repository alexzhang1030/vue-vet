# Engineering conventions

## Rule contract

- Built-in IDs use `vue-vet/<category>/<name>` and are user-facing stable
  identifiers. Every rule declares category, default severity, confidence, and
  a documentation key. The live inventory is `vue-vet --list-rules`; the human
  catalog `docs/rules/README.md` is `vue-vet --list-rules --format markdown`
  (`just rules-catalog`); CI fails on drift via `just rules-catalog-check`.
  Cache identity (`RULESET_VERSION`) lives in `crates/vue_vet_cache/src/lib.rs`;
  graph identity (`REACTIVITY_GRAPH_VERSION`) in `vue_vet_core`.
- Prefer the practice channel (`category: practice`) when the pattern remains
  correct and the finding only recommends a newer / ecosystem API. Reserve
  Warning for real risk, unused waste, or likely bugs. Lint severity weights
  feed the density score (Info 1 / Warning 3 / Error 10); practice and
  `migration` findings do not, and neither affects the default CI exit.
  `practice = "off"` drops the whole channel; `migration` is enabled with
  `assessment = "vapor"`, `--group vapor-migration`, or `[rules]`.
- Every built-in lint rule is a self-contained module under
  `vue_vet_rules/src/rules` (one file per standalone rule). **Matrix
  families** may share an implementation type plus a hand-maintained catalog
  of unique ids in `rules/matrix/`; each matrix id still needs docs and
  fixtures. The registry only assembles `&'static dyn Rule` — never a behavior
  dispatcher. Practice suggestions live in `vue_vet_practice` with the same
  per-rule shape and an optional `recommendation` payload; practice-only
  helpers stay in `vue_vet_practice::util`.
- Shared block-access and control-flow queries live in `vue_vet_rule_query`.
  Those helpers borrow: path formatters return `MemberPath`, walks yield `&T`,
  `RuleContext::script` / `template` / `source` / `file` use the stored
  lifetime so `run_once` can `report` without collecting clones. `SourceSpan`
  is `Copy`; pass `call.span`, never `.clone()` it.
- Rules use the pass API: declare `fact_kinds`, implement `run_on` for per-fact
  checks, and use `run_once` only for true multi-fact aggregation. Report
  immediately inside the visitor; do not filter the fact set into a temporary
  `Vec` and iterate it again.
- Prefer high-precision fact links over broad call presence: exact provenance
  (Vue / `#imports` / exact `@vueuse/*` origin), completed capability proof on
  the concrete receiver, and proven executed regions. Routine `unref(ref)` /
  numeric MaybeRef unwrapping stays quiet.
- A rule lands with rationale, bad/good examples, limitations, positive
  fixtures, common safe patterns, false-positive regressions, exact-span
  assertions, and reporter snapshots.
- A diagnostic whose **premise is Vue runtime behavior** (effect run count,
  loop vs coalesce, first-run / `immediate`, flush timing, cleanup order) must
  ship **rerunnable runtime evidence** at the locked oracle Vue version via a
  `just oracle-*` Node recipe (not the onTrack JSON `just oracle` gate), plus a
  true-positive fixture, a quiet fixture for the coalesced / incomplete-coverage
  case, a precision pin when the quality corpus is affected, and exact-span
  snapshots. Removed IDs leave the runtime catalog
  (`docs/rules/removed-ids.md`) and fail config validation; keep the Vue
  behavior evidence as semantic regressions, not as quiet registered rules.
- Source-contract, lifetime, and derivation facts are collected in the Oxc
  adapter and consumed by thin rules; dedicated uncertainty roles (toRef
  identity, callback capability, clone native identity, demand
  `closed_key_unknown`) stay distinct from generic source5 `escaped` /
  `uncertain`. Details: [architecture](./architecture.md#semantic-ir-layers).
- Low-confidence heuristics are opt-in and never enter the default preset
  merely to increase rule count.
- Canonical rule groups are declared on `RuleMeta.group` (`Option<RuleGroupId>`);
  `vue_vet_session::groups` derives inventory and `--group` filtering from the
  composed registry, so registering a rule is its `RuleMeta` plus its doc page.
  `--list-rules` is the live registry, **not** a scan with the current
  `vue-vet.toml`. `--group` unions only change which known IDs are `off` in
  effective config; they must not re-enable `preset = "none"`,
  `practice = "off"`, or explicit `off` entries.

## Source locations

Internal locations are byte offsets into the original SFC source. User-facing
line and column values are derived explicitly. `SourceSpan` is four `usize`s
and is `Copy`, same as `ByteRange`. Span changes require ASCII, Unicode,
multiline, and relevant CRLF fixtures. Never assume a byte offset is a
character index.

## Fixtures and snapshots

Per-rule fixtures live under `fixtures/rules/<rule>/{invalid,valid}/`. One session integration walker (`crates/vue_vet_session/tests/session/rule_fixtures.rs`) analyzes every non-skipped rule directory through `ProjectSession` (the same finalize path as the CLI, including overlap consolidation and config defaults). The temp workspace writes a `vue-vet.toml` that turns off `vue-vet/project/unresolved-import` and `vue-vet/project/unused-component` so missing stub packages do not pollute rule snapshots; every other rule stays at defaults. Invalid fixtures compare pretty-printed diagnostics to `fixtures/snapshots/<rule>/<stem>.json`; set `UPDATE_RULE_SNAPSHOTS=1` to refresh those files. Snapshots are post-overlap: they record the diagnostics that survive `DiagnosticFinalizer` (config, suppressions, computed-impurity / watch-source / nested-watch overlap), not the raw per-rule `analyze_sfc` stream. Companion SFCs that another fixture in the same directory imports use a PascalCase stem (`Child.vue`); they are scanned and snapshotted but are not required to report the target rule. `no-stale-prop-flow` stays skipped: `join_prop_flows` only attaches Prop edges to the child graph, so the rule (which reads the parent graph) currently never fires; its fixtures are kept for the fix tracked in #273. Every walked rule keeps `invalid/unicode*.vue` (multibyte text before the offending span) and `invalid/crlf*.vue` (literal `\r\n`). Project and migration rules stay on their dedicated project-fixture tests; `recommended/` remains a pack fixture in `golden.rs`.

## Deterministic output

Sort diagnostics by normalized repository-relative path, byte offset, and rule
ID. Do not expose platform path separators or hash-map iteration order in
snapshots, JSON, baselines, or cache identities.

Machine-readable finding IDs are opaque and deterministic. Their readable
prefix uses normalized path, line/column, and rule ID; their digest changes
with user-visible severity or message changes. Exact scan coverage and an
explicit completeness flag accompany findings so empty output is never
ambiguous.

## Edit contracts

Text edits use byte offsets into the original file, carry explicit safe/unsafe
applicability and rule provenance, and are sorted by normalized path and range
before any consumer sees a plan. Reject overflowing ranges and all
order-dependent overlap before touching disk. Two non-empty half-open ranges
may meet at a boundary, but insertions at replacement boundaries conflict. Core
planning and reporters never mutate files.

Attach an edit candidate to the diagnostic that authorizes it; rule overrides
and suppressions remove both together. Safe application validates scan-scope
containment, file bounds, and UTF-8 boundaries, applies later byte ranges
first, commits through atomic replacement, and reports a fresh post-fix scan.
Fix modes never consume a cached edit plan. A rule may advertise a safe edit
only for syntax it can replace completely; keep the diagnostic but omit the
edit when source coverage is incomplete. See [edit model](../../docs/edit-model.md).

## Crate and directory names

Workspace crates use **snake_case** for both the Cargo package name and the
directory under `crates/` (for example `vue_vet_reactivity`), matching the Oxc
/ Rolldown layout. The CLI package remains `vue-vet` so the installed binary
stays `vue-vet`.

## npm launcher boundary

JavaScript under `npm/` may only select a native binary and forward process
I/O. Do not move analysis, parsing, or rule logic into Node. Use the `just`
recipes `npm-test`, `pack-platform`, `npm-smoke`, and `npm-consumer-check` for
launcher work; `npm-consumer-check` accepts an already-built binary without
invoking Cargo.

## Dependency boundaries

Vize and Oxc types remain inside their adapters. Stable downstream code
consumes Vue Vet-owned facts. Dependency upgrades are reviewed as behavior
changes and include compatibility evidence rather than blind snapshot
replacement ([procedure](../../docs/vize-compatibility.md)).

## Testing and completion

Use `just` as the canonical task interface (`just --list`); keep local and CI
commands behind the same recipes. Rust work is not complete until
`just roll-rust` passes (format, the Rolldown-derived Clippy policy with
warnings denied, workspace tests with the lockfile, fixture/integration tests).
Do not add a lint exception without a narrow reason tied to code or an upstream
constraint. Hooks are managed with `prek` from `.pre-commit-config.yaml`. When
local execution is unavailable, say so and use CI as the evidence; never claim
a check passed when it was not run.

autofix.ci may run only deterministic repository-owned fix recipes from a
`pull_request` workflow with read-only Actions permissions; the autofix.ci App
is the sole writer. Never expose a write token to pull-request code or use
`pull_request_target` on untrusted changes. Do not reintroduce slow
third-party review bots.

## Performance regression checks

CodSpeed's simulated-CPU results are the canonical pull-request performance
comparison. Benchmarks use committed fixtures and stable names; renaming a
benchmark or materially changing its fixture establishes a new baseline and
needs an explicit rationale. Keep inputs and filesystem I/O (including
cache-directory teardown) outside the measured closure. CodSpeed builds use
the dedicated `codspeed` profile (`lto = false`, `panic = "unwind"`,
`opt-level = 3` restated on the size-optimized product crates) because its
instrumentation does not link Oxc reliably under LTO. The release profile in
`Cargo.toml` (with its inline comment on package overrides) is the source of
truth for shipped artifacts; overrides outside the analysis closure are
accepted on the LSP/MCP gate described in
[quality baselines](../../docs/quality-baselines.md). Benchmark builds are
always unwind; time the shipped abort CLI separately.

CI size gates use artifact mode on the matrix binary plus the committed budget
table; they must not rebuild. Do not bake a local `CARGO_TARGET_DIR` or host
byte count into pack/smoke scripts.

Codecov is the canonical coverage comparison: project coverage may fall by at
most one percentage point relative to the base commit, changed lines keep at
least 80 % line coverage, and CI and local runs share `just coverage-lcov`.
Methodology and release checklists: [quality gates](../../docs/quality-gates.md).

## Commits and pull requests

Commit messages follow Conventional Commits: `type(scope): imperative summary`
with `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`,
or `revert`; use `!` and a `BREAKING CHANGE:` footer when a stable contract
breaks. The scope names the affected crate or product boundary.

Normal development happens on a focused branch reviewed through a pull request
linked to its GitHub issue; keep the PR draft while acceptance criteria or
checks remain incomplete. Direct commits to `main` are reserved for an explicit
maintainer request or a documented emergency.

## Planning and records

GitHub issues hold live implementation tasks. [ROADMAP.md](../../ROADMAP.md)
holds what is ahead and the release gates. PCR records hold durable rationale,
architecture, conventions, and traps. Update the appropriate layer instead of
duplicating the same plan in all three.
