# Vapor migration research oracles

Research-only Node harnesses behind the opt-in `vapor-migration` assessment
group. Production semantic analysis stays in Rust with Vize and Oxc; the
JavaScript here is a development oracle that compiles real SFCs with the
pinned Vue 3.6 compiler and mounts the output in jsdom. The audited toolchain
tuple, observed matrix, admitted runtime envelope, and verdict mapping are on
the evidence page: [`docs/research/vapor-migration.md`](../../docs/research/vapor-migration.md).

## Run

From the repository root (Node required; no Rust rebuild):

```sh
just vapor-migration-research
```

CI runs the same recipe (`vapor-migration-research` in
`.github/workflows/ci.yml`). It installs from this directory's
`package-lock.json` via `npm ci` and runs `npm run validate`, which chains
`compile` (compiler matrix + focused assertions), `runtime:prod` and
`runtime:dev` (runtime differential on both inlined vapor browser builds),
`negative-control` (invalid SFC child must exit 1), and `check-surface`
(public prose, link, and pin portability).

## Results

Generated outputs land under `output/` (gitignored); committed files are the
harness, lockfile, compact `expectations/`, and this README. Widening the
product `runtime-envelope` whitelist requires a passing runtime fixture pair here first.
