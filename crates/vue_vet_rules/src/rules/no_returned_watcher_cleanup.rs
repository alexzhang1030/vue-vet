use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-returned-watcher-cleanup",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-returned-watcher-cleanup",
};

pub(super) struct NoReturnedWatcherCleanup;
pub(super) static RULE: NoReturnedWatcherCleanup = NoReturnedWatcherCleanup;

impl Rule for NoReturnedWatcherCleanup {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::RETURNED_WATCHER_CLEANUP
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::ReturnedWatcherCleanup { fact, .. } = fact else {
      return;
    };
    let api = fact.api.as_str();
    context.report(
      self.meta(),
      fact.returned_span,
      format!("`{api}` ignores a returned function; it is not registered as watcher cleanup"),
      Some(format!(
        "Register cleanup with `onCleanup(...)` or `onWatcherCleanup(...)` instead of returning a function from the callback (callback {}:{}, registration {}:{}).",
        fact.callback_span.line,
        fact.callback_span.column,
        fact.registration_span.line,
        fact.registration_span.column
      )),
    );
  }
}
