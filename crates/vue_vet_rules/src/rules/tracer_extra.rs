//! Tracer-differentiated rules that need reactivity-graph read kinds / guards.

use vue_vet_core::{
  Confidence, FactKinds, FactRef, ReactiveReadKind, Rule, RuleContext, RuleMeta, Severity,
  TrackingScopeKind,
};

use vue_vet_rule_query::{binding_path, effect_family, script_has_call};

const DEEP_WATCH_PROPERTY: &str = "*";

// --- deep watch on reactive root -------------------------------------------------

const DEEP_WATCH_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-deep-watch-on-reactive-root",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-deep-watch-on-reactive-root",
};

pub(super) struct NoDeepWatchOnReactiveRoot;
pub(super) static NO_DEEP_WATCH_ON_REACTIVE_ROOT: NoDeepWatchOnReactiveRoot =
  NoDeepWatchOnReactiveRoot;

impl Rule for NoDeepWatchOnReactiveRoot {
  fn meta(&self) -> &'static RuleMeta {
    &DEEP_WATCH_META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TRACKING_SCOPE
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TrackingScope { scope, .. } = fact else {
      return;
    };
    if scope.kind != TrackingScopeKind::WatchSources {
      return;
    }
    for read in &scope.reads {
      if read.property.as_deref() != Some(DEEP_WATCH_PROPERTY) {
        continue;
      }
      context.report(
        self.meta(),
        read.span,
        format!(
          "`watch({})` deep-tracks the reactive root; prefer a getter or explicit sources",
          read.binding
        ),
        Some(
          "Use `watch(() => state.field, …)` or `watch(() => ({ … }), …)` so dependencies stay precise."
            .into(),
        ),
      );
    }
  }
}

// --- pauseTracking ---------------------------------------------------------------

const PAUSE_TRACKING_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-reactive-read-during-pause-tracking",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-reactive-read-during-pause-tracking",
};

pub(super) struct NoReactiveReadDuringPauseTracking;
pub(super) static NO_REACTIVE_READ_DURING_PAUSE_TRACKING: NoReactiveReadDuringPauseTracking =
  NoReactiveReadDuringPauseTracking;

impl Rule for NoReactiveReadDuringPauseTracking {
  fn meta(&self) -> &'static RuleMeta {
    &PAUSE_TRACKING_META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TRACKING_SCOPE
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TrackingScope { scope, .. } = fact else {
      return;
    };
    if !scope.kind.tracks_dependencies() {
      return;
    }
    if !script_has_call(context.script(), "pauseTracking") {
      return;
    }
    for read in &scope.reads {
      if read.kind != ReactiveReadKind::OutsideTracking {
        continue;
      }
      let path = binding_path(read);
      context.report(
        self.meta(),
        read.span,
        format!(
          "`{path}` is read while tracking is paused, so `{}` will not track it",
          scope.callee
        ),
        Some(
          "Read the dependency outside `pauseTracking()`…`enableTracking()` / `resetTracking()`, or list it in explicit `watch` sources."
            .into(),
        ),
      );
    }
  }
}

// --- conditional dependency in render --------------------------------------------
//
// Scope-aware Conditional reads for computed / watch sources / effectScope live in
// the matrix family. Effect-family scopes use `no-conditional-watch-effect-dependency`.
// Per-`ReactiveGuardRole` rule ids were removed (#136): they stacked on the same
// Conditional fact and inflated finding count without extra precision.

const RENDER_CONDITIONAL_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-conditional-dependency-in-render",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-conditional-dependency-in-render",
};

pub(super) struct NoConditionalDependencyInRender;
pub(super) static NO_CONDITIONAL_DEPENDENCY_IN_RENDER: NoConditionalDependencyInRender =
  NoConditionalDependencyInRender;

impl Rule for NoConditionalDependencyInRender {
  fn meta(&self) -> &'static RuleMeta {
    &RENDER_CONDITIONAL_META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TRACKING_SCOPE
  }

  fn run_on(&self, _fact: FactRef<'_>, _context: &mut RuleContext<'_>) {
    // Vue tracks dynamic dependencies when the guard is itself reactive.
    // Premise withdrawn; rule ID stays for config compatibility.
  }
}

// --- deferred OutsideTracking in effects -----------------------------------------

const DEFERRED_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-deferred-callback-reactive-read-in-effect",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-deferred-callback-reactive-read-in-effect",
};

pub(super) struct NoDeferredCallbackReactiveReadInEffect;
pub(super) static NO_DEFERRED_CALLBACK_REACTIVE_READ_IN_EFFECT:
  NoDeferredCallbackReactiveReadInEffect = NoDeferredCallbackReactiveReadInEffect;

impl Rule for NoDeferredCallbackReactiveReadInEffect {
  fn meta(&self) -> &'static RuleMeta {
    &DEFERRED_META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TRACKING_SCOPE
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TrackingScope { scope, .. } = fact else {
      return;
    };
    if !effect_family(scope.kind) {
      return;
    }
    // pauseTracking reads are owned by no-reactive-read-during-pause-tracking.
    if script_has_call(context.script(), "pauseTracking") {
      return;
    }
    for read in &scope.reads {
      if read.kind != ReactiveReadKind::OutsideTracking {
        continue;
      }
      let path = binding_path(read);
      context.report(
        self.meta(),
        read.span,
        format!(
          "`{path}` is read inside a deferred callback in `{}`, so the effect will not track it",
          scope.callee
        ),
        Some(
          "Read the dependency synchronously in the effect, or watch it with explicit sources."
            .into(),
        ),
      );
    }
  }
}

#[must_use]
pub(super) fn tracer_extra_rules() -> Vec<&'static dyn Rule> {
  vec![
    &NO_DEEP_WATCH_ON_REACTIVE_ROOT,
    &NO_REACTIVE_READ_DURING_PAUSE_TRACKING,
    &NO_CONDITIONAL_DEPENDENCY_IN_RENDER,
    &NO_DEFERRED_CALLBACK_REACTIVE_READ_IN_EFFECT,
  ]
}
