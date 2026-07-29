//! Vue Vet-owned diagnostics, spans, edits, and reactivity fact contracts.
//!
//! Parser adapters and the reactivity tracer produce these types; rules,
//! reporters, cache, and the CLI consume them. Vize and Oxc AST types never
//! appear in this crate.
//!
//! Modules: [`diagnostics`], [`facts`], [`rule`], plus identity/edits/source helpers.

mod digest;
mod diagnostics;
mod edits;
mod facts;
mod identity;
mod line_index;
mod rule;
mod source_context;

pub use digest::{content_digest, serde_digest};
pub use diagnostics::*;
pub use edits::{ByteRange, EditApplicability, EditPlan, EditPlanError, TextEdit};
pub use facts::*;
pub use identity::{FileId, ModuleId, PhysicalPath, WorkspaceRoot};
pub use line_index::LineIndex;
pub use rule::*;
pub use source_context::SourceContext;

#[cfg(test)]
mod tests {
  use super::*;

  struct TestRule(&'static RuleMeta);

  impl Rule for TestRule {
    fn meta(&self) -> &'static RuleMeta {
      self.0
    }
  }

  static A_META: RuleMeta = RuleMeta {
    id: "vue-vet/test/a",
    category: "test",
    default_severity: Severity::Info,
    confidence: Confidence::High,
    documentation: "rules/test/a",
  };
  static Z_META: RuleMeta = RuleMeta {
    id: "vue-vet/test/z",
    category: "test",
    default_severity: Severity::Info,
    confidence: Confidence::High,
    documentation: "rules/test/z",
  };
  static A_RULE: TestRule = TestRule(&A_META);
  static Z_RULE: TestRule = TestRule(&Z_META);

  #[test]
  fn score_is_deterministic_and_density_normalized() {
    let diagnostic = Diagnostic {
      rule_id: "test/rule".into(),
      category: "test".into(),
      severity: Severity::Warning,
      confidence: Some(Confidence::High),
      documentation: Some("rules/test/rule".into()),
      message: "test".into(),
      help: None,
      file: "Component.vue".into(),
      span: SourceSpan { offset: 0, length: 1, line: 1, column: 1 },
      edits: Vec::new(),
      recommendation: None,
    };
    let concentrated =
      ScanSummary { files_scanned: 1, diagnostics: vec![diagnostic.clone(); 40], score: 100 }
        .finish();
    let sparse =
      ScanSummary { files_scanned: 200, diagnostics: vec![diagnostic; 100], score: 100 }.finish();

    // 40 warnings in 1 file → raw 120, capacity 50 → score floor(100*50/170)=29
    assert_eq!(concentrated.score, 29);
    // 100 warnings across 200 files → raw 300, capacity 10000 → score floor(100*10000/10300)=97
    assert_eq!(sparse.score, 97);
    assert_eq!(density_score(0, 10), 100);
    assert_eq!(density_score(3, 1), 94); // one warning in a tiny project still matters
    assert_eq!(density_score(300, 200), 97); // same absolute count is mild when sparse
    assert!(concentrated.fails(true));
    assert!(!concentrated.fails(false));
  }

  #[test]
  fn practice_findings_do_not_affect_score_or_exit() {
    let practice = Diagnostic {
      rule_id: "vue-vet/practice/example".into(),
      category: PRACTICE_CATEGORY.into(),
      severity: Severity::Error,
      confidence: Some(Confidence::Medium),
      documentation: Some("rules/practice/example".into()),
      message: "prefer a library helper".into(),
      help: None,
      file: "App.vue".into(),
      span: SourceSpan { offset: 0, length: 1, line: 1, column: 1 },
      edits: Vec::new(),
      recommendation: Some(Recommendation {
        kind: "ecosystem_api".into(),
        package: "@vueuse/core".into(),
        export: "useDebounceFn".into(),
        docs_url: "https://vueuse.org/core/useDebounceFn/".into(),
        import_example: "import { useDebounceFn } from '@vueuse/core'".into(),
      }),
    };
    let summary = ScanSummary { files_scanned: 1, diagnostics: vec![practice], score: 0 }.finish();
    assert_eq!(summary.score, 100);
    assert!(!summary.fails(true));
  }

