# vue_vet_vize

**Vize-powered Vue SFC analysis** for [Vue Vet](https://github.com/alexzhang1030/vue-vet).

Parses `.vue` files with `vize_croquis::sfc` (never `vize_atelier_sfc`), walks
the template with `vize_atelier_core`, and extracts Vue Vet-owned `SfcFacts` /
`TemplateFacts`. Script blocks are delegated to `vue_vet_oxc`. Dual-script SFCs
expose both `script setup` and ordinary `<script>` as independent module sources
(`path` and `path#script`).

## Status

Workspace-internal (`publish = false`). Adapter only: Vize AST must not escape
this crate into rules, reporters, cache, or public contracts.

## Layout

```text
lib.rs        analyze_sfc_* + block reuse fingerprints
template.rs   template walk → TemplateFacts
style.rs      <style> v-bind(ident) expressions
span.rs       analysis-scoped line index
```

## Related docs

- [architecture PCR](../../.agents/docs/architecture.md) (adapter layouts)
- [Vize compatibility](../../docs/vize-compatibility.md)
- [gotchas PCR](../../.agents/docs/gotchas.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
