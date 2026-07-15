# AGENTS.md

This file is the single source of truth for coding agents working in Vue Vet. `CLAUDE.md` is a symlink to this file; do not maintain parallel instruction files.

## Start here

1. Read [the project context map](.agents/docs/README.md) and the exact records routed for the area you will change.
2. Find the relevant GitHub issue from [the implementation tracker](https://github.com/alexzhang1030/vue-vet/issues/14). Keep the change inside its scope and acceptance criteria.
3. Before editing public contracts or dependency boundaries, read [architecture](.agents/docs/architecture.md), [technology stack](.agents/docs/technology-stack.md), and [conventions](.agents/docs/conventions.md).

## Repository working agreement

- Keep the engine and CLI Rust-native. JavaScript is allowed only for the future thin npm launcher; do not move analysis logic into it.
- Treat Vize as the Vue SFC/template semantic source, Oxc as the JS/TS semantic source, and ast-grep as the custom structural-rule lane. Do not substitute pattern matching for built-in semantic rules.
- Keep Vue Vet's stable types independent of Vize, Oxc, and ast-grep AST types.
- Introduce a crate only when a working vertical slice exercises its boundary.
- Every built-in rule change includes metadata, documentation, positive fixtures, safe-pattern fixtures, false-positive regressions, exact-span assertions, and reporter snapshots.
- Keep results deterministic across operating systems: normalize paths, sort diagnostics, and never depend on hash-map iteration order.
- Preserve source byte offsets and explicitly convert them to user-facing line/column positions. Test Unicode and CRLF when span logic changes.
- Do not loosen a pinned analysis dependency or update snapshots merely to make CI green. Record why behavior changed and add compatibility evidence.
- Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --locked` before declaring Rust work complete. If the environment cannot run them, report that explicitly and rely on CI rather than claiming success.
- Update the relevant PCR record in the same change when architecture, intent, constraints, conventions, or known traps change.

<!-- PCR:START -->
## Project Context Records (PCR)

This project follows **Project Context Records (PCR)** — methodology: https://github.com/hyf0/project-context-records. PCR keeps the project's durable design context — the *why*, the decisions, the architecture — so you inherit it instead of re-deriving or re-litigating what's already settled.

When working here:
- **Where they live.** Records are in `.agents/docs/`, one topic per file, cross-linked with relative Markdown links. A `README.md` there is the **map**: it routes code areas or hotspots to the exact record or heading. Create one when retrieval stops being a glance or one record grows into a long ledger.
- **Read first.** Start from the map if present, else scan the folder. Open the exact records or headings that cover an area before changing or answering for it.
- **Use the strongest durable form.** Put machine-checkable constraints in types, tests, lints, or CI; put local rationale beside the code with a link; use PCR for cross-cutting judgment, intent, and other context that must remain prose.
- **Record as you go.** Capture context when a decision lands, a trap costs you, a human corrects you, or a human asks. If it is true about this project, not durable in a stronger form, and useful beyond the moment, it is worth a record. Report records you change so a human can review or vouch them.
- **Keep it fresh.** Update affected records with the same change. When code and a record disagree, decide whether implementation drifted from intent or description went stale, then update the stale side; surface a vouched conflict. Back facts with durable evidence such as tests, reproducible commands, committed artifacts, stable URLs, or commit hashes — not ephemeral paths or missing screenshots.
- **Provenance.** Unstamped text is AI-accumulated: challenge and verify it freely. `[VOUCHED @handle YYYY-MM-DD]` means the named human explicitly accepts the covered words as current project direction, not that a factual claim is proven. At a non-heading line's end it covers that line; on its own line as the first nonblank line below a non-title heading it covers that section; on its own line as the first nonblank line below the document title it covers the file. Never put a new stamp in heading text: it breaks link anchors. Legacy stamps before a title or in a heading retain the project's prior scope; never move or reinterpret them without explicit human approval. Add one only on explicit instruction. A stamp added by work under review counts only if the named human confirms it; an unchanged stamp on the target branch is inherited project state. Material edits or scope-boundary changes remove stamps; formatting keeps them only if the covered words stay identical. Legacy undated stamps remain valid until re-vouched.
- **Distill when a human reviews.** Accumulation is noisy by design; the valve is a human review pass. Draft what to prune, merge, or promote, and flag vouches plausibly affected by changes to the areas or evidence they cover. The human decides and vouches.
- **Unattended.** With no human between iterations: keep the running plan as one live record, overwritten as truth changes; tidy your own unstamped layer — merge duplicates, prune dead notes — never the vouched one; when evidence argues with vouched direction, record the conflict and stay inside that direction unless progress becomes impossible; end by drafting the distillation for the returning human, conflicts included. No run, however long or green, vouches anything.
- **The basics.** The recommended starting list — most projects need these; draft the missing ones that apply:
  - `goal.md` — audience, goal, and non-goals; enroll the README instead if it already covers them.
  - `technology-stack.md` — why tools, restrictions, or pins exist; not a manifest dump.
  - `architecture.md` — units, boundaries, and why the lines are where they are.
  - `conventions.md` — deliberate departures from ecosystem defaults.
  - `gotchas.md` — traps already paid for, each with its why.
  - `DESIGN.md` — only for a visual surface; follow https://github.com/google-labs-code/design.md, keep it at the root, and enroll it in the map.
<!-- PCR:END -->

