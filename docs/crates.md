# Workspace crates

Each crate under `crates/` owns one pipeline stage. Prefer the crate README for
role and boundaries; PCR ([architecture](../.agents/docs/architecture.md)) holds
cross-cutting judgment.

| Crate | Role | Publish |
| --- | --- | --- |
| [`vue_vet_core`](../crates/vue_vet_core/README.md) | Stable facts, diagnostics, spans, edits, `Rule` | crates.io |
| [`vue_vet_reactivity`](../crates/vue_vet_reactivity/README.md) | Static reactivity tracer / module summaries | crates.io |
| [`vue_vet_plugins`](../crates/vue_vet_plugins/README.md) | Ecosystem named API bags (Nuxt, vue-i18n, …) | crates.io |
| [`vue_vet_vize`](../crates/vue_vet_vize/README.md) | Vize SFC / template → Vue Vet facts | workspace |
| [`vue_vet_oxc`](../crates/vue_vet_oxc/README.md) | Oxc script / JSX → Vue Vet facts | workspace |
| [`vue_vet_project`](../crates/vue_vet_project/README.md) | Project graph + enrichment + project rules | workspace |
| [`vue_vet_rule_query`](../crates/vue_vet_rule_query/README.md) | Shared fact queries for file rules | workspace |
| [`vue_vet_rules`](../crates/vue_vet_rules/README.md) | Built-in lint / gate rules | workspace |
| [`vue_vet_practice`](../crates/vue_vet_practice/README.md) | Practice suggestions (off score / default CI) | workspace |
| [`vue_vet_config`](../crates/vue_vet_config/README.md) | `vue-vet.toml`, presets, suppressions | workspace |
| [`vue_vet_cache`](../crates/vue_vet_cache/README.md) | Content-addressed cache, baselines, git diff | workspace |
| [`vue_vet_session`](../crates/vue_vet_session/README.md) | Long-lived analysis session | workspace |
| [`vue_vet_reporters`](../crates/vue_vet_reporters/README.md) | Text / JSON / SARIF / GitHub / explain | workspace |
| [`vue_vet_cli`](../crates/vue_vet_cli/README.md) (`vue-vet`) | CLI binary + `--lsp` / `--mcp` entry | workspace |
| [`vue_vet_lsp`](../crates/vue_vet_lsp/README.md) | Thin diagnostics LSP adapter | workspace |
| [`vue_vet_mcp`](../crates/vue_vet_mcp/README.md) | Thin MCP agent-tools adapter | workspace |

Pipeline sketch (see architecture for the full stage map):

```text
CLI / LSP / MCP
  → session (config, cache, discovery)
      → vize / oxc → facts
      → project (+ reactivity + plugins)
      → rule_query → rules + practice
      → finalize → reporters
```

Published crates stay independent of Vize/Oxc AST types. Workspace crates may
use those adapters internally but must not leak them through stable product
contracts.
