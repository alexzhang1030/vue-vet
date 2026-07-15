use vue_vet_core::{Confidence, Rule, RuleContext, RuleMeta, RuleRegistry, Severity};

const NO_V_HTML_META: RuleMeta = RuleMeta {
  id: "vue-vet/security/no-v-html",
  category: "security",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/security/no-v-html",
};

struct NoVHtml;

impl Rule for NoVHtml {
  fn meta(&self) -> &'static RuleMeta {
    &NO_V_HTML_META
  }

  fn run(&self, context: &mut RuleContext<'_>) {
    let spans = context
      .template()
      .elements
      .iter()
      .flat_map(|element| &element.directives)
      .filter(|directive| directive.name == "html")
      .map(|directive| directive.span.clone())
      .collect::<Vec<_>>();

    for span in spans {
      context.report(
        self.meta(),
        span,
        "`v-html` can render untrusted HTML into the page".into(),
        Some(
          "Prefer normal template interpolation. If raw HTML is required, sanitize it at the trust boundary."
            .into(),
        ),
      );
    }
  }
}

static NO_V_HTML: NoVHtml = NoVHtml;

#[must_use]
pub fn builtin_registry() -> RuleRegistry {
  RuleRegistry::new(vec![&NO_V_HTML])
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn builtins_have_stable_metadata() {
    let metadata = builtin_registry().metadata();
    assert_eq!(metadata.len(), 1, "the initial registry contains one reference rule");
    assert_eq!(metadata.first().map(|meta| meta.id), Some("vue-vet/security/no-v-html"));
    assert_eq!(metadata.first().map(|meta| meta.confidence), Some(Confidence::High));
  }
}
