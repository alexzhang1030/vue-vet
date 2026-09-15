//! Vue API source-contract rules (issue #224). Consume stable facts only.

use vue_vet_core::{
  Confidence, CustomRefLostNotificationReason, Rule, RuleContext, RuleGroupId, RuleMeta, Severity,
  SourceContractSiteFact, ToRefIgnoredKeyReason, WatchCallbackContractReason,
  WatchIgnoredOptionReason, WatchSignatureMismatchReason,
};

const TRIGGER_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-trigger-ref-on-non-ref",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-trigger-ref-on-non-ref",
  group: Some(RuleGroupId::SourceContracts),
};

const TOREFS_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-torefs-on-non-proxy",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-torefs-on-non-proxy",
  group: Some(RuleGroupId::SourceContracts),
};

const PRIMITIVE_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-primitive-reactive-target",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-primitive-reactive-target",
  group: Some(RuleGroupId::SourceContracts),
};

const UNWRAPPED_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-watch-unwrapped-source",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-watch-unwrapped-source",
  group: Some(RuleGroupId::SourceContracts),
};

const REPLACED_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-watch-replaced-object-source",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-watch-replaced-object-source",
  group: Some(RuleGroupId::SourceContracts),
};

const IGNORED_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-watch-ignored-option",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-watch-ignored-option",
  group: Some(RuleGroupId::SourceContracts),
};

const SIGNATURE_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-watch-signature-mismatch",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-watch-signature-mismatch",
  group: Some(RuleGroupId::SourceContracts),
};

const ONCE_IMMEDIATE_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-once-immediate-discard",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-once-immediate-discard",
  group: Some(RuleGroupId::SourceContracts),
};

const ALIAS_OLD_NEW_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-watch-alias-old-new",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-watch-alias-old-new",
  group: Some(RuleGroupId::SourceContracts),
};

const TOREF_KEY_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-toref-ignored-key",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-toref-ignored-key",
  group: Some(RuleGroupId::SourceContracts),
};

const EFFECT_SCOPE_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-effect-scope-callback-argument",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-effect-scope-callback-argument",
  group: Some(RuleGroupId::SourceContracts),
};

const CUSTOM_REF_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-invalid-custom-ref-interface",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-invalid-custom-ref-interface",
  group: Some(RuleGroupId::SourceContracts),
};

const INACTIVE_SCOPE_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-inactive-scope-result",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-inactive-scope-result",
  group: Some(RuleGroupId::SourceContracts),
};

const MISSING_TOREFS_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-missing-torefs-key",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-missing-torefs-key",
  group: Some(RuleGroupId::SourceContracts),
};

const INJECT_SAME_INSTANCE_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-inject-same-instance-provide",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-inject-same-instance-provide",
  group: Some(RuleGroupId::SourceContracts),
};

const UNTIL_TIMEOUT_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-until-timeout-unmatched-demand",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-until-timeout-unmatched-demand",
  group: Some(RuleGroupId::SourceContracts),
};

const EXTRACTED_METHOD_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-extracted-reactive-collection-method",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-extracted-reactive-collection-method",
  group: Some(RuleGroupId::SourceContracts),
};

const RAW_PROXY_MAP_KEY_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-raw-proxy-map-key",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-raw-proxy-map-key",
  group: Some(RuleGroupId::SourceContracts),
};

const CUSTOM_REF_LOST_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-custom-ref-lost-notification",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-custom-ref-lost-notification",
  group: Some(RuleGroupId::SourceContracts),
};

const MEMOIZE_STALE_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-memoize-stale-result-demand",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-memoize-stale-result-demand",
  group: Some(RuleGroupId::SourceContracts),
};

const CONTROLLED_STALE_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-controlled-computed-stale-result-demand",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-controlled-computed-stale-result-demand",
  group: Some(RuleGroupId::SourceContracts),
};

const PRIVATE_FIELD_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-reactive-private-field-access",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-reactive-private-field-access",
  group: Some(RuleGroupId::SourceContracts),
};

