use std::{
  fmt::Write,
  path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod edits;

pub use edits::{ByteRange, EditApplicability, EditPlan, EditPlanError, TextEdit};

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

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
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
  pub confidence: Option<Confidence>,
  pub documentation: Option<String>,
  pub message: String,
  pub help: Option<String>,
  pub file: PathBuf,
  pub span: SourceSpan,
}

/// Builds the stable, opaque identity used by machine-readable report consumers.
///
/// The caller supplies a repository-relative path because only the orchestration
/// layer knows the scan root. The content digest changes with user-visible
/// severity or message changes while the readable prefix keeps triage practical.
#[must_use]
pub fn diagnostic_id(diagnostic: &Diagnostic, normalized_file_path: &str) -> String {
  let mut hasher = Sha256::new();
  let severity = match diagnostic.severity {
    Severity::Info => "info",
    Severity::Warning => "warning",
    Severity::Error => "error",
  };
  hash_identity_field(&mut hasher, b"severity", severity.as_bytes());
  hash_identity_field(&mut hasher, b"message", diagnostic.message.as_bytes());
  let digest = hex_digest(&hasher.finalize());
  format!(
    "{normalized_file_path}::{}:{}::{}::{digest}",
    diagnostic.span.line, diagnostic.span.column, diagnostic.rule_id
  )
}

fn hash_identity_field(hasher: &mut Sha256, name: &[u8], value: &[u8]) {
  let name_length = u64::try_from(name.len()).unwrap_or(u64::MAX);
  let value_length = u64::try_from(value.len()).unwrap_or(u64::MAX);
  hasher.update(name_length.to_le_bytes());
  hasher.update(name);
  hasher.update(value_length.to_le_bytes());
  hasher.update(value);
}

