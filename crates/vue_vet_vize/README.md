# vue_vet_vize

**Vize-powered Vue SFC analysis** for [Vue Vet](https://github.com/alexzhang1030/vue-vet).

Owns `.vue` parse → Vue Vet `SfcFacts`, template/style surfaces, and dual-script
`ModuleSource` construction. Script semantics are delegated to `vue_vet_oxc`.
Vize AST must not escape this crate into rules, reporters, cache, or public
contracts.

SFC parse uses `vize_croquis::sfc` only — **never** `vize_atelier_sfc` (that
crate pulls LightningCSS / DOM / SSR compile). Template AST walks use
`vize_atelier_core`.

## Status

Workspace-internal (`publish = false`). Called from `vue_vet_session` file
pipeline.

## Public API

| Item | Role |
| --- | --- |
| `analyze_sfc_with_facts(path, source)` | Full SFC → `AnalyzedSfc` |
| `analyze_sfc_facts_reusing(...)` | Block-level reuse via fingerprints |
| `AnalyzedSfc` | `facts`, primary/ordinary `ModuleSource`, `revisions` |
| `SfcBlockRevisions` / `BlockFingerprint` | Content digest + absolute span |
| `AnalyzeError::{Parse, Template, Script}` | Scoped failures |

Primary module prefers `<script setup>`; ordinary `<script>` companion id is
`{path}#script` so both blocks re-trace with module seeds independently.

## Layout

| Module | Why it exists |
| --- | --- |
| `lib.rs` | Analyze entry + reuse orchestration |
| `template.rs` | `vize_atelier_core` walk → `TemplateFacts` |
| `style.rs` | `<style>` `v-bind(ident)` expressions (not in block revisions — refreshed on full reuse) |
| `span.rs` | Analysis-scoped `LineIndex` install/clear for SFC-absolute spans |

Template reuse only when script + setup fingerprints also match (JSX merge
depends on script).

## Related docs

- [architecture PCR](../../.agents/docs/architecture.md) (adapter layouts)
- [Vize compatibility](../../docs/vize-compatibility.md)
- [gotchas PCR](../../.agents/docs/gotchas.md) (SFC surfaces, style join)
- [Workspace crates](../../docs/crates.md)

## License

MIT
