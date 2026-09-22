//! `VueUse` replacements for a setup-owned observer constructed without `disconnect`.

use vue_vet_core::{Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, Severity};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{
    already_uses_target, is_test_path, observer_ctor_without_disconnect, recommendation_from,
    vueuse_help,
  },
};

pub(super) struct ObserverRule {
  meta: &'static RuleMeta,
  recipe: PracticeRecipe,
  ctor: &'static str,
  message: &'static str,
}

impl Rule for ObserverRule {
  fn meta(&self) -> &'static RuleMeta {
    self.meta
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
      .filter(|block| !already_uses_target(block, self.recipe.recommend.export))
      .filter_map(|block| {
        observer_ctor_without_disconnect(block, self.ctor)
          .map(|call| (call.span, vueuse_help(&environment, block, self.recipe.recommend.export)))
      })
      .collect::<Vec<_>>();
    for (span, help) in findings {
      context.report_with_recommendation(
        self.meta(),
        span,
        self.message.into(),
        Some(help),
        recommendation_from(self.recipe.recommend),
      );
    }
  }
}

const fn recipe(
  rule_id: &'static str,
  documentation: &'static str,
  export: &'static str,
  docs_url: &'static str,
  import_example: &'static str,
) -> PracticeRecipe {
  PracticeRecipe {
    rule_id,
    documentation,
    confidence: Confidence::Medium,
    min_vue: None,
    recommend: EcosystemApi { package: "@vueuse/core", export, docs_url, import_example },
  }
}

const INTERSECTION_RECIPE: PracticeRecipe = recipe(
  "vue-vet/practice/vueuse-use-intersection-observer",
  "rules/practice/vueuse-use-intersection-observer",
  "useIntersectionObserver",
  "https://vueuse.org/core/useIntersectionObserver/",
  "import { useIntersectionObserver } from '@vueuse/core'",
);

const MUTATION_RECIPE: PracticeRecipe = recipe(
  "vue-vet/practice/vueuse-use-mutation-observer",
  "rules/practice/vueuse-use-mutation-observer",
  "useMutationObserver",
  "https://vueuse.org/core/useMutationObserver/",
  "import { useMutationObserver } from '@vueuse/core'",
);

const RESIZE_RECIPE: PracticeRecipe = recipe(
  "vue-vet/practice/vueuse-use-resize-observer",
  "rules/practice/vueuse-use-resize-observer",
  "useResizeObserver",
  "https://vueuse.org/core/useResizeObserver/",
  "import { useResizeObserver } from '@vueuse/core'",
);

const fn meta(recipe: &PracticeRecipe) -> RuleMeta {
  RuleMeta {
    id: recipe.rule_id,
    category: PRACTICE_CATEGORY,
    default_severity: Severity::Info,
    confidence: recipe.confidence,
    documentation: recipe.documentation,
    group: None,
  }
}

static INTERSECTION_META: RuleMeta = meta(&INTERSECTION_RECIPE);
static MUTATION_META: RuleMeta = meta(&MUTATION_RECIPE);
static RESIZE_META: RuleMeta = meta(&RESIZE_RECIPE);

pub(super) static INTERSECTION: ObserverRule = ObserverRule {
  meta: &INTERSECTION_META,
  recipe: INTERSECTION_RECIPE,
  ctor: "IntersectionObserver",
  message: "This constructs an `IntersectionObserver` inside a setup lifecycle hook without `disconnect`; consider VueUse `useIntersectionObserver` for automatic cleanup.",
};

pub(super) static MUTATION: ObserverRule = ObserverRule {
  meta: &MUTATION_META,
  recipe: MUTATION_RECIPE,
  ctor: "MutationObserver",
  message: "This constructs a `MutationObserver` inside a setup lifecycle hook without `disconnect`; consider VueUse `useMutationObserver` for automatic cleanup.",
};

pub(super) static RESIZE: ObserverRule = ObserverRule {
  meta: &RESIZE_META,
  recipe: RESIZE_RECIPE,
  ctor: "ResizeObserver",
  message: "This constructs a `ResizeObserver` inside a setup lifecycle hook without `disconnect`; consider VueUse `useResizeObserver` for automatic cleanup.",
};

#[cfg(test)]
mod tests {
  use std::path::Path;

  use vue_vet_core::{
    ReactivityGraph, ScriptBlockFacts, ScriptCallFact, ScriptFacts, ScriptKind, SourceSpan,
    TemplateFacts,
  };

  use super::{INTERSECTION, MUTATION, RESIZE};
  use crate::practice_registry;

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 24, line: 1, column: offset.saturating_add(1) }
  }

  fn call(callee: &str, offset: usize) -> ScriptCallFact {
    ScriptCallFact { callee: callee.into(), span: span(offset), ..ScriptCallFact::default() }
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
        template_ref_demands: vue_vet_core::TemplateRefDemandFacts::default(),
        reactivity_graph: std::sync::Arc::new(ReactivityGraph::default()),
        vapor: false,
        open_span: None,
        runtime_export_spans: Vec::new(),
      }],
    };
    practice_registry().run(Path::new("src/Mo.vue"), "", &TemplateFacts::default(), &script)
  }

  fn reports(ctor: &str, rule_id: &str) {
    let diagnostics = run(vec![call("onMounted", 0), call(ctor, 20)]);
    assert_eq!(diagnostics.len(), 1, "{ctor}");
    assert_eq!(diagnostics.first().map(|diagnostic| diagnostic.rule_id.as_str()), Some(rule_id));
    assert!(diagnostics.first().is_some_and(|diagnostic| diagnostic.recommendation.is_some()));
  }

  #[test]
  fn reports_lifecycle_observer_without_disconnect() {
    reports("IntersectionObserver", INTERSECTION.recipe.rule_id);
    reports("MutationObserver", MUTATION.recipe.rule_id);
    reports("ResizeObserver", RESIZE.recipe.rule_id);
  }

  #[test]
  fn stays_quiet_when_disconnect_present() {
    for ctor in ["IntersectionObserver", "MutationObserver", "ResizeObserver"] {
      let diagnostics =
        run(vec![call("onMounted", 0), call(ctor, 20), call("observer.disconnect", 40)]);
      assert!(diagnostics.is_empty(), "{ctor}");
    }
  }

  #[test]
  fn stays_quiet_without_lifecycle_hook() {
    for ctor in ["IntersectionObserver", "MutationObserver", "ResizeObserver"] {
      assert!(run(vec![call(ctor, 0)]).is_empty(), "{ctor}");
    }
  }
}
