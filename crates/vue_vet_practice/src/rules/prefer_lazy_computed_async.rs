use vue_vet_core::{
  Confidence, LazyComputedAsyncFact, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, Severity,
};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::recommendation_from,
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/prefer-lazy-computed-async",
  documentation: "rules/practice/prefer-lazy-computed-async",
  confidence: Confidence::High,
  min_vue: None,
  recommend: EcosystemApi {
    package: "@vueuse/core",
    export: "computedAsync",
    docs_url: "https://vueuse.org/core/computedAsync/",
    import_example: "import { computedAsync } from '@vueuse/core'",
  },
};

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
};

pub(super) struct PreferLazyComputedAsync;

pub(super) static RULE: PreferLazyComputedAsync = PreferLazyComputedAsync;

impl Rule for PreferLazyComputedAsync {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.scheduling_practice.lazy_computed_async {
        report_lazy(context, site);
      }
    }
  }
}

fn report_lazy(context: &mut RuleContext<'_>, site: &LazyComputedAsyncFact) {
  context.report_with_recommendation(
    &META,
    site.call_span,
    "Eager `computedAsync` starts work before any consumer reads the result".into(),
    Some(
      "Pass `{ lazy: true }` as the third argument. Startup evaluations are skipped until the first read, that read sees `initialState`, and the output is readonly. The producer stays started after the first demand. The earlier source write and later loading-tolerant consumer are the causal spans."
        .into(),
    ),
    recommendation_from(RECIPE.recommend),
  );
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use vue_vet_core::{
    LazyComputedAsyncFact, RuleEnvironment, ScriptBlockFacts, ScriptFacts, ScriptKind,
    SourceContractFacts, SourceSpan, TemplateFacts,
  };

  use super::*;
  use crate::practice_registry;

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 12, line: 1, column: offset.saturating_add(1) }
  }

  #[test]
  fn reports_closed_lazy_async_fact() {
    let script = ScriptFacts {
      blocks: vec![ScriptBlockFacts {
        kind: ScriptKind::Setup,
        language: "ts".into(),
        imports: Vec::new(),
        bindings: Vec::new(),
        calls: Vec::new(),
        member_writes: Vec::new(),
        destructures: Vec::new(),
        top_level_await_ends: Vec::new(),
        operands: Vec::new(),
        lifetime: vue_vet_core::ReactivityLifetimeFacts::default(),
        source_contracts: SourceContractFacts {
          scheduling_practice: vue_vet_core::SchedulingPracticeFacts {
            lazy_computed_async: vec![LazyComputedAsyncFact {
              call_span: span(10),
              source_span: span(40),
              demand_span: span(80),
            }],
            ..vue_vet_core::SchedulingPracticeFacts::default()
          },
          ..SourceContractFacts::default()
        },
        template_ref_demands: vue_vet_core::TemplateRefDemandFacts::default(),
        reactivity_graph: std::sync::Arc::new(vue_vet_core::ReactivityGraph::default()),
        vapor: false,
        open_span: None,
        runtime_export_spans: Vec::new(),
      }],
    };
    let diagnostics = practice_registry().run_with_environment(
      Path::new("src/App.vue"),
      "",
      &TemplateFacts::default(),
      &script,
      RuleEnvironment::default(),
    );
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert_eq!(diagnostic.category, PRACTICE_CATEGORY);
    assert!(!diagnostic.affects_score());
    assert!(!diagnostic.affects_exit());
  }
}
