//! Rule trait, registry, and fact visitor surface.

use std::path::Path;

use crate::diagnostics::{Diagnostic, Recommendation, RuleMeta, SourceSpan};
use crate::edits::{ByteRange, EditApplicability, TextEdit};
use crate::facts::{
  ReactiveBindingFact, ReactivityEffectFact, RuleEnvironment, ScriptBindingFact, ScriptCallFact,
  ScriptDestructureFact, ScriptFacts, ScriptKind, ScriptMemberWriteFact, ScriptOperandFact,
  TemplateElementFact, TemplateFacts, TrackingScopeFact,
};
use crate::identity::FileId;

/// Interest set for the single facts pass (oxlint `NODE_TYPES` analogue).
///
/// Rules declare which fact kinds they visit. The registry walks each fact
/// vector once and dispatches only interested rules — no per-rule full scans.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FactKinds(u32);

impl FactKinds {
  pub const NONE: Self = Self(0);
  pub const TEMPLATE_ELEMENT: Self = Self(1 << 0);
  pub const SCRIPT_CALL: Self = Self(1 << 1);
  pub const SCRIPT_MEMBER_WRITE: Self = Self(1 << 2);
  pub const SCRIPT_DESTRUCTURE: Self = Self(1 << 3);
  pub const SCRIPT_BINDING: Self = Self(1 << 4);
  pub const REACTIVE_BINDING: Self = Self(1 << 5);
  pub const TRACKING_SCOPE: Self = Self(1 << 6);
  pub const REACTIVITY_EFFECT: Self = Self(1 << 7);
  pub const SCRIPT_OPERAND: Self = Self(1 << 8);

  #[must_use]
  pub const fn union(self, other: Self) -> Self {
    Self(self.0 | other.0)
  }

  #[must_use]
  pub const fn contains(self, kind: Self) -> bool {
    self.0 & kind.0 == kind.0 && kind.0 != 0
  }

  #[must_use]
  pub const fn is_empty(self) -> bool {
    self.0 == 0
  }
}

