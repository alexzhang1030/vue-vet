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
    let spans: Vec<_> =
      extra_setup_calls(context.script(), "defineSlots").map(|call| call.span.clone()).collect();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "`defineSlots` may only be called once in `<script setup>`".into(),
        Some("Merge the declarations into a single `defineSlots` call.".into()),
      );
    }
  }
}
