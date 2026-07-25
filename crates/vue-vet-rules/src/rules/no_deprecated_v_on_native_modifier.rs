use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

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
    let Some(directive) = element.directives.iter().find(|directive| {
      directive.name == "on" && directive.modifiers.iter().any(|modifier| modifier == "native")
    }) else {
      return;
    };
    context.report(
      self.meta(),
      directive.span.clone(),
      "the `.native` event modifier was removed in Vue 3".into(),
      Some(
        "Declare emitted events on the child component; undeclared listeners fall through natively."
          .into(),
      ),
    );
  }
}