/// One fact in the single pass over Vue Vet-owned surfaces (not dependency AST).
#[derive(Clone, Copy, Debug)]
pub enum FactRef<'a> {
  TemplateElement(&'a TemplateElementFact),
  ScriptCall { block_kind: ScriptKind, call: &'a ScriptCallFact },
  ScriptMemberWrite { block_kind: ScriptKind, write: &'a ScriptMemberWriteFact },
  ScriptDestructure { block_kind: ScriptKind, destructure: &'a ScriptDestructureFact },
  ScriptBinding { block_kind: ScriptKind, binding: &'a ScriptBindingFact },
  ReactiveBinding { block_kind: ScriptKind, binding: &'a ReactiveBindingFact },
  TrackingScope { block_kind: ScriptKind, scope: &'a TrackingScopeFact },
  ReactivityEffect { block_kind: ScriptKind, effect: &'a ReactivityEffectFact },
  ScriptOperand { block_kind: ScriptKind, operand: &'a ScriptOperandFact },
}

/// Built-in rule contract (oxlint-style pass hooks over stable facts).
///
/// - [`Self::run_once`]: whole-file / cross-fact aggregation (oxlint `run_once`)
/// - [`Self::run_on`]: per-fact visitor during the single facts pass (oxlint `run`)
/// - [`Self::fact_kinds`]: interest bitset for bucketed dispatch (oxlint `NODE_TYPES`)
///
/// Prefer `run_on` + immediate `report`. Do not `collect` intermediate vectors
/// and re-scan them. Use `run_once` only when the rule needs multi-fact state.
pub trait Rule: Sync {
  fn meta(&self) -> &'static RuleMeta;

  /// Facts this rule visits. Empty means only [`Self::run_once`] runs.
  fn fact_kinds(&self) -> FactKinds {
    FactKinds::NONE
  }

  /// File-level pass. Default no-op.
  fn run_once(&self, _context: &mut RuleContext<'_>) {}

  /// Per-fact pass. Default no-op. Report immediately; do not buffer findings.
  fn run_on(&self, _fact: FactRef<'_>, _context: &mut RuleContext<'_>) {}
}

pub struct RuleContext<'a> {
  file: &'a Path,
  source: &'a str,
  template: &'a TemplateFacts,
  script: &'a ScriptFacts,
  environment: RuleEnvironment,
  diagnostics: &'a mut Vec<Diagnostic>,
}

impl<'a> RuleContext<'a> {
  pub const fn new(
    file: &'a Path,
    source: &'a str,
    template: &'a TemplateFacts,
    script: &'a ScriptFacts,
    environment: RuleEnvironment,
    diagnostics: &'a mut Vec<Diagnostic>,
  ) -> Self {
    Self { file, source, template, script, environment, diagnostics }
  }

  #[must_use]
  pub const fn file(&self) -> &'a Path {
    self.file
  }

  #[must_use]
  pub const fn source(&self) -> &'a str {
    self.source
  }

  /// Template facts for this file. Same stored lifetime as [`Self::script`].
  #[must_use]
  pub const fn template(&self) -> &'a TemplateFacts {
    self.template
  }

  /// Script facts for this file. The lifetime is the stored borrow, not `&self`,
  /// so `run_once` can walk helpers and `report` without collecting clones.
  #[must_use]
  pub const fn script(&self) -> &'a ScriptFacts {
    self.script
  }

  #[must_use]
  pub const fn environment(&self) -> &RuleEnvironment {
    &self.environment
  }

  pub fn report(
    &mut self,
    meta: &RuleMeta,
    span: SourceSpan,
    message: String,
    help: Option<String>,
  ) {
    self.push_diagnostic(meta, span, message, help, Vec::new(), None);
  }

  pub fn report_with_recommendation(
    &mut self,
    meta: &RuleMeta,
    span: SourceSpan,
    message: String,
    help: Option<String>,
    recommendation: Recommendation,
  ) {
    self.push_diagnostic(meta, span, message, help, Vec::new(), Some(recommendation));
  }

  pub fn report_with_safe_edit(
    &mut self,
    meta: &RuleMeta,
    span: SourceSpan,
    message: String,
    help: Option<String>,
    range: ByteRange,
    replacement: String,
  ) {
    let edit = TextEdit {
      file: FileId::from(self.file),
      range,
      replacement,
      applicability: EditApplicability::Safe,
      rule_id: meta.id.into(),
    };
    self.push_diagnostic(meta, span, message, help, vec![edit], None);
  }

  fn push_diagnostic(
    &mut self,
    meta: &RuleMeta,
    span: SourceSpan,
    message: String,
    help: Option<String>,
    edits: Vec<TextEdit>,
    recommendation: Option<Recommendation>,
  ) {
    self.diagnostics.push(Diagnostic {
      rule_id: meta.id.into(),
      category: meta.category.into(),
      severity: meta.default_severity,
      confidence: Some(meta.confidence),
      documentation: Some(meta.documentation.into()),
      message,
      help,
      file: FileId::from(self.file),
      span,
      edits,
      recommendation,
    });
  }
}

/// Per-kind rule buckets for the single facts pass (oxlint `RuleBuckets` analogue).
#[derive(Default)]
struct FactBuckets {
  template_element: Vec<&'static dyn Rule>,
  script_call: Vec<&'static dyn Rule>,
  script_member_write: Vec<&'static dyn Rule>,
  script_destructure: Vec<&'static dyn Rule>,
  script_binding: Vec<&'static dyn Rule>,
  reactive_binding: Vec<&'static dyn Rule>,
  tracking_scope: Vec<&'static dyn Rule>,
  reactivity_effect: Vec<&'static dyn Rule>,
  script_operand: Vec<&'static dyn Rule>,
}

impl FactBuckets {
  fn push(&mut self, kinds: FactKinds, rule: &'static dyn Rule) {
    if kinds.contains(FactKinds::TEMPLATE_ELEMENT) {
      self.template_element.push(rule);
    }
    if kinds.contains(FactKinds::SCRIPT_CALL) {
      self.script_call.push(rule);
    }
    if kinds.contains(FactKinds::SCRIPT_MEMBER_WRITE) {
      self.script_member_write.push(rule);
    }
    if kinds.contains(FactKinds::SCRIPT_DESTRUCTURE) {
      self.script_destructure.push(rule);
    }
    if kinds.contains(FactKinds::SCRIPT_BINDING) {
      self.script_binding.push(rule);
    }
    if kinds.contains(FactKinds::REACTIVE_BINDING) {
      self.reactive_binding.push(rule);
    }
    if kinds.contains(FactKinds::TRACKING_SCOPE) {
      self.tracking_scope.push(rule);
    }
    if kinds.contains(FactKinds::REACTIVITY_EFFECT) {
      self.reactivity_effect.push(rule);
    }
    if kinds.contains(FactKinds::SCRIPT_OPERAND) {
      self.script_operand.push(rule);
    }
  }

  fn needs_script_pass(&self) -> bool {
    !self.script_call.is_empty()
      || !self.script_member_write.is_empty()
      || !self.script_destructure.is_empty()
      || !self.script_binding.is_empty()
      || !self.reactive_binding.is_empty()
      || !self.tracking_scope.is_empty()
      || !self.reactivity_effect.is_empty()
      || !self.script_operand.is_empty()
  }
}

pub struct RuleRegistry {
  rules: Vec<&'static dyn Rule>,
  /// Rules that implement file-level aggregation.
  once_rules: Vec<&'static dyn Rule>,
  /// Per-kind buckets for the single facts pass.
  buckets: FactBuckets,
}

impl RuleRegistry {
  #[must_use]
  pub fn new(mut rules: Vec<&'static dyn Rule>) -> Self {
    rules.sort_by_key(|rule| rule.meta().id);
    let mut once_rules = Vec::new();
    let mut buckets = FactBuckets::default();
    for rule in &rules {
      // Always schedule run_once; no-op default is free. Rules that only use
      // run_on simply leave it empty.
      once_rules.push(*rule);
      buckets.push(rule.fact_kinds(), *rule);
    }
    Self { rules, once_rules, buckets }
  }

  #[must_use]
  pub fn run(
    &self,
    file: &Path,
    source: &str,
    template: &TemplateFacts,
    script: &ScriptFacts,
  ) -> Vec<Diagnostic> {
    self.run_with_environment(file, source, template, script, RuleEnvironment::default())
  }

  #[must_use]
  pub fn run_with_environment(
    &self,
    file: &Path,
    source: &str,
    template: &TemplateFacts,
    script: &ScriptFacts,
    environment: RuleEnvironment,
  ) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut context =
      RuleContext::new(file, source, template, script, environment, &mut diagnostics);

    // Pass 1: file-level hooks (oxlint run_once).
    for rule in &self.once_rules {
      rule.run_once(&mut context);
    }

    // Pass 2: single walk over each fact surface with type-bucketed dispatch.
    if !self.buckets.template_element.is_empty() {
      for element in &template.elements {
        let fact = FactRef::TemplateElement(element);
        for rule in &self.buckets.template_element {
          rule.run_on(fact, &mut context);
        }
      }
    }

    if self.buckets.needs_script_pass() {
      for block in &script.blocks {
        if !self.buckets.script_call.is_empty() {
          for call in &block.calls {
            let fact = FactRef::ScriptCall { block_kind: block.kind, call };
            for rule in &self.buckets.script_call {
              rule.run_on(fact, &mut context);
            }
          }
        }
        if !self.buckets.script_member_write.is_empty() {
          for write in &block.member_writes {
            let fact = FactRef::ScriptMemberWrite { block_kind: block.kind, write };
            for rule in &self.buckets.script_member_write {
              rule.run_on(fact, &mut context);
            }
          }
        }
        if !self.buckets.script_destructure.is_empty() {
          for destructure in &block.destructures {
            let fact = FactRef::ScriptDestructure { block_kind: block.kind, destructure };
            for rule in &self.buckets.script_destructure {
              rule.run_on(fact, &mut context);
            }
          }
        }
        if !self.buckets.script_binding.is_empty() {
          for binding in &block.bindings {
            let fact = FactRef::ScriptBinding { block_kind: block.kind, binding };
            for rule in &self.buckets.script_binding {
              rule.run_on(fact, &mut context);
            }
          }
        }
        if !self.buckets.reactive_binding.is_empty() {
          for binding in &block.reactivity_graph.bindings {
            let fact = FactRef::ReactiveBinding { block_kind: block.kind, binding };
            for rule in &self.buckets.reactive_binding {
              rule.run_on(fact, &mut context);
            }
          }
        }
        if !self.buckets.tracking_scope.is_empty() {
          for scope in &block.reactivity_graph.scopes {
            let fact = FactRef::TrackingScope { block_kind: block.kind, scope };
            for rule in &self.buckets.tracking_scope {
              rule.run_on(fact, &mut context);
            }
          }
        }
        if !self.buckets.reactivity_effect.is_empty() {
          for effect in &block.reactivity_graph.effects {
            let fact = FactRef::ReactivityEffect { block_kind: block.kind, effect };
            for rule in &self.buckets.reactivity_effect {
              rule.run_on(fact, &mut context);
            }
          }
        }
        if !self.buckets.script_operand.is_empty() {
          for operand in &block.operands {
            let fact = FactRef::ScriptOperand { block_kind: block.kind, operand };
            for rule in &self.buckets.script_operand {
              rule.run_on(fact, &mut context);
            }
          }
        }
      }
    }

    diagnostics
  }

  #[must_use]
  pub fn metadata(&self) -> Vec<&'static RuleMeta> {
    self.rules.iter().map(|rule| rule.meta()).collect()
  }
}
