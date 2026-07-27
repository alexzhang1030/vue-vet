use vue_vet_core::{Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, Severity};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{already_uses_target, is_test_path, recommendation_from, vueuse_help},
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/vueuse-use-interval-fn",
  documentation: "rules/practice/vueuse-use-interval-fn",
  confidence: Confidence::Medium,
  min_vue: None,
  recommend: EcosystemApi {
    package: "@vueuse/core",
    export: "useIntervalFn",
    docs_url: "https://vueuse.org/core/useIntervalFn/",
    import_example: "import { useIntervalFn } from '@vueuse/core'",
  },
};

/// Setup lifecycle hooks that commonly start intervals without cleanup.
const LIFECYCLE_HOOKS: &[&str] = &["onMounted", "onBeforeMount", "onActivated"];

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
};

pub(super) struct VueuseUseIntervalFn;

pub(super) static RULE: VueuseUseIntervalFn = VueuseUseIntervalFn;

impl Rule for VueuseUseIntervalFn {
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
      .filter(|block| !block.calls.iter().any(|call| is_clear_interval(&call.callee)))
      .filter_map(|block| {
        block.calls.iter().find(|call| is_set_interval(&call.callee)).map(|call| {
          (call.span.clone(), vueuse_help(&environment, block, RECIPE.recommend.export))
        })
      })
      .collect::<Vec<_>>();
    for (span, help) in findings {
      context.report_with_recommendation(
        self.meta(),
        span,
        "This starts a timer interval inside a setup lifecycle hook without `clearInterval`; consider VueUse `useIntervalFn` for pause/resume and automatic cleanup.".into(),
        Some(help),
        recommendation_from(RECIPE.recommend),
      );
    }
  }
}

fn is_lifecycle_hook(callee: &str) -> bool {
  LIFECYCLE_HOOKS.contains(&callee)
}

fn is_set_interval(callee: &str) -> bool {
  callee == "setInterval" || callee.ends_with(".setInterval")
}

fn is_clear_interval(callee: &str) -> bool {
  callee == "clearInterval" || callee.ends_with(".clearInterval")
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
    SourceSpan { offset, length: 18, line: 1, column: offset.saturating_add(1) }
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
    practice_registry().run(Path::new("src/Interval.vue"), "", &TemplateFacts::default(), &script)
  }

  #[test]
  fn reports_lifecycle_set_interval_without_clear() {
    let diagnostics = run(vec![call("onMounted", 0), call("setInterval", 20)]);
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert!(diagnostic.recommendation.is_some());
  }

  #[test]
  fn stays_quiet_when_clear_interval_present() {
    let diagnostics =
      run(vec![call("onMounted", 0), call("setInterval", 20), call("clearInterval", 40)]);
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_without_lifecycle_hook() {
    let diagnostics = run(vec![call("setInterval", 0)]);
    assert!(diagnostics.is_empty());
  }
}
