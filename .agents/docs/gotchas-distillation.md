# gotchas distillation draft

Draft for a human vouch of [gotchas.md](./gotchas.md). This file does not replace that ledger. Nothing here is vouched. Do not delete ledger text from this draft alone.

The ledger is already grouped and much shorter than the 1 377-line survey. The cut below is a disposition of the headings that exist now.

## Keep in the ledger

These are cross-cutting traps. A later cut should leave them in `gotchas.md`.

- Cargo target per worktree.
- Native-size budget is a 3 % regression guard, and production `WorkCounter` stays zero-sized.
- Source spans stay byte offsets. Line/column is a separate conversion. Unicode and CRLF are tests.
- Paths are identities. `ends_with` is the wrong monorepo lookup.
- Vize owns template semantics. Oxc owns JS/TS. Pins stay exact.
- Config is applied after semantic analysis and is part of cache identity.
- Demand reach is not source5 evidence.
- Template-only SFC edits re-run script analysis, because template-ref demand joins during the script walk.
- Cache identity is `AnalysisStackIdentity` plus the hand versions. A forgotten bump serves stale diagnostics.
- Context invalidation is not a re-parse. Dirty file ids are not dirty work.
- No parallel pattern engine. The score is provisional. Vue behavior is capability-gated.

## Move next to the rule or the code

Rule-shaped traps belong in the rule essay, with the premise script named there. `just oracle-all` runs those scripts.

- Accessible content vs `has_children`.
- Element spans are start-tag only.
- JSX / SFC macros, and `<script vapor>` as setup.
- Tracking-semantics bullets, absence rules, typed ref parameters, cross-module summaries.
- Nuxt / auto-import seeds, style `v-bind`, identifier escapes.
- Proxy identity, collection escape-depth exhaustion, `customRef` lost notification, watch cleanup identity, `once: true`.
- Vapor migration envelope.
- Benchmark unwind vs release abort, crates.io `User-Agent`, safe-fix ranges, session sharing, reporter color / progress / MCP framing.

## Already updated in the ledger

- `SourceContractStats` is five `u64` fields. The old nine-field sentence was stale.
- Premise recipes `oracle-self-trigger`, `oracle-cleanup-identity`, and `oracle-stale-settlement` are gone. The ledger now names `self-trigger-runs.mjs`, `cleanup-identity-runs.mjs`, `stale-settlement-runs.mjs`, and `just oracle-all`.

## Leave for the vouch

Deleted-machinery notes belong in `git log`, not a second copy of this draft. `gotchas.md` stays the ledger until that cut is vouched.
