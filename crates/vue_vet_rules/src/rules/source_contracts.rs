//! Vue API source-contract rules (issue #224). Consume stable facts only.

use vue_vet_core::{
  Confidence, Rule, RuleContext, RuleMeta, Severity, SourceContractSiteFact,
  WatchIgnoredOptionReason, WatchSignatureMismatchReason,
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
  ]
}
