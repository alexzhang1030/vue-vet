use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, ScriptKind, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-nonreactive-props-destructure",
  category: "reactivity",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-nonreactive-props-destructure",
};

pub(super) struct NoNonreactivePropsDestructure;

pub(super) static RULE: NoNonreactivePropsDestructure = NoNonreactivePropsDestructure;

impl Rule for NoNonreactivePropsDestructure {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let Some(version) = context.environment().vue_version else {
      return;
    };
    if version.is_at_least(3, 5) {
      return;
    }
    let spans = context
      .script()
      .blocks
      .iter()
      .filter(|block| block.kind == ScriptKind::Setup)
      .flat_map(|block| &block.destructures)
      .filter(|destructure| destructure.source_call == "defineProps")
      .map(|destructure| destructure.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "destructured props are not reactive before Vue 3.5".into(),
        Some(
          "Assign defineProps() to an object, then destructure toRefs(props), or keep property            access through the props object."
            .into(),
        ),
      );
    }
  }
}
