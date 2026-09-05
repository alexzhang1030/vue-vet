//! Additional reactivity / behavior-clarify rules beyond the generated matrix.

use std::collections::{BTreeMap, BTreeSet};

use vue_vet_core::{
  Confidence, FactKinds, FactRef, ReactiveBindingKind, ReactiveDependencyKind, Rule, RuleContext,
  RuleMeta, Severity,
};

use vue_vet_rule_query::{effect_family, member_path, script_binding_at, used_reactive_names};

const MULTI_EFFECT_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-multiple-effects-same-target",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-multiple-effects-same-target",
};

pub(super) struct NoMultipleEffectsSameTarget;
pub(super) static NO_MULTIPLE_EFFECTS_SAME_TARGET: NoMultipleEffectsSameTarget =
  NoMultipleEffectsSameTarget;

impl Rule for NoMultipleEffectsSameTarget {
  fn meta(&self) -> &'static RuleMeta {
    &MULTI_EFFECT_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      let mut writers: BTreeMap<(&str, Option<&str>), usize> = BTreeMap::new();
      let mut counts: BTreeMap<(&str, Option<&str>), usize> = BTreeMap::new();
      for scope in &block.reactivity_graph.scopes {
        if !effect_family(scope.kind) {
          continue;
        }
        for write in &scope.writes {
          let key = (write.binding.as_str(), write.property.as_deref());
          *counts.entry(key).or_insert(0) += 1;
          writers.entry(key).or_insert(write.span.offset);
        }
      }
      for (key, count) in counts {
        if count < 2 {
          continue;
        }
        let Some(&offset) = writers.get(&key) else {
          continue;
        };
        let Some(write) = block
          .reactivity_graph
          .scopes
          .iter()
          .flat_map(|scope| scope.writes.iter())
          .find(|write| write.span.offset == offset)
        else {
          continue;
        };
        let path = member_path(key.0, key.1);
        context.report(
          self.meta(),
          write.span,
          format!("multiple effects write `{path}`, which races updates to the same target"),
          Some("Keep a single writer effect, or merge the updates into one scope.".into()),
        );
      }
    }
  }
}

const PROPS_SNAPSHOT_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-props-snapshot-in-ref",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-props-snapshot-in-ref",
};

pub(super) struct NoPropsSnapshotInRef;
pub(super) static NO_PROPS_SNAPSHOT_IN_REF: NoPropsSnapshotInRef = NoPropsSnapshotInRef;

impl Rule for NoPropsSnapshotInRef {
  fn meta(&self) -> &'static RuleMeta {
    &PROPS_SNAPSHOT_META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::SCRIPT_CALL
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::ScriptCall { call, .. } = fact else {
      return;
    };
    if !matches!(call.callee.as_str(), "ref" | "shallowRef") {
      return;
    }
    if !call.argument_identifiers.iter().any(|name| name == "props") {
      return;
    }
    context.report(
      self.meta(),
      call.span,
      format!("`{}(props…)` snapshots props and loses reactivity", call.callee),
      Some(
        "Use `toRef(props, 'field')`, `toRefs(props)`, or read `props.field` inside computed/watch."
          .into(),
      ),
    );
  }
}

const VMODEL_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-v-model-nonreactive-source",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-v-model-nonreactive-source",
};

pub(super) struct NoVModelNonreactiveSource;
pub(super) static NO_V_MODEL_NONREACTIVE_SOURCE: NoVModelNonreactiveSource =
  NoVModelNonreactiveSource;

impl Rule for NoVModelNonreactiveSource {
  fn meta(&self) -> &'static RuleMeta {
    &VMODEL_META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    let Some(model) = element.directive("model") else {
      return;
    };
    let Some(expression) = model.expression.as_deref() else {
      return;
    };
    let name = expression.trim();
    if !is_simple_ident(name) {
      return;
    }
    let reactive = context
      .script()
      .blocks
      .iter()
      .any(|block| block.reactivity_graph.bindings.iter().any(|binding| binding.name == name));
    if reactive {
      return;
    }
    let known_script = context
      .script()
      .blocks
      .iter()
      .any(|block| block.bindings.iter().any(|binding| binding.name == name));
    if !known_script {
      return;
    }
    context.report(
      self.meta(),
      model.span,
      format!("`v-model=\"{name}\"` binds a non-reactive script value"),
      Some(format!("Make `{name}` a `ref` / `computed`, or bind a reactive property.")),
    );
  }
}

const STALE_PROP_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-stale-prop-flow",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-stale-prop-flow",
};

pub(super) struct NoStalePropFlow;
pub(super) static NO_STALE_PROP_FLOW: NoStalePropFlow = NoStalePropFlow;

impl Rule for NoStalePropFlow {
  fn meta(&self) -> &'static RuleMeta {
    &STALE_PROP_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      let reactive_names: BTreeSet<&str> =
        block.reactivity_graph.bindings.iter().map(|binding| binding.name.as_str()).collect();
      for edge in &block.reactivity_graph.edges {
        if edge.kind != ReactiveDependencyKind::Prop {
          continue;
        }
        if reactive_names.contains(edge.from.as_str()) {
          continue;
        }
        if edge.from.contains(':')
          || edge.from.starts_with("effect")
          || edge.from.starts_with("watch_")
          || edge.from.starts_with("computed")
        {
          continue;
        }
        context.report(
          self.meta(),
          edge.span,
          format!(
            "prop flow `{}` → `{}` does not start from a reactive binding",
            edge.from,
            edge.to_path()
          ),
          Some(
            "Pass a ref/computed/reactive field, or read the source inside a tracking scope."
              .into(),
          ),
        );
      }
    }
  }
}

