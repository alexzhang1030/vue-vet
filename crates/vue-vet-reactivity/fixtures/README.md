# Reactivity tracer fixtures
This directory keeps corpus input separate from the Rust harness.
- `corpus/systematic`: 10 batches, 100 explicit primitive/effect/control-flow/import cases.
- `corpus/complex`: 10 control-flow batches, 100 explicit single-module cases.
- `corpus/modules`: 8 topology batches, 80 explicit multi-module cases with resolved links.
- `regressions`: source trees for symbol-identity false-positive regressions.
- `real-world`: five cross-module cases distilled from fixed Nuxt, VueUse, Vue Router, and Pinia commits.
The corpus tests hard-assert 100 + 100 + 80 cases, unique names, and unique source/module signatures. Real-world files record an exact upstream repository, commit, and path. Their source is a small semantic distillation rather than vendored upstream code, so provenance remains reviewable without coupling the test suite to unrelated implementation detail.
When adding a corpus case, update an existing batch and preserve the exact category count unless issue #28 is intentionally revised. Each module fixture must keep files separate and supply resolved links; concatenated source is not a valid cross-module test.
