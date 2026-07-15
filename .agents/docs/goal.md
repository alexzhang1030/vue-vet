# Product goal

Vue Vet is a Rust-native health check for Vue and Nuxt codebases. It should tell a developer what is wrong or risky, why the finding matters and how confident it is, and whether a change made the repository healthier.

## Audience

The primary audience is Vue and Nuxt developers running a fast local command or CI check. Secondary consumers are editor integrations and coding agents that need the same stable diagnostic identities and evidence.

## Product shape

The target product combines Vue-aware semantic diagnostics, deterministic scoring, baselines and diff-aware CI, cross-file project intelligence, conservative fixes, and project-defined structural rules. CLI, CI, LSP, and agent surfaces must consume the same underlying analysis instead of growing independent implementations.

## Priorities

1. Precision before rule count.
2. Semantic evidence before heuristic pattern matching.
3. Deterministic, explainable findings before clever scoring.
4. Safe, previewable changes before broad autofix coverage.
5. Measured compatibility and performance before release claims.

## Non-goals before Beta

- Replacing `vue-tsc` for every TypeScript type-checking case.
- Becoming a formatter, compiler, or bundler.
- Moving analysis logic into the npm launcher.
- Enabling speculative AI-generated fixes by default.
- Matching another doctor's rule count at the expense of Vue-specific precision.

The milestone plan and executable release gates live in [the root roadmap](../../ROADMAP.md) and [tracking issue #14](https://github.com/alexzhang1030/vue-vet/issues/14).

