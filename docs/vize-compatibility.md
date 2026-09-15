# Vize compatibility baseline

Vue Vet exact-pins `vize_croquis`, `vize_atelier_core`, and `vize_carton` to
one version (currently `0.387.0`; machine-checked in
[`fixtures/quality/compat-matrix.json`](../fixtures/quality/compat-matrix.json)
via `just compat-matrix`). Oxc stays on the family that Vize's lock pins
(`0.142.0` today); do not jump Oxc ahead of Vize, or two Oxc graphs ship. The
repository Rust toolchain is pinned in `rust-toolchain.toml` and must satisfy
Vize's MSRV.

Do not depend on `vize_atelier_sfc`. That crate is the full SFC compiler and
still default-enables LightningCSS plus DOM / SSR / Vapor. The accepted
parse-only path is `vize_croquis::sfc`
([ubugeeei-prod/vize#4746](https://github.com/ubugeeei-prod/vize/pull/4746),
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
  content and returns a root AST plus recoverable or fatal parser errors. The
  allocator is passed by reference; there is no `as_bump()`.
- Template node locations are `SourceLocation { span: Span { start, end } }`.
  There is no copied `loc.source` text; compound expression text is
  reconstructed from children. Offsets are template-relative bytes.
- Element `tag`, attribute `name`, and directive `name` / `raw_name` are
  `&str` slices of the template (calling `.as_str()` hits unstable
  `str::as_str`).
- Element properties expose directives as `PropNode::Directive`; the
  normalized directive name for `v-html` is `html`.
- `SfcDescriptor.css_vars` lists CSS `v-bind` expressions without spans. Vue
  Vet keeps its own span-aware under-approx scanner for the join.
- Oxc `SemanticBuilder` does **not** populate `Semantic::nodes` by default;
  call `.with_build_nodes(true)` wherever facts walk `semantic.nodes()`.
  Without it, imports / calls / scopes are empty and rules go quiet.
- Vize types stay inside `vue_vet_vize`; downstream crates consume Vue Vet
  diagnostics and facts.

## Lockfile constraints inherited from the pin

- Vize exact-pins `serde`, `serde_json`, and `compact_str`; CodSpeed exact-pins
  `divan-macros`. `cargo update` cannot float those and they must not be
  `[patch]`ed to jump the pin.
- `oxc_codegen` and `oxc_transformer` stay in the lock via
  `vize_atelier_core`; do not `[patch]` them out.
- `oxc_resolver` stays `11.21.0` until a dedicated resolver upgrade reviews
  resolve-quiet behavior (see the `Cargo.toml` comment).
- Available Vize / Oxc surfaces deliberately **not** adopted (ownership and
  under-approx stay with Vue Vet): `parse_document`, script `BindingMetadata`,
  `extract_css_vars` (no spans), `oxc_ecmascript::MayHaveSideEffects`,
  `oxc_semantic` `cfg` / `oxc_cfg`, croquis `effect_graph`, croquis identifier
  walkers. Template free-identifier reads stay Oxc-owned.

Per-upgrade adapter notes (what renamed, what moved in the lock) live in the
upgrade PRs and `git log` for this file.

## Upgrade procedure

1. Keep the new Vize version exact-pinned while evaluating it.
2. Confirm its MSRV and Oxc dependency versions against `rust-toolchain.toml`.
3. Update `fixtures/quality/compat-matrix.json` pins with the new versions.
4. Run `just roll-rust`, `just compat-matrix`, `just quality-gates`, and
   `just smoke` on Linux, macOS, and Windows.
5. Run the parser and rule golden fixtures, including malformed, Unicode,
   CRLF, comment, and safe-pattern cases.
6. Review every diagnostic snapshot change for rule identity, message, help,
   severity, and exact byte/line/column spans.
7. Record API or behavioral differences here and in the relevant PCR record.
8. Merge the upgrade only with a committed lockfile and green CI evidence.

Do not loosen the dependency range or replace snapshots solely to unblock an
upgrade. A changed snapshot is evidence to review, not proof of correctness.
