use vue_vet_core::{
  Confidence, ReactiveBindingKind, Rule, RuleContext, RuleMeta, ScriptKind, Severity,
};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/prefer-use-template-ref",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/prefer-use-template-ref",
};

pub(super) struct PreferUseTemplateRef;

pub(super) static RULE: PreferUseTemplateRef = PreferUseTemplateRef;

impl Rule for PreferUseTemplateRef {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let Some(version) = context.environment().vue_version else {
      return;
    };
    if !version.is_at_least(3, 5) {
      return;
    }
    let template_refs = context
      .template()
      .elements
      .iter()
      .filter_map(|element| element.attribute("ref"))
      .filter_map(|attribute| attribute.value.as_deref())
      .collect::<Vec<_>>();
    let bindings = context
      .script()
      .blocks
      .iter()
      .filter(|block| block.kind == ScriptKind::Setup)
      .flat_map(|block| &block.reactivity_graph.bindings)
      .filter(|binding| {
        binding.kind == ReactiveBindingKind::Ref
          && binding.initialized_with_null
          && template_refs.iter().any(|template_ref| **template_ref == binding.name)
      })
      .map(|binding| (binding.span.clone(), binding.name.clone()))
      .collect::<Vec<_>>();
    for (span, name) in bindings {
      context.report(
        self.meta(),
        span,
        format!("`{name}` mirrors a static template ref with `ref(null)`"),
        Some(format!("Use `useTemplateRef('{name}')`, available in Vue 3.5 and newer.")),
      );
    }
  }
}
