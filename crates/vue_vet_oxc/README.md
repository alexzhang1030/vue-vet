# vue_vet_oxc

**Oxc-powered JavaScript / TypeScript facts** for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

One Oxc parse yields `ScriptBlockFacts`, optional JSX/TSX `TemplateFacts`, and a
`ModuleSummary` for cross-file linking. Reactivity graphs come from
`vue_vet_reactivity` with the product default plugin catalog
(`vue_vet_plugins::default_trace_config`).

Oxc arena / AST values must not cross this crate or thread boundary into rules,
reporters, or cache.

## Status

Workspace-internal (`publish = false`). Used by `vue_vet_vize` (SFC scripts) and
`vue_vet_session` / project follow loads for standalone modules.

## Public API

| Item | Role |
| --- | --- |
| `analyze_script(...)` | Script block → `ScriptBlockFacts` (+ graph) |
| `analyze_module_source(...)` | Full module → `ModuleAnalysis` |
| `ModuleAnalysis` | `script_facts`, `template_facts`, `module_trace` |
| `AnalyzeScriptError` | Parse / semantic / unsupported language |
| `template_expression_identifiers*` | Free-id reads for template surfaces |
| `v_for_alias_identifiers` / `slot_prop_alias_identifiers` | Alias shadowing |

Supported languages: `js` / `javascript`, `jsx`, `ts` / `typescript`, `tsx`.
Spans map to original SFC offsets via `script_offset` + `LineIndex`. JSX walk
is skipped for non-jsx/tsx sources.

## Layout

| Module | Why it exists |
| --- | --- |
| `lib.rs` | Parse + semantic build + trace handoff |
| `facts.rs` | Import / binding / call / write collectors |
| `jsx.rs` | JSX → `TemplateFacts` (no Babel transform) |
| `template_expr.rs` | Identifier extraction for template expression strings |

## Related docs

- [architecture PCR](../../.agents/docs/architecture.md) (adapter layouts)
- [reactivity tracer PCR](../../.agents/docs/reactivity-tracer.md)
- [vue_vet_plugins](../vue_vet_plugins/README.md) (auto-loaded catalog)
- [Workspace crates](../../docs/crates.md)

## License

MIT
