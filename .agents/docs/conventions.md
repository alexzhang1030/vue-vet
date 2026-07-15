# Engineering conventions

## Rule contract

- Built-in IDs use `vue-vet/<category>/<name>` and are treated as user-facing stable identifiers.
- Every rule declares category, default severity, confidence, and a documentation key.
- A rule lands with rationale, bad/good examples, limitations, positive fixtures, common safe patterns, false-positive regressions, exact-span assertions, and reporter snapshots.
- Low-confidence heuristics are opt-in and never enter the default preset merely to increase rule count.

## Source locations

Internal locations are byte offsets into the original SFC source. User-facing line and column values are derived explicitly. Span changes require ASCII, Unicode, multiline, and relevant CRLF fixtures. Never assume a byte offset is a character index.

## Deterministic output

Sort diagnostics by normalized repository-relative path, byte offset, and rule ID. Do not expose platform path separators or hash-map iteration order in snapshots, JSON, baselines, or cache identities.

## Dependency boundaries

Vize, Oxc, and ast-grep types remain inside their adapters. Stable downstream code consumes Vue Vet-owned facts. Dependency upgrades are reviewed as behavior changes and include compatibility evidence rather than blind snapshot replacement.

## Testing and completion

Rust work is not complete until format, Clippy with warnings denied, workspace tests with the lockfile, and relevant fixture/integration tests pass. When local execution is unavailable, state that limitation and use CI as the evidence. Never claim a check passed when it was not run.

## Commits and pull requests

Commit messages follow Conventional Commits: `type(scope): imperative summary`. Use `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, or `revert`; use `!` and a `BREAKING CHANGE:` footer when a stable contract breaks. The scope names the affected product or crate boundary when that improves retrieval, for example `feat(rules): add stable v-for key diagnostic`.

Normal development happens on a focused branch and is reviewed through a pull request linked to its GitHub issue. Keep the PR draft while acceptance criteria or checks remain incomplete. Direct commits to `main` are reserved for an explicit maintainer request or a documented emergency; convenience or missing local tooling is not sufficient reason to bypass review.

## Planning and records

GitHub issues hold live implementation tasks and checklists. [ROADMAP.md](../../ROADMAP.md) holds milestone intent and release gates. PCR records hold durable rationale, architecture, conventions, and traps. Update the appropriate layer instead of duplicating the same plan in all three.
