# Project graph

Vue Vet's versioned project graph connects Vue SFCs and JavaScript/TypeScript
modules without exposing Vize or Oxc AST objects. Run `vue-vet --print-graph`
to inspect deterministic JSON nodes, edges, evidence spans, diagnostics, and
invalidation inputs.

## Resolution

Import resolution uses [`oxc_resolver`](https://crates.io/crates/oxc_resolver)
(the same enhanced-resolve stack Rolldown uses). Defaults target Vite/Vue ESM
apps:

- Extensions: `.vue`, `.tsx`, `.ts`, `.jsx`, `.js`, `.mjs`, `.cjs`, `.json`
- Conditions: `import`, `module`, `browser`, `default`
- Main fields: `browser`, `module`, `main`
- Vite-style aliases: `@` → `<root>/src`, `~` → `<root>`
- `tsconfig` paths via Auto discovery, preferring `.nuxt/tsconfig.json` when present
- Yarn PnP when `.pnp.cjs` / `.pnp.data.json` exists

Classification after a successful resolve:

- Path inside the scanned file set → project `Import` edge (and module seed link)
- Path outside the scanned set (including `node_modules`) → `ExternalImport`
- Resolve failure → `vue-vet/project/unresolved-import` at the import span

`#imports` stays an external node (Nuxt virtual module) even when `.nuxt` is
absent. Other `#*` specifiers go through the resolver (typically via Nuxt
tsconfig paths).

Vue Vet does **not** execute `vite.config.*` / `nuxt.config.*`. Alias and path
mapping enter through tsconfig and the Vite defaults above. Resolver-affecting
inputs (root lockfiles, `package.json`, `tsconfig*.json`, `.nuxt/tsconfig.json`)
are part of the graph invalidation set and the content-addressed cache key.
Project roots are absolutized and, on Windows, normalized out of `\\?\` verbatim
form so alias joins and resolve results share one path representation.

## Nuxt conventions

Convention recognition covers files under `components`, `composables`,
`pages`, `layouts`, `plugins`, `middleware`, and `stores`. Component tags and
composable calls create auto-import edges. Explicit imports shadow convention
matches. `CONVENTIONS_VERSION` (currently 3) invalidates cached graphs when
convention or resolver semantics change.

Component auto-import names follow Nuxt defaults without executing
`nuxt.config`:

- Strip `.client` / `.server` / `.global` / `.island` from the file stem
- Prefix nested directories (`components/base/Button.vue` → `BaseButton`)
- Treat `index.vue` as the parent folder name (`components/ui/index.vue` → `Ui`)
- Match template tags with an optional `Lazy` prefix (`LazyHeroDemo` → `HeroDemo`)
- Pair `.client` / `.server` halves under one PascalCase name

When `.nuxt/components.d.ts` or `.nuxt/types/components.d.ts` exists, those
generated name→path maps enrich (and can override) the convention names so
`pathPrefix: false` and custom `components` dirs stay accurate. Those dts files
are part of the graph invalidation set.

## Component navigation (not prop dataflow)

JSON reports expose a compact `component_nav` digest (and the reactivity TUI /
VS Code host surface the same facts) built only from `ComponentUsage` and
`AutoComponent` edges: per file `uses` / `used_by` with template-tag evidence
spans. This is **structural** parent→child component reference navigation.

It does **not** model parent `:foo="bar"` → child `props.foo` reactivity edges,
runtime component trees, `keep-alive`, or dynamic `:is`. Those remain deferred
cross-file dataflow work.

## Initial cross-file rules

- `vue-vet/project/unresolved-import` reports imports that fail bundler
  resolution at the import span.
- `vue-vet/project/unused-component` reports files under a component directory
  that have no import or template usage edge (after Nuxt auto-import naming).
