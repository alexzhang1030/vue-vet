use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

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

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .template()
      .elements
      .iter()
      .filter(|element| {
        element.tag.eq_ignore_ascii_case("component")
          && element.attribute("is").is_none()
          && element.bound_attribute("is").is_none()
      })
      .map(|element| element.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "dynamic `<component>` requires an `is` binding".into(),
        Some(
          "Add `:is=\"component\"` with a component definition or registered component name."
            .into(),
        ),
      );
    }
  }
}
