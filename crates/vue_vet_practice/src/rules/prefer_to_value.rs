use vue_vet_core::{
  Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, ScriptBlockFacts, ScriptCallFact,
  Severity,
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

  fn run_once(&self, context: &mut RuleContext<'_>) {
    if is_test_path(context.file()) {
      return;
    }
    let Some(version) = context.environment().vue_version else {
      return;
    };
    if !RECIPE.meets_vue(version.major, version.minor) {
      return;
    }
    let findings = context
      .script()
      .blocks
      .iter()
      .flat_map(|block| {
        block
          .calls
          .iter()
          .filter(move |call| is_vue_unref_call(call, block) && call.has_function_argument)
          .map(|call| call.span)
      })
      .collect::<Vec<_>>();
    for span in findings {
      context.report_with_recommendation(
        self.meta(),
        span,
        "`unref` leaves a function payload intact; Vue 3.3+ `toValue` invokes getters.".into(),
        Some(
          "`toValue` invokes a function payload; `unref` returns the function. Use `toValue` only when getter-or-ref normalization is intended.".into(),
        ),
        recommendation_from(RECIPE.recommend),
      );
    }
  }
}

fn is_vue_unref_call(call: &ScriptCallFact, block: &ScriptBlockFacts) -> bool {
  if let Some((source, imported)) = call.resolved_import.as_ref() {
    return is_vue_runtime_source(source) && imported == "unref";
  }
  // Nuxt / unplugin-auto-import: bare `unref(...)` with no local binding.
  call.callee == "unref"
    && !block.bindings.iter().any(|binding| binding.name == "unref")
    && !block.imports.iter().any(|import| import.local == "unref")
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use vue_vet_core::{
    ReactiveBindingFact, ReactiveBindingKind, ReactivityGraph, RuleEnvironment, ScriptBindingFact,
    ScriptBlockFacts, ScriptCallFact, ScriptFacts, ScriptKind, SourceSpan, TemplateFacts,
    VueVersion,
  };

  use super::*;
  use crate::practice_registry;

  fn span() -> SourceSpan {
    SourceSpan { offset: 0, length: 5, line: 1, column: 1 }
  }

  fn ref_graph(name: &str) -> ReactivityGraph {
    ReactivityGraph {
      bindings: vec![ReactiveBindingFact {
        name: name.into(),
        kind: ReactiveBindingKind::Ref,
        initialized_with_null: false,
        span: span(),
        alias_of: None,
        alias_of_span: None,
      }],
      ..ReactivityGraph::default()
    }
  }

  fn run(
    call: ScriptCallFact,
    bindings: Vec<ScriptBindingFact>,
    minor: u64,
    graph: ReactivityGraph,
  ) -> Vec<vue_vet_core::Diagnostic> {
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
        lifetime: vue_vet_core::ReactivityLifetimeFacts::default(),
        reactivity_graph: std::sync::Arc::new(graph),
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
  fn reports_resolved_vue_unref_on_function_payload() {
    let diagnostics = run(
      ScriptCallFact {
        callee: "unref".into(),
        resolved_import: Some(("vue".into(), "unref".into())),
        has_function_argument: true,
        span: span(),
        ..ScriptCallFact::default()
      },
      Vec::new(),
      3,
      ReactivityGraph::default(),
    );
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert!(diagnostic.recommendation.is_some());
    assert!(
      diagnostic.message.contains("invokes")
        || diagnostic.help.as_deref().is_some_and(|help| help.contains("invokes")),
      "{}",
      diagnostic.message
    );
  }

  #[test]
  fn reports_bare_auto_import_unref_on_function_payload() {
    let diagnostics = run(
      ScriptCallFact {
        callee: "unref".into(),
        has_function_argument: true,
        span: span(),
        ..ScriptCallFact::default()
      },
      Vec::new(),
      3,
      ReactivityGraph::default(),
    );
    assert_eq!(diagnostics.len(), 1);
  }

  #[test]
  fn stays_quiet_for_known_ref_identifier() {
    let diagnostics = run(
      ScriptCallFact {
        callee: "unref".into(),
        resolved_import: Some(("vue".into(), "unref".into())),
        argument_identifiers: vec!["count".into()],
        span: span(),
        ..ScriptCallFact::default()
      },
      Vec::new(),
      3,
      ref_graph("count"),
    );
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_for_maybe_ref_numeric_without_getter_evidence() {
    let diagnostics = run(
      ScriptCallFact {
        callee: "unref".into(),
        resolved_import: Some(("vue".into(), "unref".into())),
        argument_identifiers: vec!["input".into()],
        span: span(),
        ..ScriptCallFact::default()
      },
      Vec::new(),
      3,
      ReactivityGraph::default(),
    );
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_for_local_unref_binding() {
    let diagnostics = run(
      ScriptCallFact { callee: "unref".into(), span: span(), ..ScriptCallFact::default() },
      vec![ScriptBindingFact {
        name: "unref".into(),
        reads: 1,
        writes: 0,
        span: span(),
        exported: false,
        plain_initializer: false,
        escaped: false,
      }],
      3,
      ReactivityGraph::default(),
    );
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn reports_nuxt_imports_unref_on_function_payload() {
    let diagnostics = run(
      ScriptCallFact {
        callee: "unref".into(),
        resolved_import: Some(("#imports".into(), "unref".into())),
        has_function_argument: true,
        span: span(),
        ..ScriptCallFact::default()
      },
      Vec::new(),
      3,
      ReactivityGraph::default(),
    );
    assert_eq!(diagnostics.len(), 1);
  }

  #[test]
  fn stays_quiet_before_vue_3_3() {
    let diagnostics = run(
      ScriptCallFact {
        callee: "unref".into(),
        resolved_import: Some(("vue".into(), "unref".into())),
        has_function_argument: true,
        span: span(),
        ..ScriptCallFact::default()
      },
      Vec::new(),
      2,
      ReactivityGraph::default(),
    );
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_for_unrelated_calls() {
    let diagnostics = run(
      ScriptCallFact {
        callee: "toValue".into(),
        resolved_import: Some(("vue".into(), "toValue".into())),
        span: span(),
        ..ScriptCallFact::default()
      },
      Vec::new(),
      3,
      ReactivityGraph::default(),
    );
    assert!(diagnostics.is_empty());
  }
}
