use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-orphaned-scope-watcher",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-orphaned-scope-watcher",
};

pub(super) struct NoOrphanedScopeWatcher;
pub(super) static RULE: NoOrphanedScopeWatcher = NoOrphanedScopeWatcher;

impl Rule for NoOrphanedScopeWatcher {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::ORPHANED_SCOPE_WATCHER
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::OrphanedScopeWatcher { fact, .. } = fact else {
      return;
    };
    let api = fact.api.as_str();
    context.report(
      self.meta(),
      fact.watcher_span,
      format!(
        "`{api}` created after `await` inside `effectScope().run` is not owned by that scope"
      ),
      Some(format!(
        "Create the watcher synchronously, re-enter `scope.run` synchronously, or keep the stop handle (owner {}:{}, run {}:{}, await {}:{}).",
        fact.owner_span.line,
        fact.owner_span.column,
        fact.run_span.line,
        fact.run_span.column,
        fact.await_span.line,
        fact.await_span.column
      )),
    );
  }
}
