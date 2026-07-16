use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

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

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .template()
      .elements
      .iter()
      .filter_map(|element| {
        let directive = element.directive("html")?;
        let invalid = directive.expression.as_deref().is_none_or(str::is_empty)
          || directive.argument.is_some()
          || !directive.modifiers.is_empty()
          || element.has_children;
        invalid.then_some(directive.span.clone())
      })
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "invalid `v-html` usage".into(),
        Some(
          "Provide exactly one expression, no argument or modifiers, and no child content.".into(),
        ),
      );
    }
  }
}
