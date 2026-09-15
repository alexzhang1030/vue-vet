use vue_vet_core::{
  Confidence, FactKinds, FactRef, LateCancellationGuardKind, Rule, RuleContext, RuleMeta, Severity,
};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-late-cancellation-guard",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-late-cancellation-guard",
};

pub(super) struct NoLateCancellationGuard;
pub(super) static RULE: NoLateCancellationGuard = NoLateCancellationGuard;

impl Rule for NoLateCancellationGuard {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::LATE_CANCELLATION_GUARD
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::LateCancellationGuard { fact, .. } = fact else {
      return;
    };
    let LateCancellationGuardKind::AfterSourceDependentAwait = fact.kind;
    context.report(
      self.meta(),
      fact.registration_span,
      format!(
        "Cancellation guard for `{}` is registered after a source-dependent `await`, so an earlier invalidation can still write this run's result",
        fact.api.as_str()
      ),
      Some(format!(
        "Register the callback-bound `onCleanup` that sets the flag before `await`, then write only if the flag is clear (await {}:{}, write {}:{}, flag {}:{}).",
        fact.await_span.line,
        fact.await_span.column,
        fact.write_span.line,
        fact.write_span.column,
        fact.flag_span.line,
        fact.flag_span.column
      )),
    );
  }
}
