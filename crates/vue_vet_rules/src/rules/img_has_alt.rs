use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

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

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    if !element.tag.eq_ignore_ascii_case("img")
      || element.attribute("alt").is_some()
      || element.bound_attribute("alt").is_some()
    {
      return;
    }
    context.report(
      self.meta(),
      element.span.clone(),
      "image is missing an `alt` attribute".into(),
      Some("Describe meaningful images, or use alt=\"\" for decorative images.".into()),
    );
  }
}
