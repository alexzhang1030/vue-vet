//! Shared helpers for practice recipe rules.

use std::path::Path;

use vue_vet_core::{Recommendation, ScriptBlockFacts};

use crate::recipe::EcosystemApi;

#[must_use]
pub fn already_uses_target(block: &ScriptBlockFacts, export: &str) -> bool {
  block.imports.iter().any(|import| {
    is_vueuse_source(&import.source) && (import.imported == export || import.local == export)
  }) || block.calls.iter().any(|call| call.callee == export)
}

#[must_use]
pub fn has_vueuse_import(block: &ScriptBlockFacts) -> bool {
  block.imports.iter().any(|import| is_vueuse_source(&import.source))
}

#[must_use]
pub fn is_vueuse_source(source: &str) -> bool {
  source == "@vueuse/core" || source.starts_with("@vueuse/")
}

#[must_use]
pub fn is_test_path(path: &Path) -> bool {
  let normalized = path.to_string_lossy().replace('\\', "/");
  normalized.contains("/__tests__/")
    || normalized.contains(".test.")
    || normalized.contains(".spec.")
}

#[must_use]
pub fn recommendation_from(api: EcosystemApi) -> Recommendation {
  Recommendation {
    kind: "ecosystem_api".into(),
    package: api.package.into(),
    export: api.export.into(),
    docs_url: api.docs_url.into(),
    import_example: api.import_example.into(),
  }
}

#[must_use]
pub fn optional_dependency_help(block: &ScriptBlockFacts, export: &str) -> String {
  if has_vueuse_import(block) {
    format!("Prefer `{export}` from `@vueuse/core` for this pattern.")
  } else {
    format!(
      "Optional dependency: install `@vueuse/core` when you want `{export}`, then replace the hand-rolled pattern."
    )
  }
}