const IGNORABLE_WINDOW_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-ignorable-async-ignore-window",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-ignorable-async-ignore-window",
  group: Some(RuleGroupId::SourceContracts),
};

const SHARED_FIRST_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-shared-composable-first-instance-args",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-shared-composable-first-instance-args",
  group: Some(RuleGroupId::SourceContracts),
};

const CANCELLED_FILTER_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-cancelled-filter-promise-demand",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-cancelled-filter-promise-demand",
  group: Some(RuleGroupId::SourceContracts),
};

const JSON_CLONE_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-json-clone-lossy-type",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-json-clone-lossy-type",
  group: Some(RuleGroupId::SourceContracts),
};

const HISTORY_ALIAS_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-ref-history-snapshot-alias",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-ref-history-snapshot-alias",
  group: Some(RuleGroupId::SourceContracts),
};

const MODEL_DEFAULT_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-model-default-unsynced-parent-demand",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-model-default-unsynced-parent-demand",
  group: Some(RuleGroupId::SourceContracts),
};

const SHARED_DEFAULT_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-shared-default-cross-instance-demand",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-shared-default-cross-instance-demand",
  group: Some(RuleGroupId::SourceContracts),
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

pub(super) struct NoWatchIgnoredOption;
pub(super) static NO_WATCH_IGNORED_OPTION: NoWatchIgnoredOption = NoWatchIgnoredOption;

pub(super) struct NoWatchSignatureMismatch;
pub(super) static NO_WATCH_SIGNATURE_MISMATCH: NoWatchSignatureMismatch = NoWatchSignatureMismatch;

pub(super) struct NoOnceImmediateDiscard;
pub(super) static NO_ONCE_IMMEDIATE_DISCARD: NoOnceImmediateDiscard = NoOnceImmediateDiscard;

pub(super) struct NoWatchAliasOldNew;
pub(super) static NO_WATCH_ALIAS_OLD_NEW: NoWatchAliasOldNew = NoWatchAliasOldNew;

pub(super) struct NoToRefIgnoredKey;
pub(super) static NO_TOREF_IGNORED_KEY: NoToRefIgnoredKey = NoToRefIgnoredKey;

pub(super) struct NoEffectScopeCallbackArgument;
pub(super) static NO_EFFECT_SCOPE_CALLBACK_ARGUMENT: NoEffectScopeCallbackArgument =
  NoEffectScopeCallbackArgument;

pub(super) struct NoInvalidCustomRefInterface;
pub(super) static NO_INVALID_CUSTOM_REF_INTERFACE: NoInvalidCustomRefInterface =
  NoInvalidCustomRefInterface;

pub(super) struct NoInactiveScopeResult;
pub(super) static NO_INACTIVE_SCOPE_RESULT: NoInactiveScopeResult = NoInactiveScopeResult;

pub(super) struct NoMissingToRefsKey;
pub(super) static NO_MISSING_TOREFS_KEY: NoMissingToRefsKey = NoMissingToRefsKey;

pub(super) struct NoExtractedReactiveCollectionMethod;
pub(super) static NO_EXTRACTED_REACTIVE_COLLECTION_METHOD: NoExtractedReactiveCollectionMethod =
  NoExtractedReactiveCollectionMethod;

pub(super) struct NoRawProxyMapKey;
pub(super) static NO_RAW_PROXY_MAP_KEY: NoRawProxyMapKey = NoRawProxyMapKey;

pub(super) struct NoCustomRefLostNotification;
pub(super) static NO_CUSTOM_REF_LOST_NOTIFICATION: NoCustomRefLostNotification =
  NoCustomRefLostNotification;

pub(super) struct NoMemoizeStaleResultDemand;
pub(super) static NO_MEMOIZE_STALE_RESULT_DEMAND: NoMemoizeStaleResultDemand =
  NoMemoizeStaleResultDemand;

