use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};
use vue_vet_rule_query::extra_setup_calls;

const META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-duplicate-define-props",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-duplicate-define-props",
};

pub(super) struct NoDuplicateDefineProps;

pub(super) static RULE: NoDuplicateDefineProps = NoDuplicateDefineProps;

impl Rule for NoDuplicateDefineProps {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for call in extra_setup_calls(context.script(), "defineProps") {
      context.report(
        self.meta(),
        call.span.clone(),
        "`defineProps` may only be called once in `<script setup>`".into(),
        Some("Merge the declarations into a single `defineProps` call.".into()),
      );
    }
  }
}