  #[test]
  fn diagnostic_identity_is_stable_and_tracks_user_visible_content() {
    let diagnostic = Diagnostic {
      rule_id: "vue-vet/test/rule".into(),
      category: "test".into(),
      severity: Severity::Warning,
      confidence: Some(Confidence::High),
      documentation: Some("rules/test/rule".into()),
      message: "finding".into(),
      help: None,
      file: "ignored-absolute-path/App.vue".into(),
      span: SourceSpan { offset: 8, length: 3, line: 2, column: 4 },
      edits: Vec::new(),
      recommendation: None,
    };
    let first = diagnostic_id(&diagnostic, "src/App.vue");
    let second = diagnostic_id(&diagnostic, "src/App.vue");
    assert_eq!(first, second, "unchanged findings must retain their identity");
    assert!(
      first.starts_with("src/App.vue::2:4::vue-vet/test/rule::"),
      "the opaque identity must retain a useful normalized prefix"
    );

    let mut changed = diagnostic;
    changed.severity = Severity::Error;
    assert_ne!(
      first,
      diagnostic_id(&changed, "src/App.vue"),
      "a user-visible severity change must produce a distinct identity"
    );
  }

  #[test]
  fn parses_vue_dependency_requirements() {
    assert_eq!(
      VueVersion::parse_requirement("workspace:^3.5.13"),
      Some(VueVersion { major: 3, minor: 5, patch: 13 })
    );
    assert!(VueVersion::parse_requirement("latest").is_none());
    assert!(
      VueVersion::parse_requirement("~3.4").is_some_and(|version| !version.is_at_least(3, 5))
    );
  }

  #[test]
  fn join_template_reads_accepts_optional_member_chains_on_instances() {
    let mut graph = ReactivityGraph::default();
    let mut shape = std::collections::BTreeMap::new();
    shape.insert("signal".into(), ReactiveBindingKind::Ref);
    graph.composable_instances.insert("bag".into(), shape);
    graph.join_template_reads(&TemplateFacts {
      elements: Vec::new(),
      expressions: vec![
        TemplateExpressionFact {
          surface: "interpolation".into(),
          expression: "bag?.signal".into(),
          span: SourceSpan { offset: 10, length: 12, line: 1, column: 11 },
          identifiers: None,
        },
        TemplateExpressionFact {
          surface: "interpolation".into(),
          expression: "bag?.signal?.value".into(),
          span: SourceSpan { offset: 30, length: 18, line: 2, column: 1 },
          identifiers: None,
        },
        TemplateExpressionFact {
          surface: "interpolation".into(),
          expression: "bag?.[signal]".into(),
          span: SourceSpan { offset: 60, length: 13, line: 3, column: 1 },
          identifiers: None,
        },
      ],
    });
    assert!(
      graph.template_reads.iter().any(|read| read.binding == "signal" && read.span.offset == 10),
      "bag?.signal must join instance field; got {:?}",
      graph.template_reads
    );
    assert!(
      graph.template_reads.iter().any(|read| read.binding == "signal" && read.span.offset == 30),
      "bag?.signal?.value must join instance field; got {:?}",
      graph.template_reads
    );
    assert!(
      !graph.template_reads.iter().any(|read| read.span.offset == 60),
      "computed optional brackets must stay quiet; got {:?}",
      graph.template_reads
    );
  }

  #[test]
  fn rule_registry_orders_rules_by_stable_id() {
    let registry = RuleRegistry::new(vec![&Z_RULE, &A_RULE]);
    let ids = registry.metadata().into_iter().map(|meta| meta.id).collect::<Vec<_>>();
    assert_eq!(ids, ["vue-vet/test/a", "vue-vet/test/z"]);
  }
}
