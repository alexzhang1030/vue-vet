# Vize compatibility baseline

Vue Vet currently pins `vize_croquis` and `vize_atelier_core` to `0.387.0`.
This release requires Rust 1.95 or newer and pins Oxc `0.142.0`; Vue Vet pins
Rust 1.98.0 for the repository baseline. Do not jump Oxc ahead of Vize.

Do not depend on `vize_atelier_sfc`. That crate is the full SFC compiler and
still default-enables LightningCSS plus DOM / SSR / Vapor. The accepted
parse-only path is `vize_croquis::sfc` ([ubugeeei-prod/vize#4746](https://github.com/ubugeeei-prod/vize/pull/4746),
closes [#4565](https://github.com/ubugeeei-prod/vize/issues/4565)). Do not
revive the rejected `compile` feature on `vize_atelier_sfc`
([#4566](https://github.com/ubugeeei-prod/vize/pull/4566)).

## API assumptions

- SFC parse is `vize_croquis::sfc::parse_sfc`. It returns an `SfcDescriptor`
  or an `SfcError`. Canonical rustdoc / `type_name` paths live under
  `vize_croquis::sfc`; do not import the same types through
  `vize_atelier_sfc` re-exports.
- `SfcError` exposes a `message` (`vize_carton::String` / `CompactString`) but
  does not implement `Display`.
- SFC block locations are byte offsets into the original source
  (`BlockLocation.start` / `end`).
- `vize_atelier_core::parse(&Allocator, template)` parses extracted template
  content and returns a root AST plus recoverable or fatal parser errors.
  The allocator is passed by reference; there is no `as_bump()`.
- Template node locations are `SourceLocation { span: Span { start, end } }`
  (Davinci P1-3). There is no copied `loc.source` text; compound expression
  text is reconstructed from children. Offsets are template-relative bytes.
- Element `tag`, attribute `name`, and directive `name` / `raw_name` are
  `&str` slices of the template (calling `.as_str()` hits unstable `str::as_str`).
- Element properties expose directives as `PropNode::Directive`; the normalized
  directive name for `v-html` is `html`.
- `SfcDescriptor.css_vars` lists CSS `v-bind` expressions without spans. Vue
  Vet keeps its own span-aware under-approx scanner for join.
- Vize types stay inside `vue_vet_vize`; downstream crates consume Vue Vet
  diagnostics and facts.

## 0.355 → 0.387 adapter notes (2026-08-25)

Reviewed against `vize_croquis` / `vize_atelier_core` 0.387.0 and Oxc 0.142.0.
Adapter call sites did not need product-contract edits:
`parse_sfc`, `SfcError.message`, `BlockLocation.start` / `end`, and
`vize_atelier_core::parse` still match the 0.355 usage. The change is the
crate boundary and the lockfile graph.

- Consume [ubugeeei-prod/vize#4746](https://github.com/ubugeeei-prod/vize/pull/4746):
  `use vize_croquis::sfc::{SfcDescriptor, SfcParseOptions, parse_sfc}`.
  First published train with that facade is `v0.384.0` (2026-08-24);
  Vue Vet pins the latest published `0.387.0`.
- DashMap: [ubugeeei-prod/vize#4567](https://github.com/ubugeeei-prod/vize/pull/4567)
  moved Vize's exact pin from `dashmap =6.1.0` to `=6.2.1`. The lock now
  records `6.2.1` from Croquis. Leftover `dashmap 5.5.3` is still from
  `tower-lsp`, not Vize.
- Dropped from the Vue Vet lock: `vize_atelier_sfc`, `vize_atelier_dom`,
  `vize_atelier_ssr`, `vize_atelier_vapor`, LightningCSS, and
  `parcel_selectors`. Those were compiler / CSS-engine crates Vue Vet never
  called. `default-features = false` on `vize_atelier_sfc` already dropped
  LightningCSS in 0.355; #4746 is what drops the remaining compile backends
  without a `compile` feature.
- Still in the lock via `vize_atelier_core` (template parse): `oxc_codegen`
  and `oxc_transformer`. Vize recorded optionalizing those on atelier-core
  as a follow-up, not part of #4746. Do not `[patch]` them out.
- `oxc_resolver` stays `11.21.0` (latest 11.24.3). The old dashmap 6.1
  conflict is gone; this pin is now a resolve-quiet review, not a Vize
  constraint. Do not jump the resolver in the same change.
- Oxc stays `0.142.0` (latest 0.147). Jumping the adapter to 0.147 while
  Vize stays on 0.142 would ship two Oxc graphs.
- Vize still exact-pins `serde =1.0.228`, `serde_json =1.0.149`, and
  `compact_str =0.9.0`. Do not `[patch]` those to float patches.
- Not adopted (available, keep under-approx / ownership): `parse_document`,
  script `BindingMetadata`, `extract_css_vars` (no spans),
  `oxc_ecmascript::MayHaveSideEffects`, `oxc_semantic` `cfg` / `oxc_cfg`,
  croquis `effect_graph`, croquis identifier walkers (vize#4762 / #4765 /
  #4786). Template free-identifier reads stay Oxc-owned.
- `CONVENTIONS_VERSION` stays 14. This upgrade does not change resolve-quiet
  rules.

## 0.291 → 0.355 adapter notes (2026-08-21)

Reviewed against `vize_atelier_sfc` / `vize_atelier_core` 0.355.0 and Oxc
0.142.0. Adapter-only mappings (no product contract change):

- Oxc `ParserReturn` / `SemanticBuilderReturn` renamed `errors` → `diagnostics`
  (`oxc_diagnostics::Diagnostics` newtype, `Deref` to `Vec<OxcDiagnostic>`).
- Oxc 0.142 `SemanticBuilder` does **not** populate `Semantic::nodes` by
  default (`with_build_nodes` is off). Vue Vet must call
  `.with_build_nodes(true)` wherever facts walk `semantic.nodes()`. Without
  it, imports/calls/scopes are empty and rules go quiet.
- `vize_croquis` still exact-pins `dashmap =6.1.0`, so `oxc_resolver` stays
  `11.21.0` (`11.22+` needs `dashmap 6.2.1`).
- Not adopted this upgrade (available, keep under-approx / ownership):
  `parse_document` (full HTML / petite-vue), script `BindingMetadata`,
  `extract_css_vars` (no spans), `oxc_ecmascript::MayHaveSideEffects`,
  `oxc_semantic` `cfg` / `oxc_cfg`, croquis `effect_graph`.
- Latest crates.io Oxc is 0.146; stay on 0.142 until Vize moves.
  Jumping our adapter to 0.146 while Vize stays on 0.142 would ship two Oxc
  graphs and break the “Vize owns SFC, Oxc family matches Vize” contract.
- Workspace-direct leftovers that *can* move are now latest:
  `atomic-write-file` 0.3.1 (rand 0.10.2 / nix 0.31.3 only), `globset`
  0.4.20, `ignore` 0.4.33, plus a full `cargo update` of unconstrained
  transitives. Already latest: Vize 0.355.0, clap 4.6.6, rayon 1.12,
  thiserror 2.0.20, ratatui 0.30.2, tokio 1.53.1, tower-lsp 0.20.0,
  anstyle 1.0.14, sha2 0.11.0.
- Cargo still reports packages behind latest. Those are **not** skipped
  leftovers — Vize / CodSpeed exact-pin them:
  Oxc `0.142.0` (latest 0.146), `oxc_resolver` `11.21.0` (dashmap
  `=6.1.0`), `serde` / `serde_core` / `serde_derive` `=1.0.228` and
  `serde_json` `=1.0.149`, `compact_str` `=0.9.0`, `divan-macros`
  `=0.1.17` (via `codspeed-divan-compat =5.0.1`). Do not `[patch]` them
  to jump the pin. LightningCSS is no longer in the lock: `vize_atelier_sfc`
  is pinned with `default-features = false`.
- Host `x86_64-unknown-linux-gnu` release binary (2026-08-21, stripped):
  12_327_536 bytes thin LTO + default Vize `native` → 10_071_744 bytes
  fat LTO + `panic = "abort"` + no LightningCSS / termwiz (~18% smaller).
  The remaining mass is Oxc + Vize parse/semantic plus one CLI/LSP/MCP/TUI
  binary. Do not UPX or split analysis into a second executable.

## Initial performance baseline

Measured on 2026-07-16 on x86_64 Linux with the registry cache warm:

- workspace build with an empty target directory: 36.622 seconds
- warm CLI scan of one fixture: 0.147 seconds

These are development-environment observations, not release budgets. Preserve
the commands and environment when comparing future Vize or toolchain upgrades.

## Upgrade procedure

1. Keep the new Vize version exact-pinned while evaluating it.
2. Confirm its MSRV and Oxc dependency versions against `rust-toolchain.toml`.
3. Update `fixtures/quality/compat-matrix.json` pins with the new versions.
4. Run `just roll-rust`, `just compat-matrix`, `just quality-gates`, and
   `just smoke` on Linux, macOS, and Windows.
5. Run the parser and rule golden fixtures, including malformed, Unicode, CRLF,
   comment, and safe-pattern cases.
6. Review every diagnostic snapshot change for rule identity, message, help,
   severity, and exact byte/line/column spans.
7. Record API or behavioral differences here and in the relevant PCR record.
8. Merge the upgrade only with a committed lockfile and green CI evidence.

Do not loosen the dependency range or replace snapshots solely to unblock an
upgrade. A changed snapshot is evidence to review, not proof of correctness.
