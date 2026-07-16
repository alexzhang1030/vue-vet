use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/require-v-for-key",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/require-v-for-key",
};

pub(super) struct RequireVForKey;

pub(super) static RULE: RequireVForKey = RequireVForKey;

impl Rule for RequireVForKey {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .template()
      .elements
      .iter()
      .filter_map(|element| {
        element.directive("for").filter(|_| !element.has_key()).map(|directive| directive.span.clone())
      })
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "`v-for` requires a stable `:key`".into(),
        Some(
          "Bind a stable identity from the item; do not use the array index when order can change."
            .into(),
        ),
      );
    }
  }
}
