# Quality gates (precision, performance, release)

Issue [#13](https://github.com/alexzhang1030/vue-vet/issues/13). This document is the
executable methodology for measured correctness and performance. Anecdotal
confidence is not a release argument.

## Principles

- Prefer committed, checksummed inputs over downloading mutable third-party
  repositories in CI.
- Do not claim universal recall from a small corpus.
- Performance checks complement correctness tests; never disable real analysis
  to improve a number.
- Snapshot and threshold changes are reviewed evidence, not rubber stamps.

## Reference corpus

Canonical inventory: [`fixtures/quality/manifest.json`](../fixtures/quality/manifest.json).

| Project | Roles | Why it is in the corpus |
| --- | --- | --- |
| `fixtures/projects/basic` | vue, smoke, precision | Smallest default finding (`no-v-html`). |
| `fixtures/projects/nuxt-graph` | nuxt, performance, precision | Cross-file graph, unresolved import, unused component. |
| `fixtures/projects/configured` | vue, config | Config discovery and severity overrides. |
| `fixtures/projects/vue-3.5` | vue, compatibility | Version-aware rule surface. |

Selection rules:

1. MIT/Apache-2.0 or project-owned fixtures only.
2. Small enough for every PR; large enough to exercise graph + cache paths.
3. Every corpus member has a `tree_digest` (see below). Changing files without
   updating the digest fails CI.
4. External showcase repos may be reviewed offline; they are not CI inputs.

### Tree digest algorithm

Skip directories named `node_modules`, `target`, and files named `.DS_Store`.

For each remaining file under the project root, in lexicographic order of
repository-relative POSIX paths:

```text
{relative/path}\t{sha256_hex_of_bytes}
```

Join those lines with `\n` (no trailing newline after the last line) and take
SHA-256 of the UTF-8 blob. That hex digest is `tree_digest`.

Recompute with `just quality-digest` after intentional fixture edits.

## Precision review

Labeled expectations live under
[`fixtures/quality/precision/`](../fixtures/quality/precision/).

Each file is JSON:

```json
{
  "project": "nuxt-graph",
  "findings": [
    {
      "rule_id": "vue-vet/project/unresolved-import",
      "file": "pages/broken.vue",
      "classification": "true_positive",
      "rationale": "broken.vue intentionally imports a missing module"
    }
  ]
}
```

`classification` is one of `true_positive`, `false_positive`, or
`known_limitation`.

Gate behavior (`just quality-gates` / `quality_gates` tests):

- The scan of a project must emit exactly the `(rule_id, file)` pairs listed as
  `true_positive` or `known_limitation`.
- Entries classified `false_positive` document bugs that must not appear; if they
  appear, the gate fails.
- Adding or removing an expected finding requires an explicit PR rationale.

Rule-level `valid/` fixtures remain the primary false-positive regressions; the
quality corpus pins multi-file project expectations.

## Performance baselines

Modes measured on the quality corpus:

| Mode | How it is exercised |
| --- | --- |
| Cold | Fresh cache directory, full analyze. |
| Warm | Second analyze against a primed content-addressed cache. |
| Incremental | `analyze_with_overlays` with one open buffer (session API). |
| Diff | Full analyze then `filter_diff` with a fixed changed-file set (no git). |

Canonical PR comparison uses CodSpeed (`just bench` / `just bench-codspeed-*`):

- SFC micro-benchmarks: `vue_vet_vize` `analyze_sfc`
- Project scan modes: `vue_vet_session` `scan_modes`

Local timing notes (developer machines) may be recorded in PRs but are not
release budgets. CodSpeed simulated-CPU results are the comparable signal
([conventions](../.agents/docs/conventions.md)).

Cold/warm diagnostic identity must match for the same corpus snapshot (enforced
in `quality_gates` tests).

## Compatibility evidence

| Dependency | Required evidence on upgrade |
| --- | --- |
| Rust toolchain | `rust-toolchain.toml` bump + `just roll-rust` on CI matrix |
| Vize | [vize-compatibility](./vize-compatibility.md) procedure |
| Oxc | Pin note in workspace Cargo.toml + fixture/snapshot review |
| Vue / Nuxt surface | `vue-3.4` / `vue-3.5` project fixtures + quality corpus |

`ast-grep` is not part of the analysis stack; ignore historical mentions.

## Executable release checklists

### Alpha (complete)

- [x] M0–M3 delivery issues closed for the Alpha contract
- [x] JSON `schema_version: 1` documented
- [x] Reference fixture corpus no-crash test
- [x] CodSpeed + Codecov wired on pull requests

### Beta (blocking)

Run and keep green:

1. `just roll-rust`
2. `just quality-gates`
3. `just oracle`
4. CodSpeed suite (SFC + scan modes) without unexplained regressions
5. Codecov thresholds (project ≤1pp drop; patch ≥80%)
6. Cross-platform CI (`ubuntu`, `macos`, `windows`)
7. Published methodology: this file + `fixtures/quality/manifest.json`
8. Native release matrix builds (`release.yml`) for supported targets

A Beta tag must not ship while `just quality-gates` fails or while an unexplained
CodSpeed / precision expectation change lands without PR rationale.

### 1.0 (blocking, in addition to Beta)

1. Stable configuration and diagnostic contracts (no silent JSON breakage)
2. Quality corpus still green; expand only with digests + precision labels
3. Documented Vize/Oxc upgrade policy followed for the release train
4. SECURITY / CONTRIBUTING / support policy docs present
5. Release workflow refuses publish when a required gate job fails

## Commands

```bash
just quality-digest    # print tree digests for corpus projects
just quality-gates     # integrity + precision + cold/warm identity
just bench             # local Divan benches (SFC + scan modes)
just bench-codspeed-build
just bench-codspeed-run
```

## Changing a gate

1. Update fixtures and/or precision JSON.
2. Refresh `tree_digest` values via `just quality-digest`.
3. Explain the behavior or threshold change in the PR body.
4. Do not loosen a gate solely to make CI green.
