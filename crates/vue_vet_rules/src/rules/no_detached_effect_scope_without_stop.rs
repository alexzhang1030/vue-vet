use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-detached-effect-scope-without-stop",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-detached-effect-scope-without-stop",
};

pub(super) struct NoDetachedEffectScopeWithoutStop;
pub(super) static RULE: NoDetachedEffectScopeWithoutStop = NoDetachedEffectScopeWithoutStop;

impl Rule for NoDetachedEffectScopeWithoutStop {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::DETACHED_EFFECT_SCOPE_WITHOUT_STOP
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::DetachedEffectScopeWithoutStop { fact, .. } = fact else {
      return;
    };
    context.report(
      self.meta(),
      fact.scope_span,
      "detached `effectScope(true)` creates a live watcher but is discarded without `stop`"
        .to_string(),
      Some(format!(
        "Return a disposer that calls `scope.stop()`, or keep the scope and stop it from owner cleanup (run {}:{}, watcher {}:{}, source {}:{}, outer {}:{}).",
        fact.run_span.line,
        fact.run_span.column,
        fact.watcher_span.line,
        fact.watcher_span.column,
        fact.source_span.line,
        fact.source_span.column,
        fact.outer_span.line,
        fact.outer_span.column
      )),
    );
  }
}
