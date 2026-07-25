use vue_vet_core::{
  Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, ScriptKind, Severity,
};

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

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::SCRIPT_DESTRUCTURE
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::ScriptDestructure { block_kind, destructure } = fact else {
      return;
    };
    if block_kind != ScriptKind::Setup || destructure.source_call != "defineProps" {
      return;
    }
    let Some(version) = context.environment().vue_version else {
      return;
    };
    if version.is_at_least(3, 5) {
      return;
    }
    context.report(
      self.meta(),
      destructure.span.clone(),
      "destructured props are not reactive before Vue 3.5".into(),
      Some(
        "Assign defineProps() to an object, then destructure toRefs(props), or keep property            access through the props object."
          .into(),
      ),
    );
  }
}
