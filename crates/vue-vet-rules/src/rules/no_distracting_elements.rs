use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/no-distracting-elements",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/no-distracting-elements",
};

pub(super) struct NoDistractingElements;

pub(super) static RULE: NoDistractingElements = NoDistractingElements;

impl Rule for NoDistractingElements {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .template()
      .elements
      .iter()
      .filter(|element| {
        element.tag.eq_ignore_ascii_case("blink") || element.tag.eq_ignore_ascii_case("marquee")
      })
      .map(|element| element.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "distracting animated element is obsolete and inaccessible".into(),
        Some(
          "Use normal content and respect the user's reduced-motion preference for animation."
            .into(),
        ),
      );
    }
  }
}
