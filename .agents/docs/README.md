# Vue Vet context map

Open the record routed for the code or decision you are about to change. These records hold durable project judgment; temporary execution state belongs in GitHub issues and the root roadmap.

| When changing | Read | Why |
| --- | --- | --- |
| Product scope, target users, scoring intent, or release promises | [goal](./goal.md) and [root roadmap](../../ROADMAP.md) | Defines what Vue Vet is trying to become and what it deliberately refuses before Beta. |
| Dependencies, tool ownership, Rust-only constraints, or version pins | [technology stack](./technology-stack.md) | Explains why Vize, Oxc, and ast-grep have different jobs and why analysis dependencies are pinned. |
| Crate boundaries, data flow, diagnostics, project graph, reporters, or public APIs | [architecture](./architecture.md) and [analysis-stack ADR](../../docs/adr/0001-analysis-stack.md) | Defines the stable Vue Vet-owned boundaries around third-party parsers and semantic engines. |
| Rule IDs, fixtures, source spans, deterministic output, configuration, or completion checks | [conventions](./conventions.md) | Records deliberate engineering rules that contributors and agents must preserve. |
| Vize upgrades, offsets, SFC surface extraction, ast-grep overlap, or score stability | [gotchas](./gotchas.md) | Lists traps already identified and the evidence required before changing the current approach. |
| Current implementation order or delivery status | [GitHub tracker](https://github.com/alexzhang1030/vue-vet/issues/14) and [root roadmap](../../ROADMAP.md) | Issues are the live execution plan; the roadmap records milestone intent and release gates. |

Related records should be cross-linked. When a new record or important heading is added, update this map in the same change.

