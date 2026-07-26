use std::path::Path;

use vue_vet_core::{
  Confidence, PRACTICE_CATEGORY, Recommendation, Rule, RuleContext, RuleMeta, Severity,
};

use crate::recipe::{EcosystemApi, PracticeRecipe};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/vueuse-use-debounce-fn",
  documentation: "rules/practice/vueuse-use-debounce-fn",
  recommend: EcosystemApi {
    package: "@vueuse/core",
    export: "useDebounceFn",
    docs_url: "https://vueuse.org/core/useDebounceFn/",
    import_example: "import { useDebounceFn } from '@vueuse/core'",
  },
};

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: Confidence::Medium,
  documentation: RECIPE.documentation,
};

pub(super) struct VueuseUseDebounceFn;

pub(super) static RULE: VueuseUseDebounceFn = VueuseUseDebounceFn;

impl Rule for VueuseUseDebounceFn {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    if is_test_path(context.file()) {
      return;
    }
    let spans = context
      .script()
      .blocks
      .iter()
      .filter(|block| !already_uses_target(block, RECIPE.recommend.export))
      .filter(|block| block.calls.iter().any(|call| call.callee == "clearTimeout"))
      .filter_map(|block| {
        block
          .calls
          .iter()
          .find(|call| call.callee == "setTimeout" && call.assigned_to.is_some())
          .map(|call| call.span.clone())
      })
      .collect::<Vec<_>>();
    for span in spans {
      context.report_with_recommendation(
        self.meta(),
        span,
        "This looks like a hand-rolled debounce with `setTimeout` / `clearTimeout`; consider VueUse `useDebounceFn`.".into(),
        Some(
          "Optional dependency: install `@vueuse/core` when you want the helper, then replace the timer wrapper."
            .into(),
        ),
        recommendation_from(RECIPE.recommend),
      );
    }
  }
}

fn already_uses_target(block: &vue_vet_core::ScriptBlockFacts, export: &str) -> bool {
  block.imports.iter().any(|import| {
    is_vueuse_source(&import.source) && (import.imported == export || import.local == export)
  }) || block.calls.iter().any(|call| call.callee == export)
}

fn is_vueuse_source(source: &str) -> bool {
  source == "@vueuse/core" || source.starts_with("@vueuse/")
}

fn is_test_path(path: &Path) -> bool {
  let normalized = path.to_string_lossy().replace('\\', "/");
  normalized.contains("/__tests__/")
    || normalized.contains(".test.")
    || normalized.contains(".spec.")
}

fn recommendation_from(api: EcosystemApi) -> Recommendation {
  Recommendation {
    kind: "ecosystem_api".into(),
    package: api.package.into(),
    export: api.export.into(),
    docs_url: api.docs_url.into(),
    import_example: api.import_example.into(),
  }
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use vue_vet_core::{
    ReactivityGraph, ScriptBlockFacts, ScriptCallFact, ScriptFacts, ScriptKind, SourceSpan,
    TemplateFacts,
  };

  use super::*;
  use crate::practice_registry;

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 10, line: 1, column: offset.saturating_add(1) }
  }

  fn run(calls: Vec<ScriptCallFact>) -> Vec<vue_vet_core::Diagnostic> {
    let script = ScriptFacts {
      blocks: vec![ScriptBlockFacts {
        kind: ScriptKind::Setup,
        language: "ts".into(),
        imports: Vec::new(),
        bindings: Vec::new(),
        calls,
        member_writes: Vec::new(),
        destructures: Vec::new(),
        reactivity_graph: ReactivityGraph::default(),
      }],
    };
    practice_registry().run(Path::new("src/Debounce.vue"), "", &TemplateFacts::default(), &script)
  }

  #[test]
  fn reports_assigned_set_timeout_with_clear() {
    let diagnostics = run(vec![
      ScriptCallFact {
        callee: "clearTimeout".into(),
        assigned_to: None,
        resolved_import: None,
        span: span(0),
      },
      ScriptCallFact {
        callee: "setTimeout".into(),
        assigned_to: Some("timer".into()),
        resolved_import: None,
        span: span(20),
      },
    ]);
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert_eq!(diagnostic.category, PRACTICE_CATEGORY);
    assert!(diagnostic.recommendation.is_some());
  }

  #[test]
  fn stays_quiet_without_clear_timeout() {
    let diagnostics = run(vec![ScriptCallFact {
      callee: "setTimeout".into(),
      assigned_to: Some("timer".into()),
      resolved_import: None,
      span: span(0),
    }]);
    assert!(diagnostics.is_empty());
  }
}
