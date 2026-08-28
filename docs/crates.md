# Workspace crates

Each crate under `crates/` owns one analysis-pipeline stage. Prefer the crate
README for ownership, public API, and non-goals. Cross-cutting judgment stays in
[architecture](../.agents/docs/architecture.md).

```text
CLI / --lsp / --mcp
  → session          config, cache, discovery, scan, explain, path bounds
      → vize / oxc   SFC + script/JSX → Vue Vet facts (+ module summaries)
      → project      graph + enrichment + reactivity handoff + project rules
      → rule_query → rules + practice
      → finalize     DiagnosticFinalizer → ScanSummary
  → reporters        text / JSON / SARIF / GitHub / explain renderers
```

| Crate | Owns | Does not |
| --- | --- | --- |
| [`vue_vet_core`](../crates/vue_vet_core/README.md) | Facts, diagnostics, spans, edits, `Rule` | Parser AST |
| [`vue_vet_reactivity`](../crates/vue_vet_reactivity/README.md) | Tracer engine / module summaries | Ecosystem callee names |
| [`vue_vet_plugins`](../crates/vue_vet_plugins/README.md) | Nuxt / vue-i18n named API bags | Dynamic JS plugin host |
| [`vue_vet_vize`](../crates/vue_vet_vize/README.md) | SFC parse → `SfcFacts` | Script semantics (→ oxc) |
| [`vue_vet_oxc`](../crates/vue_vet_oxc/README.md) | Script/JSX → facts + `ModuleSummary` | Leaking Oxc arena |
| [`vue_vet_project`](../crates/vue_vet_project/README.md) | Project graph pipeline | File-rule packs / session I/O |
| [`vue_vet_rule_query`](../crates/vue_vet_rule_query/README.md) | Shared fact walks for rules | Rule packs / Vize / Oxc |
| [`vue_vet_rules`](../crates/vue_vet_rules/README.md) | Built-in lint / gate rules | Practice channel |
| [`vue_vet_practice`](../crates/vue_vet_practice/README.md) | Practice suggestions | Score / default CI exit |
| [`vue_vet_config`](../crates/vue_vet_config/README.md) | `vue-vet.toml` + suppressions | Workspace discovery |
| [`vue_vet_cache`](../crates/vue_vet_cache/README.md) | Cache, baselines, git-diff filter | Analysis / AST persistence |
| [`vue_vet_session`](../crates/vue_vet_session/README.md) | Long-lived analysis handle | Protocol adapters / disk apply |
| [`vue_vet_reporters`](../crates/vue_vet_reporters/README.md) | Deterministic renderers | Session state / exit policy |
| [`vue_vet_cli`](../crates/vue_vet_cli/README.md) (`vue-vet`) | Clap, I/O policy, fix apply | Analysis logic |
| [`vue_vet_lsp`](../crates/vue_vet_lsp/README.md) | Stdio LSP adapter | Writing files / re-tracing |
| [`vue_vet_mcp`](../crates/vue_vet_mcp/README.md) | Stdio MCP tools | Applying edits |

Published on crates.io (dependency order): `vue_vet_core` → `vue_vet_reactivity`
→ `vue_vet_plugins`. Everything else is `publish = false`.
