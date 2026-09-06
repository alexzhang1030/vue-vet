use vue_vet_core::{
  Confidence, FactKinds, FactRef, LateWatcherCleanupKind, Rule, RuleContext, RuleMeta, Severity,
};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-late-watcher-cleanup",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-late-watcher-cleanup",
};

pub(super) struct NoLateWatcherCleanup;
pub(super) static RULE: NoLateWatcherCleanup = NoLateWatcherCleanup;

impl Rule for NoLateWatcherCleanup {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::LATE_WATCHER_CLEANUP
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::LateWatcherCleanup { fact, .. } = fact else {
      return;
    };
    let api = fact.api.as_str();
    let reason = match fact.kind {
      LateWatcherCleanupKind::Await => "after `await`",
      LateWatcherCleanupKind::DeferredCallback => "inside a deferred callback",
    };
    context.report(
      self.meta(),
      fact.cleanup_span,
      format!("`onWatcherCleanup` {reason} has no active `{api}` to associate with"),
      Some(format!(
        "Register cleanup synchronously, or use the callback-bound `onCleanup` argument which stays valid after `await` (callback {}:{}, boundary {}:{}, registration {}:{}).",
        fact.callback_span.line,
        fact.callback_span.column,
        fact.boundary_span.line,
        fact.boundary_span.column,
        fact.registration_span.line,
        fact.registration_span.column
      )),
    );
  }
}
