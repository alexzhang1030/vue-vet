use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};
use vue_vet_rule_query::{block_calls, setup_blocks};

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
    let prop_bindings: Vec<String> = setup_blocks(context.script())
      .flat_map(|block| {
        block_calls(block, "defineProps").filter_map(|call| call.assigned_to.clone())
      })
      .collect();
    if prop_bindings.is_empty() {
      return;
    }
    let spans: Vec<_> = setup_blocks(context.script())
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
