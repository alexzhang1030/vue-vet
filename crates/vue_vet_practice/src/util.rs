//! Shared helpers for practice recipe rules.

use std::path::Path;

use vue_vet_core::{Recommendation, RuleEnvironment, ScriptBlockFacts};

use crate::recipe::EcosystemApi;

#[must_use]
pub fn already_uses_target(block: &ScriptBlockFacts, export: &str) -> bool {
  block.imports.iter().any(|import| {
    is_vueuse_source(&import.source) && (import.imported == export || import.local == export)
  }) || block.calls.iter().any(|call| call.callee == export)
}

#[must_use]
pub fn is_vueuse_source(source: &str) -> bool {
  source == "@vueuse/core" || source.starts_with("@vueuse/")
}

#[must_use]
pub fn is_vue_runtime_source(source: &str) -> bool {
  source == "vue" || source == "vue-demi" || source.starts_with("@vue/")
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
pub fn vueuse_help(
  environment: &RuleEnvironment,
  block: &ScriptBlockFacts,
  export: &str,
) -> String {
  if already_uses_target(block, export) {
    return format!("Prefer `{export}` from `@vueuse/core` for this pattern.");
  }
  if environment.has_package("@vueuse/core")
    || block.imports.iter().any(|import| is_vueuse_source(&import.source))
  {
    format!("`@vueuse/core` is already available; prefer `{export}` for this pattern.")
  } else {
    format!(
      "Optional dependency: install `@vueuse/core` when you want `{export}`, then replace the hand-rolled pattern."
    )
  }
}

#[cfg(test)]
mod tests {
  use vue_vet_core::{ReactivityGraph, RuleEnvironment, ScriptBlockFacts, ScriptKind};

  use super::*;

  fn empty_block() -> ScriptBlockFacts {
    ScriptBlockFacts {
      kind: ScriptKind::Setup,
      language: "ts".into(),
      imports: Vec::new(),
      bindings: Vec::new(),
      calls: Vec::new(),
      member_writes: Vec::new(),
      destructures: Vec::new(),
      reactivity_graph: ReactivityGraph::default(),
    }
  }

  #[test]
  fn help_mentions_installed_vueuse_from_package_json() {
    let environment = RuleEnvironment { vue_version: None, packages: vec!["@vueuse/core".into()] };
    let help = vueuse_help(&environment, &empty_block(), "useDebounceFn");
    assert!(help.contains("already available"));
  }

  #[test]
  fn help_mentions_optional_install_when_missing() {
    let help = vueuse_help(&RuleEnvironment::default(), &empty_block(), "useDebounceFn");
    assert!(help.contains("Optional dependency"));
  }
}
