//! Vue API source-contract rules (issue #224). Consume stable facts only.

use vue_vet_core::{
  Confidence, Rule, RuleContext, RuleMeta, Severity, SourceContractSiteFact,
  WatchCallbackContractReason,
};

const TRIGGER_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-trigger-ref-on-non-ref",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-trigger-ref-on-non-ref",
};

const TOREFS_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-torefs-on-non-proxy",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-torefs-on-non-proxy",
};

const PRIMITIVE_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-primitive-reactive-target",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-primitive-reactive-target",
};

const UNWRAPPED_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-watch-unwrapped-source",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-watch-unwrapped-source",
};

const REPLACED_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-watch-replaced-object-source",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-watch-replaced-object-source",
};

const ONCE_IMMEDIATE_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-once-immediate-discard",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-once-immediate-discard",
};

const ALIAS_OLD_NEW_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-watch-alias-old-new",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-watch-alias-old-new",
};

pub(super) struct NoTriggerRefOnNonRef;
pub(super) static NO_TRIGGER_REF_ON_NON_REF: NoTriggerRefOnNonRef = NoTriggerRefOnNonRef;

pub(super) struct NoToRefsOnNonProxy;
pub(super) static NO_TOREFS_ON_NON_PROXY: NoToRefsOnNonProxy = NoToRefsOnNonProxy;

pub(super) struct NoPrimitiveReactiveTarget;
pub(super) static NO_PRIMITIVE_REACTIVE_TARGET: NoPrimitiveReactiveTarget =
  NoPrimitiveReactiveTarget;

pub(super) struct NoWatchUnwrappedSource;
pub(super) static NO_WATCH_UNWRAPPED_SOURCE: NoWatchUnwrappedSource = NoWatchUnwrappedSource;

pub(super) struct NoWatchReplacedObjectSource;
pub(super) static NO_WATCH_REPLACED_OBJECT_SOURCE: NoWatchReplacedObjectSource =
  NoWatchReplacedObjectSource;

pub(super) struct NoOnceImmediateDiscard;
pub(super) static NO_ONCE_IMMEDIATE_DISCARD: NoOnceImmediateDiscard = NoOnceImmediateDiscard;

pub(super) struct NoWatchAliasOldNew;
pub(super) static NO_WATCH_ALIAS_OLD_NEW: NoWatchAliasOldNew = NoWatchAliasOldNew;

impl Rule for NoTriggerRefOnNonRef {
  fn meta(&self) -> &'static RuleMeta {
    &TRIGGER_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.trigger_ref_non_ref {
        report_site(
          context,
          self.meta(),
          site,
          "`triggerRef` does not notify watchers unless the argument is a ref",
          "Pass a `ref` / `shallowRef` / `customRef`, or mutate the reactive object instead.",
        );
      }
    }
  }
}

impl Rule for NoToRefsOnNonProxy {
  fn meta(&self) -> &'static RuleMeta {
    &TOREFS_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.torefs_non_proxy {
        report_site(
          context,
          self.meta(),
          site,
          "`toRefs` expects a reactive object, not a plain object or array",
          "Wrap the value with `reactive` / `readonly` first, or use `toRef` on a real proxy.",
        );
      }
    }
  }
}

impl Rule for NoPrimitiveReactiveTarget {
  fn meta(&self) -> &'static RuleMeta {
    &PRIMITIVE_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.primitive_reactive_target {
        let api = if site.api.is_empty() { "reactive" } else { site.api.as_str() };
        context.report(
          self.meta(),
          site.span,
          format!("`{api}` cannot make a primitive or null value reactive"),
          Some("Pass a non-null object, or use `ref` for primitive state.".into()),
        );
      }
    }
  }
}

impl Rule for NoWatchUnwrappedSource {
  fn meta(&self) -> &'static RuleMeta {
    &UNWRAPPED_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.watch_unwrapped_source {
        report_site(
          context,
          self.meta(),
          site,
          "`watch` source is an unwrapped primitive, so later writes will not re-run the callback",
          "Pass the ref itself (`watch(count, …)`) or a getter (`watch(() => state.n, …)`).",
        );
      }
    }
  }
}

impl Rule for NoWatchReplacedObjectSource {
  fn meta(&self) -> &'static RuleMeta {
    &REPLACED_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.watch_replaced_object_source {
        context.report(
          self.meta(),
          site.source_span,
          format!(
            "`watch({}.{})` subscribes to the current object identity; replacing `{}.{}` later bypasses that subscription",
            site.object, site.property, site.object, site.property
          ),
          Some(format!(
            "The replacement at {}:{} does not retarget the watcher. Use `watch(() => {}.{}, …, {{ deep: true }})` if replacement must be observed.",
            site.replacement_span.line,
            site.replacement_span.column,
            site.object,
            site.property
          )),
        );
      }
    }
  }
}

impl Rule for NoOnceImmediateDiscard {
  fn meta(&self) -> &'static RuleMeta {
    &ONCE_IMMEDIATE_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.watch_callback_contracts {
        if site.reason != WatchCallbackContractReason::OnceImmediateUndefinedGuard {
          continue;
        }
        context.report(
          self.meta(),
          site.guard_span,
          "`watch` with `{ once: true, immediate: true }` returns before the only invocation can use the new value".into(),
          Some(
            "The initial `old` value is `undefined` for a single source, so this guard consumes the callback's only run. Drop `once`, drop the guard, or do the initial work without returning."
              .into(),
          ),
        );
      }
    }
  }
}

impl Rule for NoWatchAliasOldNew {
  fn meta(&self) -> &'static RuleMeta {
    &ALIAS_OLD_NEW_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.watch_callback_contracts {
        if site.reason != WatchCallbackContractReason::ReactiveRootIdentityGuard {
          continue;
        }
        context.report(
          self.meta(),
          site.guard_span,
          "`watch` on a reactive root compares the same proxy identity, so later new-value work never runs".into(),
          Some(
            "Vue reuses the reactive object for `old` and `new` on nested changes. Compare a field, watch a getter, or drop the identity guard."
              .into(),
          ),
        );
      }
    }
  }
}

fn report_site(
  context: &mut RuleContext<'_>,
  meta: &RuleMeta,
  site: &SourceContractSiteFact,
  message: &str,
  help: &str,
) {
  context.report(meta, site.span, message.into(), Some(help.into()));
}

#[must_use]
pub(super) fn source_contract_rules() -> Vec<&'static dyn Rule> {
  vec![
    &NO_TRIGGER_REF_ON_NON_REF,
    &NO_TOREFS_ON_NON_PROXY,
    &NO_PRIMITIVE_REACTIVE_TARGET,
    &NO_WATCH_UNWRAPPED_SOURCE,
    &NO_WATCH_REPLACED_OBJECT_SOURCE,
    &NO_ONCE_IMMEDIATE_DISCARD,
    &NO_WATCH_ALIAS_OLD_NEW,
  ]
}
