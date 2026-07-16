use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/no-positive-tabindex",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/no-positive-tabindex",
};

pub(super) struct NoPositiveTabindex;

pub(super) static RULE: NoPositiveTabindex = NoPositiveTabindex;

impl Rule for NoPositiveTabindex {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .template()
      .elements
      .iter()
      .filter_map(|element| element.attribute("tabindex"))
      .filter(|attribute| {
        attribute
          .value
          .as_deref()
          .and_then(|value| value.trim().parse::<i32>().ok())
          .is_some_and(|value| value > 0)
      })
      .map(|attribute| attribute.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "positive tabindex creates a surprising keyboard navigation order".into(),
        Some(
          "Use tabindex=\"0\" to join the natural order or tabindex=\"-1\" for programmatic focus."
            .into(),
        ),
      );
    }
  }
}
