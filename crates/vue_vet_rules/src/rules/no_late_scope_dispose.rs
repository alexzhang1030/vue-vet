use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-late-scope-dispose",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-late-scope-dispose",
};

pub(super) struct NoLateScopeDispose;
pub(super) static RULE: NoLateScopeDispose = NoLateScopeDispose;

impl Rule for NoLateScopeDispose {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::LATE_SCOPE_DISPOSE
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::LateScopeDispose { fact, .. } = fact else {
      return;
    };
    context.report(
      self.meta(),
      fact.dispose_span,
      "`onScopeDispose` after `await` inside `effectScope().run` has no active scope".into(),
      Some(format!(
        "Register `onScopeDispose` before `await`, or re-enter `scope.run` synchronously. Explicit `failSilently: true` stays quiet because Vue suppresses the runtime warning (callback {}:{}, await {}:{}, owner {}:{}).",
        fact.callback_span.line,
        fact.callback_span.column,
        fact.await_span.line,
        fact.await_span.column,
        fact.owner_span.line,
        fact.owner_span.column
      )),
    );
  }
}
