use vue_vet_core::{Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, Severity};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{already_uses_target, is_test_path, optional_dependency_help, recommendation_from},
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/vueuse-use-event-listener",
  documentation: "rules/practice/vueuse-use-event-listener",
  recommend: EcosystemApi {
    package: "@vueuse/core",
    export: "useEventListener",
    docs_url: "https://vueuse.org/core/useEventListener/",
    import_example: "import { useEventListener } from '@vueuse/core'",
  },
};

/// Setup lifecycle hooks that commonly wrap listeners without cleanup.
const LIFECYCLE_HOOKS: &[&str] = &["onMounted", "onBeforeMount", "onActivated"];

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: Confidence::Medium,
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
    let findings =
      context
        .script()
        .blocks
        .iter()
        .filter(|block| !already_uses_target(block, RECIPE.recommend.export))
        .filter(|block| block.calls.iter().any(|call| is_lifecycle_hook(&call.callee)))
        .filter(|block| !block.calls.iter().any(|call| is_remove_listener(&call.callee)))
        .filter_map(|block| {
          block.calls.iter().find(|call| is_add_listener(&call.callee)).map(|call| {
            (call.span.clone(), optional_dependency_help(block, RECIPE.recommend.export))
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

fn is_lifecycle_hook(callee: &str) -> bool {
  LIFECYCLE_HOOKS.contains(&callee)
}

fn is_add_listener(callee: &str) -> bool {
  callee == "addEventListener" || callee.ends_with(".addEventListener")
}

fn is_remove_listener(callee: &str) -> bool {
  callee == "removeEventListener" || callee.ends_with(".removeEventListener")
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
