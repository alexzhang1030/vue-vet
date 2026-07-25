use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/button-has-content",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/button-has-content",
};

pub(super) struct ButtonHasContent;

pub(super) static RULE: ButtonHasContent = ButtonHasContent;

impl Rule for ButtonHasContent {
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
    if !element.tag.eq_ignore_ascii_case("button")
      || element.has_children
      || element.attribute("aria-label").is_some()
      || element.bound_attribute("aria-label").is_some()
      || element.attribute("aria-labelledby").is_some()
      || element.bound_attribute("aria-labelledby").is_some()
    {
      return;
    }
    context.report(
      self.meta(),
      element.span.clone(),
      "button has no accessible content".into(),
      Some("Add visible content or an aria-label/aria-labelledby binding.".into()),
    );
  }
}
