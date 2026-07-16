use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/anchor-has-content",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/anchor-has-content",
};

pub(super) struct AnchorHasContent;

pub(super) static RULE: AnchorHasContent = AnchorHasContent;

impl Rule for AnchorHasContent {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .template()
      .elements
      .iter()
      .filter(|element| {
        element.tag.eq_ignore_ascii_case("a")
          && !element.has_children
          && element.attribute("aria-label").is_none()
          && element.bound_attribute("aria-label").is_none()
          && element.attribute("aria-labelledby").is_none()
          && element.bound_attribute("aria-labelledby").is_none()
      })
      .map(|element| element.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "link has no accessible content".into(),
        Some("Add visible content or an aria-label/aria-labelledby binding.".into()),
      );
    }
  }
}
