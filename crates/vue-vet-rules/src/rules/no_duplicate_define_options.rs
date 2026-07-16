use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, ScriptKind, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/correctness/no-duplicate-define-options",
  category: "correctness",
  default_severity: Severity::Error,
  confidence: Confidence::High,
  documentation: "rules/correctness/no-duplicate-define-options",
};

pub(super) struct NoDuplicateDefineOptions;

pub(super) static RULE: NoDuplicateDefineOptions = NoDuplicateDefineOptions;

impl Rule for NoDuplicateDefineOptions {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .script()
      .blocks
      .iter()
      .filter(|block| block.kind == ScriptKind::Setup)
      .flat_map(|block| block.calls.iter().filter(|call| call.callee == "defineOptions").skip(1))
      .map(|call| call.span.clone())
      .collect::<Vec<_>>();
    for span in spans {
      context.report(
        self.meta(),
        span,
        "`defineOptions` may only be called once in `<script setup>`".into(),
        Some("Merge the declarations into a single `defineOptions` call.".into()),
      );
    }
  }
}
