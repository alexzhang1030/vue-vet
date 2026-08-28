use vue_vet_core::{Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleMeta, Severity};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{is_script_setup_block, is_test_path, recommendation_from},
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/prefer-define-model",
  documentation: "rules/practice/prefer-define-model",
  confidence: Confidence::Medium,
  min_vue: Some((3, 4)),
  recommend: EcosystemApi {
    package: "vue",
    export: "defineModel",
    docs_url: "https://vuejs.org/api/sfc-script-setup.html#definemodel",
    import_example: "const model = defineModel()",
  },
};

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
};

pub(super) struct PreferDefineModel;

pub(super) static RULE: PreferDefineModel = PreferDefineModel;

impl Rule for PreferDefineModel {
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
      // `defineModel` is a `<script setup>` compiler macro — never recommend it for
      // ordinary script / standalone JSX modules (ScriptKind::Script).
      .filter(|block| is_script_setup_block(block))
      .filter(|block| !block.calls.iter().any(|call| call.callee == "defineModel"))
      .filter(|block| block.calls.iter().any(|call| call.callee == "defineProps"))
      .filter(|block| block.calls.iter().any(|call| call.callee == "defineEmits"))
      .filter_map(|block| {
        block.bindings.iter().find(|binding| binding.name == "modelValue").map(|binding| {
          block
            .calls
            .iter()
            .find(|call| call.callee == "defineProps")
            .map_or_else(|| binding.span, |call| call.span)
        })
      })
      .collect::<Vec<_>>();
    for span in findings {
      context.report_with_recommendation(
        self.meta(),
        span,
        "`modelValue` prop plus `update:modelValue` emit can be replaced with `defineModel`".into(),
        Some(
          "Vue 3.4+ `defineModel()` declares the prop and event together and returns a writable ref."
            .into(),
        ),
        recommendation_from(RECIPE.recommend),
      );
    }
  }
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use vue_vet_core::{
    ReactivityGraph, RuleEnvironment, ScriptBindingFact, ScriptBlockFacts, ScriptCallFact,
    ScriptFacts, ScriptKind, SourceSpan, TemplateFacts, VueVersion,
  };

  use super::*;
  use crate::practice_registry;

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 12, line: 1, column: offset.saturating_add(1) }
  }

  fn call(callee: &str, offset: usize) -> ScriptCallFact {
    ScriptCallFact {
      callee: callee.into(),
      assigned_to: None,
      resolved_import: None,
      argument_identifiers: Vec::new(),
      span: span(offset),
    }
  }

  fn run_on(
    path: &str,
    kind: ScriptKind,
    calls: Vec<ScriptCallFact>,
    bindings: Vec<ScriptBindingFact>,
    minor: u64,
  ) -> Vec<vue_vet_core::Diagnostic> {
    let script = ScriptFacts {
      blocks: vec![ScriptBlockFacts {
        kind,
        language: "ts".into(),
        imports: Vec::new(),
        bindings,
        calls,
        member_writes: Vec::new(),
        destructures: Vec::new(),
        top_level_await_ends: Vec::new(),
        operands: Vec::new(),
        reactivity_graph: std::sync::Arc::new(ReactivityGraph::default()),
      }],
    };
    practice_registry().run_with_environment(
      Path::new(path),
      "",
      &TemplateFacts::default(),
      &script,
      RuleEnvironment {
        vue_version: Some(VueVersion { major: 3, minor, patch: 0 }),
        packages: Vec::new(),
      },
    )
  }

  fn run(
    calls: Vec<ScriptCallFact>,
    bindings: Vec<ScriptBindingFact>,
    minor: u64,
  ) -> Vec<vue_vet_core::Diagnostic> {
    run_on("src/Toggle.vue", ScriptKind::Setup, calls, bindings, minor)
  }

  #[test]
  fn reports_props_plus_emits_model_value_pattern() {
    let diagnostics = run(
      vec![call("defineProps", 0), call("defineEmits", 20)],
      vec![ScriptBindingFact { name: "modelValue".into(), reads: 1, writes: 0, span: span(0) }],
      4,
    );
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert!(diagnostic.recommendation.is_some());
  }

  #[test]
  fn stays_quiet_when_define_model_already_used() {
    let diagnostics = run(
      vec![call("defineProps", 0), call("defineEmits", 20), call("defineModel", 40)],
      vec![ScriptBindingFact { name: "modelValue".into(), reads: 1, writes: 0, span: span(0) }],
      4,
    );
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_without_model_value_binding() {
    let diagnostics = run(vec![call("defineProps", 0), call("defineEmits", 20)], Vec::new(), 4);
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_before_vue_3_4() {
    let diagnostics = run(
      vec![call("defineProps", 0), call("defineEmits", 20)],
      vec![ScriptBindingFact { name: "modelValue".into(), reads: 1, writes: 0, span: span(0) }],
      3,
    );
    assert!(diagnostics.is_empty());
  }

  #[test]
  fn stays_quiet_on_ordinary_script_and_jsx_modules() {
    let calls = vec![call("defineProps", 0), call("defineEmits", 20)];
    let bindings =
      vec![ScriptBindingFact { name: "modelValue".into(), reads: 1, writes: 0, span: span(0) }];
    assert!(
      run_on("src/Toggle.tsx", ScriptKind::Script, calls.clone(), bindings.clone(), 4).is_empty(),
      "standalone JSX must not be told to use defineModel"
    );
    assert!(
      run_on("src/Toggle.vue", ScriptKind::Script, calls, bindings, 4).is_empty(),
      "ordinary <script> must not be told to use defineModel"
    );
  }
}
