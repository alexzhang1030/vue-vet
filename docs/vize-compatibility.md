# Vize compatibility baseline

Vue Vet currently pins `vize_atelier_sfc` and `vize_atelier_core` to `0.291.0`.
This release requires Rust 1.95 or newer; Vue Vet pins Rust 1.97.0 for the
repository baseline.

## API assumptions

- `parse_sfc` returns an `SfcDescriptor` or an `SfcError`.
- `SfcError` exposes a message but does not implement `Display`.
- SFC block locations are byte offsets into the original source.
- `vize_atelier_core::parse` parses extracted template content and returns a
  root AST plus recoverable or fatal parser errors.
- Template node offsets are byte offsets relative to the extracted template.
- Element properties expose directives as `PropNode::Directive`; the normalized
  directive name for `v-html` is `html`.
- Vize types stay inside `vue-vet-vize`; downstream crates consume Vue Vet
  diagnostics and facts.

## Initial performance baseline

Measured on 2026-07-16 on x86_64 Linux with the registry cache warm:

- workspace build with an empty target directory: 36.622 seconds
- warm CLI scan of one fixture: 0.147 seconds

These are development-environment observations, not release budgets. Preserve
the commands and environment when comparing future Vize or toolchain upgrades.

## Upgrade procedure

1. Keep the new Vize version exact-pinned while evaluating it.
2. Confirm its MSRV and Oxc dependency versions against `rust-toolchain.toml`.
3. Run `just roll-rust` and `just smoke` on Linux, macOS, and Windows.
4. Run the parser and rule golden fixtures, including malformed, Unicode, CRLF,
   comment, and safe-pattern cases.
5. Review every diagnostic snapshot change for rule identity, message, help,
   severity, and exact byte/line/column spans.
6. Record API or behavioral differences here and in the relevant PCR record.
7. Merge the upgrade only with a committed lockfile and green CI evidence.

Do not loosen the dependency range or replace snapshots solely to unblock an
upgrade. A changed snapshot is evidence to review, not proof of correctness.
