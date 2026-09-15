use vue_vet_core::{
  ConditionalWatchSourceFact, Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleGroupId,
  RuleMeta, Severity, TemplateFacts,
};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{expression_mentions_any, recommendation_from},
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/prefer-conditional-watch-source",
  documentation: "rules/practice/prefer-conditional-watch-source",
  confidence: Confidence::High,
  min_vue: None,
  recommend: EcosystemApi {
    package: "vue",
    export: "watch",
    docs_url: "https://vuejs.org/guide/essentials/watchers.html#watching-a-getter",
    import_example: "watch(() => [flag.value, flag.value ? selected.value : undefined], callback)",
  },
};

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
  group: Some(RuleGroupId::Derivation),
};

pub(super) struct PreferConditionalWatchSource;

pub(super) static RULE: PreferConditionalWatchSource = PreferConditionalWatchSource;

impl Rule for PreferConditionalWatchSource {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    let template = context.template();
    for block in &context.script().blocks {
      for site in &block.source_contracts.derivation_practice.conditional_watch_source {
        if template_reads_binding(template, &site.producer_names, &site.producer_name) {
          continue;
        }
        report_conditional(context, site);
      }
    }
  }
}

fn report_conditional(context: &mut RuleContext<'_>, site: &ConditionalWatchSourceFact) {
  context.report_with_recommendation(
    &META,
    site.source_array_span,
    "This array source keeps an idle computed producer live while the guard is inactive".into(),
    Some(format!(
      "`{name}` re-evaluates on idle dependency writes. A getter tuple `[guard.value, guard.value ? {name}.value : undefined]` skips those evaluations, preserves activation when branch values collide, and keeps the guarded assignment. Guard, producer, and idle write are the causal spans.",
      name = site.producer_name
    )),
    recommendation_from(RECIPE.recommend),
  );
}

fn template_reads_binding(template: &TemplateFacts, names: &[String], fallback: &str) -> bool {
  template.expressions.iter().any(|expression| {
    expression_mentions_any(
      expression.identifiers.as_deref(),
      &expression.expression,
      names,
      fallback,
    )
  })
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use vue_vet_core::{
    ConditionalWatchSourceFact, RuleEnvironment, ScriptBlockFacts, ScriptFacts, ScriptKind,
    SourceContractFacts, SourceSpan, TemplateExpressionFact, TemplateFacts,
  };

  use super::*;
  use crate::practice_registry;

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 8, line: 1, column: offset.saturating_add(1) }
  }

  #[test]
  fn reports_idle_producer_fact() {
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
          derivation_practice: vue_vet_core::DerivationPracticeFacts {
            conditional_watch_source: vec![ConditionalWatchSourceFact {
              source_array_span: span(10),
              guard_span: span(12),
              producer_span: span(30),
              idle_write_span: span(80),
              producer_name: "heavy".into(),
              producer_names: Vec::new(),
            }],
            ..vue_vet_core::DerivationPracticeFacts::default()
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
    assert!(diagnostic.help.as_deref().is_some_and(|help| help.contains("undefined")));
  }

  #[test]
  fn template_alias_render_stays_quiet() {
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
          derivation_practice: vue_vet_core::DerivationPracticeFacts {
            conditional_watch_source: vec![ConditionalWatchSourceFact {
              source_array_span: span(10),
              guard_span: span(12),
              producer_span: span(30),
              idle_write_span: span(80),
              producer_name: "heavy".into(),
              producer_names: vec!["heavy".into(), "shown".into()],
            }],
            ..vue_vet_core::DerivationPracticeFacts::default()
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
    let template = TemplateFacts {
      elements: Vec::new(),
      expressions: vec![TemplateExpressionFact {
        surface: "interpolation".into(),
        expression: "shown".into(),
        span: span(0),
        identifiers: Some(vec!["shown".into()]),
      }],
      allocations: Vec::new(),
      ..Default::default()
    };
    let diagnostics = practice_registry().run_with_environment(
      Path::new("src/App.vue"),
      "",
      &template,
      &script,
      RuleEnvironment::default(),
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
  }
}
