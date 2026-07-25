use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

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

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    if !element.tag.eq_ignore_ascii_case("blink") && !element.tag.eq_ignore_ascii_case("marquee") {
      return;
    }
    context.report(
      self.meta(),
      element.span.clone(),
      "distracting animated element is obsolete and inaccessible".into(),
      Some(
        "Use normal content and respect the user's reduced-motion preference for animation.".into(),
      ),
    );
  }
}
