# Technology stack decisions

## Rust is the implementation language

The scanner, semantic product layer, CLI, cache, graph, reporters, and fix engine stay in Rust. The npm package `@vue-vet/cli` is a thin installer/launcher under `npm/vue-vet`: it selects an `@vue-vet/{os}-{arch}` optional dependency, spawns the native binary (`vue-vet`), and forwards arguments, signals, output, and exit codes. It must never grow analysis logic. See [install docs](../../docs/install.md).

PR / `main` continuous previews use [pkg.pr.new](https://github.com/stackblitz-labs/pkg.pr.new)
(`.github/workflows/pkg-pr-new.yml`): path-filtered triggers (no push+PR double
runs on feature branches), `Swatinem/rust-cache`, matrix-build platform packages,
copy the launcher into `dist/`, then
`npm exec pkg-pr-new publish --bin --previewVersion --no-template` from the
locked install under `npm/pkg-pr-new/`. Requires the
[pkg-pr-new GitHub App](https://github.com/apps/pkg-pr-new) on the repository.

Interactive reactivity browsing uses exact-pinned `ratatui` (crossterm backend only)
inside the CLI crate. Analysis logic stays out of the TUI; it only presents
`ReactivityModuleStats` already produced by the scan.

Local scans default to `--progress auto`: a live stderr status line when
stderr is a TTY and `CI` is unset (`TERM=dumb` and pipes stay a compact
phase log under `--progress always`). Text diagnostics are a single
finalized document after the scan. Force with `--progress always` /
silence with `never`.

The thin diagnostics LSP (`vue-vet --lsp`) uses `tower-lsp` + `tokio` in
`vue_vet_lsp` as a protocol adapter only: it maps session diagnostics (including
unsaved buffer overlays, safe quick-fix edits, and explain-scope hover) to LSP
types and must not re-implement analysis. The thin MCP adapter (`vue-vet --mcp`) lives in
`vue_vet_mcp` with a minimal stdio JSON-RPC tools subset (no heavy MCP SDK) over
the same session; it must not re-implement analysis or silently apply fixes.

The workspace tracks the latest stable Rust release and latest stable edition, following Rolldown's toolchain baseline. The repository pins the exact compiler in `rust-toolchain.toml`; all crates inherit the workspace `rust-version` and edition. Shipped CLI/npm binaries use `profile.release` (`opt-level = 2`, `lto = "fat"`, `panic = "abort"`, `strip = "symbols"`) plus `opt-level = "z"` on runtime protocol/UI packages (`tower-lsp`, `lsp-types`, `vue_vet_lsp`, `vue_vet_mcp`, `clap`, `clap_builder`, `ratatui`, `ratatui-core`, `ratatui-widgets`, `ratatui-crossterm`, `crossterm`) and on measured product crates (`vue_vet_rules`, `vue_vet_practice`, `vue_vet_rule_query` at `opt-level = "z"`; `vue_vet_core` and `vue_vet_reactivity` at `opt-level = "s"`). Parser, `vue_vet_oxc`, resolver, regex, serde, and `vue_vet_reporters` stay on the profile default. Proc-macros are not overridden. Fat LTO still inlines across crates. Absolute file bytes vary by target; each platform gate uses its own artifact. `just native-size` builds then measures via Cargo JSON; CI measures the already-built matrix artifact with `scripts/native_size.py --binary --target --budget-file` and `fixtures/quality/native-size-budget.json` (gzip-9 is a binary compression proxy, not npm/archive bytes). Maxima are ceil(candidate bytes × 1.03) from pkg.pr.new run 34039577281 (`fa2debc`); baseline sizes are run 34034720314 (`2dabaad`). Those product-crate overrides are accepted only with exact scan JSON equality plus same-tree release comparisons of `whole_project::scan_cold_mixed_1k`, `whole_project::scan_warm_mixed_1k`, and `whole_project::json_render_mixed_1k` (not `analyze_sfc` alone); CLI cold/warm process times remain a separate user-facing check. See [quality baselines](../../docs/quality-baselines.md). `profile.codspeed` restates `opt-level = 3` for those five packages so CodSpeed keeps instrumentation `opt-level = 3`. The remaining mass is still one process (Oxc + Vize + product IR + LSP/MCP/TUI). Unused LightningCSS and ratatui termwiz backends stay out of the lock. Do not add UPX or a second analysis binary. Rolldown's lint policy is a floor: Vue Vet additionally denies the Clippy `all`, `cargo`, `pedantic`, and `nursery` groups, forbids unsafe Rust, and denies panic-prone conveniences such as unchecked indexing, string slicing, `unwrap`, and `expect`. The group-level duplicate-version exception covers the reviewed Vize and atomic-writer dependency graphs; each additional duplicate still requires explicit rationale. `just` is the task runner and the canonical interface for local and CI validation. `prek` manages Git hooks from `.pre-commit-config.yaml` without adding a Python runtime requirement.

## Vize owns Vue semantics

Vize parses Vue SFC structure and is the source of truth for template nodes, directives, bindings, and Vue-specific semantics. Vue Vet adapts Vize output into its own stable facts rather than exposing Vize AST types. Vize is experimental and releases rapidly, so its version is pinned exactly and upgrades require fixtures, snapshots, and compatibility evidence.

The Vize adapter uses `vize_carton::is_native_tag` for HTML/SVG/MathML tag
classification, including collisions with lowercase script imports. Its direct
dependency keeps the same exact version as the other Vize crates; the
compatibility matrix checks that shared pin.

## Oxc owns JavaScript and TypeScript semantics

Oxc will parse script blocks and provide scopes, symbols, references, imports, and expression facts. Vue Vet is not initially a complete TypeScript type checker; rules requiring unavailable type certainty must remain out of the default preset or explicitly experimental.

The first adapter is pinned to the Oxc 0.142 family already present in Vize
0.387's locked dependency graph. It parses each extracted script surface using
its declared `lang`, builds semantics with syntax checking, and maps every fact
span back through the SFC block offset. Direct Oxc types remain private to
`vue_vet_oxc`. Do not jump Oxc to 0.147 (latest) until Vize moves.

SFC parse is `vize_croquis::sfc` (ubugeeei-prod/vize#4746). That facade does
not pull LightningCSS, parcel_selectors, or the DOM / SSR / Vapor compile
backends. Template AST stays on `vize_atelier_core`. Do not depend on
`vize_atelier_sfc` and do not revive the rejected `compile` feature
(ubugeeei-prod/vize#4566). Re-add a CSS-engine crate only if a rule needs
LightningCSS AST spans.

## Vue Vapor migration research

Vue Vapor component-migration work is research-only at this pin. The public
record is [docs/research/vapor-migration.md](../../docs/research/vapor-migration.md);
rerunnable Node oracles live under `research/vapor-migration` and stay out of
the Rust engine. The audited identity is the exact `vue` /
`@vue/compiler-sfc` / `@vue/compiler-vapor` / `@vue/runtime-vapor`
**3.6.0-rc.7** plus `@vitejs/plugin-vue@6.0.8` tuple. Later Vue 3.6 releases
stay gated on a re-audit of that identity. Published
`@vue/compiler-sfc@3.5.42` has no Vapor compiler and is a specific blocker for
that toolchain; other unmatched versions are unsupported or need verification.
Shipped CLI modes and rules stay at delta 0. Analysis remains on
`vize_croquis` / `vize_atelier_core` without `vize_atelier_vapor`. The product
Vue 3.5.40 reactivity oracle is unchanged. Opt-in, completeness, and aggregate
verdicts are assessment fields.

## oxc_resolver owns bundler module resolution

Cross-file import edges in `vue_vet_project` resolve through exact-pinned
`oxc_resolver` (the Rolldown / enhanced-resolve stack), not a parallel path
engine. Vue Vet owns classification of resolve results into project edges,
external nodes, and `unresolved-import` diagnostics. The resolver does not
execute Vite or Nuxt config files; tsconfig paths and Vite default aliases are
the configuration surface. Pin note: stay on `11.21.0` until a dedicated
resolver upgrade reviews resolve-quiet behavior. Vize 0.387 lifted
`dashmap` to `=6.2.1` (ubugeeei-prod/vize#4567), so the old 6.1.0 conflict
that blocked `oxc_resolver 11.22+` is gone; that lift is not permission to
jump the resolver in the same change.

## Vize and Oxc are the complete analysis stack

Vue Vet does not embed a parallel structural-pattern engine. Template and SFC
diagnostics extend Vize-backed facts; JavaScript and TypeScript diagnostics
extend Oxc-backed facts. This keeps source offsets, confidence, suppression,
scoring, caching, baselines, and fixes on one semantic path. Teams that need
repository-specific structural conventions can run standalone search tools in
CI without making their grammar and rule contracts part of Vue Vet's stable
product surface.

## atomic-write-file owns the single-file commit

The safe-fix executor uses exact-pinned `atomic-write-file` 0.3.1 for the final
same-directory replacement on Unix, Windows, and WASI. 0.3.1 is the crate bump
that moves `rand` to 0.10.2 and `nix` to 0.31.3 (shared with CodSpeed); the
public commit API is unchanged. Vue Vet still owns edit classification,
planning, scan-scope containment, byte/UTF-8 validation, and post-fix
rescanning; the dependency receives only a completely rendered file body and
owns the filesystem-specific atomic commit. It does not provide or imply a
multi-file transaction. Dropping unused ratatui backends (termwiz / wezterm)
removed the extra `nix` 0.29 / `sha2` 0.10 / `thiserror` 1.x copies; the
remaining `nix` 0.31.3 is shared with CodSpeed.

## Vue Vet owns the product contracts

Diagnostics, source spans, rule metadata, confidence, configuration, suppression, fingerprints, scoring, project facts, cache formats, baselines, reporters, and edits are Vue Vet-owned types. This boundary prevents dependency churn from becoming a public API break.

See [the analysis-stack ADR](../../docs/adr/0001-analysis-stack.md) for the decision and [architecture](./architecture.md) for the planned data flow.
