use std::collections::BTreeSet;

use vue_vet_core::{
  Confidence, PRACTICE_CATEGORY, ReactiveBindingKind, Recommendation, Rule, RuleContext, RuleMeta,
  Severity,
};
use vue_vet_rule_query::{setup_blocks, static_template_ref_names};

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
    let template_refs: BTreeSet<&str> =
      static_template_ref_names(&context.template().elements).collect();
    if template_refs.is_empty() {
      return;
    }
    for binding in setup_blocks(context.script()).flat_map(|block| &block.reactivity_graph.bindings)
    {
      if binding.kind != ReactiveBindingKind::Ref
        || !binding.initialized_with_null
        || !template_refs.contains(binding.name.as_str())
      {
        continue;
      }
      context.report_with_recommendation(
        self.meta(),
        binding.span.clone(),
        format!("`{}` mirrors a static template ref with `ref(null)`", binding.name),
        Some(format!(
          "Prefer `useTemplateRef('{}')`, available in Vue 3.5 and newer.",
          binding.name
        )),
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
