use vue_vet_core::{
  Confidence, FactKinds, FactRef, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, Severity,
};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{is_test_path, is_vue_runtime_source, recommendation_from},
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/prefer-to-value",
  documentation: "rules/practice/prefer-to-value",
  confidence: Confidence::High,
  min_vue: Some((3, 3)),
  recommend: EcosystemApi {
    package: "vue",
    export: "toValue",
    docs_url: "https://vuejs.org/api/reactivity-utilities.html#tovalue",
    import_example: "import { toValue } from 'vue'",
  },
};

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
};

pub(super) struct PreferToValue;

pub(super) static RULE: PreferToValue = PreferToValue;

impl Rule for PreferToValue {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::SCRIPT_CALL
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    if is_test_path(context.file()) {
      return;
    }
    let Some(version) = context.environment().vue_version else {
      return;
    };
    if !RECIPE.meets_vue(version.major, version.minor) {
      return;
    }
    let FactRef::ScriptCall { call, .. } = fact else {
      return;
    };
    if !is_vue_unref_call(call) {
      return;
    }
    context.report_with_recommendation(
      self.meta(),
      call.span.clone(),
      "`unref` can be replaced with Vue 3.3+ `toValue`, which also accepts getters.".into(),
      Some(
        "Prefer `toValue(...)` for values that may be a ref, a plain value, or a getter.".into(),
      ),
      recommendation_from(RECIPE.recommend),
    );
  }
}

fn is_vue_unref_call(call: &vue_vet_core::ScriptCallFact) -> bool {
  call
    .resolved_import
    .as_ref()
    .is_some_and(|(source, imported)| is_vue_runtime_source(source) && imported == "unref")
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use vue_vet_core::{
    ReactivityGraph, RuleEnvironment, ScriptBlockFacts, ScriptCallFact, ScriptFacts, ScriptKind,
    SourceSpan, TemplateFacts, VueVersion,
  };

  use super::*;
  use crate::practice_registry;

  fn span() -> SourceSpan {
    SourceSpan { offset: 0, length: 5, line: 1, column: 1 }
  }

  fn run(call: ScriptCallFact, minor: u64) -> Vec<vue_vet_core::Diagnostic> {
    let script = ScriptFacts {
      blocks: vec![ScriptBlockFacts {
        kind: ScriptKind::Setup,
        language: "ts".into(),
        imports: Vec::new(),
        bindings: Vec::new(),
        calls: vec![call],
        member_writes: Vec::new(),
        destructures: Vec::new(),
        reactivity_graph: ReactivityGraph::default(),
      }],
    };
    practice_registry().run_with_environment(
      Path::new("src/App.vue"),
      "",
      &TemplateFacts::default(),
      &script,
      RuleEnvironment {
        vue_version: Some(VueVersion { major: 3, minor, patch: 0 }),
        packages: vec!["vue".into()],
      },
    )
  }

  #[test]
  fn reports_resolved_vue_unref_on_3_3() {
    let diagnostics = run(
      ScriptCallFact {
        callee: "unref".into(),
        assigned_to: None,
        resolved_import: Some(("vue".into(), "unref".into())),
        argument_identifiers: vec!["count".into()],
        span: span(),
      },
      3,
    );
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert!(diagnostic.recommendation.is_some());
  }

  #[test]
  fn stays_quiet_before_vue_3_3() {
    let diagnostics = run(
      ScriptCallFact {
        callee: "unref".into(),
        assigned_to: None,
        resolved_import: Some(("vue".into(), "unref".into())),
        argument_identifiers: Vec::new(),
        span: span(),
      },
      2,
    );
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_for_unrelated_calls() {
    let diagnostics = run(
      ScriptCallFact {
        callee: "toValue".into(),
        assigned_to: None,
        resolved_import: Some(("vue".into(), "toValue".into())),
        argument_identifiers: Vec::new(),
        span: span(),
      },
      3,
    );
    assert!(diagnostics.is_empty());
  }
}
