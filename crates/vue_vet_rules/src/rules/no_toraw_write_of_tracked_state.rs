//! Raw-view writes of a path a proxy consumer already tracks.

use vue_vet_core::{
  Confidence, FactKinds, FactRef, NotificationBypassKind, Rule, RuleContext, RuleMeta, Severity,
};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/reactivity/no-toraw-write-of-tracked-state",
  category: "reactivity",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/reactivity/no-toraw-write-of-tracked-state",
};

pub(super) struct NoToRawWriteOfTrackedState;
pub(super) static RULE: NoToRawWriteOfTrackedState = NoToRawWriteOfTrackedState;

impl Rule for NoToRawWriteOfTrackedState {
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
    if bypass.kind != NotificationBypassKind::ToRawWrite {
      return;
    }
    let path = display_path(&bypass.source_name, &bypass.path);
    context.report(
      self.meta(),
      bypass.write_span,
      "This raw write bypasses the proxy notification used by the consumer".into(),
      Some(format!(
        "Source `{}` at {}:{}, path `{path}`; consumer at {}:{}. Write through the reactive proxy instead of `toRaw`.",
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
