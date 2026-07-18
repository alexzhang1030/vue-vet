use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/click-events-have-key-events",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/click-events-have-key-events",
};

pub(super) struct ClickEventsHaveKeyEvents;

pub(super) static RULE: ClickEventsHaveKeyEvents = ClickEventsHaveKeyEvents;

impl Rule for ClickEventsHaveKeyEvents {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    const NON_INTERACTIVE: [&str; 6] = ["div", "span", "p", "section", "article", "li"];
    let spans = context
      .template()
      .elements
      .iter()
      .filter(|element| {
        NON_INTERACTIVE.iter().any(|tag| element.tag.eq_ignore_ascii_case(tag))
          && element.event("click").is_some()
          && element.event("keydown").is_none()
          && element.event("keyup").is_none()
          && element.event("keypress").is_none()
      })
      .map(|element| {
        element.event("click").map_or_else(|| element.span.clone(), |event| event.span.clone())
      })
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "clickable non-interactive element has no keyboard handler".into(),
        Some(
          "Prefer a native button or link; otherwise add keyboard behavior and an appropriate role."
            .into(),
        ),
      );
    }
  }
}
