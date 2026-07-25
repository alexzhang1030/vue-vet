# vue_vet_core

Stable diagnostics, source spans, scoring, edits, and reactivity fact types for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

This crate is the Vue Vet-owned contract layer. Parser adapters (`vue_vet_vize`,
`vue_vet_oxc`) and the reactivity tracer (`vue_vet_reactivity`) produce these
types; rules, reporters, cache, and CLI consume them. Dependency AST types from
Vize or Oxc never appear here.

## Status

Early `0.x`. Wire formats that cross process or cache boundaries carry explicit
schema / graph version fields (for example `REACTIVITY_GRAPH_VERSION`). Treat
the Rust API as evolving until Vue Vet hits a stable release.

## Install

```toml
[dependencies]
vue_vet_core = "0.1"
```

## License

MIT
