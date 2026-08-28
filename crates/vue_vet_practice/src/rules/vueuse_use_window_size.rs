use vue_vet_core::{
  Confidence, PRACTICE_CATEGORY, ReactiveBindingKind, Rule, RuleContext, RuleMeta, Severity,
};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{
    already_uses_target, callee_is, is_setup_lifecycle_hook, is_test_path, recommendation_from,
    vueuse_help,
  },
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/vueuse-use-window-size",
  documentation: "rules/practice/vueuse-use-window-size",
  confidence: Confidence::Medium,
  min_vue: None,
  recommend: EcosystemApi {
    package: "@vueuse/core",
    export: "useWindowSize",
    docs_url: "https://vueuse.org/core/useWindowSize/",
    import_example: "import { useWindowSize } from '@vueuse/core'",
  },
};

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
};

pub(super) struct VueuseUseWindowSize;

pub(super) static RULE: VueuseUseWindowSize = VueuseUseWindowSize;

impl Rule for VueuseUseWindowSize {
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
      .filter(|block| block.calls.iter().any(|call| callee_is(&call.callee, "addEventListener")))
      .filter(|block| has_width_and_height_refs(block))
      .filter_map(|block| {
        block
          .calls
          .iter()
          .find(|call| callee_is(&call.callee, "addEventListener"))
          .map(|call| (call.span, vueuse_help(&environment, block, RECIPE.recommend.export)))
      })
      .collect::<Vec<_>>();
    for (span, help) in findings {
      context.report_with_recommendation(
        self.meta(),
        span,
        "This hand-rolls `width`/`height` refs with a `resize` listener; consider VueUse `useWindowSize` for a reactive, cleanup-safe size tracker.".into(),
        Some(help),
        recommendation_from(RECIPE.recommend),
      );
    }
  }
}

fn has_width_and_height_refs(block: &vue_vet_core::ScriptBlockFacts) -> bool {
  let width = block.reactivity_graph.bindings.iter().any(|binding| {
    matches!(binding.kind, ReactiveBindingKind::Ref | ReactiveBindingKind::ShallowRef)
      && binding.name.to_ascii_lowercase().contains("width")
  });
  let height = block.reactivity_graph.bindings.iter().any(|binding| {
    matches!(binding.kind, ReactiveBindingKind::Ref | ReactiveBindingKind::ShallowRef)
      && binding.name.to_ascii_lowercase().contains("height")
  });
  width && height
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use vue_vet_core::{
    ReactiveBindingFact, ReactivityGraph, ScriptBlockFacts, ScriptCallFact, ScriptFacts,
    ScriptKind, SourceSpan, TemplateFacts,
  };

  use super::*;
  use crate::practice_registry;

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 20, line: 1, column: offset.saturating_add(1) }
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

  fn size_refs() -> ReactivityGraph {
    let mut graph = ReactivityGraph::default();
    graph.bindings.push(ReactiveBindingFact {
      name: "width".into(),
      kind: ReactiveBindingKind::Ref,
      initialized_with_null: false,
      span: span(0),
    });
    graph.bindings.push(ReactiveBindingFact {
      name: "height".into(),
      kind: ReactiveBindingKind::Ref,
      initialized_with_null: false,
      span: span(0),
    });
    graph
  }

  fn run(calls: Vec<ScriptCallFact>, graph: ReactivityGraph) -> Vec<vue_vet_core::Diagnostic> {
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
        reactivity_graph: std::sync::Arc::new(graph),
      }],
    };
    // `addEventListener` inside a lifecycle hook also matches the generic
    // vueuse-use-event-listener recipe; scope assertions to this rule's id.
    practice_registry()
      .run(Path::new("src/Size.vue"), "", &TemplateFacts::default(), &script)
      .into_iter()
      .filter(|diagnostic| diagnostic.rule_id == RECIPE.rule_id)
      .collect()
  }

  #[test]
  fn reports_lifecycle_resize_listener_with_size_refs() {
    let diagnostics =
      run(vec![call("onMounted", 0), call("window.addEventListener", 20)], size_refs());
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert!(diagnostic.recommendation.is_some());
  }

  #[test]
  fn stays_quiet_without_size_refs() {
    let diagnostics = run(
      vec![call("onMounted", 0), call("window.addEventListener", 20)],
      ReactivityGraph::default(),
    );
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_without_lifecycle_hook() {
    let diagnostics = run(vec![call("window.addEventListener", 0)], size_refs());
    assert!(diagnostics.is_empty());
  }
}
