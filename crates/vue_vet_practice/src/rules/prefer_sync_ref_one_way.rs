use vue_vet_core::{
  Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleGroupId, RuleMeta, Severity,
  SyncRefOneWayFact, TemplateFacts,
};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{expression_mentions_any, recommendation_from},
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/prefer-sync-ref-one-way",
  documentation: "rules/practice/prefer-sync-ref-one-way",
  confidence: Confidence::High,
  min_vue: None,
  recommend: EcosystemApi {
    package: "@vueuse/core",
    export: "syncRef",
    docs_url: "https://vueuse.org/shared/syncRef/",
    import_example: "import { syncRef } from '@vueuse/core'",
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

pub(super) struct PreferSyncRefOneWay;

pub(super) static RULE: PreferSyncRefOneWay = PreferSyncRefOneWay;

impl Rule for PreferSyncRefOneWay {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    let template = context.template();
    for block in &context.script().blocks {
      for site in &block.source_contracts.derivation_practice.sync_ref_one_way {
        if template_writes_binding(template, &site.sink_names, &site.sink_name) {
          continue;
        }
        report_sync_ref(context, site);
      }
    }
  }
}

fn report_sync_ref(context: &mut RuleContext<'_>, site: &SyncRefOneWayFact) {
  context.report_with_recommendation(
    &META,
    site.call_span,
    "Default two-way `syncRef` installs a reverse watcher that this sink never writes".into(),
    Some(format!(
      "`{name}` is only read after a changed left write. `{{ direction: 'ltr' }}` keeps those observations and removes the reverse watcher. Left update and right demand are the causal spans.",
      name = site.sink_name
    )),
    recommendation_from(RECIPE.recommend),
  );
}

fn template_writes_binding(template: &TemplateFacts, names: &[String], fallback: &str) -> bool {
  template.expressions.iter().any(|expression| {
    let surface = expression.surface.as_str();
    (surface == "model" || surface == "on")
      && expression_mentions_any(
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
    RuleEnvironment, ScriptBlockFacts, ScriptFacts, ScriptKind, SourceContractFacts, SourceSpan,
    SyncRefOneWayFact, TemplateExpressionFact, TemplateFacts,
  };

  use super::*;
  use crate::practice_registry;

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 8, line: 1, column: offset.saturating_add(1) }
  }

  #[test]
  fn reports_closed_sync_ref_fact() {
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
            sync_ref_one_way: vec![SyncRefOneWayFact {
              call_span: span(10),
              source_span: span(40),
              sink_span: span(20),
              demand_span: span(50),
              sink_name: "display".into(),
              sink_names: Vec::new(),
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
    assert_eq!(diagnostic.severity, vue_vet_core::Severity::Info);
    assert!(diagnostic.recommendation.is_some());
    assert!(!diagnostic.affects_score());
    assert!(!diagnostic.affects_exit());
  }

  #[test]
  fn template_alias_model_write_stays_quiet() {
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
            sync_ref_one_way: vec![SyncRefOneWayFact {
              call_span: span(10),
              source_span: span(40),
              sink_span: span(20),
              demand_span: span(50),
              sink_name: "right".into(),
              sink_names: vec!["alias".into(), "right".into()],
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
        surface: "model".into(),
        expression: "alias".into(),
        span: span(0),
        identifiers: Some(vec!["alias".into()]),
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
