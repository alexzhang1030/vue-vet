use vue_vet_core::{
  Confidence, ReactiveReadKind, Rule, RuleContext, RuleMeta, Severity, TrackingScopeKind,
};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/prefer-computed",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/prefer-computed",
};

pub(super) struct PreferComputed;

pub(super) static RULE: PreferComputed = PreferComputed;

impl Rule for PreferComputed {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let findings = context
      .script()
      .blocks
      .iter()
      .flat_map(|block| &block.reactivity_graph.scopes)
      .filter(|scope| {
        matches!(
          scope.kind,
          TrackingScopeKind::WatchEffect
            | TrackingScopeKind::WatchPostEffect
            | TrackingScopeKind::WatchSyncEffect
        )
      })
      .filter(|scope| scope.assignment_only)
      .filter(|scope| !scope.writes.is_empty())
      .filter(|scope| {
        !scope.reads.is_empty()
          && scope.reads.iter().all(|read| read.kind == ReactiveReadKind::Unconditional)
      })
      .filter(|scope| {
        // Pure derivation: every write target is a ref-like `.value`, and at least one
        // tracked read is not among the written bindings.
        let write_bindings =
          scope.writes.iter().map(|write| write.binding.as_str()).collect::<Vec<_>>();
        scope.writes.iter().all(|write| write.property.as_deref() == Some("value"))
          && scope.reads.iter().any(|read| !write_bindings.contains(&read.binding.as_str()))
      })
      .map(|scope| {
        let targets =
          scope.writes.iter().map(|write| write.binding.as_str()).collect::<Vec<_>>().join("`, `");
        (scope.span.clone(), targets)
      })
      .collect::<Vec<_>>();

    for (span, targets) in findings {
      context.report(
        self.meta(),
        span,
        format!("`watchEffect` only assigns `{targets}` from other reactive reads"),
        Some(
          "Use `computed(() => …)` for pure derived state instead of syncing refs in `watchEffect`."
            .into(),
        ),
      );
    }
  }
}
