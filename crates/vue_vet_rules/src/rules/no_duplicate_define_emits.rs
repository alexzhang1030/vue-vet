use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};
use vue_vet_rule_query::extra_setup_calls;

const META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-duplicate-define-emits",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-duplicate-define-emits",
};

pub(super) struct NoDuplicateDefineEmits;

pub(super) static RULE: NoDuplicateDefineEmits = NoDuplicateDefineEmits;

impl Rule for NoDuplicateDefineEmits {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    let spans: Vec<_> =
      extra_setup_calls(context.script(), "defineEmits").map(|call| call.span.clone()).collect();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "`defineEmits` may only be called once in `<script setup>`".into(),
        Some("Merge the declarations into a single `defineEmits` call.".into()),
      );
    }
  }
}
