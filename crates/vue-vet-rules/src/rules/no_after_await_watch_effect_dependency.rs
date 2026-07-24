use vue_vet_core::{Confidence, ReactiveReadKind, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-after-await-watch-effect-dependency",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-after-await-watch-effect-dependency",
};

pub(super) struct NoAfterAwaitWatchEffectDependency;

pub(super) static RULE: NoAfterAwaitWatchEffectDependency = NoAfterAwaitWatchEffectDependency;

impl Rule for NoAfterAwaitWatchEffectDependency {
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
          .filter(|read| {
            matches!(read.kind, ReactiveReadKind::AfterAwait | ReactiveReadKind::OutsideTracking)
          })
          .map(|read| {
            let binding = read.property.as_ref().map_or_else(
              || read.binding.clone(),
              |property| format!("{}.{property}", read.binding),
            );
            let reason = match read.kind {
              ReactiveReadKind::AfterAwait => "after `await`",
              ReactiveReadKind::OutsideTracking => {
                "inside a deferred callback (`then` / `nextTick` / …)"
              }
              ReactiveReadKind::Unconditional | ReactiveReadKind::Conditional => "outside tracking",
            };
            (read.span.clone(), binding, reason)
          })
          .collect::<Vec<_>>()
      })
      .collect::<Vec<_>>();
    for (span, binding, reason) in reads {
      context.report(
        self.meta(),
        span,
        format!("`{binding}` is read {reason}, so `watchEffect` will not track it"),
        Some(
          "Read every dependency before the first `await`, or use explicit `watch` sources for values needed after async work."
            .into(),
        ),
      );
    }
  }
}