pub(super) struct NoControlledComputedStaleResultDemand;
pub(super) static NO_CONTROLLED_COMPUTED_STALE_RESULT_DEMAND:
  NoControlledComputedStaleResultDemand = NoControlledComputedStaleResultDemand;

pub(super) struct NoReactivePrivateFieldAccess;
pub(super) static NO_REACTIVE_PRIVATE_FIELD_ACCESS: NoReactivePrivateFieldAccess =
  NoReactivePrivateFieldAccess;

pub(super) struct NoUntilTimeoutUnmatchedDemand;
pub(super) static NO_UNTIL_TIMEOUT_UNMATCHED_DEMAND: NoUntilTimeoutUnmatchedDemand =
  NoUntilTimeoutUnmatchedDemand;

pub(super) struct NoInjectSameInstanceProvide;
pub(super) static NO_INJECT_SAME_INSTANCE_PROVIDE: NoInjectSameInstanceProvide =
  NoInjectSameInstanceProvide;

pub(super) struct NoIgnorableAsyncIgnoreWindow;
pub(super) static NO_IGNORABLE_ASYNC_IGNORE_WINDOW: NoIgnorableAsyncIgnoreWindow =
  NoIgnorableAsyncIgnoreWindow;

pub(super) struct NoSharedComposableFirstInstanceArgs;
pub(super) static NO_SHARED_COMPOSABLE_FIRST_INSTANCE_ARGS: NoSharedComposableFirstInstanceArgs =
  NoSharedComposableFirstInstanceArgs;

pub(super) struct NoCancelledFilterPromiseDemand;
pub(super) static NO_CANCELLED_FILTER_PROMISE_DEMAND: NoCancelledFilterPromiseDemand =
  NoCancelledFilterPromiseDemand;

pub(super) struct NoJsonCloneLossyType;
pub(super) static NO_JSON_CLONE_LOSSY_TYPE: NoJsonCloneLossyType = NoJsonCloneLossyType;

pub(super) struct NoRefHistorySnapshotAlias;
pub(super) static NO_REF_HISTORY_SNAPSHOT_ALIAS: NoRefHistorySnapshotAlias =
  NoRefHistorySnapshotAlias;

pub(super) struct NoModelDefaultUnsyncedParentDemand;
pub(super) static NO_MODEL_DEFAULT_UNSYNCED_PARENT_DEMAND: NoModelDefaultUnsyncedParentDemand =
  NoModelDefaultUnsyncedParentDemand;

pub(super) struct NoSharedDefaultCrossInstanceDemand;
pub(super) static NO_SHARED_DEFAULT_CROSS_INSTANCE_DEMAND: NoSharedDefaultCrossInstanceDemand =
  NoSharedDefaultCrossInstanceDemand;

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

impl Rule for NoWatchIgnoredOption {
  fn meta(&self) -> &'static RuleMeta {
    &IGNORED_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.watch_ignored_option {
        let (message, help) = ignored_copy(site.reason);
        context.report(self.meta(), site.span, message.into(), Some(help.into()));
      }
    }
  }
}

impl Rule for NoWatchSignatureMismatch {
  fn meta(&self) -> &'static RuleMeta {
    &SIGNATURE_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.watch_signature_mismatch {
        let (message, help) = signature_copy(site.api.as_str(), site.reason);
        context.report(self.meta(), site.span, message, Some(help.into()));
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

impl Rule for NoToRefIgnoredKey {
  fn meta(&self) -> &'static RuleMeta {
    &TOREF_KEY_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.toref_ignored_key {
        let (message, help) = match site.reason {
          ToRefIgnoredKeyReason::Function => (
            "`toRef` treats a function source as a getter ref and ignores this key",
            "The `'value'` key is also ignored on getters. Pass the function alone, or bind a property on an object source.",
          ),
          ToRefIgnoredKeyReason::Primitive => (
            "`toRef` wraps a primitive or nullish source as a new ref and ignores this key",
            "Drop the key, or pass an object if a live property binding is intended.",
          ),
          ToRefIgnoredKeyReason::Ref => (
            "`toRef` already received a ref, so this key is ignored",
            "`toRef(existingRef, 'value')` keeps writeback to the same ref. Other keys do not select a property.",
          ),
        };
        context.report(self.meta(), site.span, message.into(), Some(help.into()));
      }
    }
  }
}

