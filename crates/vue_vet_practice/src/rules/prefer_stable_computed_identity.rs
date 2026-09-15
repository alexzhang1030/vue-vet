use vue_vet_core::{
  Confidence, PRACTICE_CATEGORY, Rule, RuleContext, RuleGroupId, RuleMeta, Severity,
  StableComputedIdentityFact,
};

use crate::{
  recipe::{EcosystemApi, PracticeRecipe},
  util::{is_test_path, recommendation_from},
};

const RECIPE: PracticeRecipe = PracticeRecipe {
  rule_id: "vue-vet/practice/prefer-stable-computed-identity",
  documentation: "rules/practice/prefer-stable-computed-identity",
  confidence: Confidence::High,
  min_vue: Some((3, 4)),
  recommend: EcosystemApi {
    package: "vue",
    export: "computed",
    docs_url: "https://vuejs.org/api/reactivity-core.html#computed",
    import_example: "computed((previous) => { const next = /* projection */; return same(previous, next) ? previous : next })",
  },
};

const META: RuleMeta = RuleMeta {
  id: RECIPE.rule_id,
  category: PRACTICE_CATEGORY,
  default_severity: Severity::Info,
  confidence: RECIPE.confidence,
  documentation: RECIPE.documentation,
  group: Some(RuleGroupId::Derivation),
};

pub(super) struct PreferStableComputedIdentity;

pub(super) static RULE: PreferStableComputedIdentity = PreferStableComputedIdentity;

impl Rule for PreferStableComputedIdentity {
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
    for block in &context.script().blocks {
      for site in &block.source_contracts.stable_computed_identity {
        report_site(context, site);
      }
    }
  }
}

fn report_site(context: &mut RuleContext<'_>, site: &StableComputedIdentityFact) {
  context.report_with_recommendation(
    &META,
    site.computed_span,
    "This computed returns a new object with the same primitive contents when its source is replaced, so a downstream identity consumer repeats work"
      .into(),
    Some(format!(
      "Equal-content replacement at {}:{} still allocates a new result; the consumer at {}:{} reruns on the new identity. Vue 3.4+ `computed` getters receive the previous value — finish every reactive read, then reuse that identity when Object.is-equal primitive contents match. Keep a fresh identity when the consumer needs every allocation.",
      site.replacement_span.line,
      site.replacement_span.column,
      site.consumer_span.line,
      site.consumer_span.column
    )),
    recommendation_from(RECIPE.recommend),
  );
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use vue_vet_core::{
    ReactivityGraph, RuleEnvironment, ScriptBlockFacts, ScriptFacts, ScriptKind,
    SourceContractFacts, SourceSpan, StableComputedIdentityFact, StableComputedIdentityReason,
    TemplateFacts, VueVersion,
  };

  use super::*;
  use crate::practice_registry;

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 8, line: 1, column: offset.saturating_add(1) }
  }

  fn run(minor: u64, facts: SourceContractFacts, path: &str) -> Vec<vue_vet_core::Diagnostic> {
    let script = ScriptFacts {
      blocks: vec![ScriptBlockFacts {
        kind: ScriptKind::Setup,
        language: "ts".into(),
        imports: Vec::new(),
        bindings: Vec::new(),
        calls: Vec::new(),
        member_writes: Vec::new(),
        destructures: Vec::new(),
        top_level_await_ends: Vec::new(),
        operands: Vec::new(),
        lifetime: vue_vet_core::ReactivityLifetimeFacts::default(),
        source_contracts: facts,
        template_ref_demands: vue_vet_core::TemplateRefDemandFacts::default(),
        reactivity_graph: std::sync::Arc::new(ReactivityGraph::default()),
        vapor: false,
        open_span: None,
        runtime_export_spans: Vec::new(),
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

  fn finding() -> SourceContractFacts {
    SourceContractFacts {
      stable_computed_identity: vec![StableComputedIdentityFact {
        computed_span: span(10),
        source_span: span(20),
        replacement_span: span(30),
        consumer_span: span(40),
        reason: StableComputedIdentityReason::FreshPrimitiveProjection,
      }],
      ..SourceContractFacts::default()
    }
  }

  #[test]
  fn reports_on_vue_3_4_with_recommendation() {
    let diagnostics = run(4, finding(), "src/List.vue");
    assert_eq!(diagnostics.len(), 1);
    let Some(diagnostic) = diagnostics.first() else {
      return;
    };
    assert_eq!(diagnostic.rule_id, RECIPE.rule_id);
    assert_eq!(diagnostic.category, PRACTICE_CATEGORY);
    assert_eq!(diagnostic.severity, Severity::Info);
    assert!(diagnostic.recommendation.is_some());
    assert!(diagnostic.edits.is_empty());
    assert!(
      diagnostic.help.as_ref().is_some_and(|help| help.contains("1:31") && help.contains("1:41"))
    );
  }

  #[test]
  fn stays_quiet_before_vue_3_4() {
    assert!(run(3, finding(), "src/List.vue").is_empty());
  }

  #[test]
  fn stays_quiet_on_test_paths() {
    assert!(run(4, finding(), "src/List.spec.ts").is_empty());
  }

  #[test]
  fn stays_quiet_without_facts() {
    assert!(run(4, SourceContractFacts::default(), "src/List.vue").is_empty());
  }

  #[test]
  fn stays_quiet_when_vue_version_is_unset() {
    let script = ScriptFacts {
      blocks: vec![ScriptBlockFacts {
        kind: ScriptKind::Setup,
        language: "ts".into(),
        imports: Vec::new(),
        bindings: Vec::new(),
        calls: Vec::new(),
        member_writes: Vec::new(),
        destructures: Vec::new(),
        top_level_await_ends: Vec::new(),
        operands: Vec::new(),
        lifetime: vue_vet_core::ReactivityLifetimeFacts::default(),
        source_contracts: finding(),
        template_ref_demands: vue_vet_core::TemplateRefDemandFacts::default(),
        reactivity_graph: std::sync::Arc::new(ReactivityGraph::default()),
        vapor: false,
        open_span: None,
        runtime_export_spans: Vec::new(),
      }],
    };
    let diagnostics = practice_registry().run_with_environment(
      Path::new("src/List.vue"),
      "",
      &TemplateFacts::default(),
      &script,
      RuleEnvironment { vue_version: None, packages: Vec::new() },
    );
    assert!(diagnostics.is_empty());
  }
}
