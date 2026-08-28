use vue_vet_core::{
  Confidence, FactKinds, FactRef, ReactiveReadKind, Rule, RuleContext, RuleMeta, Severity,
  TrackingScopeKind,
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

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TRACKING_SCOPE
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TrackingScope { scope, .. } = fact else {
      return;
    };
    if !matches!(
      scope.kind,
      TrackingScopeKind::WatchEffect
        | TrackingScopeKind::WatchPostEffect
        | TrackingScopeKind::WatchSyncEffect
    ) {
      return;
    }
    if !scope.assignment_only || scope.writes.is_empty() {
      return;
    }
    if scope.reads.is_empty()
      || !scope.reads.iter().all(|read| read.kind == ReactiveReadKind::Unconditional)
    {
      return;
    }
    // Pure derivation: every write target is a ref-like `.value`, and at least one
    // tracked read is not among the written bindings.
    let write_bindings: Vec<&str> =
      scope.writes.iter().map(|write| write.binding.as_str()).collect();
    if !scope.writes.iter().all(|write| write.property.as_deref() == Some("value")) {
      return;
    }
    if !scope.reads.iter().any(|read| !write_bindings.contains(&read.binding.as_str())) {
      return;
    }
    let targets = write_bindings.join("`, `");
    context.report(
      self.meta(),
      scope.span,
      format!("`watchEffect` only assigns `{targets}` from other reactive reads"),
      Some(
        "Use `computed(() => …)` for pure derived state instead of syncing refs in `watchEffect`."
          .into(),
      ),
    );
  }
}
