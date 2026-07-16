use vue_vet_core::{Confidence, ReactiveReadKind, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-conditional-watch-effect-dependency",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-conditional-watch-effect-dependency",
};

pub(super) struct NoConditionalWatchEffectDependency;

pub(super) static RULE: NoConditionalWatchEffectDependency = NoConditionalWatchEffectDependency;

impl Rule for NoConditionalWatchEffectDependency {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let reads = context
      .script()
      .blocks
      .iter()
      .flat_map(|block| &block.reactivity_graph.effects)
      .flat_map(|effect| {
        effect
          .reads
          .iter()
          .filter(|read| read.kind == ReactiveReadKind::Conditional)
          .filter(|read| {
            !effect.reads.iter().any(|candidate| {
              candidate.kind == ReactiveReadKind::Unconditional
                && candidate.span.offset < read.span.offset
                && candidate.binding == read.binding
                && candidate.property == read.property
            })
          })
          .map(|read| {
            let binding = read.property.as_ref().map_or_else(
              || read.binding.clone(),
              |property| format!("{}.{property}", read.binding),
            );
            let guards = read
              .guards
              .iter()
              .map(|guard| {
                guard.property.as_ref().map_or_else(
                  || guard.binding.clone(),
                  |property| format!("{}.{property}", guard.binding),
                )
              })
              .collect::<Vec<_>>()
              .join("`, `");
            (read.span.clone(), binding, guards)
          })
          .collect::<Vec<_>>()
      })
      .collect::<Vec<_>>();
    for (span, binding, guards) in reads {
      context.report(
        self.meta(),
        span,
        format!("`{binding}` is only tracked after the `{guards}` guard passes"),
        Some(
          "If every value must invalidate the effect, use explicit watch sources or read each             dependency before the guard."
            .into(),
        ),
      );
    }
  }
}
