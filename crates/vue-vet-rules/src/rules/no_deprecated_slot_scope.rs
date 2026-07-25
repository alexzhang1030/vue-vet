use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-deprecated-slot-scope",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-deprecated-slot-scope",
};

pub(super) struct NoDeprecatedSlotScope;

pub(super) static RULE: NoDeprecatedSlotScope = NoDeprecatedSlotScope;

impl Rule for NoDeprecatedSlotScope {
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
    let attribute = element.attribute("slot-scope").or_else(|| {
      if element.tag.eq_ignore_ascii_case("template") { element.attribute("scope") } else { None }
    });
    let Some(attribute) = attribute else {
      return;
    };
    context.report(
      self.meta(),
      attribute.span.clone(),
      "slot-scope syntax was removed in Vue 3".into(),
      Some("Use v-slot or the # shorthand on <template> or the receiving component.".into()),
    );
  }
}
