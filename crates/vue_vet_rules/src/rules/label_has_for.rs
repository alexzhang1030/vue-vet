use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/label-has-for",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/label-has-for",
};

pub(super) struct LabelHasFor;

pub(super) static RULE: LabelHasFor = LabelHasFor;

impl Rule for LabelHasFor {
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
    if !element.tag.eq_ignore_ascii_case("label")
      || element.attribute("for").is_some()
      || element.bound_attribute("for").is_some()
      || element.has_labelable_descendant
    {
      return;
    }
    context.report(
      self.meta(),
      element.span.clone(),
      "label has no associated control".into(),
      Some(
        "Add a `for`/`:for` binding that matches a control `id`, or nest an input/select/textarea/button inside the label."
          .into(),
      ),
    );
  }
}
