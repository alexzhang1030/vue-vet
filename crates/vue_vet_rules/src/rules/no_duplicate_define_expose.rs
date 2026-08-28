use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};
use vue_vet_rule_query::extra_setup_calls;

const META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-duplicate-define-expose",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-duplicate-define-expose",
};

pub(super) struct NoDuplicateDefineExpose;

pub(super) static RULE: NoDuplicateDefineExpose = NoDuplicateDefineExpose;

impl Rule for NoDuplicateDefineExpose {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for call in extra_setup_calls(context.script(), "defineExpose") {
      context.report(
        self.meta(),
        call.span.clone(),
        "`defineExpose` may only be called once in `<script setup>`".into(),
        Some("Merge the declarations into a single `defineExpose` call.".into()),
      );
    }
  }
}
