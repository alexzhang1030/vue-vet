# Project graph

Vue Vet's versioned project graph connects Vue SFCs and JavaScript/TypeScript
modules without exposing Vize or Oxc AST objects. Run `vue-vet --print-graph`
to inspect deterministic JSON nodes, edges, evidence spans, diagnostics, and
invalidation inputs.

## Resolution

The first resolver supports explicit file paths, `.vue`/JS/TS extensions,
directory index files, relative imports, `@/` mapped to `src/`, and `~/` mapped
to the project root. Package imports and Nuxt `#imports` remain visible as
external nodes. Other `#` aliases and missing project files produce
`vue-vet/project/unresolved-import` rather than disappearing silently.

## Nuxt conventions

Convention version 1 recognizes files under `components`, `composables`,
`pages`, `layouts`, `plugins`, `middleware`, and `stores`. Component tags and
composable calls create auto-import edges. Explicit imports shadow convention
matches.

## Initial cross-file rules

- `vue-vet/project/unresolved-import` reports missing or unsupported project
  references at the import span.
- `vue-vet/project/unused-component` reports files under a component directory
  that have no import or template usage edge.

The resolver intentionally does not execute Nuxt, Vite, or TypeScript config and
does not promise full Node/bundler resolution. Future resolver inputs must be
added to the graph's versioned invalidation set.
