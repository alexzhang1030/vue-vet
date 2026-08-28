//! Shared helpers for practice recipe rules.

use std::path::Path;

use vue_vet_core::{Recommendation, RuleEnvironment, ScriptBlockFacts};

use crate::recipe::EcosystemApi;

/// Compiler macros (`defineModel`, `defineProps`, …) exist only in `<script setup>`.
pub use vue_vet_rule_query::is_setup_block as is_script_setup_block;

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
  source == "vue" || source == "vue-demi" || source == "#imports" || source.starts_with("@vue/")
}

#[must_use]
pub fn is_test_path(path: &Path) -> bool {
  let normalized = path.to_string_lossy().replace('\\', "/");
  normalized.contains("/__tests__/")
    || normalized.contains(".test.")
    || normalized.contains(".spec.")
}

/// Setup lifecycle hooks that commonly wrap side effects without cleanup.
const SETUP_LIFECYCLE_HOOKS: &[&str] = &["onMounted", "onBeforeMount", "onActivated"];

#[must_use]
pub fn is_setup_lifecycle_hook(callee: &str) -> bool {
  SETUP_LIFECYCLE_HOOKS.contains(&callee)
}

/// Bare name or static member like `window.setTimeout`.
#[must_use]
pub fn callee_is(callee: &str, name: &str) -> bool {
  callee == name || callee.rsplit_once('.').is_some_and(|(_, property)| property == name)
}

/// First `new Ctor(...)` / bare ctor call in a block that also has a setup lifecycle
/// hook and no `disconnect` (including `observer.disconnect`).
#[must_use]
pub fn observer_ctor_without_disconnect<'a>(
  block: &'a ScriptBlockFacts,
  ctor: &str,
) -> Option<&'a vue_vet_core::ScriptCallFact> {
  if !block.calls.iter().any(|call| is_setup_lifecycle_hook(&call.callee)) {
    return None;
  }
  if block.calls.iter().any(|call| callee_is(&call.callee, "disconnect")) {
    return None;
  }
  block.calls.iter().find(|call| call.callee == ctor)
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
      top_level_await_ends: Vec::new(),
      operands: Vec::new(),
      reactivity_graph: std::sync::Arc::new(ReactivityGraph::default()),
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

  #[test]
  fn callee_is_matches_bare_and_static_members() {
    assert!(callee_is("setTimeout", "setTimeout"));
    assert!(callee_is("window.setTimeout", "setTimeout"));
    assert!(!callee_is("setInterval", "setTimeout"));
    assert!(!callee_is("mysetTimeout", "setTimeout"));
  }

  #[test]
  fn setup_lifecycle_hooks_are_stable() {
    assert!(is_setup_lifecycle_hook("onMounted"));
    assert!(is_setup_lifecycle_hook("onBeforeMount"));
    assert!(is_setup_lifecycle_hook("onActivated"));
    assert!(!is_setup_lifecycle_hook("onBeforeUnmount"));
  }
}
