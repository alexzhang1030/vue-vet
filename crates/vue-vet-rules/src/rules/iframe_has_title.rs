use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/iframe-has-title",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/iframe-has-title",
};

pub(super) struct IframeHasTitle;

pub(super) static RULE: IframeHasTitle = IframeHasTitle;

impl Rule for IframeHasTitle {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .template()
      .elements
      .iter()
      .filter(|element| {
        element.tag.eq_ignore_ascii_case("iframe")
          && element.attribute("title").is_none()
          && element.bound_attribute("title").is_none()
      })
      .map(|element| element.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "iframe is missing a `title` attribute".into(),
        Some("Add a concise title describing the embedded content.".into()),
      );
    }
  }
}
