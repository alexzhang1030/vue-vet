use vue_vet_core::{
  Confidence, FactKinds, FactRef, ReactiveReadKind, Rule, RuleContext, RuleMeta, Severity,
};
use vue_vet_rule_query::binding_path;

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
      // OutsideTracking (nextTick / then / pauseTracking) is owned by tracer_extra rules.
      if read.kind != ReactiveReadKind::AfterAwait {
        continue;
      }
      let binding = binding_path(read);
      context.report(
        self.meta(),
        read.span,
        format!("`{binding}` is read after `await`, so `watchEffect` will not track it"),
        Some(
          "Read every dependency before the first `await`, or use explicit `watch` sources for values needed after async work."
            .into(),
        ),
      );
    }
  }
}
