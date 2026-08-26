use serde::{Deserialize, Serialize};

use crate::diagnostics::SourceSpan;

use super::TemplateFacts;

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

impl ReactiveBindingKind {
  /// Kinds that unwrap via `.value` (Vue Ref contract).
  ///
  /// Reactive objects are **not** ref-like — bare reads track the object root;
  /// deep `watch(reactive)` uses [`Self::is_deep_watch_source`] + `property: "*"`.
  #[must_use]
  pub const fn is_ref_like(self) -> bool {
    matches!(
      self,
      Self::Ref
        | Self::ShallowRef
        | Self::Computed
        | Self::CustomRef
        | Self::ToRef
        | Self::TemplateRef
        | Self::ModelRef
    )
  }

  /// Object roots for bare `watch(reactive)` (deep-root sentinel, not per-key invent).
  #[must_use]
  pub const fn is_deep_watch_source(self) -> bool {
    matches!(self, Self::Reactive | Self::ShallowReactive)
  }

  /// Merge two ref-like kinds for ternary / dual-arm Known exports.
  ///
  /// Same kind keeps it; distinct ref-like kinds still share `.value` tracking → [`Self::Ref`].
  /// Callers must only pass ref-like kinds (under-approx: non-ref-like stay quiet upstream).
  #[must_use]
  pub const fn merge_ref_like(self, other: Self) -> Self {
    match (self, other) {
      (Self::Ref, Self::Ref)
      | (Self::ShallowRef, Self::ShallowRef)
      | (Self::Computed, Self::Computed)
      | (Self::CustomRef, Self::CustomRef)
      | (Self::ToRef, Self::ToRef)
      | (Self::TemplateRef, Self::TemplateRef)
      | (Self::ModelRef, Self::ModelRef) => self,
      _ => Self::Ref,
    }
  }
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
  /// Component render function body (options `render`, `setup`→render, functional, …).
  Render,
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
        | Self::Render
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
      Self::Render => "render",
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
  /// Cross-file parent `:prop` → child `props.prop` (static identifier binds only).
  Prop,
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
  /// Every statement is an assignment, or a followed assignment-only local helper
  /// (no awaits/control). Same-file zero-arg helpers count when their bodies are
  /// assignment-only (depth-capped; async/args stay false).
  #[serde(default)]
  pub assignment_only: bool,
  /// For `computed` scopes: the binding name assigned from that call, when known.
  #[serde(default)]
  pub binding: Option<String>,
  /// Identifier roots of `.value` / `unref` / `toValue` that were analyzed but
  /// could not be classified as known reactive bindings (under-approx miss).
  /// Rules may surface these as `(maybe: …)` rather than inventing edges.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub uncertain_accesses: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReactiveDependencyEdge {
  /// Dependent binding or synthetic scope label.
  pub from: String,
  /// Dependency binding name that `from` reads (bare; rules match on this).
  pub to: String,
  /// Span-qualified identity `{name}@{offset}` for multi-consumer disambiguation.
  /// Absent on legacy payloads; equals bare `to` when offset is unknown.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub to_id: Option<String>,
  /// Member path on `to` when the read was `bag.field` (e.g. `props.count`).
  /// Absent for bare binding reads and template joins that only name the binding.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub property: Option<String>,
  pub kind: ReactiveDependencyKind,
  pub span: SourceSpan,
}

impl ReactiveDependencyEdge {
  /// Prefer span-qualified `to_id`, else bare [`Self::to`].
  #[must_use]
  pub fn to_identity(&self) -> &str {
    self.to_id.as_deref().unwrap_or(self.to.as_str())
  }

