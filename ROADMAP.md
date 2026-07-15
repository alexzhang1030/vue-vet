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

Status: in progress

Delivered:

- Rust workspace and `vue-vet` CLI
- `.vue` discovery with ignore support
- Vize SFC parsing
- stable serializable diagnostic model
- text and JSON reporters
- deterministic score and CI exit policy
- first diagnostic: `vue-vet/security/no-v-html`
- Linux, macOS, and Windows CI definition

Exit criteria:

- CI compiles and tests the workspace on all three operating systems
- `vue-vet fixtures --deny-warnings` finds the expected diagnostic
- output locations point to the exact source span
- malformed SFCs fail predictably without panics

Immediate tasks:

1. Run the first GitHub Actions build and fix dependency/API drift.
2. Replace the temporary attribute scanner with Vize template AST traversal.
3. Add golden fixtures for valid, invalid, commented, and Unicode SFC input.
4. Define the rule trait and a test harness for diagnostic snapshots.
5. Add license, contribution guide, and security policy before outside access.

## M1 — useful local doctor

Target: 15 high-confidence built-in rules.

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

### Alpha

- M0 complete
- at least 10 documented high-confidence rules
- JSON contract versioned
- no crashers in the reference fixture corpus

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

Complete M0 by getting CI green, then land Vize AST-backed `no-v-html` as the
reference implementation for the built-in rule API.
