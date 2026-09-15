//! Pre-flush demand of a still-null conditional template ref.

use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, Severity};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-pre-flush-template-ref-demand",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-pre-flush-template-ref-demand",
};

pub(super) struct NoPreFlushTemplateRefDemand;
pub(super) static RULE: NoPreFlushTemplateRefDemand = NoPreFlushTemplateRefDemand;

impl Rule for NoPreFlushTemplateRefDemand {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn run_once(&self, context: &mut RuleContext<'_>) {
    for block in &context.script().blocks {
      for site in &block.template_ref_demands.pre_flush {
        context.report(
          self.meta(),
          site.demand_span,
          "Pre-flush watch demands a template ref before the matching patch creates the node"
            .into(),
          Some(format!(
            "This ordinary native node is created in the same patch. Use `flush: 'post'` (effective `{}` runs before patch). The condition write is at {}:{} and the watcher at {}:{}.",
            site.flush,
            site.condition_write_span.line,
            site.condition_write_span.column,
            site.watch_span.line,
            site.watch_span.column
          )),
        );
      }
    }
  }
}
