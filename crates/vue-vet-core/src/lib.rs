use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
  Info,
  Warning,
  Error,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
  High,
  Medium,
  Low,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct RuleMeta {
  pub id: &'static str,
  pub category: &'static str,
  pub default_severity: Severity,
  pub confidence: Confidence,
  pub documentation: &'static str,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceSpan {
  pub offset: usize,
  pub length: usize,
  pub line: usize,
  pub column: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Diagnostic {
  pub rule_id: String,
  pub category: String,
  pub severity: Severity,
  pub message: String,
  pub help: Option<String>,
  pub file: PathBuf,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateDirectiveFact {
  pub name: String,
  pub raw_name: String,
  pub argument: Option<String>,
  pub expression: Option<String>,
  pub modifiers: Vec<String>,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateAttributeFact {
  pub name: String,
  pub value: Option<String>,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateElementFact {
  pub tag: String,
  pub span: SourceSpan,
  pub attributes: Vec<TemplateAttributeFact>,
  pub directives: Vec<TemplateDirectiveFact>,
  pub has_children: bool,
}

impl TemplateElementFact {
  #[must_use]
  pub fn attribute(&self, name: &str) -> Option<&TemplateAttributeFact> {
    self.attributes.iter().find(|attribute| attribute.name.eq_ignore_ascii_case(name))
  }

  #[must_use]
  pub fn directive(&self, name: &str) -> Option<&TemplateDirectiveFact> {
    self.directives.iter().find(|directive| directive.name == name)
  }

  #[must_use]
  pub fn bound_attribute(&self, name: &str) -> Option<&TemplateDirectiveFact> {
    self.directives.iter().find(|directive| {
      directive.name == "bind"
        && directive.argument.as_deref().is_some_and(|argument| argument.eq_ignore_ascii_case(name))
    })
  }

  #[must_use]
  pub fn event(&self, name: &str) -> Option<&TemplateDirectiveFact> {
    self.directives.iter().find(|directive| {
      directive.name == "on"
        && directive.argument.as_deref().is_some_and(|argument| argument.eq_ignore_ascii_case(name))
    })
  }

  #[must_use]
  pub fn has_key(&self) -> bool {
    self.attribute("key").is_some() || self.bound_attribute("key").is_some()
  }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TemplateFacts {
  pub elements: Vec<TemplateElementFact>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptKind {
  Script,
  Setup,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptImportFact {
  pub source: String,
  pub imported: String,
  pub local: String,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptBindingFact {
  pub name: String,
  pub reads: usize,
  pub writes: usize,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptCallFact {
  pub callee: String,
  pub resolved_import: Option<(String, String)>,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptMemberWriteFact {
  pub object: String,
  pub property: Option<String>,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptBlockFacts {
  pub kind: ScriptKind,
  pub language: String,
  pub imports: Vec<ScriptImportFact>,
  pub bindings: Vec<ScriptBindingFact>,
  pub calls: Vec<ScriptCallFact>,
  pub member_writes: Vec<ScriptMemberWriteFact>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScriptFacts {
  pub blocks: Vec<ScriptBlockFacts>,
}

pub trait Rule: Sync {
  fn meta(&self) -> &'static RuleMeta;
  fn run(&self, context: &mut RuleContext<'_>);
}

pub struct RuleContext<'a> {
  file: &'a Path,
  source: &'a str,
  template: &'a TemplateFacts,
  script: &'a ScriptFacts,
  diagnostics: &'a mut Vec<Diagnostic>,
}

impl<'a> RuleContext<'a> {
  pub const fn new(
    file: &'a Path,
    source: &'a str,
    template: &'a TemplateFacts,
    script: &'a ScriptFacts,
    diagnostics: &'a mut Vec<Diagnostic>,
  ) -> Self {
    Self { file, source, template, script, diagnostics }
  }

  #[must_use]
  pub const fn source(&self) -> &str {
    self.source
  }

  #[must_use]
  pub const fn template(&self) -> &TemplateFacts {
    self.template
  }

  #[must_use]
  pub const fn script(&self) -> &ScriptFacts {
    self.script
  }

  pub fn report(
    &mut self,
    meta: &RuleMeta,
    span: SourceSpan,
    message: String,
    help: Option<String>,
  ) {
    self.diagnostics.push(Diagnostic {
      rule_id: meta.id.into(),
      category: meta.category.into(),
      severity: meta.default_severity,
      message,
      help,
      file: self.file.to_path_buf(),
      span,
    });
  }
}

pub struct RuleRegistry {
  rules: Vec<&'static dyn Rule>,
}

impl RuleRegistry {
  #[must_use]
  pub fn new(mut rules: Vec<&'static dyn Rule>) -> Self {
    rules.sort_by_key(|rule| rule.meta().id);
    Self { rules }
  }

  #[must_use]
  pub fn run(
    &self,
    file: &Path,
    source: &str,
    template: &TemplateFacts,
    script: &ScriptFacts,
  ) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for rule in &self.rules {
      let mut context = RuleContext::new(file, source, template, script, &mut diagnostics);
      rule.run(&mut context);
    }
    diagnostics
  }

  #[must_use]
  pub fn metadata(&self) -> Vec<&'static RuleMeta> {
    self.rules.iter().map(|rule| rule.meta()).collect()
  }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScanSummary {
  pub files_scanned: usize,
  pub diagnostics: Vec<Diagnostic>,
  pub score: u8,
}

impl ScanSummary {
  #[must_use]
  pub fn finish(mut self) -> Self {
    self.diagnostics.sort_by(|left, right| {
      (&left.file, left.span.offset, &left.rule_id).cmp(&(
        &right.file,
        right.span.offset,
        &right.rule_id,
      ))
    });

    let penalty = self.diagnostics.iter().fold(0_u16, |total, diagnostic| {
      total
        + match diagnostic.severity {
          Severity::Error => 10,
          Severity::Warning => 3,
          Severity::Info => 1,
        }
    });
    self.score = 100_u16.saturating_sub(penalty).try_into().unwrap_or(0);
    self
  }

  #[must_use]
  pub fn fails(&self, deny_warnings: bool) -> bool {
    self.diagnostics.iter().any(|diagnostic| {
      diagnostic.severity == Severity::Error
        || (deny_warnings && diagnostic.severity == Severity::Warning)
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  struct TestRule(&'static RuleMeta);

  impl Rule for TestRule {
    fn meta(&self) -> &'static RuleMeta {
      self.0
    }

    fn run(&self, _context: &mut RuleContext<'_>) {}
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
  fn score_is_deterministic_and_saturating() {
    let diagnostic = Diagnostic {
      rule_id: "test/rule".into(),
      category: "test".into(),
      severity: Severity::Warning,
      message: "test".into(),
      help: None,
      file: "Component.vue".into(),
      span: SourceSpan { offset: 0, length: 1, line: 1, column: 1 },
    };
    let summary =
      ScanSummary { files_scanned: 1, diagnostics: vec![diagnostic; 40], score: 100 }.finish();

    assert_eq!(summary.score, 0);
    assert!(summary.fails(true));
    assert!(!summary.fails(false));
  }

  #[test]
  fn rule_registry_orders_rules_by_stable_id() {
    let registry = RuleRegistry::new(vec![&Z_RULE, &A_RULE]);
    let ids = registry.metadata().into_iter().map(|meta| meta.id).collect::<Vec<_>>();
    assert_eq!(ids, ["vue-vet/test/a", "vue-vet/test/z"]);
  }
}
