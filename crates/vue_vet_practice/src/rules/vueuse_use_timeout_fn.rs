use vue_vet_core::{Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, Severity};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{already_uses_target, is_test_path, recommendation_from, vueuse_help},
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/vueuse-use-timeout-fn",
  documentation: "rules/practice/vueuse-use-timeout-fn",
  confidence: Confidence::Medium,
  min_vue: None,
  recommend: EcosystemApi {
    package: "@vueuse/core",
    export: "useTimeoutFn",
    docs_url: "https://vueuse.org/core/useTimeoutFn/",
    import_example: "import { useTimeoutFn } from '@vueuse/core'",
  },
};

/// Setup lifecycle hooks that commonly start timeouts without cleanup.
const LIFECYCLE_HOOKS: &[&str] = &["onMounted", "onBeforeMount", "onActivated"];

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
};

pub(super) struct VueuseUseTimeoutFn;

pub(super) static RULE: VueuseUseTimeoutFn = VueuseUseTimeoutFn;

impl Rule for VueuseUseTimeoutFn {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    if is_test_path(context.file()) {
      return;
    }
    let environment = context.environment().clone();
    let findings = context
      .script()
      .blocks
      .iter()
      .filter(|block| !already_uses_target(block, RECIPE.recommend.export))
      .filter(|block| block.calls.iter().any(|call| is_lifecycle_hook(&call.callee)))
      .filter(|block| !block.calls.iter().any(|call| is_clear_timeout(&call.callee)))
      .filter_map(|block| {
        block.calls.iter().find(|call| is_set_timeout(&call.callee)).map(|call| {
          (call.span.clone(), vueuse_help(&environment, block, RECIPE.recommend.export))
        })
      })
      .collect::<Vec<_>>();
    for (span, help) in findings {
      context.report_with_recommendation(
        self.meta(),
        span,
        "This starts a timeout inside a setup lifecycle hook without `clearTimeout`; consider VueUse `useTimeoutFn` for cancellable delays and automatic cleanup.".into(),
        Some(help),
        recommendation_from(RECIPE.recommend),
      );
    }
  }
}

fn is_lifecycle_hook(callee: &str) -> bool {
  LIFECYCLE_HOOKS.contains(&callee)
}

fn is_set_timeout(callee: &str) -> bool {
  callee == "setTimeout" || callee.ends_with(".setTimeout")
}

fn is_clear_timeout(callee: &str) -> bool {
  callee == "clearTimeout" || callee.ends_with(".clearTimeout")
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use vue_vet_core::{
    ReactivityGraph, ScriptBlockFacts, ScriptCallFact, ScriptFacts, ScriptKind, SourceSpan,
    TemplateFacts,
  };

  use super::*;
  use crate::practice_registry;

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 16, line: 1, column: offset.saturating_add(1) }
  }

  fn call(callee: &str, offset: usize) -> ScriptCallFact {
    ScriptCallFact {
      callee: callee.into(),
      assigned_to: None,
      resolved_import: None,
      argument_identifiers: Vec::new(),
      span: span(offset),
    }
  }

  fn run(calls: Vec<ScriptCallFact>) -> Vec<vue_vet_core::Diagnostic> {
    let script = ScriptFacts {
      blocks: vec![ScriptBlockFacts {
        kind: ScriptKind::Setup,
        language: "ts".into(),
        imports: Vec::new(),
        bindings: Vec::new(),
        calls,
        member_writes: Vec::new(),
        destructures: Vec::new(),
        reactivity_graph: ReactivityGraph::default(),
      }],
    };
    practice_registry().run(Path::new("src/Timeout.vue"), "", &TemplateFacts::default(), &script)
  }

  #[test]
  fn reports_lifecycle_set_timeout_without_clear() {
    let diagnostics = run(vec![call("onMounted", 0), call("setTimeout", 20)]);
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert!(diagnostic.recommendation.is_some());
  }

  #[test]
  fn stays_quiet_when_clear_timeout_present() {
    let diagnostics =
      run(vec![call("onMounted", 0), call("setTimeout", 20), call("clearTimeout", 40)]);
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_without_lifecycle_hook() {
    let diagnostics = run(vec![call("setTimeout", 0)]);
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_for_hand_rolled_debounce_inside_lifecycle() {
    // Debounce pattern has clearTimeout; timeout recipe must not compete with useDebounceFn.
    let diagnostics =
      run(vec![call("onMounted", 0), call("clearTimeout", 10), call("setTimeout", 20)]);
    assert!(
      !diagnostics.iter().any(|diagnostic| diagnostic.rule_id == RECIPE.rule_id),
      "linked clear+set belongs to useDebounceFn, not useTimeoutFn"
    );
  }
}
