use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/img-has-alt",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/img-has-alt",
};

pub(super) struct ImgHasAlt;

pub(super) static RULE: ImgHasAlt = ImgHasAlt;

impl Rule for ImgHasAlt {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .template()
      .elements
      .iter()
      .filter(|element| {
        element.tag.eq_ignore_ascii_case("img")
          && element.attribute("alt").is_none()
          && element.bound_attribute("alt").is_none()
      })
      .map(|element| element.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "image is missing an `alt` attribute".into(),
        Some("Describe meaningful images, or use alt=\"\" for decorative images.".into()),
      );
    }
  }
}
