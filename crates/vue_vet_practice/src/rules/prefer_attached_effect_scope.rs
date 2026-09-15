use vue_vet_core::{
  AttachedEffectScopeFact, Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, Severity,
};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::recommendation_from,
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/prefer-attached-effect-scope",
  documentation: "rules/practice/prefer-attached-effect-scope",
  confidence: Confidence::High,
  min_vue: None,
  recommend: EcosystemApi {
    package: "vue",
    export: "effectScope",
    docs_url: "https://vuejs.org/api/reactivity-advanced.html#effectscope",
    import_example: "import { effectScope } from 'vue'",
  },
};

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
};

pub(super) struct PreferAttachedEffectScope;

pub(super) static RULE: PreferAttachedEffectScope = PreferAttachedEffectScope;

impl Rule for PreferAttachedEffectScope {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.scheduling_practice.attached_effect_scope {
        report_attached(context, site);
      }
    }
  }
}

fn report_attached(context: &mut RuleContext<'_>, site: &AttachedEffectScopeFact) {
  context.report_with_recommendation(
    &META,
    site.detached_span,
    "A detached child scope keeps running while its parent is paused".into(),
    Some(
      "Create the child with `effectScope()` so it joins parent pause and coalesces last-value work. `onScopeDispose(() => child.stop())` still owns disposal. Parent construction, cleanup, pause, and paused-period writes are the causal spans."
        .into(),
    ),
    recommendation_from(RECIPE.recommend),
  );
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use vue_vet_core::{
    AttachedEffectScopeFact, RuleEnvironment, ScriptBlockFacts, ScriptFacts, ScriptKind,
    SourceContractFacts, SourceSpan, TemplateFacts,
  };

  use super::*;
  use crate::practice_registry;

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 4, line: 1, column: offset.saturating_add(1) }
  }

  #[test]
  fn reports_closed_attached_scope_fact() {
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
            attached_effect_scope: vec![AttachedEffectScopeFact {
              detached_span: span(10),
              parent_span: span(20),
              cleanup_span: span(40),
              pause_span: span(60),
              write_span: span(80),
            }],
            ..vue_vet_core::SchedulingPracticeFacts::default()
          },
          ..SourceContractFacts::default()
        },
        reactivity_graph: std::sync::Arc::new(vue_vet_core::ReactivityGraph::default()),
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
