use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

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

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    if !element.tag.eq_ignore_ascii_case("iframe")
      || element.attribute("title").is_some()
      || element.bound_attribute("title").is_some()
    {
      return;
    }
    context.report(
      self.meta(),
      element.span.clone(),
      "iframe is missing a `title` attribute".into(),
      Some("Add a concise title describing the embedded content.".into()),
    );
  }
}
