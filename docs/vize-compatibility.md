# Vize compatibility baseline

Vue Vet currently pins `vize_atelier_sfc` and `vize_atelier_core` to `0.355.0`.
This release requires Rust 1.95 or newer and pins Oxc `0.142.0`; Vue Vet pins
Rust 1.97.0 for the repository baseline. Do not jump Oxc ahead of Vize.

## API assumptions

- `parse_sfc` returns an `SfcDescriptor` or an `SfcError`.
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
