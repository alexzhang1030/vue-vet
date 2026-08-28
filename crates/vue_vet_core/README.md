# vue_vet_core

Stable diagnostics, source spans, scoring, edits, and reactivity fact types for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

This crate is the Vue Vet-owned contract layer. Parser adapters (`vue_vet_vize`,
`vue_vet_oxc`), the reactivity tracer (`vue_vet_reactivity`), and ecosystem
plugins (`vue_vet_plugins`) produce or consume these types; rules, reporters,
cache, and CLI consume them. Dependency AST types from Vize or Oxc never appear
here.

Published library crates (dependency order): `vue_vet_core` →
`vue_vet_reactivity` → `vue_vet_plugins`.

## Status

Early `0.x`. Wire formats that cross process or cache boundaries carry explicit
schema / graph version fields (for example `REACTIVITY_GRAPH_VERSION`). Treat
the Rust API as evolving until Vue Vet hits a stable release.

## Install

```toml
[dependencies]
vue_vet_core = "0.1"
```

## Related docs

- [Workspace crates](../../docs/crates.md)
- [architecture PCR](../../.agents/docs/architecture.md)

## License

MIT
