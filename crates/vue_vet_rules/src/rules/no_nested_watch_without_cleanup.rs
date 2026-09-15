use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-nested-watch-without-cleanup",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-nested-watch-without-cleanup",
};

pub(super) struct NoNestedWatchWithoutCleanup;
pub(super) static RULE: NoNestedWatchWithoutCleanup = NoNestedWatchWithoutCleanup;

impl Rule for NoNestedWatchWithoutCleanup {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::NESTED_WATCH_WITHOUT_CLEANUP
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::NestedWatchWithoutCleanup { fact, .. } = fact else {
      return;
    };
    let api = fact.api.as_str();
    context.report(
      self.meta(),
      fact.inner_span,
      format!(
        "`{api}` created inside a repeating watcher callback is not stopped when that callback ends"
      ),
      Some(format!(
        "Keep the inner stop handle, register it with `onCleanup` / `onWatcherCleanup`, or re-enter an owner `scope.run` (outer {}:{}, source {}:{}). The first immediate run may be scope-owned; later callbacks still leak.",
        fact.outer_span.line,
        fact.outer_span.column,
        fact.source_span.line,
        fact.source_span.column
      )),
    );
  }
}
