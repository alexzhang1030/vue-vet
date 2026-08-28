# vue_vet_oxc

**Oxc-powered JavaScript / TypeScript semantic facts** for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

One Oxc parse builds `ScriptBlockFacts`, optional JSX/TSX `TemplateFacts`, and a
`ModuleSummary` for cross-file linking. Reactivity graphs come from
`vue_vet_reactivity` with the product default plugin catalog
(`vue_vet_plugins::default_trace_config`).

## Status

Workspace-internal (`publish = false`). Adapter only: Oxc arena / AST values
must not cross thread or crate boundaries into rules, reporters, or cache.

## Layout

```text
lib.rs           analyze_script / analyze_module_source
facts.rs         import / binding / call / write collectors
template_expr.rs free-identifier reads for template surfaces
jsx.rs           JSX → TemplateFacts
```

## Related docs

- [architecture PCR](../../.agents/docs/architecture.md) (adapter layouts)
- [reactivity tracer PCR](../../.agents/docs/reactivity-tracer.md)
- [vue_vet_plugins](../vue_vet_plugins/README.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