impl Rule for NoEffectScopeCallbackArgument {
  fn meta(&self) -> &'static RuleMeta {
    &EFFECT_SCOPE_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.effect_scope_callback {
        context.report(
          self.meta(),
          site.span,
          "`effectScope` treats a function first argument as a truthy detached option; the callback body never runs".into(),
          Some(
            "Call `effectScope()` or `effectScope(true)` for detached ownership, then `scope.run(callback)`. Do not pass the callback to the constructor."
              .into(),
          ),
        );
      }
    }
  }
}

impl Rule for NoModelDefaultUnsyncedParentDemand {
  fn meta(&self) -> &'static RuleMeta {
    &MODEL_DEFAULT_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.unsynced_model_parent_demands {
        context.report(
          self.meta(),
          site.demand_span,
          format!(
            "parent `{}.value.{}` runs while the v-model binding is still undefined; the child `defineModel` default was not written back",
            site.parent_binding, site.demanded_member
          ),
          Some(format!(
            "Initialize `{}` before mount, or guard the optional value. Assigning the child default back to itself does not emit. v-model `{}` at {}:{} and child default in {} at {}:{}.",
            site.parent_binding,
            site.model_name,
            site.v_model_span.line,
            site.v_model_span.column,
            site.child_file,
            site.child_default_span.line,
            site.child_default_span.column
          )),
        );
      }
    }
  }
}

impl Rule for NoSharedDefaultCrossInstanceDemand {
  fn meta(&self) -> &'static RuleMeta {
    &SHARED_DEFAULT_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.shared_default_cross_instance_demands {
        context.report(
          self.meta(),
          site.demand_span,
          format!(
            "sibling instance `.{}` demand fails after a write through a shared model default",
            site.demanded_member
          ),
          Some(format!(
            "Create a fresh default per instance when isolated state is required. Shared default in {} at {}:{}, write at {}:{}.",
            site.child_file,
            site.default_span.line,
            site.default_span.column,
            site.write_span.line,
            site.write_span.column
          )),
        );
      }
    }
  }
}

const fn ignored_copy(reason: WatchIgnoredOptionReason) -> (&'static str, &'static str) {
  match reason {
    WatchIgnoredOptionReason::Equals => (
      "Vue ignores `equals` on `watch` / `watchEffect`; change detection uses `hasChanged`",
      "Drop `equals`. Project a primitive/getter, or compare a cloned snapshot inside the callback.",
    ),
    WatchIgnoredOptionReason::Immediate => (
      "`watchEffect` / `watchPostEffect` / `watchSyncEffect` ignore `immediate`",
      "Remove `immediate`. Effects already run immediately; use `watch` if you need a source callback.",
    ),
    WatchIgnoredOptionReason::Deep => (
      "`watchEffect` / `watchPostEffect` / `watchSyncEffect` ignore `deep`",
      "Remove `deep`. Deep tracking is automatic for reactive reads inside the effect.",
    ),
    WatchIgnoredOptionReason::Once => (
      "`watchEffect` / `watchPostEffect` / `watchSyncEffect` ignore `once`",
      "Remove `once`. It does not stop later reruns; stop the handle or use `watch(..., { once: true })`.",
    ),
  }
}

