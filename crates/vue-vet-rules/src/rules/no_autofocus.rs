use vue_vet_core::{ByteRange, Confidence, Rule, RuleContext, RuleMeta, Severity, SourceSpan};

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

  fn run(&self, context: &mut RuleContext<'_>) {
    let findings = context
      .template()
      .elements
      .iter()
      .filter_map(|element| element.attribute("autofocus"))
      .map(|attribute| {
        let edit =
          attribute.value.is_none().then(|| removal_range(context.source(), &attribute.span));
        (attribute.span.clone(), edit)
      })
      .collect::<Vec<_>>();
    for (span, edit) in findings {
      let message = "autofocus can disorient keyboard and screen-reader users".into();
      let help = Some(
        "Let users choose focus, or move focus programmatically only after an explicit interaction."
          .into(),
      );
      if let Some(range) = edit {
        context.report_with_safe_edit(self.meta(), span, message, help, range, String::new());
      } else {
        context.report(self.meta(), span, message, help);
      }
    }
  }
}

fn removal_range(source: &str, span: &SourceSpan) -> ByteRange {
  let bytes = source.as_bytes();
  let mut offset = span.offset;
  while offset > 0
    && bytes.get(offset.saturating_sub(1)).is_some_and(|byte| matches!(byte, b' ' | b'\t'))
  {
    offset = offset.saturating_sub(1);
  }
  ByteRange { offset, length: span.offset.saturating_add(span.length).saturating_sub(offset) }
}
