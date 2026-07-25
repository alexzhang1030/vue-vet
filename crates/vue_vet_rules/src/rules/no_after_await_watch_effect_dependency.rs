use vue_vet_core::{
  Confidence, FactKinds, FactRef, ReactiveReadKind, Rule, RuleContext, RuleMeta, Severity,
};

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

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::REACTIVITY_EFFECT
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::ReactivityEffect { effect, .. } = fact else {
      return;
    };
    for read in &effect.reads {
      if !matches!(read.kind, ReactiveReadKind::AfterAwait | ReactiveReadKind::OutsideTracking) {
        continue;
      }
      let binding = read
        .property
        .as_ref()
        .map_or_else(|| read.binding.clone(), |property| format!("{}.{property}", read.binding));
      let reason = match read.kind {
        ReactiveReadKind::AfterAwait => "after `await`",
        ReactiveReadKind::OutsideTracking => "inside a deferred callback (`then` / `nextTick` / …)",
        ReactiveReadKind::Unconditional | ReactiveReadKind::Conditional => "outside tracking",
      };
      context.report(
        self.meta(),
        read.span.clone(),
        format!("`{binding}` is read {reason}, so `watchEffect` will not track it"),
        Some(
          "Read every dependency before the first `await`, or use explicit `watch` sources for values needed after async work."
            .into(),
        ),
      );
    }
  }
}
