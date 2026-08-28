use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/security/no-v-html",
  category: "security",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/security/no-v-html",
};

pub(super) struct NoVHtml;

pub(super) static RULE: NoVHtml = NoVHtml;

impl Rule for NoVHtml {
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
    context.report(
      self.meta(),
      directive.span,
      "`v-html` can render untrusted HTML into the page".into(),
      Some(
        "Prefer normal template interpolation. If raw HTML is required, sanitize it at the trust boundary."
          .into(),
      ),
    );
  }
}
