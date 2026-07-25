use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, ScriptKind, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-mutating-props",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-mutating-props",
};

pub(super) struct NoMutatingProps;

pub(super) static RULE: NoMutatingProps = NoMutatingProps;

impl Rule for NoMutatingProps {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    let prop_bindings: Vec<String> = context
      .script()
      .blocks
      .iter()
      .filter(|block| block.kind == ScriptKind::Setup)
      .flat_map(|block| {
        block
          .calls
          .iter()
          .filter(|call| call.callee == "defineProps")
          .filter_map(|call| call.assigned_to.clone())
      })
      .collect();
    if prop_bindings.is_empty() {
      return;
    }
    let spans: Vec<_> = context
      .script()
      .blocks
      .iter()
      .filter(|block| block.kind == ScriptKind::Setup)
      .flat_map(|block| &block.member_writes)
      .filter(|write| prop_bindings.iter().any(|name| name == &write.object))
      .map(|write| write.span.clone())
      .collect();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "props are readonly and must not be mutated".into(),
        Some("Emit an event or copy the prop into local state owned by this component.".into()),
      );
    }
  }
}
