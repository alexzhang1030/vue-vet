use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/no-autofocus",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/no-autofocus",
};

pub(super) struct NoAutofocus;

pub(super) static RULE: NoAutofocus = NoAutofocus;

impl Rule for NoAutofocus {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .template()
      .elements
      .iter()
      .filter_map(|element| element.attribute("autofocus"))
      .map(|attribute| attribute.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "autofocus can disorient keyboard and screen-reader users".into(),
        Some(
          "Let users choose focus, or move focus programmatically only after an explicit interaction."
            .into(),
        ),
      );
    }
  }
}
