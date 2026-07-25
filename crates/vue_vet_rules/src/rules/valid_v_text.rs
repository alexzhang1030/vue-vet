use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/valid-v-text",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/valid-v-text",
};

pub(super) struct ValidVText;

pub(super) static RULE: ValidVText = ValidVText;

impl Rule for ValidVText {
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
    let Some(directive) = element.directive("text") else {
      return;
    };
    let invalid = directive.expression.as_deref().is_none_or(str::is_empty)
      || directive.argument.is_some()
      || !directive.modifiers.is_empty()
      || element.has_children;
    if !invalid {
      return;
    }
    context.report(
      self.meta(),
      directive.span.clone(),
      "invalid `v-text` usage".into(),
      Some(
        "Provide exactly one expression, no argument or modifiers, and no child content.".into(),
      ),
    );
  }
}
