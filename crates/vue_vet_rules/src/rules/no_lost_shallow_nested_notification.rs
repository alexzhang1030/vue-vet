//! Nested writes past a shallow container's notifying frontier.

use vue_vet_core::{
  Confidence, FactKinds, FactRef, NotificationBypassKind, Rule, RuleContext, RuleMeta, Severity,
};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-lost-shallow-nested-notification",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-lost-shallow-nested-notification",
};

pub(super) struct NoLostShallowNestedNotification;
pub(super) static RULE: NoLostShallowNestedNotification = NoLostShallowNestedNotification;

impl Rule for NoLostShallowNestedNotification {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::NOTIFICATION_BYPASS
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::NotificationBypass { bypass, .. } = fact else {
      return;
    };
    if bypass.kind != NotificationBypassKind::ShallowNested {
      return;
    }
    let path = display_path(&bypass.source_name, &bypass.path);
    context.report(
      self.meta(),
      bypass.write_span,
      "This nested write bypasses the shallow container notification used by the consumer".into(),
      Some(format!(
        "Source `{}` at {}:{}, path `{path}`; consumer at {}:{}. Replace the tracked slot or keep the nested value reactive.",
        bypass.source_name,
        bypass.source_span.line,
        bypass.source_span.column,
        bypass.consumer_span.line,
        bypass.consumer_span.column
      )),
    );
  }
}

fn display_path(root: &str, path: &[String]) -> String {
  if path.is_empty() {
    return root.to_string();
  }
  format!("{root}.{}", path.join("."))
}
