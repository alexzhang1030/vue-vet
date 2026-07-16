# Vue Vet

**Vet your Vue.**

Vue Vet is a Rust-native health check for Vue and Nuxt codebases. It aims to
combine project-level diagnostics, an actionable score, diff-aware CI, and
user-defined structural rules without routing Vue source through JavaScript.

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

`--format json` emits a versioned top-level object. Alpha uses
`"schema_version": 1`; consumers must reject unsupported major schema versions
instead of guessing. Field ordering is not part of the contract. See
[the JSON output contract](docs/json-output.md) for compatibility rules.

## Architecture

```text
vue-vet CLI       discovery, configuration, output, exit policy
      |
vue-vet-vize      Vize SFC/template facts + Oxc script facts
      |
vue-vet-rules     deterministic high-confidence rule registry
      |
vue-vet-core      diagnostics, spans, scoring, stable public model
```

Vize is the semantic source of truth for Vue SFCs. Oxc owns JavaScript and
TypeScript semantics. ast-grep remains the planned extension engine for declarative
project rules; it will not replace Vue-aware analysis.

See [the architecture decision](docs/adr/0001-analysis-stack.md) and
[the roadmap](ROADMAP.md). Project resolution and Nuxt convention limitations
are documented in [the project graph guide](docs/project-graph.md).
Cache keys, baseline fingerprints, and diff completeness guarantees are
documented in [the cache and diff guide](docs/cache-baseline-diff.md).

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
