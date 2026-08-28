use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

use super::template_attr::strip_native_on_modifier;

const META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-deprecated-v-on-native-modifier",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-deprecated-v-on-native-modifier",
};

pub(super) struct NoDeprecatedVOnNativeModifier;

pub(super) static RULE: NoDeprecatedVOnNativeModifier = NoDeprecatedVOnNativeModifier;

impl Rule for NoDeprecatedVOnNativeModifier {
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
    for directive in &element.directives {
      if directive.name != "on" || !directive.modifiers.iter().any(|modifier| modifier == "native")
      {
        continue;
      }
      let message = "the `.native` event modifier was removed in Vue 3".into();
      let help =
        Some("Remove `.native`; undeclared listeners fall through natively in Vue 3.".into());
      // Reconstruct the contiguous `@event.native` / `v-on:event.native` name
      // from Vize's `@` / `v-on` prefix span. Dangling `@` / `v-on:` stays
      // report-only.
      if let Some((range, replacement)) = strip_native_on_modifier(context.source(), directive) {
        context.report_with_safe_edit(
          self.meta(),
          directive.span,
          message,
          help,
          range,
          replacement,
        );
      } else {
        context.report(self.meta(), directive.span, message, help);
      }
    }
  }
}
