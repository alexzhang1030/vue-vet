use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};
use vue_vet_rule_query::{binding_path, guard_path, unguarded_conditional_reads};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-conditional-watch-effect-dependency",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-conditional-watch-effect-dependency",
};

pub(super) struct NoConditionalWatchEffectDependency;

pub(super) static RULE: NoConditionalWatchEffectDependency = NoConditionalWatchEffectDependency;

impl Rule for NoConditionalWatchEffectDependency {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::REACTIVITY_EFFECT
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::ReactivityEffect { effect, .. } = fact else {
      return;
    };
    for read in unguarded_conditional_reads(&effect.reads) {
      let binding = binding_path(read);
      let guards = read.guards.iter().map(guard_path).collect::<Vec<_>>().join("`, `");
      context.report(
        self.meta(),
        read.span.clone(),
        format!("`{binding}` is only tracked after the `{guards}` guard passes"),
        Some(
          "If every value must invalidate the effect, use explicit watch sources or read each             dependency before the guard."
            .into(),
        ),
      );
    }
  }
}
