//! Reconstruct complete static / bound attribute extents from name-only fact spans.

use vue_vet_core::{ByteRange, SourceSpan, TemplateAttributeFact};

/// Prefer removing ` name="value"` including the leading space and quoted value.
/// Falls back to the attribute name span (plus leading space) when the value
/// extent cannot be reconstructed — never a partial mid-attribute edit.
pub(super) fn static_attribute_removal_range(
  source: &str,
  attribute: &TemplateAttributeFact,
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
  Some(expand_leading_space(source, attribute.span.offset, end))
}

/// Full `name="expected"` / `:name="expected"` removal when quotes and the
/// value reconstruct exactly. Incomplete coverage returns [`None`].
pub(super) fn quoted_name_value_removal_range(
  source: &str,
  name_span: &SourceSpan,
  expected_value: &str,
) -> Option<ByteRange> {
  let bytes = source.as_bytes();
  let mut index = name_span.offset.saturating_add(name_span.length);
  while bytes.get(index).is_some_and(|byte| matches!(byte, b' ' | b'\t')) {
    index = index.saturating_add(1);
  }
  if bytes.get(index) != Some(&b'=') {
    return None;
  }
  index = index.saturating_add(1);
  while bytes.get(index).is_some_and(|byte| matches!(byte, b' ' | b'\t')) {
    index = index.saturating_add(1);
  }
  let quote = *bytes.get(index)?;
  if quote != b'"' && quote != b'\'' {
    return None;
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
  if parsed != expected_value {
    return None;
  }
  let end = index.saturating_add(1);
  Some(expand_leading_space(source, name_span.offset, end))
}

/// Reconstruct `:arg="value"` / `v-bind:arg="value"` when the fact span covers
/// only the directive prefix (`:` or `v-bind`) rather than the full raw name.
pub(super) fn bound_quoted_value_removal_range(
  source: &str,
  directive_span: &SourceSpan,
  argument: &str,
  expected_value: &str,
) -> Option<ByteRange> {
  if argument.is_empty() {
    return None;
  }
  let bytes = source.as_bytes();
  let mut index = directive_span.offset.saturating_add(directive_span.length);
  index = skip_horizontal_space(bytes, index);
  if bytes.get(index) == Some(&b':') {
    index = skip_horizontal_space(bytes, index.saturating_add(1));
  }
  let found_argument = source.get(index..)?.starts_with(argument);
  if found_argument {
    index = skip_horizontal_space(bytes, index.saturating_add(argument.len()));
  } else if bytes.get(index) != Some(&b'=') {
    return None;
  }
  if bytes.get(index) != Some(&b'=') {
    return None;
  }
  if !found_argument {
    let prefix = source.get(directive_span.offset..index)?;
    if !prefix.contains(argument) {
      return None;
    }
  }
  index = skip_horizontal_space(bytes, index.saturating_add(1));
  let quote = *bytes.get(index)?;
  if quote != b'"' && quote != b'\'' {
    return None;
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
  if parsed != expected_value {
    return None;
  }
  let end = index.saturating_add(1);
  Some(expand_leading_space(source, directive_span.offset, end))
}

fn skip_horizontal_space(bytes: &[u8], mut index: usize) -> usize {
  while bytes.get(index).is_some_and(|byte| matches!(byte, b' ' | b'\t')) {
    index = index.saturating_add(1);
  }
  index
}

fn expand_leading_space(source: &str, name_offset: usize, end: usize) -> ByteRange {
  let bytes = source.as_bytes();
  let mut offset = name_offset;
  while offset > 0
    && bytes.get(offset.saturating_sub(1)).is_some_and(|byte| matches!(byte, b' ' | b'\t'))
  {
    offset = offset.saturating_sub(1);
  }
  ByteRange { offset, length: end.saturating_sub(offset) }
}

fn name_only_removal_range(source: &str, span: &SourceSpan) -> ByteRange {
  expand_leading_space(source, span.offset, span.offset.saturating_add(span.length))
}

#[cfg(test)]
mod tests {
  use super::{bound_quoted_value_removal_range, quoted_name_value_removal_range};
  use vue_vet_core::SourceSpan;

  fn name_span(source: &str, name: &str) -> SourceSpan {
    let offset = source.find(name).unwrap_or(0);
    SourceSpan { offset, length: name.len(), line: 1, column: offset.saturating_add(1) }
  }

  #[test]
  #[expect(clippy::panic, reason = "unit test asserts a reconstructed range exists")]
  fn quoted_true_includes_leading_space_and_value() {
    let source = r#"<button aria-hidden="true">Save</button>"#;
    let span = name_span(source, "aria-hidden");
    let Some(range) = quoted_name_value_removal_range(source, &span, "true") else {
      panic!("quoted true must reconstruct");
    };
    let removed = source.get(range.offset..range.offset.saturating_add(range.length));
    assert_eq!(removed, Some(r#" aria-hidden="true""#));
  }

  #[test]
  #[expect(clippy::panic, reason = "unit test asserts a reconstructed range exists")]
  fn bound_true_reconstructs_from_colon_prefix_span() {
    let source = r#"<button :aria-hidden="true">Save</button>"#;
    let colon = name_span(source, ":");
    let colon = SourceSpan { offset: colon.offset, length: 1, line: 1, column: colon.column };
    let Some(range) = bound_quoted_value_removal_range(source, &colon, "aria-hidden", "true")
    else {
      panic!("colon-prefix bind must reconstruct");
    };
    let removed = source.get(range.offset..range.offset.saturating_add(range.length));
    assert_eq!(removed, Some(r#" :aria-hidden="true""#));
  }

  #[test]
  #[expect(clippy::panic, reason = "unit test asserts a reconstructed range exists")]
  fn v_bind_true_reconstructs_from_v_bind_prefix_span() {
    let source = r#"<button v-bind:aria-hidden="true">Save</button>"#;
    let prefix = name_span(source, "v-bind");
    let Some(range) = bound_quoted_value_removal_range(source, &prefix, "aria-hidden", "true")
    else {
      panic!("v-bind prefix must reconstruct");
    };
    let removed = source.get(range.offset..range.offset.saturating_add(range.length));
    assert_eq!(removed, Some(r#" v-bind:aria-hidden="true""#));
  }

  #[test]
  fn unquoted_value_stays_incomplete() {
    let source = "<button aria-hidden=true>Save</button>";
    let span = name_span(source, "aria-hidden");
    assert!(quoted_name_value_removal_range(source, &span, "true").is_none());
  }
}
