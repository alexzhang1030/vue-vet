use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/require-component-is",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/require-component-is",
};

pub(super) struct RequireComponentIs;

pub(super) static RULE: RequireComponentIs = RequireComponentIs;

impl Rule for RequireComponentIs {
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
    if !element.tag.eq_ignore_ascii_case("component")
      || element.attribute("is").is_some()
      || element.bound_attribute("is").is_some()
    {
      return;
    }
    context.report(
      self.meta(),
      element.span,
      "dynamic `<component>` requires an `is` binding".into(),
      Some(
        "Add `:is=\"component\"` with a component definition or registered component name.".into(),
      ),
    );
  }
}
