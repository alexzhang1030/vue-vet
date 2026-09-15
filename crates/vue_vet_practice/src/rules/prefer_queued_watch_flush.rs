use vue_vet_core::{
  Confidence, PRACTICE_CATEGORY, QueuedWatchFlushFact, Rule, RuleContext, RuleMeta, Severity,
};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::recommendation_from,
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/prefer-queued-watch-flush",
  documentation: "rules/practice/prefer-queued-watch-flush",
  confidence: Confidence::High,
  min_vue: None,
  recommend: EcosystemApi {
    package: "vue",
    export: "watch",
    docs_url: "https://vuejs.org/api/reactivity-core.html#watch",
    import_example: "import { watch } from 'vue'",
  },
};

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
};

pub(super) struct PreferQueuedWatchFlush;

pub(super) static RULE: PreferQueuedWatchFlush = PreferQueuedWatchFlush;

impl Rule for PreferQueuedWatchFlush {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.scheduling_practice.queued_watch_flush {
        report_queued(context, site);
      }
    }
  }
}

fn report_queued(context: &mut RuleContext<'_>, site: &QueuedWatchFlushFact) {
  context.report_with_recommendation(
    &META,
    site.flush_span,
    "Synchronous `flush: 'sync'` runs this last-value watcher once per write in the same tick"
      .into(),
    Some(format!(
      "`{name}` is only read after `await nextTick()`. Default `pre` flush keeps that later value and reduces callback work from 2 to 1. The clustered writes and after-flush demand are the causal spans. Keep `immediate` if the watcher already uses it.",
      name = site.sink_name
    )),
    recommendation_from(RECIPE.recommend),
  );
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use vue_vet_core::{
    QueuedWatchFlushFact, RuleEnvironment, ScriptBlockFacts, ScriptFacts, ScriptKind,
    SourceContractFacts, SourceSpan, TemplateFacts,
  };

  use super::*;
  use crate::practice_registry;

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 8, line: 1, column: offset.saturating_add(1) }
  }

  #[test]
  fn reports_closed_queued_flush_fact() {
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
            queued_watch_flush: vec![QueuedWatchFlushFact {
              flush_span: span(10),
              write_span: span(40),
              demand_span: span(80),
              sink_name: "sink".into(),
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
    assert_eq!(diagnostic.severity, vue_vet_core::Severity::Info);
    assert!(diagnostic.recommendation.is_some());
    assert!(!diagnostic.affects_score());
    assert!(!diagnostic.affects_exit());
  }
}
