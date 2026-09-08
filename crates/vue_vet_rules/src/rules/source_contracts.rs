//! Vue API source-contract rules (issue #224). Consume stable facts only.

use vue_vet_core::{
  Confidence, Rule, RuleContext, RuleMeta, Severity, SourceContractSiteFact, ToRefIgnoredKeyReason,
  WatchCallbackContractReason, WatchIgnoredOptionReason, WatchSignatureMismatchReason,
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

const IGNORED_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-watch-ignored-option",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-watch-ignored-option",
};

const SIGNATURE_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-watch-signature-mismatch",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-watch-signature-mismatch",
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

const TOREF_KEY_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-toref-ignored-key",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-toref-ignored-key",
};

const EFFECT_SCOPE_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-effect-scope-callback-argument",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-effect-scope-callback-argument",
};

const CUSTOM_REF_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-invalid-custom-ref-interface",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-invalid-custom-ref-interface",
};

const INACTIVE_SCOPE_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-inactive-scope-result",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-inactive-scope-result",
};

const MISSING_TOREFS_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-missing-torefs-key",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-missing-torefs-key",
};

const EXTRACTED_METHOD_META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-extracted-reactive-collection-method",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-extracted-reactive-collection-method",
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
  ]
}
