use vue_vet_core::{
  ByteRange, Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity, SourceSpan,
};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/accessibility/no-autofocus",
  category: "accessibility",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/accessibility/no-autofocus",
};

pub(super) struct NoAutofocus;

pub(super) static RULE: NoAutofocus = NoAutofocus;

impl Rule for NoAutofocus {
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
    let Some(attribute) = element.attribute("autofocus") else {
      return;
    };
    let message = "autofocus can disorient keyboard and screen-reader users".into();
    let help = Some(
      "Let users choose focus, or move focus programmatically only after an explicit interaction."
        .into(),
    );
    if attribute.value.is_none() {
      let range = removal_range(context.source(), attribute.span);
      context.report_with_safe_edit(
        self.meta(),
        attribute.span,
        message,
        help,
        range,
        String::new(),
      );
    } else {
      context.report(self.meta(), attribute.span, message, help);
    }
  }
}

fn removal_range(source: &str, span: SourceSpan) -> ByteRange {
  let bytes = source.as_bytes();
  let mut offset = span.offset;
  while offset > 0
    && bytes.get(offset.saturating_sub(1)).is_some_and(|byte| matches!(byte, b' ' | b'\t'))
  {
    offset = offset.saturating_sub(1);
  }
  ByteRange { offset, length: span.offset.saturating_add(span.length).saturating_sub(offset) }
}
