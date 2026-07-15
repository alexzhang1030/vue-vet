# Vue Vet

**Vet your Vue.**

Vue Vet is a Rust-native health check for Vue and Nuxt codebases. It aims to
combine project-level diagnostics, an actionable score, diff-aware CI, and
user-defined structural rules without routing Vue source through JavaScript.

The project is an early vertical slice. Today it:

- discovers `.vue` files while respecting `.gitignore`;
- parses every SFC with Vize;
- reports unsafe `v-html` usage with source locations;
- emits human-readable or JSON output;
- returns a CI-friendly exit code and a deterministic score.

## Try it

```bash
just vet .
just vet . --format json
just vet fixtures --deny-warnings
```

Exit codes are `0` for a passing scan, `1` when diagnostics cross the configured
threshold, and `2` for an operational failure.

## Architecture

```text
vue-vet-cli   project discovery, output, exit policy
     |
vue-vet-vize  Vue SFC parsing and Vue-aware analysis
     |
vue-vet-core  diagnostics, spans, scoring, stable public model
```

Vize is the semantic source of truth for Vue SFCs. Oxc will own JavaScript and
TypeScript semantics. ast-grep will be an extension engine for declarative
project rules; it will not replace Vue-aware analysis.

See [the architecture decision](docs/adr/0001-analysis-stack.md) and
[the roadmap](ROADMAP.md).

Durable project rationale and agent guidance are indexed in the
[project context map](.agents/docs/README.md).

## Status

Experimental. Vize itself is moving quickly, so its dependency is pinned and
upgrades should be paired with fixture and compatibility tests.

## Development

The repository pins Rust 1.97.0 and uses `just` as its task runner. Install
`just` and [`prek`](https://prek.j178.dev/), then run:

```sh
just install-hooks
just roll-rust
```

Use `just --list` to see focused formatting, linting, test, and fix tasks. CI
invokes the same recipes as local development.
