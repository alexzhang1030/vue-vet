use vue_vet_core::{
  Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity, SourceSpan,
  TemplateElementFact,
};

use super::template_attr::{bound_quoted_value_removal_range, quoted_name_value_removal_range};

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

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    if !element_is_focusable(element) {
      return;
    }
    let Some((span, expected_value)) = hidden_true_target(element) else {
      return;
    };
    let message = "focusable element is hidden from assistive technology".into();
    let help =
      Some("Remove aria-hidden, or remove the element from keyboard interaction as well.".into());
    if let Some(range) = quoted_name_value_removal_range(context.source(), span, expected_value)
      .or_else(|| {
        bound_quoted_value_removal_range(context.source(), span, "aria-hidden", expected_value)
      })
    {
      context.report_with_safe_edit(self.meta(), span, message, help, range, String::new());
    } else {
      context.report(self.meta(), span, message, help);
    }
  }
}

fn hidden_true_target(element: &TemplateElementFact) -> Option<(SourceSpan, &str)> {
  if let Some(attribute) = element.attribute("aria-hidden")
    && let Some(value) = attribute.value.as_deref()
    && value.eq_ignore_ascii_case("true")
  {
    return Some((attribute.span, value));
  }
  let directive = element.bound_attribute("aria-hidden")?;
  let expression = directive.expression.as_deref()?;
  (expression == "true").then_some((directive.span, expression))
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
