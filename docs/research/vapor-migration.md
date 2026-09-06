# Vue Vapor component-migration assessment

**Status:** research. This work ships **0** Vue Vet rule IDs. The product
reactivity oracle remains `vue@3.5.40`. This note records an audited
compiler/runtime identity and a proposed assessment shape. Shipped CLI modes,
rules, and product dependencies stay unchanged.

Rerunnable oracles: [`research/vapor-migration`](../../research/vapor-migration/README.md)
(`just vapor-migration-research`).

## Recommendation

Keep a candidate **`vapor-migration` assessment group** off the correctness
score. Opt-in, completeness, unknown reasons, and aggregate verdict are
**assessment fields**.

| Outcome | Meaning |
| --- | --- |
| **compiler-candidate** | Static compilation facts for this exact toolchain tuple support a Vapor compile path. |
| **blocked** | A construct is proven unsupported, or observably changed versus VDOM, under the audited default compiler. |
| **needs-verification** | App, plugin, runtime, dependency, or hydration behavior is still open. Legal interop that was not executed stays here. |
| **unsupported** | The resolved toolchain identity is outside the audited tuple and is not the known 3.5 compiler-sfc gap below. |

A **ready** result is reserved for a closed capability envelope: this exact
tuple, source opt-in or a planned migration target, supported SFC shape, known
script and template surfaces, resolved component boundaries and app mode, and
complete coverage for every relevant fact.

## Audited toolchain tuple

Each later Vue, compiler, or plugin-vue release is a new identity and stays
gated on a re-audit of that identity.

