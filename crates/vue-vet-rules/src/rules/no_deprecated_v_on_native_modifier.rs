use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

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

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .template()
      .elements
      .iter()
      .filter_map(|element| {
        element.directives.iter().find(|directive| {
          directive.name == "on" && directive.modifiers.iter().any(|modifier| modifier == "native")
        })
      })
      .map(|directive| directive.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "the `.native` event modifier was removed in Vue 3".into(),
        Some(
          "Declare emitted events on the child component; undeclared listeners fall through natively."
            .into(),
        ),
      );
    }
  }
}
