use vue_vet_core::{Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, Severity};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{is_test_path, is_vue_runtime_source, recommendation_from},
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/prefer-use-slots-attrs",
  documentation: "rules/practice/prefer-use-slots-attrs",
  confidence: Confidence::Medium,
  min_vue: None,
  recommend: EcosystemApi {
    package: "vue",
    export: "useSlots",
    docs_url: "https://vuejs.org/api/composition-api-setup.html#useslots-useattrs",
    import_example: "import { useSlots } from 'vue'",
  },
};

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
};

pub(super) struct PreferUseSlotsAttrs;

pub(super) static RULE: PreferUseSlotsAttrs = PreferUseSlotsAttrs;

impl Rule for PreferUseSlotsAttrs {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    if is_test_path(context.file()) {
      return;
    }
    let findings = context
      .script()
      .blocks
      .iter()
      .flat_map(|block| {
        block.calls.iter().filter(move |call| is_get_current_instance_call(call, block))
      })
      .map(|call| call.span.clone())
      .collect::<Vec<_>>();
    for span in findings {
      context.report_with_recommendation(
        self.meta(),
        span,
        "`getCurrentInstance` is an advanced escape hatch; `useSlots()` / `useAttrs()` cover the common cases".into(),
        Some(
          "Prefer `useSlots()` for `instance.slots` and `useAttrs()` for `instance.attrs` inside `<script setup>`."
            .into(),
        ),
        recommendation_from(RECIPE.recommend),
      );
    }
  }
}

fn is_get_current_instance_call(
  call: &vue_vet_core::ScriptCallFact,
  block: &vue_vet_core::ScriptBlockFacts,
) -> bool {
  if let Some((source, imported)) = call.resolved_import.as_ref() {
    return is_vue_runtime_source(source) && imported == "getCurrentInstance";
  }
  call.callee == "getCurrentInstance"
    && !block.bindings.iter().any(|binding| binding.name == "getCurrentInstance")
    && !block.imports.iter().any(|import| import.local == "getCurrentInstance")
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use vue_vet_core::{
    ReactivityGraph, ScriptBindingFact, ScriptBlockFacts, ScriptCallFact, ScriptFacts, ScriptKind,
    SourceSpan, TemplateFacts,
  };

  use super::*;
  use crate::practice_registry;

  fn span() -> SourceSpan {
    SourceSpan { offset: 0, length: 20, line: 1, column: 1 }
  }

  fn run(call: ScriptCallFact, bindings: Vec<ScriptBindingFact>) -> Vec<vue_vet_core::Diagnostic> {
    let script = ScriptFacts {
      blocks: vec![ScriptBlockFacts {
        kind: ScriptKind::Setup,
        language: "ts".into(),
        imports: Vec::new(),
        bindings,
        calls: vec![call],
        member_writes: Vec::new(),
        destructures: Vec::new(),
        top_level_await_ends: Vec::new(),
        operands: Vec::new(),
        reactivity_graph: std::sync::Arc::new(ReactivityGraph::default()),
      }],
    };
    practice_registry().run(Path::new("src/Instance.vue"), "", &TemplateFacts::default(), &script)
  }

  #[test]
  fn reports_resolved_vue_get_current_instance() {
    let diagnostics = run(
      ScriptCallFact {
        callee: "getCurrentInstance".into(),
        assigned_to: Some("instance".into()),
        resolved_import: Some(("vue".into(), "getCurrentInstance".into())),
        argument_identifiers: Vec::new(),
        span: span(),
      },
      Vec::new(),
    );
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert!(diagnostic.recommendation.is_some());
  }

  #[test]
  fn reports_bare_auto_import_without_local_binding() {
    let diagnostics = run(
      ScriptCallFact {
        callee: "getCurrentInstance".into(),
        assigned_to: None,
        resolved_import: None,
        argument_identifiers: Vec::new(),
        span: span(),
      },
      Vec::new(),
    );
    assert_eq!(diagnostics.len(), 1);
  }

  #[test]
  fn stays_quiet_for_local_binding() {
    let diagnostics = run(
      ScriptCallFact {
        callee: "getCurrentInstance".into(),
        assigned_to: None,
        resolved_import: None,
        argument_identifiers: Vec::new(),
        span: span(),
      },
      vec![ScriptBindingFact {
        name: "getCurrentInstance".into(),
        reads: 1,
        writes: 0,
        span: span(),
      }],
    );
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_for_unrelated_calls() {
    let diagnostics = run(
      ScriptCallFact {
        callee: "useSlots".into(),
        assigned_to: None,
        resolved_import: Some(("vue".into(), "useSlots".into())),
        argument_identifiers: Vec::new(),
        span: span(),
      },
      Vec::new(),
    );
    assert!(diagnostics.is_empty());
  }
}
