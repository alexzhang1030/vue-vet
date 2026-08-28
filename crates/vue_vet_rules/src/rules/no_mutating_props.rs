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
    let script = context.script();
    let prop_bindings: Vec<&str> = setup_blocks(script)
      .flat_map(|block| {
        block_calls(block, "defineProps").filter_map(|call| call.assigned_to.as_deref())
      })
      .collect();
    if prop_bindings.is_empty() {
      return;
    }
    for write in setup_blocks(script).flat_map(|block| block.member_writes.iter()) {
      if prop_bindings.iter().any(|name| write.object == *name) {
        context.report(
          self.meta(),
          write.span.clone(),
          "props are readonly and must not be mutated".into(),
          Some("Emit an event or copy the prop into local state owned by this component.".into()),
        );
      }
    }
  }
}
