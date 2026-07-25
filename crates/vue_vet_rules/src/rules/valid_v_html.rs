use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/valid-v-html",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/valid-v-html",
};

pub(super) struct ValidVHtml;

pub(super) static RULE: ValidVHtml = ValidVHtml;

impl Rule for ValidVHtml {
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
    let Some(directive) = element.directive("html") else {
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
      "invalid `v-html` usage".into(),
      Some(
        "Provide exactly one expression, no argument or modifiers, and no child content.".into(),
      ),
    );
  }
}
