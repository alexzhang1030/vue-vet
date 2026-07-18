use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-v-if-with-v-for",
  category: "correctness",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-v-if-with-v-for",
};

pub(super) struct NoVIfWithVFor;

pub(super) static RULE: NoVIfWithVFor = NoVIfWithVFor;

impl Rule for NoVIfWithVFor {
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
          .directive("for")
          .filter(|_| element.directive("if").is_some())
          .map(|directive| directive.span.clone())
      })
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "`v-if` and `v-for` on the same element have surprising precedence".into(),
        Some("Move `v-if` to a wrapping `<template>` or pre-filter the collection.".into()),
      );
    }
  }
}
