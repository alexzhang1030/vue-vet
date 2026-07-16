use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/security/no-v-html",
  category: "security",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/security/no-v-html",
};

pub(super) struct NoVHtml;

pub(super) static RULE: NoVHtml = NoVHtml;

impl Rule for NoVHtml {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .template()
      .elements
      .iter()
      .filter_map(|element| element.directive("html"))
      .map(|directive| directive.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "`v-html` can render untrusted HTML into the page".into(),
        Some(
          "Prefer normal template interpolation. If raw HTML is required, sanitize it at the trust boundary."
            .into(),
        ),
      );
    }
  }
}