fn signature_copy(api: &str, reason: WatchSignatureMismatchReason) -> (String, &'static str) {
  match reason {
    WatchSignatureMismatchReason::WatchNonFunctionCallback => (
      format!(
        "`{api}` expects a function callback as its second argument; this value is not callable"
      ),
      "Use `watch(source, callback, options)`. Object `{ handler }` is the old Options API shape.",
    ),
    WatchSignatureMismatchReason::EffectFunctionAsOptions => (
      format!("`{api}` treats the second argument as options, so this function never runs"),
      "Use `watchEffect(callback, options)` — pass one effect function, then an options object.",
    ),
    WatchSignatureMismatchReason::EffectRefWithCallback => (
      format!(
        "`{api}` treats the first argument as the effect and the second as options; the callback never runs"
      ),
      "Pass a getter/effect as the first argument (`watchEffect(() => source.value)`), or use `watch(source, callback)`.",
    ),
  }
}

impl Rule for NoInvalidCustomRefInterface {
  fn meta(&self) -> &'static RuleMeta {
    &CUSTOM_REF_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.invalid_custom_ref_interface {
        context.report(
          self.meta(),
          site.interface_span,
          format!(
            "`customRef` factory is missing a callable `{}`; demanded `.value` will throw at runtime",
            site.missing.as_str()
          ),
          Some(format!(
            "Factory at {}:{} and demand at {}:{}. Return a plain object with callable `get` and `set`, or drop the unused capability.",
            site.factory_span.line,
            site.factory_span.column,
            site.demand_span.line,
            site.demand_span.column
          )),
        );
      }
    }
  }
}

impl Rule for NoInactiveScopeResult {
  fn meta(&self) -> &'static RuleMeta {
    &INACTIVE_SCOPE_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.inactive_scope_result {
        context.report(
          self.meta(),
          site.consumer_span,
          "`effectScope.run` after `stop()` returns undefined, so this object use throws".into(),
          Some(format!(
            "`stop()` at {}:{} and `run()` at {}:{}. Use the result before stopping, or skip the consumer.",
            site.stop_span.line,
            site.stop_span.column,
            site.run_span.line,
            site.run_span.column
          )),
        );
      }
    }
  }
}

impl Rule for NoMissingToRefsKey {
  fn meta(&self) -> &'static RuleMeta {
    &MISSING_TOREFS_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.missing_torefs_key {
        context.report(
          self.meta(),
          site.demand_span,
          format!(
            "`toRefs` bag has no `{}` on modeled own keys or the result prototype; dereferencing `.value` throws",
            site.key
          ),
          Some(format!(
            "`toRefs` at {}:{} does not own `{}`. Use an existing key, or `toRef(object, '{}')` for a future property.",
            site.torefs_span.line,
            site.torefs_span.column,
            site.key,
            site.key
          )),
        );
      }
    }
  }
}

impl Rule for NoExtractedReactiveCollectionMethod {
  fn meta(&self) -> &'static RuleMeta {
    &EXTRACTED_METHOD_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.extracted_reactive_collection_method {
        context.report(
          self.meta(),
          site.call_span,
          format!(
            "Calling extracted `{}.{}` loses the reactive {} receiver and throws TypeError",
            site.collection, site.method, site.collection
          ),
          Some(format!(
            "`{}` wrapped this {} at {}:{}; `{}` was extracted at {}:{}. Keep the receiver (`{}.{}(...)`) or bind it (`{}.call({}, ...)`).",
            site.api,
            site.collection,
            site.constructor_span.line,
            site.constructor_span.column,
            site.method,
            site.extraction_span.line,
            site.extraction_span.column,
            site.object,
            site.method,
            site.method,
            site.object
          )),
        );
      }
    }
  }
}

