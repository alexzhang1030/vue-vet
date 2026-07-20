# Vue Vet

**Vet your Vue.**

Vue Vet is a Rust-native health check for Vue and Nuxt codebases. It aims to
combine project-level diagnostics, an actionable score, diff-aware CI, and
conservative fixes without routing Vue source through JavaScript.

The project is an early local-doctor implementation. Today it:

- discovers `.vue` files while respecting `.gitignore`;
- parses every SFC with Vize;
- analyzes JavaScript, TypeScript, JSX, and TSX in both Vue script blocks with Oxc;
- runs 28 documented high-confidence correctness, reactivity, accessibility,
  security, and maintainability rules with original-source locations;
- supports strict versioned configuration, presets, severity overrides, path
  filters, and scoped suppressions;
- builds a deterministic Vue/Nuxt project graph with initial cross-file findings;
- emits human-readable or JSON output;
- returns a CI-friendly exit code and a deterministic score.

## Try it

```bash
just vet .
just vet . --format json
just vet fixtures/projects/basic --deny-warnings
just vet fixtures/projects/nuxt-graph --print-graph
just vet . --cache-stats
just vet . --write-baseline vue-vet-baseline.json
just vet . --baseline vue-vet-baseline.json --diff main
```

## Configuration

Vue Vet discovers `vue-vet.toml` at the scan root. Use `--config <file>` to
select another file or `--print-config` to inspect the deterministic effective
configuration.

```toml
version = 1
preset = "recommended"
include = ["src/**/*.vue"]
exclude = ["src/generated/**"]

[rules]
"vue-vet/security/no-v-html" = "error"
"vue-vet/accessibility/no-autofocus" = "off"
```

Levels are `off`, `info`, `warning`, and `error`. Unknown fields, rule IDs,
values, and future versions fail before scanning. Scoped exceptions work in
Vue template and script comments:

```vue
<!-- vue-vet-disable-next-line vue-vet/security/no-v-html -->
<div v-html="reviewedHtml" />
```

Unused suppressions are reported so temporary exceptions do not become dead
configuration.

Exit codes are `0` for a passing scan, `1` when diagnostics cross the configured
threshold, and `2` for an operational failure.

## JSON contract

`--format json` emits a versioned top-level object. The initial contract uses
`"schema_version": 1` with deterministic finding IDs, normalized paths, exact
scan coverage, and completeness metadata. Consumers must reject unsupported
schema versions instead of guessing. Field ordering is not part of the contract.
See [the JSON output contract](docs/json-output.md) for compatibility rules.

## Architecture

```text
vue-vet CLI          discovery, configuration, orchestration, exit policy
      |
vue-vet-vize/oxc     Vize SFC/template facts + Oxc script facts
      |
vue-vet-reactivity  local and cross-module reactive dependency graphs
      |
vue-vet-project      deterministic Vue/Nuxt graph and cross-file findings
      |
vue-vet-rules        deterministic high-confidence rule registry
      |
vue-vet-core         diagnostics, spans, scoring, edits, stable public model
      |
vue-vet-reporters    deterministic text and versioned JSON rendering
```

Vize is the semantic source of truth for Vue SFCs and templates. Oxc owns
JavaScript and TypeScript semantics. Vue Vet keeps one semantic analysis stack
instead of embedding a parallel structural-pattern engine.

See [the architecture decision](docs/adr/0001-analysis-stack.md) and
[the roadmap](ROADMAP.md). Project resolution and Nuxt convention limitations
are documented in [the project graph guide](docs/project-graph.md).
Cache keys, baseline fingerprints, and diff completeness guarantees are
documented in [the cache and diff guide](docs/cache-baseline-diff.md).
The preview-only edit contract and conflict rules are documented in
[the edit model guide](docs/edit-model.md).

Durable project rationale and agent guidance are indexed in the
[project context map](.agents/docs/README.md).

## Status

Alpha. The JSON contract is versioned and the reference fixture corpus is
covered by no-crash tests on Linux, macOS, and Windows. Vize itself is moving
quickly, so its dependency is pinned and upgrades must include fixture and
compatibility evidence.

See [CONTRIBUTING.md](CONTRIBUTING.md) for development workflow and
[SECURITY.md](SECURITY.md) for vulnerability reporting.

## Development

The repository pins Rust 1.97.0 and uses `just` as its task runner. Install
`just`, then prepare a fresh checkout with:

```sh
just setup
just roll-rust
```

`just setup` installs the pinned prek release, installs pre-commit and pre-push
hooks, and runs `just doctor` to verify the complete toolchain. Use
`just --list` to see focused formatting, linting, smoke-test, and fix tasks. CI
invokes the same recipes as local development.
