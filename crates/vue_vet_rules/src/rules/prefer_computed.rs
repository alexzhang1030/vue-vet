use vue_vet_core::{
  Confidence, FactKinds, FactRef, ReactiveBindingKind, ReactiveReadKind, ReactiveWriteFact, Rule,
  RuleContext, RuleMeta, ScriptBlockFacts, ScriptOperandFact, Severity, TemplateFacts,
  TrackingScopeFact, TrackingScopeKind,
};
use vue_vet_rule_query::{
  alias_root, reactive_binding_for_operand, same_reactive_target, script_block,
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
    if !scope.writes.iter().all(|write| {
      write.property.as_deref() == Some("value") && target_is_ordinary_owned_ref(block, write)
    }) {
      return;
    }
    if !scope.reads.iter().any(|read| !write_bindings.contains(&read.binding.as_str())) {
      return;
    }
    if scope
      .writes
      .iter()
      .any(|write| target_has_mutable_role(context, block, scope, write.binding.as_str()))
    {
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

fn target_is_ordinary_owned_ref(block: &ScriptBlockFacts, write: &ReactiveWriteFact) -> bool {
  let Some(binding_span) = write.binding_span else {
    return false;
  };
  let operand = ScriptOperandFact {
    name: write.binding.clone(),
    span: write.span,
    binding_span: Some(binding_span),
  };
  let Some(written) = reactive_binding_for_operand(block, &operand) else {
    return false;
  };
  let root = match (written.alias_of.as_deref(), written.alias_of_span) {
    (Some(alias_name), Some(alias_span)) => block
      .reactivity_graph
      .bindings
      .iter()
      .find(|binding| binding.name == alias_name && binding.span.offset == alias_span.offset),
    (Some(_), None) => None,
    (None, _) => Some(written),
  };
  root.is_some_and(|binding| {
    matches!(binding.kind, ReactiveBindingKind::Ref | ReactiveBindingKind::ShallowRef)
  })
}

fn target_has_mutable_role(
  context: &RuleContext<'_>,
  block: &ScriptBlockFacts,
  scope: &TrackingScopeFact,
  name: &str,
) -> bool {
  let bindings = &block.reactivity_graph.bindings;
  let root = alias_root(bindings, name);
  let aliases: Vec<&str> = bindings
    .iter()
    .filter(|binding| binding.name == root || binding.alias_of.as_deref() == Some(root))
    .map(|binding| binding.name.as_str())
    .collect();
  // Missing graph identity is not proof of a private owned local. Ref
  // parameters and other scoped bindings can be omitted from the top-level
  // list while tracking-scope writes still name them.
  if aliases.is_empty() {
    return true;
  }
  let is_target = |candidate: &str| aliases.contains(&candidate);

  if block
    .bindings
    .iter()
    .any(|binding| is_target(&binding.name) && (binding.exported || binding.escaped))
  {
    return true;
  }
  if block.calls.iter().any(|call| call.argument_identifiers.iter().any(|arg| is_target(arg))) {
    return true;
  }
  let scope_write_offsets: Vec<usize> =
    scope.writes.iter().map(|write| write.span.offset).collect();
  if block
    .member_writes
    .iter()
    .any(|write| is_target(&write.object) && !scope_write_offsets.contains(&write.span.offset))
  {
    return true;
  }
  for other in &block.reactivity_graph.scopes {
    if other.span.offset == scope.span.offset {
      continue;
    }
    if other.writes.iter().any(|write| is_target(&write.binding)) {
      return true;
    }
  }
  if event_handler_uses_target(context.template(), |name| is_target(name)) {
    return true;
  }
  for element in &context.template().elements {
    if element.directive("model").is_some_and(|model| {
      model.expression.as_deref().is_some_and(|expression| is_target(expression.trim()))
    }) {
      return true;
    }
  }
  false
}

fn event_handler_uses_target(template: &TemplateFacts, is_target: impl Fn(&str) -> bool) -> bool {
  template.expressions.iter().any(|expression| {
    expression.surface == "on"
      && expression
        .identifiers
        .as_deref()
        .is_some_and(|names| names.iter().any(|ident| is_target(ident)))
  })
}
