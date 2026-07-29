# Vue Vet execution plan

This document is the working plan for building a Rust-native doctor for Vue and
Nuxt codebases. It is intentionally ordered by dependency and confidence: each
milestone must leave behind a usable product slice and evidence that the next
layer is safe to build on.

## Product goal

Vue Vet should answer three questions for a repository:

1. What is wrong or risky?
2. Why does it matter, and how confidently do we know?
3. Did this change make the codebase healthier?

The target experience is a fast local command, a deterministic score, useful
CI output, and diagnostics that understand Vue rather than treating an SFC as
unstructured text.

## Technical commitments

- The engine and CLI are implemented in Rust.
- Vize is the source of truth for Vue SFC and template semantics.
- Oxc owns JavaScript/TypeScript syntax, scopes, symbols, and imports.
- Vize and Oxc form the complete analysis stack. New diagnostics extend their
  Vue Vet-owned semantic facts instead of introducing a parallel pattern engine.
- Vue Vet owns its diagnostic schema, scoring, suppression, caching, baselines,
  fixes, and output formats.
- Vize stays pinned until its API stabilizes. Upgrades require compatibility
  fixtures and diagnostic snapshots.

## System shape

```text
vue-vet CLI
  -> long-lived session + immutable workspace input snapshot
  -> Vize SFC/template analysis
  -> one-pass Oxc script/module facts
  -> project graph and cross-file rules
  -> unified diagnostic finalization, score, report, fix
```

Current and planned crate boundaries:

```text
vue_vet_core       stable diagnostics, spans, scoring, rule contracts
vue_vet_vize       Vize adapter and Vue semantic facts
vue_vet_oxc        JS/TS semantic facts and import resolution
vue_vet_reactivity local effect tracing and cross-module summaries/linking
vue_vet_rules      built-in rules and presets
vue_vet_project    project graph, cache, baseline, diff
vue_vet_reporters  text, JSON, SARIF, GitHub annotations
vue_vet_session    stateful project session (incremental facts, graph, explain)
vue_vet_lsp        thin stdio LSP adapter (overlays, latest-wins worker)
vue-vet            CLI binary
```

Crates are introduced only when their boundary is exercised by working code.

## M0 — runnable vertical slice

Status: complete

Delivered:

- Rust workspace and `vue-vet` CLI
- `.vue` discovery with ignore support
- Vize SFC parsing
- stable serializable diagnostic model
- dependency-neutral template facts and deterministic built-in rule registry
- text and JSON reporters
- deterministic score and CI exit policy
- Vize template AST-backed diagnostic: `vue-vet/security/no-v-html`
- golden diagnostic, parser-error, and reporter snapshots
- Unicode, multiline, multiple-finding, safe-pattern, and malformed fixtures
- Linux, macOS, and Windows CI definition

Exit criteria:

- CI compiles and tests the workspace on all three operating systems
- `vue-vet fixtures --deny-warnings` finds the expected diagnostic
- output locations point to the exact source span
- malformed SFCs fail predictably without panics

Completion evidence:

- the locked workspace is formatted, linted, and tested on Linux, macOS, and Windows
- text and JSON reporter snapshots cover exact source spans and stable rule IDs
- malformed SFCs and the complete reference fixture corpus are exercised without panics
- Vize is pinned and its compatibility assumptions are documented
- license, contribution, and security policies are published

## M1 — useful local doctor

Current: 100+ high-confidence built-in lint rules plus practice suggestions
(see [docs/rules/README.md](docs/rules/README.md)).

Status: complete

Implemented across the merged Phase 2, semantic-reactivity, and reactivity-native
rule-pack branches:

- Oxc 0.127 semantic adapter for JavaScript, TypeScript, JSX, and TSX script blocks
- dependency-neutral imports, bindings, calls, member-write, operand, and
  top-level-await facts
- versioned strict configuration, recommended/none presets, severity overrides,
  include/exclude globs, and scoped suppressions with unused-suppression diagnostics
- large fact-driven rule catalog (tracking-graph matrix, after-await/macros,
  template Essential, a11y, practice) with per-rule docs and fixtures
- normalized compiler-macro assignment facts and alias-aware readonly-props enforcement
- semantic reactivity graph facts plus package-version-gated Vue 3.4/3.5 diagnostics
- dedicated reactivity tracer crate with local control-flow analysis, module
  summaries, composable return shapes, and resolved-edge linking
- exactly 280 systematic, complex single-module, and real multi-file tracer scenarios

Work:

- add Oxc parsing and semantic analysis for `<script>` and `<script setup>`
- expose normalized Vue facts without leaking Vize/Oxc AST types
- add TOML configuration and rule severity overrides
- support file-level and inline suppression with unused-suppression warnings
- add rule documentation with bad/good examples and confidence notes
- establish correctness, reactivity, performance, accessibility, security, and
  maintainability presets

Initial rule candidates:

- unsafe `v-html`
- `v-for` without a stable key
- `v-if` combined with `v-for`
- prop mutation
- destructuring that loses reactivity
- derived state implemented with a watcher
- uncleaned lifecycle side effects
- async work without stale-result protection
- component names that collide with native elements
- inaccessible click-only interactions
- missing form labels or image alternatives
- unstable objects or functions passed through hot template paths

Exit criteria:

- every default rule has precision fixtures and documentation
- the default preset produces no known false positives on the reference suite
- configuration, suppression, text output, and JSON output are snapshot-tested
- a medium Vue repository scans from a cold start within the agreed budget

## M2 — project intelligence

Status: complete

Implemented in the project-graph branch:

