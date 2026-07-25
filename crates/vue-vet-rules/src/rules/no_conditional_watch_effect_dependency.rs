use vue_vet_core::{
  Confidence, FactKinds, FactRef, ReactiveReadKind, Rule, RuleContext, RuleMeta, Severity,
};

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

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::REACTIVITY_EFFECT
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::ReactivityEffect { effect, .. } = fact else {
      return;
    };
    for read in &effect.reads {
      if read.kind != ReactiveReadKind::Conditional {
        continue;
      }
      let already_unconditional = effect.reads.iter().any(|candidate| {
        candidate.kind == ReactiveReadKind::Unconditional
          && candidate.span.offset < read.span.offset
          && candidate.binding == read.binding
          && candidate.property == read.property
      });
      if already_unconditional {
        continue;
      }
      let binding = read
        .property
        .as_ref()
        .map_or_else(|| read.binding.clone(), |property| format!("{}.{property}", read.binding));
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
      context.report(
        self.meta(),
        read.span.clone(),
        format!("`{binding}` is only tracked after the `{guards}` guard passes"),
        Some(
          "If every value must invalidate the effect, use explicit watch sources or read each             dependency before the guard."
            .into(),
        ),
      );
    }
  }
}
