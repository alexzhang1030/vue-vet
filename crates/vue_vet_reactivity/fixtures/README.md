# Reactivity tracer fixtures
This directory keeps corpus input separate from the Rust harness.
- `corpus/systematic`: 10 batches, 100 explicit primitive/effect/control-flow/import cases.
- `corpus/complex`: 10 control-flow batches, 100 explicit single-module cases.
- `corpus/modules`: 8 topology batches, 80 explicit multi-module cases with resolved links.
- `regressions`: source trees for symbol-identity false-positive regressions.
- `real-world`: five cross-module source trees distilled from fixed Nuxt, VueUse, Vue Router, and Pinia commits. Each directory contains a metadata-only `case.json` manifest and standalone TypeScript modules.
The corpus tests hard-assert 100 + 100 + 80 cases, unique names, and unique source/module signatures. Real-world manifests record an exact upstream repository, commit, and path, while their module entries point to source files loaded from disk. The TypeScript is a small semantic distillation rather than vendored upstream code, so provenance remains reviewable without coupling the test suite to unrelated implementation detail.
When adding a corpus case, update an existing batch and preserve the exact category count unless issue #28 is intentionally revised. Each module fixture must keep files separate and supply resolved links; concatenated source is not a valid cross-module test.

Local fixtures always assert existence of one expected `(binding, kind, guards)`
triple **and** an exhaustive `expected.reads` list
`[{ binding, kind, guards }, …]` for the full effect read set (no missing, no
invented). The 100 systematic + 100 complex cases are locked this way so A4
control-flow changes cannot silently invent or drop edges.
