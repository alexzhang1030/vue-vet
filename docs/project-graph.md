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

## Nuxt conventions

Convention recognition covers files under `components`, `composables`,
`pages`, `layouts`, `plugins`, `middleware`, and `stores`. Component tags and
composable calls create auto-import edges. Explicit imports shadow convention
matches. `CONVENTIONS_VERSION` (currently 2) invalidates cached graphs when
convention or resolver semantics change.

## Initial cross-file rules

- `vue-vet/project/unresolved-import` reports imports that fail bundler
  resolution at the import span.
- `vue-vet/project/unused-component` reports files under a component directory
  that have no import or template usage edge.
