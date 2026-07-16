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
- ast-grep is the declarative extension lane for project-specific structural
  rules; it is not the primary semantic engine.
- Vue Vet owns its diagnostic schema, scoring, suppression, caching, baselines,
  fixes, and output formats.
- Vize stays pinned until its API stabilizes. Upgrades require compatibility
  fixtures and diagnostic snapshots.

## System shape

```text
vue-vet CLI
  -> project discovery and configuration
  -> Vize SFC/template analysis
  -> Oxc script analysis
  -> project graph and cross-file rules
  -> ast-grep custom rules
  -> normalize, rank, score, report, fix
```

Planned crate boundaries:

```text
vue-vet-core       stable diagnostics, spans, scoring, rule contracts
vue-vet-vize       Vize adapter and Vue semantic facts
vue-vet-oxc        JS/TS semantic facts and import resolution
vue-vet-rules      built-in rules and presets
vue-vet-patterns   ast-grep configuration and execution
vue-vet-project    project graph, cache, baseline, diff
vue-vet-reporters  text, JSON, SARIF, GitHub annotations
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

Current: 28 high-confidence built-in rules.

Status: implementation in progress

Implemented in the Phase 2 branch:

- Oxc 0.127 semantic adapter for JavaScript, TypeScript, JSX, and TSX script blocks
- dependency-neutral imports, bindings, calls, and member-write facts
- versioned strict configuration, recommended/none presets, severity overrides,
  include/exclude globs, and scoped suppressions with unused-suppression diagnostics
- 28 documented high-confidence recommended rules with positive and safe fixtures
- normalized compiler-macro assignment facts and alias-aware readonly-props enforcement
- semantic reactivity graph facts plus package-version-gated Vue 3.4/3.5 diagnostics

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

Status: implementation in progress

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

## M3 — extensibility and CI

Work:

- integrate ast-grep Rust crates behind `vue-vet-patterns`
- load project YAML rules with a versioned JSON Schema
- map custom findings into the same diagnostic and scoring model
- add SARIF and GitHub annotations
- implement baselines and `--diff <ref>`
- introduce machine-readable edits and transactional safe autofix
- publish native binaries and a thin npm launcher

ast-grep acceptance criteria:

- custom rules work for supported script and template surfaces
- invalid patterns fail during configuration loading, not halfway through a scan
- custom and built-in findings deduplicate deterministically
- semantic built-in rules remain authoritative when a pattern overlaps

## M4 — editor and agent surface

Work:

- expose diagnostics and code actions over LSP
- add `--explain` with evidence and rule documentation
- expose the project model through an MCP/agent interface
- add benchmark, precision, and regression suites over representative Vue and
  Nuxt repositories
- document a stable API for third-party integrations

Exit criteria:

- CLI, LSP, CI, and agent surfaces return the same diagnostic identities
- fixes are previewable and deterministic
- performance regressions and precision regressions block releases

## Release gates

### Alpha — complete

- [x] M0 complete
- [x] 28 documented high-confidence rules
- [x] JSON output declares `schema_version: 1`
- [x] the complete reference fixture corpus is covered by a no-crash integration test

### Beta

- M1 complete and core M2 graph operational
- baselines, diff mode, cache, and SARIF available
- native binaries for major desktop CI targets
- measured precision and performance published

### 1.0

- stable configuration and diagnostic contracts
- Vue and Nuxt reference suites maintained in CI
- upgrade policy for Vize, Oxc, and ast-grep documented
- security, contribution, release, and support policies in place

## Non-goals before beta

- replacing `vue-tsc` for every TypeScript type-checking case
- becoming a formatter or bundler
- enabling speculative AI fixes by default
- matching rule counts at the expense of precision

## Current next step

Stabilize the Alpha contract on representative external Vue and Nuxt repositories,
then complete the remaining M1 precision/performance evidence and M2 cross-file
diagnostics required for Beta.
