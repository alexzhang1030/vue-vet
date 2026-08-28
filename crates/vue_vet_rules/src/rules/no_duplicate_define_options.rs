use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};
use vue_vet_rule_query::extra_setup_calls;

const META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-duplicate-define-options",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-duplicate-define-options",
};

pub(super) struct NoDuplicateDefineOptions;

pub(super) static RULE: NoDuplicateDefineOptions = NoDuplicateDefineOptions;

impl Rule for NoDuplicateDefineOptions {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for call in extra_setup_calls(context.script(), "defineOptions") {
      context.report(
        self.meta(),
        call.span,
        "`defineOptions` may only be called once in `<script setup>`".into(),
        Some("Merge the declarations into a single `defineOptions` call.".into()),
      );
    }
  }
}