| Item | Exact pin |
| --- | --- |
| `vuejs/core` | tag `v3.6.0-rc.7`, commit [`4b2f1914e8a6da7218955593b8bc2ba5db2c6dce`](https://github.com/vuejs/core/tree/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce) |
| `vue` npm | **3.6.0-rc.7** |
| `@vue/compiler-sfc` / `@vue/compiler-vapor` / `@vue/runtime-vapor` | **3.6.0-rc.7** |
| `@vue/compiler-dom` / `compiler-ssr` / `server-renderer` | **3.6.0-rc.7** |
| `@vitejs/plugin-vue` | **6.0.8**, git [`d8ff7d0e8f557a7c1975c07b30e232c69bdbbc03`](https://github.com/vitejs/vite-plugin-vue/tree/d8ff7d0e8f557a7c1975c07b30e232c69bdbbc03) |
| Default jsdom runtime | `vue/dist/vue.runtime-with-vapor.esm-browser.prod.js` (inlined vapor). The Node `vue` export condition omits vapor APIs. |
| Vue Vet Vize (product, unchanged) | `vize_croquis` / `vize_atelier_core` **0.387.0** |
| Vue Vet oracle (unchanged) | `vue@3.5.40` |

Known **3.5** fact: published `@vue/compiler-sfc@3.5.42` has no Vapor compiler
(tarball member names and JS/JSON/DTS contents contain no `vapor` string). That
identity is **blocked** for this assessment. Other unmatched versions stay
**unsupported** or **needs-verification** until they are audited.

## Source contracts at this pin

Primary sources:

- Per-file `vapor` on `<script>` or `<template>` sets `descriptor.vapor`. [`parse.ts#L167`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/compiler-sfc/src/parse.ts#L167), [`#L178`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/compiler-sfc/src/parse.ts#L178), [`#L203`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/compiler-sfc/src/parse.ts#L203).
- `<script vapor>` is setup (`isSetup = setup \|\| vapor`). [`parse.ts#L204`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/compiler-sfc/src/parse.ts#L204).
- Ordinary-script-only SFCs go through `processNormalScript`. [`compileScript.ts#L204-L216`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/compiler-sfc/src/compileScript.ts#L204-L216). Passing `vapor: true` on that path leaves `__vapor` unset.
- Dual-script: the ordinary default export merges into the setup/vapor object. [`compileScript.ts#L449`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/compiler-sfc/src/compileScript.ts#L449), [`#L1122`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/compiler-sfc/src/compileScript.ts#L1122).
- Setup AST rejects runtime `export` / `export default` / `export *`. [`compileScript.ts#L722-L732`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/compiler-sfc/src/compileScript.ts#L722-L732). Attaching `vapor` to an ordinary script changes that block to setup and then hits this rule.
- plugin-vue 6.0.8 [`vapor.ts`](https://github.com/vitejs/vite-plugin-vue/blob/d8ff7d0e8f557a7c1975c07b30e232c69bdbbc03/packages/plugin-vue/src/utils/vapor.ts): `descriptor.vapor` selects Vapor immediately; else `features.vapor` uses `canForceVaporMode` (`.vue` + `scriptSetup` or template-only: eligible; `.vue` + ordinary script only: ineligible).
- Default vapor compiler registers `once` and omits `memo`. `memo` is a built-in, so it also skips the custom-directive IR path. [`compile.ts#L79-L102`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/compiler-vapor/src/compile.ts#L79-L102), [`transformElement.ts#L1215-L1235`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/compiler-vapor/src/transforms/transformElement.ts#L1215-L1235), [`general.ts#L106-L109`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/shared/src/general.ts#L106-L109).
- `v-once` sets `inVOnce`; effects register as one-shot operations. [`vOnce.ts#L4-L11`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/compiler-vapor/src/transforms/vOnce.ts#L4-L11).
- `<Suspense>` compiles to the VDOM `Suspense` helper and needs `vaporInteropPlugin` / `appContext.vdom`. Native `process()` is a stub. [`utils.ts#L165-L167`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/compiler-vapor/src/utils.ts#L165-L167), [`component.ts#L271-L276`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/runtime-vapor/src/component.ts#L271-L276), [`vdomInterop.ts#L2585-L2597`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/runtime-vapor/src/vdomInterop.ts#L2585-L2597).
- Client vapor + SSR uses `ssrRender` from `vue/server-renderer`. [`compileTemplate.ts#L212-L216`](https://github.com/vuejs/core/blob/4b2f1914e8a6da7218955593b8bc2ba5db2c6dce/packages/compiler-sfc/src/compileTemplate.ts#L212-L216).

## Observed matrix (compact)

Compiler: **66** rows (16 fixtures × 4 client modes + 2 SSR). Runtime: **5**
fixture pairs, **38** step/teardown checkpoints, `v-memo` divergence and
`v-once` equality under the admitted envelope. Details and codegen live with
the oracles; this table is the assessment mapping.

| Construct | Verdict at this pin |
| --- | --- |
| Toolchain is `@vue/compiler-sfc@3.5.42` (no Vapor compiler) | **blocked** |
| Other identity ≠ this tuple | **unsupported** / **needs-verification** |
| plugin `features.vapor` on `.vue` with only ordinary `<script>` | **blocked** for plugin force conversion |
| Template-only ± force / `vapor` attr | **compiler-candidate** for the plugin pipeline |
| `<template vapor>` + Options script | **needs-verification** (assembly of render + `__vapor`) |
| Dual-script + setup `vapor` | **compiler-candidate** for setup+template; inherited Options **needs-verification** |
| `<script vapor>` + `export default` | A reason within the SFC setup export contract |
| `v-if` / keyed `v-for` / click / interpolation in setup | **compiler-candidate**, runtime-verified in the envelope |
| `v-once` | runtime-verified for the admitted fixture |
| `v-memo` under the default compiler | **blocked** for memo-contract equivalence |
| `v-model` / slots / child SFCs / custom dirs | **needs-verification** |
| `<Suspense>` / Teleport-as-VDOM-primitive | **needs-verification** (interop prerequisite) |
| SSR / hydration | **needs-verification** (SSR codegen observed; hydrate unexecuted) |
| Router, Pinia, Nuxt, JSX | **needs-verification** |

Runtime closure that remains **unknown**: child components, slots, `v-model`,
event modifiers, provide/inject, KeepAlive / Transition / Teleport, Suspense
with `vaporInteropPlugin` installed, SSR hydration, custom
`directiveTransforms`, and apps that resolve the Node `vue` export condition.

## Five proposed checks

One `vapor-migration` group. Publish opt-in, completeness, unknown reasons, and
aggregate as fields.

1. **`toolchain-tuple`** — resolved Vue / compiler-sfc / compiler-vapor /
   runtime-vapor / plugin-vue / mode identity versus this audited tuple.
   Unmatched identities are **unsupported** or **needs-verification**, except
   the known 3.5 compiler-sfc gap which is **blocked**.
2. **`sfc-compile-contract`** — one check with reasons: plugin-force-ineligible
   ordinary-script `.vue`; `<script vapor>` flipping a block into setup (then
   the general export rule); dual-script inherited Options still open;
   template-vapor + ordinary script hybrid.
3. **`memo-contract-dropped`** — default vapor compiler ignores built-in
   `v-memo`. A fail-on-behavior-change policy treats this as a blocker. A
   custom compiler `directiveTransforms.memo` is its own compatibility case.
4. **`interop-required`** — resolved non-`__vapor` child or built-in
   Suspense/Teleport-as-VDOM-primitive without proven `vaporInteropPlugin` /
   `appContext.vdom`. Suspense is a reason inside this check.
5. **`ssr-hydration-unverified`** — the project uses SSR/hydration, or that
   mode is unresolved.

Illustrative assessment record:

```text
src/List.vue
  opt_in: script_setup_vapor_attr
  completeness: incomplete (child components unknown; no SSR facts)
  unknown: dependency closure, app interop plugin
  checks:
    toolchain-tuple: match 3.6.0-rc.7 / plugin-vue 6.0.8
    sfc-compile-contract: compiler-candidate
    memo-contract-dropped: blocked  (v-memo="[memoKey]"; VDOM stays 0, Vapor updates)
    interop-required: not applicable
  aggregate: blocked
```

Future validation: close the runtime envelope above, execute Suspense with the
interop plugin, hydrate an SSR vapor tree, and re-audit each new toolchain
tuple before expanding the allowlist. Quiet assessment output is still an
incomplete result when completeness is false.
