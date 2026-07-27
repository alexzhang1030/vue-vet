use vue_vet_core::{Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity};

use super::a11y_content::{has_accessible_name_attrs, is_heading, title_to_aria_label_edit};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/heading-has-content",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/heading-has-content",
};

pub(super) struct HeadingHasContent;

pub(super) static RULE: HeadingHasContent = HeadingHasContent;

impl Rule for HeadingHasContent {
  fn meta(&self) -> &'static RuleMeta {
    &META
  }

  fn fact_kinds(&self) -> FactKinds {
    FactKinds::TEMPLATE_ELEMENT
  }

  fn run_on(&self, fact: FactRef<'_>, context: &mut RuleContext<'_>) {
    let FactRef::TemplateElement(element) = fact else {
      return;
    };
    if !is_heading(&element.tag)
      || element.has_accessible_content
      || has_accessible_name_attrs(element)
    {
      return;
    }
    let message = "heading has no accessible content".into();
    let help = Some(
      "Add text content, an img/area with alt, or an aria-label/aria-labelledby binding.".into(),
    );
    if let Some((range, replacement)) = title_to_aria_label_edit(context.source(), element) {
      context.report_with_safe_edit(
        self.meta(),
        element.span.clone(),
        message,
        help,
        range,
        replacement,
      );
    } else {
      context.report(self.meta(), element.span.clone(), message, help);
    }
  }
}
