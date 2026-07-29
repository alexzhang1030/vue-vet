use vue_vet_core::{Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, Severity};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{already_uses_target, callee_is, is_test_path, recommendation_from, vueuse_help},
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/vueuse-use-debounce-fn",
  documentation: "rules/practice/vueuse-use-debounce-fn",
  confidence: Confidence::Medium,
  min_vue: None,
  recommend: EcosystemApi {
    package: "@vueuse/core",
    export: "useDebounceFn",
    docs_url: "https://vueuse.org/core/useDebounceFn/",
    import_example: "import { useDebounceFn } from '@vueuse/core'",
  },
};

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
};

pub(super) struct VueuseUseDebounceFn;

pub(super) static RULE: VueuseUseDebounceFn = VueuseUseDebounceFn;

impl Rule for VueuseUseDebounceFn {
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
        let set_timeout = block
          .calls
          .iter()
          .find(|call| callee_is(&call.callee, "setTimeout") && call.assigned_to.is_some())?;
        let timer = set_timeout.assigned_to.as_deref()?;
        let linked_clear = block.calls.iter().any(|call| {
          callee_is(&call.callee, "clearTimeout")
            && call.argument_identifiers.iter().any(|name| name == timer)
        });
        linked_clear.then(|| {
          (set_timeout.span.clone(), vueuse_help(&environment, block, RECIPE.recommend.export))
        })
      })
      .collect::<Vec<_>>();
    for (span, help) in findings {
      context.report_with_recommendation(
        self.meta(),
        span,
        "This looks like a hand-rolled debounce that clears and reassigns the same timer; consider VueUse `useDebounceFn`.".into(),
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
    SourceSpan { offset, length: 10, line: 1, column: offset.saturating_add(1) }
  }

  fn call(
    callee: &str,
    assigned_to: Option<&str>,
    argument_identifiers: &[&str],
    offset: usize,
  ) -> ScriptCallFact {
    ScriptCallFact {
      callee: callee.into(),
      assigned_to: assigned_to.map(str::to_owned),
      resolved_import: None,
      argument_identifiers: argument_identifiers.iter().map(|name| (*name).into()).collect(),
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
        top_level_await_ends: Vec::new(),
        operands: Vec::new(),
        reactivity_graph: std::sync::Arc::new(ReactivityGraph::default()),
      }],
    };
    practice_registry().run(Path::new("src/Debounce.vue"), "", &TemplateFacts::default(), &script)
  }

  #[test]
  fn reports_when_clear_and_set_share_the_same_timer() {
    let diagnostics = run(vec![
      call("clearTimeout", None, &["timer"], 0),
      call("setTimeout", Some("timer"), &[], 20),
    ]);
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert_eq!(diagnostic.category, PRACTICE_CATEGORY);
    assert!(diagnostic.recommendation.is_some());
  }

  #[test]
  fn stays_quiet_when_clear_targets_a_different_binding() {
    let diagnostics = run(vec![
      call("clearTimeout", None, &["other"], 0),
      call("setTimeout", Some("timer"), &[], 20),
    ]);
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_without_clear_timeout() {
    let diagnostics = run(vec![call("setTimeout", Some("timer"), &[], 0)]);
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn reports_window_member_timer_apis() {
    let diagnostics = run(vec![
      call("window.clearTimeout", None, &["timer"], 0),
      call("window.setTimeout", Some("timer"), &[], 20),
    ]);
    assert_eq!(diagnostics.len(), 1);
  }
}