impl Rule for NoRawProxyMapKey {
  fn meta(&self) -> &'static RuleMeta {
    &RAW_PROXY_MAP_KEY_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.raw_proxy_map_key {
        context.report(
          self.meta(),
          site.demand_span,
          format!(
            "Native `Map` lookup uses object identity, so this `get` of the {} key misses the stored {} key and the unguarded use throws",
            site.lookup_kind, site.stored_kind
          ),
          Some(format!(
            "Stored key at {}:{} and Vue wrapper at {}:{}. {}",
            site.stored_key_span.line,
            site.stored_key_span.column,
            site.wrapper_span.line,
            site.wrapper_span.column,
            if site.stored_kind == "raw" {
              "Look up the stored raw identity, or wrap the Map with `reactive` / `shallowReactive` so lookup keys are normalized."
            } else {
              "Look up the stored proxy identity; wrapping the Map does not rewrite keys that were already stored as proxies."
            }
          )),
        );
      }
    }
  }
}

impl Rule for NoCustomRefLostNotification {
  fn meta(&self) -> &'static RuleMeta {
    &CUSTOM_REF_LOST_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.custom_ref_lost_notification {
        let (message, missing) = match site.reason {
          CustomRefLostNotificationReason::GetTracking => (
            "This customRef getter never tracks, so subscribed consumers will not re-run after writes",
            "getter tracking (`track()` during get)",
          ),
          CustomRefLostNotificationReason::SetNotification => (
            "This customRef setter never notifies, so subscribed consumers will not re-run after writes",
            "setter notification (`trigger()` during set)",
          ),
        };
        context.report(
          self.meta(),
          site.span,
          message.into(),
          Some(format!(
            "Missing {missing}. customRef at {}:{}, consumer at {}:{}, changed write at {}:{}. Call `track()` in get and `trigger()` in set, or store the value in a backing `ref` / `reactive`.",
            site.source_span.line,
            site.source_span.column,
            site.consumer_span.line,
            site.consumer_span.column,
            site.write_span.line,
            site.write_span.column
          )),
        );
      }
    }
  }
}

impl Rule for NoMemoizeStaleResultDemand {
  fn meta(&self) -> &'static RuleMeta {
    &MEMOIZE_STALE_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.memoize_stale_result_demand {
        context.report(
          self.meta(),
          site.demand_span,
          format!(
            "`{}` returned a cached {}, so calling `{}` throws after the source became a {}",
            site.api,
            site.cached_kind.as_str(),
            site.member,
            site.current_kind.as_str()
          ),
          Some(format!(
            "Cache fill at {}:{}, source write at {}:{}, producer at {}:{}. Include the source in the memo key, or call `load`/`delete` after the write.",
            site.fill_span.line,
            site.fill_span.column,
            site.write_span.line,
            site.write_span.column,
            site.producer_span.line,
            site.producer_span.column
          )),
        );
      }
    }
  }
}

impl Rule for NoControlledComputedStaleResultDemand {
  fn meta(&self) -> &'static RuleMeta {
    &CONTROLLED_STALE_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.controlled_computed_stale_result_demand {
        context.report(
          self.meta(),
          site.demand_span,
          format!(
            "`{}` retained a cached {}, so calling `{}` throws after an unlisted source became a {}",
            site.api,
            site.cached_kind.as_str(),
            site.member,
            site.current_kind.as_str()
          ),
          Some(format!(
            "Cache fill at {}:{}, unlisted write at {}:{}, producer at {}:{}. List the source, call `trigger()`, or use Vue `computed` when every input should invalidate.",
            site.fill_span.line,
            site.fill_span.column,
            site.write_span.line,
            site.write_span.column,
            site.producer_span.line,
            site.producer_span.column
          )),
        );
      }
    }
  }
}

impl Rule for NoReactivePrivateFieldAccess {
  fn meta(&self) -> &'static RuleMeta {
    &PRIVATE_FIELD_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.reactive_private_field_access {
        let action = if site.getter { "reading" } else { "calling" };
        context.report(
          self.meta(),
          site.demand_span,
          format!(
            "`{}` proxy is not branded for `#{}`; {action} `{}` throws TypeError",
            site.api, site.field, site.member
          ),
          Some(format!(
            "Proxy at {}:{}; `{}` at {}:{} reads `#{}` at {}:{}. Native private fields brand the original instance, not the Vue proxy. Keep `this` as that instance with a constructor-bound method, an arrow field, or `toRaw(proxy)`. Those raw receivers are not observed by Vue when `#{}` is assigned.",
            site.proxy_span.line,
            site.proxy_span.column,
            site.member,
            site.member_span.line,
            site.member_span.column,
            site.field,
            site.private_span.line,
            site.private_span.column,
            site.field
          )),
        );
      }
    }
  }
}