  /// Display path `to` or `to.property` for inspector / humanize surfaces.
  /// Deep-watch sentinel `*` renders as `{to} (deep)`.
  #[must_use]
  pub fn to_path(&self) -> String {
    match &self.property {
      Some(property) if property == "*" => format!("{} (deep)", self.to),
      Some(property) if !property.is_empty() => format!("{}.{property}", self.to),
      _ => self.to.clone(),
    }
  }
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
///
/// v25: same-file zero-arg helper follow also fills `uncertain_accesses`
/// (unclassified `.value` / `unref` / `toValue` inside followed callees).
/// `then()`/`nextTick`-only call sites stay quiet so absence rules do not
/// invent `(maybe)` for outside-tracking accesses.
/// v34: writes inside sync Array/String HOF callbacks and `toValue(() => …)`
/// getters record the same facts as inlined assignments. `then` / `nextTick`
/// / `setTimeout`, first-arg `Array.from(() => …)`, and identifier
/// `list.map(fn)` stay quiet (dual-path with reads).
/// v33: `bag.field.value = …` on a known composable instance records the
/// same write fact as a destructured `field.value = …` (`binding = field`,
/// `property = "value"`). Replacing the ref (`bag.field = …`), computed
/// keys, unknown bags, and non-ref-like fields stay quiet.
/// v32: options `render: renderFn` and `setup() { return renderFn }` use the
/// same-file function as the Render body (dual-path with inline
/// `render() { … }`). Imports, methods, and async/generator stay quiet.
/// v31: watch-source arguments peel parens / TypeScript wrappers before
/// classifying a ref, getter, or array (`watch((count))` / `watch(count as T)`
/// agree with `watch(count)`). Nested arrays still do not treat inner arrows
/// as source getters.
/// v30: pause/enable/resetTracking inside a followed helper classify the
/// helper's reads (and later sibling reads after the call returns). Vue's
/// `shouldTrack` is process-global, so a helper that ends paused stays
/// paused in the caller. Do not compare helper spans against caller events.
/// v29: compound assignment (`+=` / `-=` / …) and update (`++` / `--`)
/// record the same write facts as `=`. Logical `&&=` / `||=` / `??=` stay
/// quiet (they may not write). `assignment_only` includes update expressions
/// so `watchEffect(() => { n.value++ })` agrees with `n.value += 1`.
/// v28: followed helper reads inherit caller control-flow guards
/// (`computed(() => cond ? load() : 0)` is Conditional, dual-path with
/// `cond ? x.value : 0`). Both-arm helper calls stay Unconditional.
/// v27: same-file local function *references* used as Vue tracking callbacks
/// (`computed(load)`, `watchEffect(load)`, `watch(load)`, `computed({ get: load })`)
/// are the tracking body — dual-path with `computed(() => load())`. Imports,
/// methods, and async/generator stay quiet. Function parameters do not matter
/// (Vue invokes the getter with no args); that differs from helper-follow
/// `load(1)` which stays unfollowed.
/// v26: same helper follow for `writes` and `assignment_only` so
/// `computed(() => load())` / `watchEffect(() => { assign() })` cannot
/// disagree with an inlined body. `then()`/`nextTick`-only helpers stay quiet.
/// v24: `useI18n` translator calls (`t`/`d`/`n`/`rt`/`te`) inject ambient
/// composer deps (`locale` / `fallbackLocale` / `messages`) per vue-i18n
/// `wrapWithDeps` / `trackReactivityValues`.
/// v23: same-file zero-arg local helpers called from a tracking scope contribute
/// ambient sync reads (bounded depth; skip async/generator) — Vue tracks callee
/// reads under `activeEffect`.
/// v22: all-path ternary/if-else same `(binding, property)` → Unconditional (A4
/// under-approx hygiene); export linking refinements that change seeded bindings
/// (`ForwardReturn` bare `#nuxt-imports`, overload Factory≻Composable, ref-like
/// ternary `Known` exports, empty-path pending composable fields).
pub const REACTIVITY_GRAPH_VERSION: u32 = 34;

const fn default_reactivity_graph_version() -> u32 {
  1
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReactivityGraph {
  /// Fact-schema version. Absent/legacy payloads deserialize as `1`.
  #[serde(default = "default_reactivity_graph_version")]
  pub version: u32,
  /// Logical module path used to qualify edge `to_id` (`{module}:{name}@{offset}`).
  /// Empty for anonymous single-script traces (falls back to `{name}@{offset}`).
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub module_id: String,
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
  /// `const bag = useFoo()` locals → composable return field kinds.
  /// Used for script `bag.field.value` and template `bag.field` joins.
  #[serde(default)]
  pub composable_instances:
    std::collections::BTreeMap<String, std::collections::BTreeMap<String, ReactiveBindingKind>>,
}

impl Default for ReactivityGraph {
  fn default() -> Self {
    Self {
      version: REACTIVITY_GRAPH_VERSION,
      module_id: String::new(),
      bindings: Vec::new(),
      scopes: Vec::new(),
      effects: Vec::new(),
      edges: Vec::new(),
      template_reads: Vec::new(),
      composable_instances: std::collections::BTreeMap::new(),
    }
  }
}

/// Build a span-qualified dependency identity, optionally module-prefixed (graph v8).
#[must_use]
pub fn qualify_dependency_to_id(module_id: &str, name: &str, offset: usize) -> String {
  if module_id.is_empty() {
    format!("{name}@{offset}")
  } else {
    format!("{module_id}:{name}@{offset}")
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
    TrackingScopeKind::Render => "render",
  };
  format!("{kind}:{}@{}", scope.callee, scope.span.offset)
}

impl ReactivityGraph {
  /// Set the logical module path and rebuild edge `to_id` values (graph v8).
  pub fn set_module_id(&mut self, module_id: impl Into<String>) {
    self.module_id = module_id.into();
    self.version = REACTIVITY_GRAPH_VERSION;
    self.rebuild_dependency_edges();
  }

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
  /// High-confidence under-approximation:
  /// - free identifiers that exactly match binding names
  /// - pure member chains `bag.field` / `bag.field.value` (and static optional
  ///   forms `bag?.field` / `bag?.field?.value`) when `bag` is a known
  ///   [`Self::composable_instances`] entry and `field` is in that shape
  ///
  /// Prefer flattened [`TemplateFacts::expressions`] (Vize interpolations +
  /// directive exp/arg with expression-absolute spans); fall back to element
  /// directives for hand-built fixtures that omit that list.
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
          push_instance_template_reads(
            &mut template_reads,
            &self.composable_instances,
            expression,
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
        push_instance_template_reads(
          &mut template_reads,
          &self.composable_instances,
          &expression.expression,
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
          // Bare name for rule matching (e.g. unused-binding).
          to: read.binding.clone(),
          // Module + span-qualified for multi-consumer identity (graph v8).
          to_id: Some(qualify_dependency_to_id(&self.module_id, &read.binding, read.span.offset)),
          property: read.property.clone(),
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
        to_id: Some(qualify_dependency_to_id(
          &self.module_id,
          &template_read.binding,
          template_read.span.offset,
        )),
        property: None,
        kind: ReactiveDependencyKind::Template,
        span: template_read.span.clone(),
      });
    }
    edges.sort_by(|left, right| {
      (left.kind, left.from.as_str(), left.to.as_str(), left.property.as_deref(), left.span.offset)
        .cmp(&(
          right.kind,
          right.from.as_str(),
          right.to.as_str(),
          right.property.as_deref(),
          right.span.offset,
        ))
    });
    edges.dedup_by(|left, right| {
      left.from == right.from
        && left.to == right.to
        && left.property == right.property
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

/// Join pure `bag.field` / `bag.field.value` (incl. `?.`) template chains onto
/// composable shape fields.
fn push_instance_template_reads(
  template_reads: &mut Vec<TemplateReactiveReadFact>,
  composable_instances: &std::collections::BTreeMap<
    String,
    std::collections::BTreeMap<String, ReactiveBindingKind>,
  >,
  expression: &str,
  surface: &str,
  span: &SourceSpan,
) {
  let Some(chain) = simple_member_chain(expression) else {
    return;
  };
  // bag.field | bag.field.value | bag?.field | bag?.field?.value
  let (Some(bag), Some(field)) = (chain.first(), chain.get(1)) else {
    return;
  };
  let trailing_ok = match chain.len() {
    2 => true,
    3 => chain.get(2).is_some_and(|part| part == "value"),
    _ => false,
  };
  if !trailing_ok {
    return;
  }
  let Some(shape) = composable_instances.get(bag.as_str()) else {
    return;
  };
  if !shape.contains_key(field.as_str()) {
    return;
  }
  template_reads.push(TemplateReactiveReadFact {
    binding: field.clone(),
    span: span.clone(),
    surface: surface.into(),
  });
}

/// `a.b.c` / `a?.b?.c` with only simple identifiers — rejects operators / calls.
fn simple_member_chain(expression: &str) -> Option<Vec<String>> {
  let trimmed = expression.trim();
  if trimmed.is_empty() {
    return None;
  }
  // Normalize optional chaining so `bag?.field` matches `bag.field`.
  let normalized = trimmed.replace("?.", ".");
  let mut parts = Vec::new();
  for part in normalized.split('.') {
    let part = part.trim();
    if !is_simple_js_identifier(part) {
      return None;
    }
    parts.push(part.to_owned());
  }
  (parts.len() >= 2).then_some(parts)
}

fn is_simple_js_identifier(text: &str) -> bool {
  let mut chars = text.chars();
  let Some(first) = chars.next() else {
    return false;
  };
  if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
    return false;
  }
  chars.all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '$')
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
