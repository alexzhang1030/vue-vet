use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

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
    if element.directive("if").is_none() {
      return;
    }
    context.report(
      self.meta(),
      directive.span.clone(),
      "`v-if` and `v-for` on the same element have surprising precedence".into(),
      Some("Move `v-if` to a wrapping `<template>` or pre-filter the collection.".into()),
    );
  }
}
