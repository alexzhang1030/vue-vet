//! Tracer-differentiated rules that need reactivity-graph read kinds / guards.

use vue_vet_core::{
  Confidence, FactKinds, FactRef, ReactiveGuardRole, ReactiveReadKind, Rule, RuleContext, RuleMeta,
  Severity, TrackingScopeKind,
};

use crate::rules::support::{binding_path, effect_family};

const DEEP_WATCH_PROPERTY: &str = "*";

fn block_calls_callee(context: &RuleContext<'_>, callee: &str) -> bool {
  context.script().blocks.iter().any(|block| block.calls.iter().any(|call| call.callee == callee))
}

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
        read.span.clone(),
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
    if !block_calls_callee(context, "pauseTracking") {
      return;
    }
    for read in &scope.reads {
      if read.kind != ReactiveReadKind::OutsideTracking {
        continue;
      }
      let path = binding_path(read);
      context.report(
        self.meta(),
        read.span.clone(),
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

// --- guard-role gated dependencies -----------------------------------------------

struct GuardRoleRule {
  meta: &'static RuleMeta,
  role: ReactiveGuardRole,
  role_label: &'static str,
}

impl Rule for GuardRoleRule {
  fn meta(&self) -> &'static RuleMeta {
    self.meta
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
    for read in &scope.reads {
      if read.kind != ReactiveReadKind::Conditional {
        continue;
      }
      if !read.guards.iter().any(|guard| guard.role == self.role) {
        continue;
      }
      let already_unconditional = scope.reads.iter().any(|candidate| {
        candidate.kind == ReactiveReadKind::Unconditional
          && candidate.span.offset < read.span.offset
          && candidate.binding == read.binding
          && candidate.property == read.property
      });
      if already_unconditional {
        continue;
      }
      let path = binding_path(read);
      context.report(
        self.meta(),
        read.span.clone(),
        format!(
          "`{path}` is only reached after a {} guard in `{}`, so tracking is incomplete",
          self.role_label, scope.callee
        ),
        Some(
          "Read the dependency before the guard, or use explicit `watch([...])` / getter sources."
            .into(),
        ),
      );
    }
  }
}

const EARLY_EXIT_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-early-exit-gated-dependency",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-early-exit-gated-dependency",
};

const SHORT_CIRCUIT_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-short-circuit-gated-dependency",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-short-circuit-gated-dependency",
};

const SWITCH_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-switch-gated-dependency",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-switch-gated-dependency",
};

static NO_EARLY_EXIT_GATED_DEPENDENCY: GuardRoleRule = GuardRoleRule {
  meta: &EARLY_EXIT_META,
  role: ReactiveGuardRole::EarlyExit,
  role_label: "early-exit",
};

static NO_SHORT_CIRCUIT_GATED_DEPENDENCY: GuardRoleRule = GuardRoleRule {
  meta: &SHORT_CIRCUIT_META,
  role: ReactiveGuardRole::ShortCircuit,
  role_label: "short-circuit",
};

static NO_SWITCH_GATED_DEPENDENCY: GuardRoleRule = GuardRoleRule {
  meta: &SWITCH_META,
  role: ReactiveGuardRole::SwitchDiscriminant,
  role_label: "switch",
};

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
    if block_calls_callee(context, "pauseTracking") {
      return;
    }
    for read in &scope.reads {
      if read.kind != ReactiveReadKind::OutsideTracking {
        continue;
      }
      let path = binding_path(read);
      context.report(
        self.meta(),
        read.span.clone(),
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
    &NO_EARLY_EXIT_GATED_DEPENDENCY,
    &NO_SHORT_CIRCUIT_GATED_DEPENDENCY,
    &NO_SWITCH_GATED_DEPENDENCY,
    &NO_DEFERRED_CALLBACK_REACTIVE_READ_IN_EFFECT,
  ]
}
