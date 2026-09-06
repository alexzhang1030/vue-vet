# Vapor migration research oracles

Research-only Node harnesses for the Vue Vapor component-migration assessment.
Production semantic analysis stays in Rust with Vize and Oxc. JavaScript here
is a development oracle: it compiles real SFCs with the pinned Vue 3.6
compiler and mounts the compiler output in jsdom.

See the public assessment: [`docs/research/vapor-migration.md`](../../docs/research/vapor-migration.md).

## Audited identity

Exact audited tuple:

| Package | Pin |
| --- | --- |
| `vue` / `@vue/compiler-sfc` / `@vue/compiler-vapor` / `@vue/runtime-vapor` | `3.6.0-rc.7` |
| `vuejs/core` | [`4b2f1914e8a6da7218955593b8bc2ba5db2c6dce`](https://github.com/vuejs/core/tree/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce) (`v3.6.0-rc.7`) |
| `@vitejs/plugin-vue` eligibility excerpt | `6.0.8` / [`d8ff7d0e8f557a7c1975c07b30e232c69bdbbc03`](https://github.com/vitejs/vite-plugin-vue/tree/d8ff7d0e8f557a7c1975c07b30e232c69bdbbc03) |
| Harness | `jsdom@26.1.0`, `es-module-lexer@1.7.0` |

A later Vue, compiler, or plugin-vue release is a new identity and needs its
own re-audit.

## Observed scope

**Compiler matrix.** 16 SFC fixtures × 4 client modes + 2 SSR rows = **66**
descriptive rows. Focused assertions cover opt-in / plugin-force controls,
template-only composed-component facts, `v-memo` / `v-once` codegen, and SSR
script shape. Full codegen is written under `output/compile/generated/`
(gitignored) for inspection.

Modes:

- `default-inline` — plugin `features.vapor=false`, `compileScript` inline + prod
- `plugin-force-inline` — plugin `features.vapor=true` eligibility from the 6.0.8 excerpt
- `naive-compileScript-vapor-inline` — compiler-sfc API `vapor: true` even when plugin force is ineligible (labeled compiler-API)
- `default-split` — separate `compileTemplate`
- `ssr-inline` — two setup fixtures through `compileTemplate` SSR

The harness invokes compiler APIs. Plugin force eligibility uses the
excerpted `canForceVaporMode` / `isVaporMode` logic. Template-only
`{ __vapor: true }` is recorded as plugin-pipeline emulation.

**Runtime differential.** Five `<script setup>` fixture pairs compiled twice
(`vapor: false` → `createApp`, `vapor: true` → `createVaporApp`) from actual
SFC compiler output. Imports are rewritten with `es-module-lexer` to a
portable relative specifier for the inlined vapor browser build. Steps cover
initial DOM, clicks, `nextTick`, and unmount. **38** checkpoints: each step or
teardown in each mode is one checkpoint.

Admitted envelope: `ref`, click without modifiers, text interpolation, `v-if` /
`v-else`, keyed `v-for` on a `ref` array with `push` / `shift`, `v-once`,
`v-memo` with a stable key. Child components, slots, `v-model`, provide/inject,
transitions, teleport, suspense, SSR hydration, and `vaporInteropPlugin` stay
outside this envelope.

The runner asserts loaded `Vue.version`, installed `@vue/compiler-sfc`
`package.version`, and the inlined-build header against the audited tuple, plus
`Comp.__vapor` on the mounted component. Default runtime channel is the
production inlined build `vue/dist/vue.runtime-with-vapor.esm-browser.prod.js`
when that file exists. `just vapor-migration-research` also runs the development
inlined build `vue/dist/vue.runtime-with-vapor.esm-browser.js` and reports both
build IDs (file, version header, bytes, sha256).

## Commands

From the repository root (Node required; no Rust rebuild):

```sh
just vapor-migration-research
```

Equivalent:

```sh
cd research/vapor-migration
npm ci
npm run validate
```

Individual entry points:

| Script | What it proves |
| --- | --- |
| `npm run compile` | 66-row matrix plus focused assertions |
| `npm run runtime:prod` | 38 checkpoints on the production browser build |
| `npm run runtime:dev` | same fixtures on the development browser build |
| `npm run negative-control` | parent `spawnSync(process.execPath, …)` of `--invalid-sfc-child`; child uses the normal run+exit path and must exit 1 |
| `npm run check-surface` | public prose and path portability |

Generated compiler and runtime outputs stay under `output/` and are gitignored.
Committed files are the harness, lockfile, compact expectations, and public docs.
