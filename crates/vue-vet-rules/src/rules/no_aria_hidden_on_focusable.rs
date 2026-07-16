use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity, TemplateElementFact};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/no-aria-hidden-on-focusable",
  category: "accessibility",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/accessibility/no-aria-hidden-on-focusable",
};

pub(super) struct NoAriaHiddenOnFocusable;

pub(super) static RULE: NoAriaHiddenOnFocusable = NoAriaHiddenOnFocusable;

impl Rule for NoAriaHiddenOnFocusable {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .template()
      .elements
      .iter()
      .filter_map(|element| {
        let span = element
          .attribute("aria-hidden")
          .filter(|attribute| {
            attribute.value.as_deref().is_some_and(|value| value.eq_ignore_ascii_case("true"))
          })
          .map(|attribute| &attribute.span)
          .or_else(|| {
            element
              .bound_attribute("aria-hidden")
              .filter(|directive| {
                directive.expression.as_deref().is_some_and(|value| value == "true")
              })
              .map(|directive| &directive.span)
          })?;
        element_is_focusable(element).then_some(span.clone())
      })
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "focusable element is hidden from assistive technology".into(),
        Some(
          "Remove aria-hidden, or remove the element from keyboard interaction as well.".into(),
        ),
      );
    }
  }
}

fn element_is_focusable(element: &TemplateElementFact) -> bool {
  if element.attribute("disabled").is_some() {
    return false;
  }
  let native = match element.tag.to_ascii_lowercase().as_str() {
    "a" => element.attribute("href").is_some() || element.bound_attribute("href").is_some(),
    "button" | "select" | "textarea" => true,
    "input" => element
      .attribute("type")
      .and_then(|attribute| attribute.value.as_deref())
      .is_none_or(|kind| !kind.eq_ignore_ascii_case("hidden")),
    _ => false,
  };
  native
    || element
      .attribute("tabindex")
      .and_then(|attribute| attribute.value.as_deref())
      .and_then(|value| value.trim().parse::<i32>().ok())
      .is_some_and(|value| value >= 0)
}
