use vue_vet_core::{
  ByteRange, Confidence, FactKinds, FactRef, Rule, RuleContext, RuleMeta, Severity, SourceSpan,
};

const META: RuleMeta = RuleMeta {
  id: "vue-vet/maintainability/no-redundant-role",
  category: "maintainability",
  default_severity: Severity::Warning,
  confidence: Confidence::High,
  documentation: "rules/maintainability/no-redundant-role",
};

pub(super) struct NoRedundantRole;

pub(super) static RULE: NoRedundantRole = NoRedundantRole;

impl Rule for NoRedundantRole {
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
    let Some(attribute) = element.attribute("role") else {
      return;
    };
    let Some(role) = attribute.value.as_deref() else {
      return;
    };
    let redundant = match element.tag.to_ascii_lowercase().as_str() {
      "a" => role.eq_ignore_ascii_case("link") && element.attribute("href").is_some(),
      "button" => role.eq_ignore_ascii_case("button"),
      "img" => role.eq_ignore_ascii_case("img"),
      "li" => role.eq_ignore_ascii_case("listitem"),
      "main" => role.eq_ignore_ascii_case("main"),
      "nav" => role.eq_ignore_ascii_case("navigation"),
      "ol" | "ul" => role.eq_ignore_ascii_case("list"),
      "table" => role.eq_ignore_ascii_case("table"),
      "textarea" => role.eq_ignore_ascii_case("textbox"),
      _ => false,
    };
    if !redundant {
      return;
    }
    let message = "explicit role duplicates the element's native semantics".into();
    let help = Some("Remove the role and keep the native element semantics.".into());
    // Static `role="…"` only: remove the full `role="value"` extent. Bound
    // `:role` stays report-only (no complete replacement span).
    if let Some(range) = static_attribute_removal_range(context.source(), attribute) {
      context.report_with_safe_edit(
        self.meta(),
        attribute.span.clone(),
        message,
        help,
        range,
        String::new(),
      );
    } else {
      context.report(self.meta(), attribute.span.clone(), message, help);
    }
  }
}

/// Prefer removing ` role="value"` including the leading space and quoted value.
/// Falls back to the attribute name span (plus leading space) when the value
/// extent cannot be reconstructed — never a partial mid-attribute edit.
fn static_attribute_removal_range(
  source: &str,
  attribute: &vue_vet_core::TemplateAttributeFact,
) -> Option<ByteRange> {
  let value = attribute.value.as_ref()?;
  let bytes = source.as_bytes();
  let mut index = attribute.span.offset.saturating_add(attribute.span.length);
  while bytes.get(index).is_some_and(|byte| matches!(byte, b' ' | b'\t')) {
    index = index.saturating_add(1);
  }
  if bytes.get(index) != Some(&b'=') {
    return Some(name_only_removal_range(source, &attribute.span));
  }
  index = index.saturating_add(1);
  while bytes.get(index).is_some_and(|byte| matches!(byte, b' ' | b'\t')) {
    index = index.saturating_add(1);
  }
  let quote = *bytes.get(index)?;
  if quote != b'"' && quote != b'\'' {
    return Some(name_only_removal_range(source, &attribute.span));
  }
  index = index.saturating_add(1);
  let value_start = index;
  while bytes.get(index).is_some_and(|byte| *byte != quote) {
    index = index.saturating_add(1);
  }
  if index >= bytes.len() {
    return None;
  }
  let parsed = source.get(value_start..index)?;
  if parsed != value.as_str() {
    return None;
  }
  let end = index.saturating_add(1);
  let mut offset = attribute.span.offset;
  while offset > 0
    && bytes.get(offset.saturating_sub(1)).is_some_and(|byte| matches!(byte, b' ' | b'\t'))
  {
    offset = offset.saturating_sub(1);
  }
  Some(ByteRange { offset, length: end.saturating_sub(offset) })
}

fn name_only_removal_range(source: &str, span: &SourceSpan) -> ByteRange {
  let bytes = source.as_bytes();
  let mut offset = span.offset;
  while offset > 0
    && bytes.get(offset.saturating_sub(1)).is_some_and(|byte| matches!(byte, b' ' | b'\t'))
  {
    offset = offset.saturating_sub(1);
  }
  ByteRange { offset, length: span.offset.saturating_add(span.length).saturating_sub(offset) }
}
