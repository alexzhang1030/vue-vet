use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

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

  fn run_on(&self, _fact: FactRef<'_>, _context: &mut RuleContext<'_>) {
    // Vue tracks dynamic dependencies when the guard is itself reactive.
    // Premise withdrawn; rule ID stays for config compatibility.
  }
}
