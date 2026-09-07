//! Vue API source-contract rules (issue #224). Consume stable facts only.

use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity, SourceContractSiteFact};

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
    &NO_EXTRACTED_REACTIVE_COLLECTION_METHOD,
  ]
}
