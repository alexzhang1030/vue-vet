//! Native `structuredClone` of a proven Vue Proxy (issue #224).

use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-proxy-structured-clone",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-proxy-structured-clone",
};

pub(super) struct NoProxyStructuredClone;
pub(super) static NO_PROXY_STRUCTURED_CLONE: NoProxyStructuredClone = NoProxyStructuredClone;

impl Rule for NoProxyStructuredClone {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.source_contracts.uncloneable_proxy_data {
        context.report(
          self.meta(),
          site.span,
          "`structuredClone` cannot clone a Vue reactive Proxy".into(),
          Some(
            "Pass a plain snapshot of cloneable fields. Do not wrap with `toRaw` as a universal fix — nested stored proxies and other uncloneable values can remain.".into(),
          ),
        );
      }
    }
  }
}
