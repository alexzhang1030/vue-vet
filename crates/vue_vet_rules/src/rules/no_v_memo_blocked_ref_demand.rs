//! After-tick demand of a memo-blocked conditional template ref.

use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-v-memo-blocked-ref-demand",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-v-memo-blocked-ref-demand",
};

pub(super) struct NoVMemoBlockedRefDemand;
pub(super) static RULE: NoVMemoBlockedRefDemand = NoVMemoBlockedRefDemand;

impl Rule for NoVMemoBlockedRefDemand {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.template_ref_demands.memo_blocked {
        context.report(
          self.meta(),
          site.demand_span,
          "v-memo reused a first-render subtree, so this template ref was never created".into(),
          Some(format!(
            "Include the condition in the memo tuple or invalidate a memo dependency before this demand. Memo at {}:{}, condition at {}:{}, ref at {}:{}.",
            site.memo_span.line,
            site.memo_span.column,
            site.condition_span.line,
            site.condition_span.column,
            site.ref_span.line,
            site.ref_span.column
          )),
        );
      }
    }
  }
}
