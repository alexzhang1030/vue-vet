use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/require-v-for-key",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/require-v-for-key",
};

pub(super) struct RequireVForKey;

pub(super) static RULE: RequireVForKey = RequireVForKey;

impl Rule for RequireVForKey {
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
    let Some(directive) = element.directive("for") else {
      return;
    };
    if element.has_key() {
      return;
    }
    context.report(
      self.meta(),
      directive.span,
      "`v-for` requires a stable `:key`".into(),
      Some(
        "Bind a stable identity from the item; do not use the array index when order can change."
          .into(),
      ),
    );
  }
}
