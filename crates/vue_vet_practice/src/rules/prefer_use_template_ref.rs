use std::collections::BTreeSet;

use vue_vet_core::{
  Confidence, PRACTICE_CATEGORY, ReactiveBindingKind, Recommendation, Rule, RuleContext, RuleMeta,
  ScriptKind, Severity,
};

/// Historical ID keeps the `reactivity` segment for config/suppression stability.
const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/prefer-use-template-ref",
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: Confidence::High,
  documentation: "rules/reactivity/prefer-use-template-ref",
};

pub(super) struct PreferUseTemplateRef;

pub(super) static RULE: PreferUseTemplateRef = PreferUseTemplateRef;

impl Rule for PreferUseTemplateRef {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    let Some(version) = context.environment().vue_version else {
      return;
    };
    if !version.is_at_least(3, 5) {
      return;
    }
    let template_refs: BTreeSet<String> = context
      .template()
      .elements
      .iter()
      .filter_map(|element| element.attribute("ref"))
      .filter_map(|attribute| attribute.value.clone())
      .collect();
    if template_refs.is_empty() {
      return;
    }
    let findings: Vec<_> = context
      .script()
      .blocks
      .iter()
      .filter(|block| block.kind == ScriptKind::Setup)
      .flat_map(|block| &block.reactivity_graph.bindings)
      .filter(|binding| {
        binding.kind == ReactiveBindingKind::Ref
          && binding.initialized_with_null
          && template_refs.contains(&binding.name)
      })
      .map(|binding| (binding.span.clone(), binding.name.clone()))
      .collect();
    for (span, name) in findings {
      context.report_with_recommendation(
        self.meta(),
        span,
        format!("`{name}` mirrors a static template ref with `ref(null)`"),
        Some(format!("Prefer `useTemplateRef('{name}')`, available in Vue 3.5 and newer.")),
        Recommendation {
          kind: "ecosystem_api".into(),
          package: "vue".into(),
          export: "useTemplateRef".into(),
          docs_url: "https://vuejs.org/api/composition-api-helpers.html#usetemplateref".into(),
          import_example: "import { useTemplateRef } from 'vue'".into(),
        },
      );
    }
  }
}
