use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

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

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .template()
      .elements
      .iter()
      .filter_map(|element| {
        element
          .attribute("slot-scope")
          .or_else(|| {
            if element.tag.eq_ignore_ascii_case("template") {
              element.attribute("scope")
            } else {
              None
            }
          })
          .map(|attribute| attribute.span.clone())
      })
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "slot-scope syntax was removed in Vue 3".into(),
        Some("Use v-slot or the # shorthand on <template> or the receiving component.".into()),
      );
    }
  }
}
