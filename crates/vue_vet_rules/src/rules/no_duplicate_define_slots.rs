use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};
use vue_vet_rule_query::extra_setup_calls;

const META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-duplicate-define-slots",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-duplicate-define-slots",
};

pub(super) struct NoDuplicateDefineSlots;

pub(super) static RULE: NoDuplicateDefineSlots = NoDuplicateDefineSlots;

impl Rule for NoDuplicateDefineSlots {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for call in extra_setup_calls(context.script(), "defineSlots") {
      context.report(
        self.meta(),
        call.span,
        "`defineSlots` may only be called once in `<script setup>`".into(),
        Some("Merge the declarations into a single `defineSlots` call.".into()),
      );
    }
  }
}
