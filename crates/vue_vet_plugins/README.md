# vue_vet_plugins

Compile-time **ecosystem plugins** for [Vue Vet](https://github.com/alexzhang1030/vue-vet)
reactivity tracing.

Nuxt data helpers (`useAsyncData`, `useFetch`, …) and vue-i18n (`useI18n` /
translator ambient-on-call) API-bag contracts live **here** — not in
`vue_vet_reactivity`. The tracer engine only consumes a [`NamedApiBag`] catalog
(`TraceConfig` / `TraceModulesOptions`).

This is still **Rust static data**, not a dynamic JS plugin host or AST
Traverse. New ecosystem surfaces add a plugin module + row; the engine seed/read
paths stay generic.

## Status

Early `0.x`. Published on [crates.io](https://crates.io/crates/vue_vet_plugins)
with tagged Vue Vet releases, after `vue_vet_core` and `vue_vet_reactivity`
(dependency order).

## Install

```toml
[dependencies]
vue_vet_plugins = "0.1"
vue_vet_reactivity = "0.1"
vue_vet_core = "0.1"
```

You also need a pinned Oxc semantic stack when calling the tracer yourself (see
`vue_vet_reactivity`).

## Auto-load (product boundary)

The Vue Vet analysis boundary installs the default catalog automatically so
CLI / LSP / MCP scans model Nuxt and vue-i18n without manual wiring:

| Surface | How plugins load |
| --- | --- |
| `vue_vet_oxc` (single-file / SFC script) | `default_trace_config()` |
| `vue_vet_project` graph | `ensure_default_plugins` on empty catalogs; `build_project_graph` uses `default_trace_modules_options()` |
| `vue_vet_session` (CLI / LSP / MCP) | `default_trace_modules_options()` then worker overrides |

Library consumers of **`vue_vet_reactivity` alone** get an **empty** catalog
(pure Vue primitives). Depend on this crate and pass a config:

```rust
use vue_vet_plugins::default_trace_config;
use vue_vet_reactivity::trace_reactivity_with_config;

let config = default_trace_config();
let graph = trace_reactivity_with_config(&semantic, source, 0, kind, &config);
```

Multi-module:

```rust
use vue_vet_plugins::default_trace_modules_options;
use vue_vet_reactivity::trace_modules_with_options;

let options = default_trace_modules_options();
let graphs = trace_modules_with_options(&modules, &links, options)?;
```

`ensure_default_plugins(options)` fills an empty `named_api_bags` vec and leaves
a non-empty custom catalog unchanged.

## Built-in plugins

| Plugin id | Type | Surface |
| --- | --- | --- |
| `nuxt-data` | `NuxtDataPlugin` | `useAsyncData`, `useLazyAsyncData`, `useFetch`, `useLazyFetch` — destructure `data` / `pending` / `error` / `status` as Ref |
| `vue-i18n` | `VueI18nPlugin` | `useI18n` — field seeds (`locale`, `messages`, …); ambient-on-call `t` / `d` / `n` / `rt` / `te` (vue-i18n `wrapWithDeps`) |

```rust
use vue_vet_plugins::{NuxtDataPlugin, VueI18nPlugin, default_plugins};
use vue_vet_reactivity::flatten_named_api_bags;

// Full default set
let bags = flatten_named_api_bags(default_plugins());

// Subset (e.g. i18n only)
let bags = flatten_named_api_bags(&[&VueI18nPlugin]);
```

### Ambient-on-call (vue-i18n)

Calling a registered translator injects ambient reactive reads:

1. Prefer **co-destructured** ambient fields (`const { locale, t } = useI18n()` →
   track `locale.value`).
2. If only translators were taken (`const { t } = useI18n()`), seed a site bag
   `{callee}@{offset}` and attribute `locale` / `fallbackLocale` / `messages`
   properties so absence rules (e.g. `no-computed-without-dependency`) stay
   quiet when Vue would re-run on locale change.

## Public API summary

| Item | Role |
| --- | --- |
| `default_plugins` | `&'static [&dyn TracerPlugin]` |
| `default_named_api_bags` | Flattened `&'static [NamedApiBag]` |
| `default_trace_config` | Single-file `TraceConfig` with defaults |
| `default_trace_modules_options` | Multi-module options with defaults |
| `ensure_default_plugins` | Fill empty catalog on `TraceModulesOptions` |
| `NuxtDataPlugin` / `VueI18nPlugin` | Individual `TracerPlugin` impls |

Engine types (`NamedApiBag`, `TracerPlugin`, `TraceConfig`,
`flatten_named_api_bags`) live in **`vue_vet_reactivity`**.

## Adding a plugin (contributors)

1. Add `src/<ecosystem>.rs` with a `TracerPlugin` impl and `NamedApiBag` rows.
2. Register in `default_plugins()` and the static list inside
   `default_named_api_bags()` (keep **callee-sorted** order).
3. Unit-test the catalog; extend tracer fixtures if seed/read behavior changes.
4. Bump `REACTIVITY_GRAPH_VERSION` when the default catalog changes edge sets
   for product scans.
5. Update this README + PCR (`reactivity-tracer.md`, `architecture.md`).

Do **not** put ecosystem callee names back into `vue_vet_reactivity` seed loops.

## Publish

Tagged releases (`.github/workflows/release.yml`) publish in order:

1. `vue_vet_core`
2. wait for crates.io index
3. `vue_vet_reactivity`
4. wait for crates.io index
5. **`vue_vet_plugins`**

Dry-run publishes all three without waiting.

## Related docs

- [vue_vet_reactivity](../vue_vet_reactivity/README.md) — engine API
- [architecture PCR](../../.agents/docs/architecture.md) — tracer plugins section
- [reactivity tracer PCR](../../.agents/docs/reactivity-tracer.md) — graph contract
- [install / release](../../docs/install.md) — crates.io secrets and order

## License

MIT
