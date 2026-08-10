# vue_vet_plugins

Compile-time **ecosystem plugins** for [Vue Vet](https://github.com/alexzhang1030/vue-vet)
reactivity tracing.

Nuxt data helpers (`useAsyncData`, `useFetch`, …) and vue-i18n (`useI18n` /
`t` / `d` / …) API-bag contracts live here — **not** in `vue_vet_reactivity`.
The tracer engine only consumes a `NamedApiBag` catalog.

This is still **Rust static data**, not a dynamic JS plugin host.

## Status

Early `0.x`, published with tagged Vue Vet releases alongside `vue_vet_core`
and `vue_vet_reactivity`.

## Install

```toml
[dependencies]
vue_vet_plugins = "0.1"
vue_vet_reactivity = "0.1"
vue_vet_core = "0.1"
```

## Auto-load (CLI / session / project)

The Vue Vet analysis boundary installs the default catalog automatically:

- `vue_vet_oxc` single-file analysis
- `vue_vet_project` project graph
- `vue_vet_session` (CLI / LSP / MCP)

Library consumers of `vue_vet_reactivity` alone get an **empty** catalog
unless they pass plugins:

```rust
use vue_vet_plugins::{default_named_api_bags, default_trace_config};
use vue_vet_reactivity::trace_reactivity_with_config;

let config = default_trace_config();
let graph = trace_reactivity_with_config(&semantic, source, 0, kind, &config);
// or: TraceConfig { named_api_bags: default_named_api_bags() }
```

## Built-in plugins

| Plugin id | Surface |
| --- | --- |
| `nuxt-data` | `useAsyncData`, `useLazyAsyncData`, `useFetch`, `useLazyFetch` |
| `vue-i18n` | `useI18n` field seeds + ambient-on-call `t`/`d`/`n`/`rt`/`te` |

```rust
use vue_vet_plugins::{NuxtDataPlugin, VueI18nPlugin, default_plugins};
use vue_vet_reactivity::{TracerPlugin, flatten_named_api_bags};

let bags = flatten_named_api_bags(default_plugins());
// or register a subset: flatten_named_api_bags(&[&VueI18nPlugin])
```

## License

MIT
