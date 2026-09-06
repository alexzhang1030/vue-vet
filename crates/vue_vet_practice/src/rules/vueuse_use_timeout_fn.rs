use vue_vet_core::{Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, Severity};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{
    already_uses_target, callee_is, is_setup_lifecycle_hook, is_test_path, recommendation_from,
    vueuse_help,
  },
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
      .filter(|block| !block.calls.iter().any(|call| callee_is(&call.callee, "clearTimeout")))
      .flat_map(|block| {
        block.calls.iter().filter(|call| callee_is(&call.callee, "setTimeout")).filter_map(|call| {
          let enclosing_lifecycle =
            call.enclosing_callees.iter().any(|name| is_setup_lifecycle_hook(name));
          if !enclosing_lifecycle {
            return None;
          }
          Some((call.span, vueuse_help(&environment, block, RECIPE.recommend.export)))
        })
      })
      .collect::<Vec<_>>();
    for (span, help) in findings {
      context.report_with_recommendation(
        self.meta(),
        span,
        "This timeout is declared in code registered by a setup lifecycle hook; consider a setup-owned useTimeoutFn for cancellation and cleanup.".into(),
        Some(format!(
          "{help} Create `useTimeoutFn` during setup with `{{ immediate: false }}`, then call `start()` from the callback."
        )),
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
    SourceSpan { offset, length: 16, line: 1, column: offset.saturating_add(1) }
  }

  fn call(callee: &str, offset: usize) -> ScriptCallFact {
    ScriptCallFact { callee: callee.into(), span: span(offset), ..ScriptCallFact::default() }
  }

  fn timeout_in_lifecycle(offset: usize) -> ScriptCallFact {
    ScriptCallFact {
      callee: "setTimeout".into(),
      span: span(offset),
      enclosing_callees: vec!["onMounted".into()],
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
        source_contracts: vue_vet_core::SourceContractFacts::default(),
        reactivity_graph: std::sync::Arc::new(ReactivityGraph::default()),
      }],
    };
    practice_registry().run(Path::new("src/Timeout.vue"), "", &TemplateFacts::default(), &script)
  }

  #[test]
  fn reports_lifecycle_set_timeout_without_clear() {
    let diagnostics = run(vec![call("onMounted", 0), timeout_in_lifecycle(20)]);
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert!(diagnostic.recommendation.is_some());
  }

  #[test]
  fn reports_timeout_in_nested_subscribe_under_lifecycle() {
    let diagnostics = run(vec![
      call("onMounted", 0),
      ScriptCallFact {
        callee: "setTimeout".into(),
        span: span(20),
        enclosing_callees: vec!["onMounted".into(), "subscribe".into()],
        ..ScriptCallFact::default()
      },
    ]);
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert!(diagnostic.message.contains("setup lifecycle hook"));
    assert!(diagnostic.message.contains("setup-owned useTimeoutFn"));
    assert!(
      diagnostic
        .help
        .as_deref()
        .is_some_and(|help| { help.contains("immediate: false") && help.contains("`start()`") }),
      "help must tell the caller to construct during setup: {:?}",
      diagnostic.help
    );
  }

  #[test]
  fn stays_quiet_when_timeout_is_inside_watch_not_lifecycle() {
    let diagnostics = run(vec![
      call("onMounted", 0),
      ScriptCallFact {
        callee: "setTimeout".into(),
        span: span(20),
        enclosing_callees: vec!["watch".into()],
        ..ScriptCallFact::default()
      },
    ]);
    assert!(diagnostics.is_empty());
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
    let diagnostics =
      run(vec![call("onMounted", 0), call("clearTimeout", 10), call("setTimeout", 20)]);
    assert!(
      !diagnostics.iter().any(|diagnostic| diagnostic.rule_id == RECIPE.rule_id),
      "linked clear+set belongs to useDebounceFn, not useTimeoutFn"
    );
  }
}