impl Rule for NoUntilTimeoutUnmatchedDemand {
  fn meta(&self) -> &'static RuleMeta {
    &UNTIL_TIMEOUT_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.until_timeout_unmatched_demand {
        context.report(
          self.meta(),
          site.demand_span,
          format!(
            "`until(...).toBe` timed out with the unmatched source value; `{}` belongs to the expected kind",
            site.capability
          ),
          Some(format!(
            "Comparison at {}:{} and timeout at {}:{} resolved the current source at {}:{}. Consume the timeout kind, wait without a timeout, or set `throwOnTimeout: true` and handle rejection.",
            site.comparison_span.line,
            site.comparison_span.column,
            site.options_span.line,
            site.options_span.column,
            site.source_span.line,
            site.source_span.column
          )),
        );
      }
    }
  }
}

impl Rule for NoInjectSameInstanceProvide {
  fn meta(&self) -> &'static RuleMeta {
    &INJECT_SAME_INSTANCE_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.inject_same_instance_provide {
        context.report(
          self.meta(),
          site.demand_span,
          format!(
            "`inject` reads the ancestor/app chain, so this {} fallback lacks `{}` that the local {} provide would supply",
            if site.default_absent { "absent (undefined)" } else { site.fallback_kind.as_str() },
            site.member,
            site.provided_kind.as_str()
          ),
          Some(format!(
            "Key at {}:{}, `provide` at {}:{}, `inject` at {}:{}. Use the local value in this setup, or inject from an ancestor that actually provides the key.",
            site.key_span.line,
            site.key_span.column,
            site.provide_span.line,
            site.provide_span.column,
            site.inject_span.line,
            site.inject_span.column
          )),
        );
      }
    }
  }
}

impl Rule for NoIgnorableAsyncIgnoreWindow {
  fn meta(&self) -> &'static RuleMeta {
    &IGNORABLE_WINDOW_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.ignorable_async_ignore_window {
        let api = if site.api.is_empty() { "watchIgnorable" } else { site.api.as_str() };
        context.report(
          self.meta(),
          site.write_span,
          format!(
            "This write reaches the `{api}` callback because the synchronous `ignoreUpdates` window has ended"
          ),
          Some(format!(
            "`ignoreUpdates` at {}:{} suspends at {}:{}. Await the data, then call `ignoreUpdates(() => {{ ... }})` around the post-await write.",
            site.ignore_span.line,
            site.ignore_span.column,
            site.await_span.line,
            site.await_span.column
          )),
        );
      }
    }
  }
}

impl Rule for NoSharedComposableFirstInstanceArgs {
  fn meta(&self) -> &'static RuleMeta {
    &SHARED_FIRST_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.shared_composable_first_instance_args {
        let api = if site.api.is_empty() { "createSharedComposable" } else { site.api.as_str() };
        context.report(
          self.meta(),
          site.demand_span,
          format!(
            "Shared `{api}` state keeps the first initializer; this demand needs `{cap}` which the retained value does not have",
            cap = site.capability
          ),
          Some(format!(
            "First call at {}:{} and later argument at {}:{}. Use independent composables or an explicitly keyed factory when per-argument instances are required.",
            site.first_call_span.line,
            site.first_call_span.column,
            site.later_arg_span.line,
            site.later_arg_span.column
          )),
        );
      }
    }
  }
}

