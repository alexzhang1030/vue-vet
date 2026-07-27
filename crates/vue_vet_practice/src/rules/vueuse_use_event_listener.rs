use vue_vet_core::{Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, Severity};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{
    already_uses_target, callee_is, is_setup_lifecycle_hook, is_test_path, recommendation_from,
    vueuse_help,
  },
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/vueuse-use-event-listener",
  documentation: "rules/practice/vueuse-use-event-listener",
  confidence: Confidence::Medium,
  min_vue: None,
  recommend: EcosystemApi {
    package: "@vueuse/core",
    export: "useEventListener",
    docs_url: "https://vueuse.org/core/useEventListener/",
    import_example: "import { useEventListener } from '@vueuse/core'",
  },
};

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
};

pub(super) struct VueuseUseEventListener;

pub(super) static RULE: VueuseUseEventListener = VueuseUseEventListener;

impl Rule for VueuseUseEventListener {
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
      .filter(|block| block.calls.iter().any(|call| is_setup_lifecycle_hook(&call.callee)))
      .filter(|block| {
        !block.calls.iter().any(|call| callee_is(&call.callee, "removeEventListener"))
      })
      .filter_map(|block| {
        block.calls.iter().find(|call| callee_is(&call.callee, "addEventListener")).map(|call| {
          (call.span.clone(), vueuse_help(&environment, block, RECIPE.recommend.export))
        })
      })
      .collect::<Vec<_>>();
    for (span, help) in findings {
      context.report_with_recommendation(
        self.meta(),
        span,
        "This registers a DOM listener inside a setup lifecycle hook without `removeEventListener`; consider VueUse `useEventListener` for automatic cleanup.".into(),
        Some(help),
        recommendation_from(RECIPE.recommend),
      );
    }
  }
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
    SourceSpan { offset, length: 24, line: 1, column: offset.saturating_add(1) }
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
    practice_registry().run(Path::new("src/Listener.vue"), "", &TemplateFacts::default(), &script)
  }

  #[test]
  fn reports_lifecycle_add_without_remove() {
    let diagnostics = run(vec![call("onMounted", 0), call("window.addEventListener", 20)]);
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert!(diagnostic.recommendation.is_some());
  }

  #[test]
  fn stays_quiet_for_bare_add_without_lifecycle() {
    let diagnostics = run(vec![call("window.addEventListener", 0)]);
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_when_remove_is_present() {
    let diagnostics = run(vec![
      call("onMounted", 0),
      call("window.addEventListener", 20),
      call("window.removeEventListener", 60),
    ]);
    assert!(diagnostics.is_empty());
  }
}
