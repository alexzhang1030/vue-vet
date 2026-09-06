use vue_vet_core::{Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, Severity};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{
    already_uses_target, callee_is, is_setup_lifecycle_hook, is_test_path, recommendation_from,
    vueuse_help,
  },
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/vueuse-use-raf-fn",
  documentation: "rules/practice/vueuse-use-raf-fn",
  confidence: Confidence::Medium,
  min_vue: None,
  recommend: EcosystemApi {
    package: "@vueuse/core",
    export: "useRafFn",
    docs_url: "https://vueuse.org/core/useRafFn/",
    import_example: "import { useRafFn } from '@vueuse/core'",
  },
};

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
};

pub(super) struct VueuseUseRafFn;

pub(super) static RULE: VueuseUseRafFn = VueuseUseRafFn;

impl Rule for VueuseUseRafFn {
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
      .filter(|block| {
        !block.calls.iter().any(|call| callee_is(&call.callee, "cancelAnimationFrame"))
      })
      .filter_map(|block| {
        block
          .calls
          .iter()
          .find(|call| {
            callee_is(&call.callee, "requestAnimationFrame")
              && call.callback_reschedules_self
              && call.enclosing_callees.iter().any(|name| is_setup_lifecycle_hook(name))
          })
          .map(|call| (call.span, vueuse_help(&environment, block, RECIPE.recommend.export)))
      })
      .collect::<Vec<_>>();
    for (span, help) in findings {
      context.report_with_recommendation(
        self.meta(),
        span,
        "This repeats `requestAnimationFrame` inside a setup lifecycle hook without `cancelAnimationFrame`; consider VueUse `useRafFn` for pause/resume and automatic cleanup.".into(),
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
    SourceSpan { offset, length: 20, line: 1, column: offset.saturating_add(1) }
  }

  fn call(callee: &str, offset: usize) -> ScriptCallFact {
    ScriptCallFact { callee: callee.into(), span: span(offset), ..ScriptCallFact::default() }
  }

  fn raf_loop(offset: usize) -> ScriptCallFact {
    ScriptCallFact {
      callee: "requestAnimationFrame".into(),
      span: span(offset),
      enclosing_callees: vec!["onMounted".into(), "requestAnimationFrame".into()],
      argument_identifiers: vec!["loop".into()],
      callback_reschedules_self: true,
      ..ScriptCallFact::default()
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
        lifetime: vue_vet_core::ReactivityLifetimeFacts::default(),
        source_contracts: vue_vet_core::SourceContractFacts::default(),
        reactivity_graph: std::sync::Arc::new(ReactivityGraph::default()),
      }],
    };
    practice_registry().run(Path::new("src/Raf.vue"), "", &TemplateFacts::default(), &script)
  }

  #[test]
  fn reports_lifecycle_raf_loop_without_cancel() {
    let diagnostics = run(vec![call("onMounted", 0), raf_loop(20)]);
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert!(diagnostic.recommendation.is_some());
    assert!(
      diagnostic.message.contains("repeats"),
      "loop recipe must not claim one-shot rAF: {}",
      diagnostic.message
    );
  }

  #[test]
  fn stays_quiet_for_one_shot_lifecycle_raf() {
    let diagnostics = run(vec![
      call("onMounted", 0),
      ScriptCallFact {
        callee: "requestAnimationFrame".into(),
        span: span(20),
        enclosing_callees: vec!["onMounted".into()],
        has_function_argument: true,
        ..ScriptCallFact::default()
      },
    ]);
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_for_named_one_shot_and_two_frame_delay() {
    let named = run(vec![
      call("onMounted", 0),
      ScriptCallFact {
        callee: "requestAnimationFrame".into(),
        span: span(20),
        enclosing_callees: vec!["onMounted".into()],
        argument_identifiers: vec!["render".into()],
        ..ScriptCallFact::default()
      },
    ]);
    let two_frame = run(vec![
      call("onMounted", 0),
      ScriptCallFact {
        callee: "requestAnimationFrame".into(),
        span: span(20),
        enclosing_callees: vec!["onMounted".into(), "requestAnimationFrame".into()],
        has_function_argument: true,
        ..ScriptCallFact::default()
      },
    ]);
    assert!(named.is_empty());
    assert!(two_frame.is_empty());
  }

  #[test]
  fn stays_quiet_when_cancel_present() {
    let diagnostics = run(vec![
      call("onMounted", 0),
      call("requestAnimationFrame", 20),
      call("cancelAnimationFrame", 40),
    ]);
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_without_lifecycle_hook() {
    let diagnostics = run(vec![call("requestAnimationFrame", 0)]);
    assert!(diagnostics.is_empty());
  }
}
