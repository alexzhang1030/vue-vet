use vue_vet_core::{Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, Severity};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{is_test_path, recommendation_from},
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/prefer-keyed-map-dependency",
  documentation: "rules/practice/prefer-keyed-map-dependency",
  confidence: Confidence::High,
  min_vue: None,
  recommend: EcosystemApi {
    package: "vue",
    export: "Map.prototype.get",
    docs_url: "https://vuejs.org/guide/essentials/reactivity-fundamentals.html#collections",
    import_example: "keyed.get('selected')",
  },
};

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
};

pub(super) struct PreferKeyedMapDependency;

pub(super) static RULE: PreferKeyedMapDependency = PreferKeyedMapDependency;

impl Rule for PreferKeyedMapDependency {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    if is_test_path(context.file()) {
      return;
    }
    for block in &context.script().blocks {
      for site in &block.source_contracts.keyed_map_dependency {
        context.report_with_recommendation(
          self.meta(),
          site.for_each_span,
          format!(
            "`forEach` over a reactive Map to select `{}` tracks every entry; `Map.get` tracks only that key",
            site.key
          ),
          Some(format!(
            "{} at {}:{} reruns when unrelated keys change. `get('{}')` yields the same selected value or `undefined` after delete.",
            site.api,
            site.result_span.line,
            site.result_span.column,
            site.key
          )),
          recommendation_from(RECIPE.recommend),
        );
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use vue_vet_core::{
    KeyedMapDependencyFact, RuleEnvironment, ScriptBlockFacts, ScriptFacts, ScriptKind,
    SourceContractFacts, SourceSpan, TemplateFacts,
  };

  use super::*;
  use crate::practice_registry;

  fn span() -> SourceSpan {
    SourceSpan { offset: 10, length: 8, line: 4, column: 3 }
  }

  fn run(facts: SourceContractFacts) -> Vec<vue_vet_core::Diagnostic> {
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
        source_contracts: facts,
        template_ref_demands: vue_vet_core::TemplateRefDemandFacts::default(),
        reactivity_graph: std::sync::Arc::new(vue_vet_core::ReactivityGraph::default()),
        vapor: false,
        open_span: None,
        runtime_export_spans: Vec::new(),
      }],
    };
    practice_registry().run_with_environment(
      Path::new("src/App.vue"),
      "",
      &TemplateFacts::default(),
      &script,
      RuleEnvironment::default(),
    )
  }

  #[test]
  fn reports_keyed_foreach_fact() {
    let diagnostics = run(SourceContractFacts {
      keyed_map_dependency: vec![KeyedMapDependencyFact {
        for_each_span: span(),
        map_span: span(),
        result_span: span(),
        key: "selected".into(),
        api: "computed".into(),
      }],
      ..SourceContractFacts::default()
    });
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert!(diagnostic.recommendation.is_some());
    assert!(diagnostic.message.contains("Map.get"));
  }

  #[test]
  fn stays_quiet_without_facts() {
    assert!(run(SourceContractFacts::default()).is_empty());
  }
}
