use std::collections::BTreeSet;

use vue_vet_core::{
  Confidence, ReactiveBindingKind, Rule, RuleContext, RuleMeta, Severity, TemplateElementFact,
};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-unused-reactive-binding",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-unused-reactive-binding",
};

pub(super) struct NoUnusedReactiveBinding;

pub(super) static RULE: NoUnusedReactiveBinding = NoUnusedReactiveBinding;

impl Rule for NoUnusedReactiveBinding {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let template_ref_names = static_template_ref_names(context.template().elements.as_slice());
    let findings = context
      .script()
      .blocks
      .iter()
      .flat_map(|block| {
        let graph = &block.reactivity_graph;
        let mut used = BTreeSet::new();
        for read in &graph.template_reads {
          used.insert(read.binding.as_str());
        }
        for scope in &graph.scopes {
          for read in &scope.reads {
            used.insert(read.binding.as_str());
          }
          for write in &scope.writes {
            used.insert(write.binding.as_str());
          }
        }
        for edge in &graph.edges {
          used.insert(edge.to.as_str());
        }
        for name in &template_ref_names {
          used.insert(name.as_str());
        }
        let script_read_counts = block
          .bindings
          .iter()
          .map(|binding| (binding.name.as_str(), binding.reads))
          .collect::<std::collections::BTreeMap<_, _>>();

        graph
          .bindings
          .iter()
          .filter(|binding| is_local_value_binding(binding.kind))
          .filter(|binding| !used.contains(binding.name.as_str()))
          .filter(|binding| {
            script_read_counts.get(binding.name.as_str()).copied().unwrap_or(0) == 0
          })
          .map(|binding| (binding.span.clone(), binding.name.clone(), binding.kind))
          .collect::<Vec<_>>()
      })
      .collect::<Vec<_>>();

    for (span, name, kind) in findings {
      let kind_label = binding_kind_label(kind);
      context.report(
        self.meta(),
        span,
        format!("reactive binding `{name}` ({kind_label}) is never read in script or template"),
        Some(
          "Remove the unused binding, or read it from a tracking scope, template expression, or other script use."
            .into(),
        ),
      );
    }
  }
}

const fn is_local_value_binding(kind: ReactiveBindingKind) -> bool {
  matches!(
    kind,
    ReactiveBindingKind::Ref
      | ReactiveBindingKind::ShallowRef
      | ReactiveBindingKind::Computed
      | ReactiveBindingKind::Reactive
      | ReactiveBindingKind::ShallowReactive
      | ReactiveBindingKind::Readonly
      | ReactiveBindingKind::ShallowReadonly
      | ReactiveBindingKind::CustomRef
  )
}

const fn binding_kind_label(kind: ReactiveBindingKind) -> &'static str {
  match kind {
    ReactiveBindingKind::Ref => "ref",
    ReactiveBindingKind::ShallowRef => "shallowRef",
    ReactiveBindingKind::Computed => "computed",
    ReactiveBindingKind::Reactive => "reactive",
    ReactiveBindingKind::ShallowReactive => "shallowReactive",
    ReactiveBindingKind::Readonly => "readonly",
    ReactiveBindingKind::ShallowReadonly => "shallowReadonly",
    ReactiveBindingKind::CustomRef => "customRef",
    ReactiveBindingKind::ToRef => "toRef",
    ReactiveBindingKind::TemplateRef => "useTemplateRef",
    ReactiveBindingKind::ModelRef => "defineModel",
  }
}

fn static_template_ref_names(elements: &[TemplateElementFact]) -> BTreeSet<String> {
  elements
    .iter()
    .filter_map(|element| element.attribute("ref"))
    .filter_map(|attribute| attribute.value.clone())
    .filter(|value| !value.is_empty())
    .collect()
}
