use vue_vet_core::{
  Confidence, FactKinds, FactRef, Rule, RuleContext, RuleGroupId, RuleMeta, Severity,
};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-watch-cleanup-current-source",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-watch-cleanup-current-source",
  group: Some(RuleGroupId::Lifetime),
};

pub(super) struct NoWatchCleanupCurrentSource;
pub(super) static RULE: NoWatchCleanupCurrentSource = NoWatchCleanupCurrentSource;

impl Rule for NoWatchCleanupCurrentSource {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::WATCH_CLEANUP_CURRENT_SOURCE
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::WatchCleanupCurrentSource { fact, .. } = fact else {
      return;
    };
    context.report(
      self.meta(),
      fact.release_span,
      "Watcher cleanup removes the listener from the current source value, not the EventTarget acquired in this run".into(),
      Some(format!(
        "Acquisition at {}:{}, source replacement at {}:{}. Capture the run-local target and call `removeEventListener` on that identity.",
        fact.acquisition_span.line,
        fact.acquisition_span.column,
        fact.replacement_span.line,
        fact.replacement_span.column
      )),
    );
  }
}