const UNUSED_COMPUTED_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-unused-computed-binding",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-unused-computed-binding",
};

pub(super) struct NoUnusedComputedBinding;
pub(super) static NO_UNUSED_COMPUTED_BINDING: NoUnusedComputedBinding = NoUnusedComputedBinding;

impl Rule for NoUnusedComputedBinding {
  fn meta(&self) -> &'static RuleMeta {
    &UNUSED_COMPUTED_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      let used = used_reactive_names(&block.reactivity_graph);
      for binding in &block.reactivity_graph.bindings {
        if binding.kind != ReactiveBindingKind::Computed || used.contains(binding.name.as_str()) {
          continue;
        }
        let Some(local) = script_binding_at(block, &binding.name, binding.span) else {
          continue;
        };
        if local.exported {
          continue;
        }
        if local.reads != 0 {
          continue;
        }
        context.report(
          self.meta(),
          binding.span,
          "computed binding is never read in script or template".into(),
          Some("Remove the unused computed, or read it from template / another scope.".into()),
        );
      }
    }
  }
}

const PREFER_EXPLICIT_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/prefer-explicit-sources-for-conditional-deps",
  category: "reactivity",
  default_severity: Severity::Info,
  confidence: Confidence::High,
  documentation: "rules/reactivity/prefer-explicit-sources-for-conditional-deps",
};

pub(super) struct PreferExplicitSourcesForConditionalDeps;
pub(super) static PREFER_EXPLICIT_SOURCES_FOR_CONDITIONAL_DEPS:
  PreferExplicitSourcesForConditionalDeps = PreferExplicitSourcesForConditionalDeps;

impl Rule for PreferExplicitSourcesForConditionalDeps {
  fn meta(&self) -> &'static RuleMeta {
    &PREFER_EXPLICIT_META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TRACKING_SCOPE
  }

  fn run_on(&self, _fact: FactRef<'_>, _context: &mut RuleContext<'_>) {
    // Same withdrawn premise as the conditional-dependency family: Vue tracks
    // dynamic deps. ID stays for config compatibility.
  }
}

macro_rules! macro_after_await {
  ($type_name:ident, $static_name:ident, $id:literal, $doc:literal, $callee:literal) => {
    pub(super) struct $type_name;
    pub(super) static $static_name: $type_name = $type_name;

    impl Rule for $type_name {
      fn meta(&self) -> &'static RuleMeta {
        &RuleMeta {
          id: $id,
          category: "correctness",
          default_severity: Severity::Warning,
          confidence: Confidence::High,
          documentation: $doc,
        }
      }

      fn run_once(&self, _context: &mut RuleContext<'_>) {
        // Compiler macros are hoisted; source position after await is not a defect.
      }
    }
  };
}

macro_after_await!(
  NoDefinePropsAfterAwait,
  NO_DEFINE_PROPS_AFTER_AWAIT,
  "vue-vet/correctness/no-define-props-after-await",
  "rules/correctness/no-define-props-after-await",
  "defineProps"
);
macro_after_await!(
  NoDefineEmitsAfterAwait,
  NO_DEFINE_EMITS_AFTER_AWAIT,
  "vue-vet/correctness/no-define-emits-after-await",
  "rules/correctness/no-define-emits-after-await",
  "defineEmits"
);
macro_after_await!(
  NoDefineModelAfterAwait,
  NO_DEFINE_MODEL_AFTER_AWAIT,
  "vue-vet/correctness/no-define-model-after-await",
  "rules/correctness/no-define-model-after-await",
  "defineModel"
);
macro_after_await!(
  NoDefineSlotsAfterAwait,
  NO_DEFINE_SLOTS_AFTER_AWAIT,
  "vue-vet/correctness/no-define-slots-after-await",
  "rules/correctness/no-define-slots-after-await",
  "defineSlots"
);
macro_after_await!(
  NoDefineOptionsAfterAwait,
  NO_DEFINE_OPTIONS_AFTER_AWAIT,
  "vue-vet/correctness/no-define-options-after-await",
  "rules/correctness/no-define-options-after-await",
  "defineOptions"
);

fn is_simple_ident(name: &str) -> bool {
  let mut chars = name.chars();
  let Some(first) = chars.next() else {
    return false;
  };
  (first.is_ascii_alphabetic() || first == '_' || first == '$')
    && chars
      .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '$')
}

#[must_use]
pub(super) fn extra_rules() -> Vec<&'static dyn Rule> {
  vec![
    &NO_MULTIPLE_EFFECTS_SAME_TARGET,
    &NO_PROPS_SNAPSHOT_IN_REF,
    &NO_V_MODEL_NONREACTIVE_SOURCE,
    &NO_STALE_PROP_FLOW,
    &NO_UNUSED_COMPUTED_BINDING,
    &PREFER_EXPLICIT_SOURCES_FOR_CONDITIONAL_DEPS,
    &NO_DEFINE_PROPS_AFTER_AWAIT,
    &NO_DEFINE_EMITS_AFTER_AWAIT,
    &NO_DEFINE_MODEL_AFTER_AWAIT,
    &NO_DEFINE_SLOTS_AFTER_AWAIT,
    &NO_DEFINE_OPTIONS_AFTER_AWAIT,
  ]
}
