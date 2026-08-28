use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/no-positive-tabindex",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/no-positive-tabindex",
};

pub(super) struct NoPositiveTabindex;

pub(super) static RULE: NoPositiveTabindex = NoPositiveTabindex;

impl Rule for NoPositiveTabindex {
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
    let Some(attribute) = element.attribute("tabindex") else {
      return;
    };
    let positive = attribute
      .value
      .as_deref()
      .and_then(|value| value.trim().parse::<i32>().ok())
      .is_some_and(|value| value > 0);
    if !positive {
      return;
    }
    context.report(
      self.meta(),
      attribute.span,
      "positive tabindex creates a surprising keyboard navigation order".into(),
      Some(
        "Use tabindex=\"0\" to join the natural order or tabindex=\"-1\" for programmatic focus."
          .into(),
      ),
    );
  }
}