impl Rule for NoCancelledFilterPromiseDemand {
  fn meta(&self) -> &'static RuleMeta {
    &CANCELLED_FILTER_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.cancelled_filter_promise_demand {
        context.report(
          self.meta(),
          site.demand_span,
          format!(
            "`{}` cancelled the earlier promise with undefined, so calling `{}` throws",
            site.api, site.member
          ),
          Some(format!(
            "First call at {}:{} was superseded at {}:{} before the timer; await at {}:{} fulfills `undefined`. Demand the latest call's result, wait for each call to settle, or set `rejectOnCancel: true` and handle rejection.",
            site.first_call_span.line,
            site.first_call_span.column,
            site.superseding_call_span.line,
            site.superseding_call_span.column,
            site.await_span.line,
            site.await_span.column
          )),
        );
      }
    }
  }
}

impl Rule for NoJsonCloneLossyType {
  fn meta(&self) -> &'static RuleMeta {
    &JSON_CLONE_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.json_clone_lossy_type {
        context.report(
          self.meta(),
          site.demand_span,
          format!(
            "JSON clone produced a {} at `{}`, so `.{}()` is not a function",
            site.output_kind, site.path, site.method
          ),
          Some(format!(
            "Date path at {}:{} and `useCloned` at {}:{}. Use a clone that keeps Date methods; JSON.stringify turns Date into an ISO string.",
            site.source_span.line,
            site.source_span.column,
            site.clone_span.line,
            site.clone_span.column
          )),
        );
      }
    }
  }
}

impl Rule for NoRefHistorySnapshotAlias {
  fn meta(&self) -> &'static RuleMeta {
    &HISTORY_ALIAS_META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.ref_history_snapshot_alias {
        context.report(
          self.meta(),
          site.write_span,
          format!(
            "In-place write to `{}` mutates a retained history snapshot, so `{}` restores the edited object",
            site.property, site.demand_kind
          ),
          Some(format!(
            "Record at {}:{} and `{}` at {}:{}. History stores the same object when `clone` is false. Replace the root, or pass `{{ clone: true }}` / a clone that copies nested values.",
            site.record_span.line,
            site.record_span.column,
            site.demand_kind,
            site.demand_span.line,
            site.demand_span.column
          )),
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
    &NO_WATCH_IGNORED_OPTION,
    &NO_WATCH_SIGNATURE_MISMATCH,
    &NO_ONCE_IMMEDIATE_DISCARD,
    &NO_WATCH_ALIAS_OLD_NEW,
    &NO_TOREF_IGNORED_KEY,
    &NO_EFFECT_SCOPE_CALLBACK_ARGUMENT,
    &NO_INVALID_CUSTOM_REF_INTERFACE,
    &NO_INACTIVE_SCOPE_RESULT,
    &NO_MISSING_TOREFS_KEY,
    &NO_EXTRACTED_REACTIVE_COLLECTION_METHOD,
    &super::no_proxy_structured_clone::NO_PROXY_STRUCTURED_CLONE,
    &NO_RAW_PROXY_MAP_KEY,
    &NO_CUSTOM_REF_LOST_NOTIFICATION,
    &NO_MEMOIZE_STALE_RESULT_DEMAND,
    &NO_CONTROLLED_COMPUTED_STALE_RESULT_DEMAND,
    &NO_REACTIVE_PRIVATE_FIELD_ACCESS,
    &NO_UNTIL_TIMEOUT_UNMATCHED_DEMAND,
    &NO_INJECT_SAME_INSTANCE_PROVIDE,
    &NO_IGNORABLE_ASYNC_IGNORE_WINDOW,
    &NO_SHARED_COMPOSABLE_FIRST_INSTANCE_ARGS,
    &NO_CANCELLED_FILTER_PROMISE_DEMAND,
    &NO_JSON_CLONE_LOSSY_TYPE,
    &NO_REF_HISTORY_SNAPSHOT_ALIAS,
    &NO_MODEL_DEFAULT_UNSYNCED_PARENT_DEMAND,
    &NO_SHARED_DEFAULT_CROSS_INSTANCE_DEMAND,
  ]
}
