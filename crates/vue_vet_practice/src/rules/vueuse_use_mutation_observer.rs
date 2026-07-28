use vue_vet_core::{Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, Severity};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{
    already_uses_target, is_test_path, observer_ctor_without_disconnect, recommendation_from,
    vueuse_help,
  },
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/vueuse-use-mutation-observer",
  documentation: "rules/practice/vueuse-use-mutation-observer",
  confidence: Confidence::Medium,
  min_vue: None,
  recommend: EcosystemApi {
    package: "@vueuse/core",
    export: "useMutationObserver",
    docs_url: "https://vueuse.org/core/useMutationObserver/",
    import_example: "import { useMutationObserver } from '@vueuse/core'",
  },
};

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
};

pub(super) struct VueuseUseMutationObserver;

pub(super) static RULE: VueuseUseMutationObserver = VueuseUseMutationObserver;

impl Rule for VueuseUseMutationObserver {
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
      .filter_map(|block| {
        observer_ctor_without_disconnect(block, "MutationObserver").map(|call| {
          (call.span.clone(), vueuse_help(&environment, block, RECIPE.recommend.export))
        })
      })
      .collect::<Vec<_>>();
    for (span, help) in findings {
      context.report_with_recommendation(
        self.meta(),
        span,
        "This constructs a `MutationObserver` inside a setup lifecycle hook without `disconnect`; consider VueUse `useMutationObserver` for automatic cleanup.".into(),
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
        reactivity_graph: std::sync::Arc::new(ReactivityGraph::default()),
      }],
    };
    practice_registry().run(Path::new("src/Mo.vue"), "", &TemplateFacts::default(), &script)
  }

  #[test]
  fn reports_lifecycle_mutation_observer_without_disconnect() {
    let diagnostics = run(vec![call("onMounted", 0), call("MutationObserver", 20)]);
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert!(diagnostic.recommendation.is_some());
  }

  #[test]
  fn stays_quiet_when_disconnect_present() {
    let diagnostics = run(vec![
      call("onMounted", 0),
      call("MutationObserver", 20),
      call("observer.disconnect", 40),
    ]);
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_without_lifecycle_hook() {
    let diagnostics = run(vec![call("MutationObserver", 0)]);
    assert!(diagnostics.is_empty());
  }
}
