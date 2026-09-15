# Technology stack decisions

## Rust is the implementation language

The scanner, semantic product layer, CLI, cache, graph, reporters, and fix
engine stay in Rust. The npm package `@vue-vet/cli` is a thin
installer/launcher under `npm/vue-vet`: it selects an `@vue-vet/{os}-{arch}`
optional dependency, spawns the native `vue-vet` binary, and forwards
arguments, signals, output, and exit codes. It must never grow analysis logic.
See [install docs](../../docs/install.md).

Continuous previews ([pkg.pr.new](https://github.com/stackblitz-labs/pkg.pr.new))
and the existing-artifact consumer check are described in
[install docs](../../docs/install.md) (`Continuous preview packages`,
`Existing-artifact consumer check`); neither runs Cargo or mutates
`npm/vue-vet/package.json`.

Surfaces inside the CLI crate are presentation only: `ratatui` (crossterm
backend only) for `--reactivity-tui` presents `ReactivityModuleStats` already
produced by the scan; `--progress auto` writes a live stderr status line when
stderr is a TTY and `CI` is unset (compact phase log under `always` when
piped; `never` silences). The thin diagnostics LSP (`vue-vet --lsp`) uses
`tower-lsp` + `tokio` in `vue_vet_lsp` as a protocol adapter only; the thin
MCP adapter (`vue-vet --mcp`, `vue_vet_mcp`) is a minimal stdio JSON-RPC tools
subset with no heavy MCP SDK. Neither re-implements analysis or silently
applies fixes.

The workspace tracks the latest stable Rust release and edition, following
Rolldown's toolchain baseline; `rust-toolchain.toml` pins the exact compiler
and all crates inherit the workspace `rust-version`. Shipped binaries use
`profile.release` (`opt-level = 2`, `lto = "fat"`, `panic = "abort"`,
`strip = "symbols"`) plus per-package size overrides documented inline in
`Cargo.toml`: `"z"` on protocol/UI runtime packages and the measured rule
crates, `"s"` on the core analysis crates, and `"z"` / `"s"` on the LSP/TUI-only
runtime closure that `cargo tree --invert` proved unreachable from the CLI
scan path. Parsers, `vue_vet_oxc`, the resolver, regex, serde, and
`vue_vet_reporters` stay on the profile default; proc-macros are not
overridden. Acceptance gates for overrides and the native-size budget are in
[quality baselines](../../docs/quality-baselines.md). The remaining mass is
one process (Oxc + Vize + product IR + LSP/MCP/TUI); unused LightningCSS and
ratatui termwiz backends stay out of the lock. Do not add UPX or a second
analysis binary.

Rolldown's lint policy is a floor: Vue Vet additionally denies the Clippy
`all`, `cargo`, `pedantic`, and `nursery` groups, forbids unsafe Rust, and
denies panic-prone conveniences such as unchecked indexing, string slicing,
`unwrap`, and `expect`. The group-level duplicate-version exception covers the
reviewed Vize and atomic-writer dependency graphs; each additional duplicate
requires explicit rationale. `just` is the task runner and canonical interface
for local and CI validation; `prek` manages Git hooks from
`.pre-commit-config.yaml` without a Python runtime requirement.

## Vize owns Vue semantics

Vize parses Vue SFC structure and is the source of truth for template nodes,
directives, bindings, and Vue-specific semantics. Vue Vet adapts Vize output
into its own stable facts rather than exposing Vize AST types. Vize is
experimental and releases rapidly, so its version is pinned exactly and
upgrades require fixtures, snapshots, and compatibility evidence
([procedure](../../docs/vize-compatibility.md)).

SFC parse is `vize_croquis::sfc` (ubugeeei-prod/vize#4746); that facade does
not pull LightningCSS, parcel_selectors, or the DOM / SSR / Vapor compile
backends. Template AST stays on `vize_atelier_core`. Do not depend on
`vize_atelier_sfc` and do not revive the rejected `compile` feature
(ubugeeei-prod/vize#4566). The adapter uses `vize_carton::is_native_tag` for
HTML/SVG/MathML tag classification; its direct dependency keeps the same exact
version as the other Vize crates, checked by `just compat-matrix`.

## Oxc owns JavaScript and TypeScript semantics

Oxc parses script blocks and provides scopes, symbols, references, imports,
and expression facts. Vue Vet is not a TypeScript type checker; rules requiring
unavailable type certainty stay out of the default preset or explicitly
experimental. The adapter is pinned to the Oxc family present in Vize's locked
dependency graph (currently 0.142 with Vize 0.387); do not move Oxc until Vize
moves. It parses each extracted script surface using its declared `lang`,
builds semantics with syntax checking, and maps every fact span back through
the SFC block offset. Direct Oxc types remain private to `vue_vet_oxc`.

## oxc_resolver owns bundler module resolution

Cross-file import edges in `vue_vet_project` resolve through exact-pinned
`oxc_resolver` (the Rolldown / enhanced-resolve stack), not a parallel path
engine. Vue Vet owns classification of resolve results into project edges,
external nodes, and `unresolved-import` diagnostics. The resolver does not
execute Vite or Nuxt config files; tsconfig paths and Vite default aliases are
the configuration surface. Pin note: stay on `11.21.0` until a dedicated
resolver upgrade reviews resolve-quiet behavior (see the `Cargo.toml`
comment; the old `dashmap` conflict is gone, which is not permission to jump
the resolver in an unrelated change).

## Vize and Oxc are the complete analysis stack

Vue Vet does not embed a parallel structural-pattern engine. Template and SFC
diagnostics extend Vize-backed facts; JavaScript and TypeScript diagnostics
extend Oxc-backed facts. This keeps source offsets, confidence, suppression,
scoring, caching, baselines, and fixes on one semantic path. Teams that need
repository-specific structural conventions can run standalone search tools in
CI without making their grammar part of Vue Vet's stable product surface.

## Vue Vapor migration research

Vue Vapor component-migration work is research-only at this pin. The public
record is [docs/research/vapor-migration.md](../../docs/research/vapor-migration.md);
rerunnable Node oracles live under `research/vapor-migration` and stay out of
the Rust engine. The audited identity is the exact Vue **3.6.0-rc.7** package
tuple plus `@vitejs/plugin-vue@6.0.8` (pinned in
`research/vapor-migration/package.json`); later Vue 3.6 releases stay gated on
a re-audit. The assessment shipped as the opt-in `vapor-migration` group;
analysis remains on `vize_croquis` / `vize_atelier_core` without
`vize_atelier_vapor`, and the product Vue 3.5.40 reactivity oracle is
unchanged.

## atomic-write-file owns the single-file commit

The safe-fix executor uses exact-pinned `atomic-write-file` for the final
same-directory replacement on Unix, Windows, and WASI. Vue Vet still owns edit
classification, planning, scan-scope containment, byte/UTF-8 validation, and
post-fix rescanning; the dependency receives only a completely rendered file
body and owns the filesystem-specific atomic commit. It does not provide or
imply a multi-file transaction.

## VueUse pin for cancelled-filter settlement

`no-cancelled-filter-promise-demand` proves `useDebounceFn` settlement against
the `@vueuse/core` / `@vueuse/shared` version pinned in the oracle package
(`crates/vue_vet_reactivity/oracle/package.json`): `createFilterWrapper`
returns a fresh Promise per call and `debounceFilter` sets
`lastRejector = options.rejectOnCancel ? reject : resolve`, so a same-turn
supersede fulfills the earlier promise with `undefined`. Re-run
`just oracle-filter-settlement` on a VueUse bump and re-check `maxWait`,
trailing/leading throttle, and timer-vs-await ordering before changing the
rule.

## Vue Vet owns the product contracts

Diagnostics, source spans, rule metadata, confidence, configuration,
suppression, fingerprints, scoring, project facts, cache formats, baselines,
reporters, and edits are Vue Vet-owned types. This boundary prevents dependency
churn from becoming a public API break.

See [the analysis-stack ADR](../../docs/adr/0001-analysis-stack.md) for the
decision and [architecture](./architecture.md) for the data flow.