fn hex_digest(bytes: &[u8]) -> String {
  let mut output = String::with_capacity(bytes.len().saturating_mul(2));
  for byte in bytes {
    if write!(&mut output, "{byte:02x}").is_err() {
      break;
    }
  }
  output
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemplateDirectiveFact {
  pub name: String,
  pub raw_name: String,
  pub argument: Option<String>,
  pub expression: Option<String>,
  pub modifiers: Vec<String>,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemplateAttributeFact {
  pub name: String,
  pub value: Option<String>,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

/// One template expression surface that may read script bindings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemplateExpressionFact {
  /// Where the expression appears (`if`, `for`, `bind`, `on`, `interpolation`, …).
  pub surface: String,
  /// Raw expression text.
  pub expression: String,
  /// Exact SFC-absolute span of the expression when known.
  pub span: SourceSpan,
  /// Free identifier reads when resolved (`Some`, possibly empty). `None` means
  /// unknown and join may fall back to a lexical scan (hand-built fixtures).
  #[serde(default)]
  pub identifiers: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemplateFacts {
  pub elements: Vec<TemplateElementFact>,
  /// Flattened expression surfaces (directives + interpolations) with spans.
  #[serde(default)]
  pub expressions: Vec<TemplateExpressionFact>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptKind {
  Script,
  Setup,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptImportFact {
  pub source: String,
  pub imported: String,
  pub local: String,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptBindingFact {
  pub name: String,
  pub reads: usize,
  pub writes: usize,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptCallFact {
  pub callee: String,
  pub assigned_to: Option<String>,
  pub resolved_import: Option<(String, String)>,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptMemberWriteFact {
  pub object: String,
  pub property: Option<String>,
  pub span: SourceSpan,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReactiveBindingKind {
  Ref,
  ShallowRef,
  Computed,
  Reactive,
  ShallowReactive,
  Readonly,
  ShallowReadonly,
  CustomRef,
  ToRef,
  TemplateRef,
  ModelRef,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReactiveBindingFact {
  pub name: String,
  pub kind: ReactiveBindingKind,
  pub initialized_with_null: bool,
  pub span: SourceSpan,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReactiveReadKind {
  /// Reached on every synchronous execution of the tracking scope.
  Unconditional,
  /// Reached only when control-flow guards pass.
  Conditional,
  /// Occurs after a top-level `await` that ends Vue's synchronous collection.
  AfterAwait,
  /// Occurs outside synchronous tracking (e.g. `then` / `nextTick` callbacks).
  OutsideTracking,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReactiveGuardRole {
  /// `if (test) return` (or equivalent) before the read.
  EarlyExit,
  /// The read sits in a branch controlled by this test.
  #[default]
  BranchTest,
  /// Short-circuit right-hand side guarded by the left-hand expression.
  ShortCircuit,
  /// The read sits in a `switch` case controlled by the discriminant.
  SwitchDiscriminant,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackingScopeKind {
  WatchEffect,
  WatchPostEffect,
  WatchSyncEffect,
  Computed,
  /// Explicit `watch(...)` source list / getter (tracked).
  WatchSources,
  /// `watch` callback body (not tracked for invalidation; side-effect surface).
  WatchCallback,
  /// `effectScope().run(...)` or `effectScope(() => ...)` callback region.
  EffectScope,
  /// `onScopeDispose(() => ...)` cleanup (not dependency-tracking).
  OnScopeDispose,
}

impl TrackingScopeKind {
  /// Effect-family scopes project into the legacy `effects` field.
  #[must_use]
  pub const fn is_effect_family(self) -> bool {
    matches!(self, Self::WatchEffect | Self::WatchPostEffect | Self::WatchSyncEffect)
  }

  /// Scopes whose reactive reads participate in Vue dependency collection.
  #[must_use]
  pub const fn tracks_dependencies(self) -> bool {
    matches!(
      self,
      Self::WatchEffect
        | Self::WatchPostEffect
        | Self::WatchSyncEffect
        | Self::Computed
        | Self::WatchSources
        | Self::EffectScope
    )
  }

  #[must_use]
  pub const fn as_callee(self) -> &'static str {
    match self {
      Self::WatchEffect => "watchEffect",
      Self::WatchPostEffect => "watchPostEffect",
      Self::WatchSyncEffect => "watchSyncEffect",
      Self::Computed => "computed",
      Self::WatchSources | Self::WatchCallback => "watch",
      Self::EffectScope => "effectScope",
      Self::OnScopeDispose => "onScopeDispose",
    }
  }

  #[must_use]
  pub fn from_vue_callee(callee: &str) -> Option<Self> {
    match callee {
      "watchEffect" => Some(Self::WatchEffect),
      "watchPostEffect" => Some(Self::WatchPostEffect),
      "watchSyncEffect" => Some(Self::WatchSyncEffect),
      "computed" => Some(Self::Computed),
      "watch" => Some(Self::WatchSources),
      "effectScope" => Some(Self::EffectScope),
      "onScopeDispose" => Some(Self::OnScopeDispose),
      _ => None,
    }
  }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReactiveDependencyKind {
  /// `const x = computed(() => …)` depends on reads inside the getter.
  Computed,
  /// Effect-family scope depends on its tracked reads.
  Effect,
  /// Template expression mentions a script reactive binding.
  Template,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReactiveGuardFact {
  pub binding: String,
  pub property: Option<String>,
  pub span: SourceSpan,
  #[serde(default)]
  pub role: ReactiveGuardRole,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReactiveReadFact {
  pub binding: String,
  pub property: Option<String>,
  pub kind: ReactiveReadKind,
  pub guards: Vec<ReactiveGuardFact>,
  /// Compatibility shortcut for consumers that only understand one guard.
  pub guarded_by: Option<String>,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReactiveWriteFact {
  pub binding: String,
  pub property: Option<String>,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrackingScopeFact {
  pub kind: TrackingScopeKind,
  /// Canonical Vue callee name (`watchEffect`, `computed`, `watch`, …).
  pub callee: String,
  pub span: SourceSpan,
  pub reads: Vec<ReactiveReadFact>,
  /// Reactive member writes inside the scope (e.g. `derived.value = …`).
  #[serde(default)]
  pub writes: Vec<ReactiveWriteFact>,
  /// Every statement is an assignment expression statement (no calls/awaits/control).
  #[serde(default)]
  pub assignment_only: bool,
  /// For `computed` scopes: the binding name assigned from that call, when known.
  #[serde(default)]
  pub binding: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReactiveDependencyEdge {
  /// Dependent binding or synthetic scope label.
  pub from: String,
  /// Dependency binding that `from` reads.
  pub to: String,
  pub kind: ReactiveDependencyKind,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemplateReactiveReadFact {
  pub binding: String,
  pub span: SourceSpan,
  /// Template surface that mentioned the binding (`if`, `for`, `bind`, `on`, `text`, …).
  pub surface: String,
}

/// Legacy projection of effect-family tracking scopes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReactivityEffectFact {
  pub callee: String,
  pub span: SourceSpan,
  pub reads: Vec<ReactiveReadFact>,
}

/// Wire format version for [`ReactivityGraph`]. Bump when consumers must
/// distinguish shape or semantic changes in serialized facts.
pub const REACTIVITY_GRAPH_VERSION: u32 = 4;

const fn default_reactivity_graph_version() -> u32 {
  1
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReactivityGraph {
  /// Fact-schema version. Absent/legacy payloads deserialize as `1`.
  #[serde(default = "default_reactivity_graph_version")]
  pub version: u32,
  pub bindings: Vec<ReactiveBindingFact>,
  /// All tracking scopes (effects, computed, watch sources/callbacks, …).
  #[serde(default)]
  pub scopes: Vec<TrackingScopeFact>,
  /// Backward-compatible projection of effect-family scopes.
  pub effects: Vec<ReactivityEffectFact>,
  /// Inverted dependency edges (computed/effect/template → binding).
  #[serde(default)]
  pub edges: Vec<ReactiveDependencyEdge>,
  /// Template expressions joined onto script reactive bindings.
  #[serde(default)]
  pub template_reads: Vec<TemplateReactiveReadFact>,
}

impl Default for ReactivityGraph {
  fn default() -> Self {
    Self {
      version: REACTIVITY_GRAPH_VERSION,
      bindings: Vec::new(),
      scopes: Vec::new(),
      effects: Vec::new(),
      edges: Vec::new(),
      template_reads: Vec::new(),
    }
  }
}

/// Stable-enough `from` label for a tracking scope in the inverted edge list.
fn scope_edge_from(scope: &TrackingScopeFact) -> String {
  if let Some(binding) = &scope.binding {
    return binding.clone();
  }
  let kind = match scope.kind {
    TrackingScopeKind::WatchEffect
    | TrackingScopeKind::WatchPostEffect
    | TrackingScopeKind::WatchSyncEffect => "effect",
    TrackingScopeKind::Computed => "computed",
    TrackingScopeKind::WatchSources => "watch_sources",
    TrackingScopeKind::WatchCallback => "watch_callback",
    TrackingScopeKind::EffectScope => "effect_scope",
    TrackingScopeKind::OnScopeDispose => "on_scope_dispose",
  };
  format!("{kind}:{}@{}", scope.callee, scope.span.offset)
}

impl ReactivityGraph {
  /// Rebuild the legacy `effects` projection and dependency edges from `scopes`.
  pub fn project_effects_from_scopes(&mut self) {
    self.version = REACTIVITY_GRAPH_VERSION;
    self.effects = self
      .scopes
      .iter()
      .filter(|scope| scope.kind.is_effect_family())
      .map(|scope| ReactivityEffectFact {
        callee: scope.callee.clone(),
        span: scope.span.clone(),
        reads: scope.reads.clone(),
      })
      .collect();
    self.rebuild_dependency_edges();
  }

  /// Join template expression text onto known script reactive bindings.
  ///
  /// High-confidence under-approximation: only identifiers that exactly match
  /// binding names are linked. Prefer flattened [`TemplateFacts::expressions`]
  /// (Vize interpolations + directive exp/arg with expression-absolute spans);
  /// fall back to element directives for hand-built fixtures that omit that list.
  ///
  /// Vize supplies expression text + spans; Oxc-backed adapters should fill
  /// [`TemplateExpressionFact::identifiers`] as `Some(...)` (empty means “no
  /// free reads”). `None` keeps the lexical fallback for hand-built fixtures.
  pub fn join_template_reads(&mut self, template: &TemplateFacts) {
    let binding_names = self
      .bindings
      .iter()
      .map(|binding| binding.name.as_str())
      .collect::<std::collections::BTreeSet<_>>();
    let mut template_reads = Vec::new();
    if template.expressions.is_empty() {
      for element in &template.elements {
        for directive in &element.directives {
          let Some(expression) = directive.expression.as_deref() else {
            continue;
          };
          let surface = if directive.name == "bind" {
            directive.argument.clone().unwrap_or_else(|| "bind".into())
          } else {
            directive.name.clone()
          };
          let identifiers = template_expression_identifiers(expression);
          push_template_reads(
            &mut template_reads,
            &binding_names,
            &identifiers,
            &surface,
            &directive.span,
          );
        }
      }
    } else {
      for expression in &template.expressions {
        let fallback = expression
          .identifiers
          .is_none()
          .then(|| template_expression_identifiers(&expression.expression));
        let identifiers = expression.identifiers.as_deref().or(fallback.as_deref()).unwrap_or(&[]);
        push_template_reads(
          &mut template_reads,
          &binding_names,
          identifiers,
          &expression.surface,
          &expression.span,
        );
      }
    }
    template_reads.sort_by(|left, right| {
      (left.binding.as_str(), left.surface.as_str(), left.span.offset).cmp(&(
        right.binding.as_str(),
        right.surface.as_str(),
        right.span.offset,
      ))
    });
    template_reads.dedup_by(|left, right| {
      left.binding == right.binding
        && left.surface == right.surface
        && left.span.offset == right.span.offset
    });
    self.template_reads = template_reads;
    self.rebuild_dependency_edges();
  }

  /// Rebuild computed/effect dependency edges from scopes and template reads.
  pub fn rebuild_dependency_edges(&mut self) {
    let mut edges = Vec::new();
    for scope in &self.scopes {
      if !scope.kind.tracks_dependencies() {
        continue;
      }
      // Prefer stable computed binding names; otherwise qualify by kind+callee+span
      // so multiple effects do not share an ambiguous bare callee label.
      let from = scope_edge_from(scope);
      let kind = if scope.kind == TrackingScopeKind::Computed {
        ReactiveDependencyKind::Computed
      } else {
        ReactiveDependencyKind::Effect
      };
      for read in &scope.reads {
        if matches!(read.kind, ReactiveReadKind::AfterAwait | ReactiveReadKind::OutsideTracking) {
          continue;
        }
        edges.push(ReactiveDependencyEdge {
          from: from.clone(),
          // Binding name only for now — consumers (e.g. unused-binding) match on it.
          // Module/symbol-qualified IDs remain a follow-up contract step.
          to: read.binding.clone(),
          kind,
          span: read.span.clone(),
        });
      }
    }
    for template_read in &self.template_reads {
      edges.push(ReactiveDependencyEdge {
        // Span-qualified so multiple interpolations are distinct nodes.
        from: format!("template:{}@{}", template_read.surface, template_read.span.offset),
        to: template_read.binding.clone(),
        kind: ReactiveDependencyKind::Template,
        span: template_read.span.clone(),
      });
    }
    edges.sort_by(|left, right| {
      (left.kind, left.from.as_str(), left.to.as_str(), left.span.offset).cmp(&(
        right.kind,
        right.from.as_str(),
        right.to.as_str(),
        right.span.offset,
      ))
    });
    edges.dedup_by(|left, right| {
      left.from == right.from
        && left.to == right.to
        && left.kind == right.kind
        && left.span.offset == right.span.offset
    });
    self.edges = edges;
  }
}

fn push_template_reads(
  template_reads: &mut Vec<TemplateReactiveReadFact>,
  binding_names: &std::collections::BTreeSet<&str>,
  identifiers: &[String],
  surface: &str,
  span: &SourceSpan,
) {
  for identifier in identifiers {
    if binding_names.contains(identifier.as_str()) {
      template_reads.push(TemplateReactiveReadFact {
        binding: identifier.clone(),
        span: span.clone(),
        surface: surface.into(),
      });
    }
  }
}

fn template_expression_identifiers(expression: &str) -> Vec<String> {
  const KEYWORDS: &[&str] = &[
    "true",
    "false",
    "null",
    "undefined",
    "typeof",
    "instanceof",
    "new",
    "void",
    "in",
    "of",
    "if",
    "else",
    "return",
    "const",
    "let",
    "var",
    "function",
    "this",
    "as",
    "await",
    "async",
  ];
  let mut identifiers = Vec::new();
  let mut current = String::new();
  for character in expression.chars() {
    if character.is_ascii_alphanumeric() || character == '_' || character == '$' {
      current.push(character);
    } else if !current.is_empty() {
      if current.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && !KEYWORDS.contains(&current.as_str())
      {
        identifiers.push(std::mem::take(&mut current));
      } else {
        current.clear();
      }
    }
  }
  if !current.is_empty()
    && current.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_' || c == '$')
    && !KEYWORDS.contains(&current.as_str())
  {
    identifiers.push(current);
  }
  identifiers
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptDestructureFact {
  pub source_call: String,
  pub span: SourceSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptBlockFacts {
  pub kind: ScriptKind,
  pub language: String,
  pub imports: Vec<ScriptImportFact>,
  pub bindings: Vec<ScriptBindingFact>,
  pub calls: Vec<ScriptCallFact>,
  pub member_writes: Vec<ScriptMemberWriteFact>,
  pub destructures: Vec<ScriptDestructureFact>,
  pub reactivity_graph: ReactivityGraph,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScriptFacts {
  pub blocks: Vec<ScriptBlockFacts>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SfcFacts {
  pub template: TemplateFacts,
  pub script: ScriptFacts,
}

impl SfcFacts {
  /// Replace the preferred script block's reactivity graph with a project-linked
  /// module graph (usually after cross-file seed linking). Prefers `script setup`.
  pub fn apply_module_reactivity(&mut self, graph: ReactivityGraph) {
    if let Some(block) = self.script.blocks.iter_mut().find(|block| block.kind == ScriptKind::Setup)
    {
      block.reactivity_graph = graph;
      return;
    }
    if let Some(block) = self.script.blocks.first_mut() {
      block.reactivity_graph = graph;
    }
  }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VueVersion {
  pub major: u64,
  pub minor: u64,
  pub patch: u64,
}

impl VueVersion {
  #[must_use]
  pub fn parse_requirement(value: &str) -> Option<Self> {
    let version = value
      .split(|character: char| !character.is_ascii_digit() && character != '.')
      .find(|part| !part.is_empty())?;
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    Some(Self { major, minor, patch })
  }

  #[must_use]
  pub const fn is_at_least(self, major: u64, minor: u64) -> bool {
    self.major > major || (self.major == major && self.minor >= minor)
  }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuleEnvironment {
  pub vue_version: Option<VueVersion>,
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

  #[must_use]
  pub const fn environment(&self) -> RuleEnvironment {
    self.environment
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
      confidence: Some(meta.confidence),
      documentation: Some(meta.documentation.into()),
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
    for rule in &self.rules {
      let mut context =
        RuleContext::new(file, source, template, script, environment, &mut diagnostics);
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
      confidence: Some(Confidence::High),
      documentation: Some("rules/test/rule".into()),
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
  fn rule_registry_orders_rules_by_stable_id() {
    let registry = RuleRegistry::new(vec![&Z_RULE, &A_RULE]);
    let ids = registry.metadata().into_iter().map(|meta| meta.id).collect::<Vec<_>>();
    assert_eq!(ids, ["vue-vet/test/a", "vue-vet/test/z"]);
  }
}
