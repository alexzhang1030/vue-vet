use vue_vet_core::{
  Confidence, FactKinds, FactRef, ReactiveReadKind, Rule, RuleContext, RuleMeta, Severity,
  TrackingScopeKind,
};
use vue_vet_rule_query::{same_reactive_target, script_block};

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
    let FactRef::TrackingScope { scope, block_kind } = fact else {
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
    if !scope_coverage_complete(scope) {
      return;
    }
    if scope.reads.is_empty()
      || !scope.reads.iter().all(|read| read.kind == ReactiveReadKind::Unconditional)
    {
      return;
    }
    let Some(block) = script_block(context.script(), block_kind) else {
      return;
    };
    let bindings = &block.reactivity_graph.bindings;
    if scope
      .reads
      .iter()
      .any(|read| scope.writes.iter().any(|write| same_reactive_target(bindings, read, write)))
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

const fn scope_coverage_complete(scope: &vue_vet_core::TrackingScopeFact) -> bool {
  scope.unknown_calls.is_empty() && scope.uncertain_accesses.is_empty() && !scope.follow_truncated
}