- dependency-neutral, deterministically serialized nodes, edges, and evidence
- relative, `@/`, `~/`, extension, and index-file resolution
- Nuxt component, composable, page, layout, plugin, middleware, and store conventions
- unresolved-import and unused-component cross-file diagnostics
- graph invalidation inputs and `--print-graph` debugging

Implemented in the stacked cache/diff branch:

- versioned SHA-256 content keys over source, config, tool, dependency, graph, and rule inputs
- atomic normalized-result caching with corruption recovery and cache stats
- versioned diagnostic-fingerprint baselines
- changed-line filtering that retains all graph-backed project findings

Work:

- build an import, component, composable, route, store, and auto-import graph
- understand Nuxt conventions and generated imports
- add unused component/composable detection
- add cross-file prop, emit, slot, route, and store diagnostics
- implement content-addressed caching and parallel scanning
- support changed-file and changed-line analysis
- introduce confidence and deduplication policies for overlapping diagnostics

Exit criteria:

- incremental results equal clean-scan results
- cache invalidation tests cover dependency and configuration changes
- project rules report evidence across every relevant file
- changed-line mode never hides a newly introduced project-level failure

## M3 — CI and distribution

Status: implementation in progress

Implemented in the reporter/edit foundation slice:

- dedicated `vue_vet_reporters` boundary exercised by unchanged text and JSON output
- serializable byte-range edits with explicit safe/unsafe applicability and rule provenance
- deterministic edit planning that rejects range overflow, overlap, and order-dependent insertions
- SARIF 2.1.0 and escaped GitHub Actions annotations

Implemented in the first safe-fix vertical slice:

- optional edit previews in JSON v1 plus `--fix-dry-run`
- `--fix-safe` for active, explicitly safe edits, with cache bypass and a fresh post-fix rescan
- scan-root containment, source bounds, UTF-8 boundaries, and deterministic conflict validation
  before writes
- atomic single-file replacement with Unicode and line-ending preservation
- a conservative producer for boolean `autofocus`; valued attributes remain manual

Implemented in the native distribution slice:

- thin npm launcher (`vue-vet`) with `@vue-vet/{os}-{arch}` optionalDependencies
- release workflow for Linux/macOS/Windows matrix builds, `SHA256SUMS`, and npm publish
- install, checksum, and rollback documentation

Work:

- extend safe fixes to multi-file transactions with failure rollback
- add additional evidence-backed edit producers without enabling unsafe or speculative changes

Exit criteria:

- SARIF and GitHub annotations preserve stable diagnostic identities
- safe fixes are previewable and applied transactionally
- supported native binaries and the npm launcher install without a Rust toolchain

## M4 — editor and agent surface

Status: complete (#12, #13 closed)

Implemented (#12):

- CLI `--explain` for rule ids and opaque finding ids
- `vue_vet_session` shared scan / explain / workspace bounds / buffer overlays
- thin `vue-vet --lsp` diagnostics (open/change/save, unsaved overlays, versioned publish)
- safe quick-fix code actions from explicitly safe diagnostic edits (client-applied)
- thin `vue-vet --mcp` agent tools (scan / explain / safe-fix preview; no silent apply)

Implemented (#13):

- quality corpus manifest + tree digests (`fixtures/quality`)
- precision labels for corpus projects (`just quality-gates`)
- cold/warm diagnostic identity tests; CodSpeed scan-mode benches
- compatibility matrix (`just compat-matrix`, CI + release job)
- release workflow runs compat + quality-gates + oracle before builds
- methodology + published baselines: [quality-gates.md](./docs/quality-gates.md),
  [quality-baselines.md](./docs/quality-baselines.md)

Work (post-#13 hygiene):

- expand corpus / precision labels as product surface grows
- cite CodSpeed + baselines in Beta release notes
- document a stable API for third-party integrations beyond JSON v1

Exit criteria:

- CLI, LSP, CI, and agent surfaces return the same diagnostic identities
- fixes are previewable and deterministic
- performance regressions and precision regressions block releases

## Release gates

### Alpha — complete

- [x] M0 complete
- [x] 100+ documented high-confidence built-in rules (plus practice channel)
- [x] JSON output declares its initial versioned contract (`schema_version: 1`)
- [x] the complete reference fixture corpus is covered by a no-crash integration test

### Beta

- M1 complete and core M2 graph operational
- baselines, diff mode, cache, and SARIF available
- native binaries for major desktop CI targets
- measured precision and performance published

### 1.0

- stable configuration and diagnostic contracts
- Vue and Nuxt reference suites maintained in CI
- upgrade policy for Vize and Oxc documented
- security, contribution, release, and support policies in place

## Non-goals before beta

- replacing `vue-tsc` for every TypeScript type-checking case
- becoming a formatter or bundler
- enabling speculative AI fixes by default
- matching rule counts at the expense of precision

## Current next step

M0–M4 delivery issues are complete on `main`. `v0.1.17` published crates.io
(`vue_vet_core`, `vue_vet_reactivity`), npm (`@vue-vet/cli`), and GitHub Release
archives. Tracker [#14](https://github.com/alexzhang1030/vue-vet/issues/14)
stays open for release coordination.

Near-term product work: keep `#13` quality gates green. The checksummed corpus
now indexes twelve projects with TP labels plus project-level `false_positive`
pins for safe patterns (see `fixtures/quality/README.md`). Deepen high-confidence
reactivity coverage without racing Beta. Beta still requires publishing measured
precision/performance evidence in the release notes
([quality-baselines.md](./docs/quality-baselines.md), CodSpeed, precision delta).

The engine lifecycle now uses exact `FileId` identities, a single discovery
snapshot for cache lookup and analysis, bounded module workers, prepared Oxc
module facts, partial file outcomes, and reverse-dependency invalidation. The
1k/5k module and continuous-edit benchmarks are the regression evidence for
large-workspace scaling.
