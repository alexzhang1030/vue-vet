//! Reconstruct complete static / bound attribute extents from name-only fact spans.

use vue_vet_core::{ByteRange, SourceSpan, TemplateAttributeFact, TemplateDirectiveFact};

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
    return Some(name_only_removal_range(source, attribute.span));
  }
  index = index.saturating_add(1);
  while bytes.get(index).is_some_and(|byte| matches!(byte, b' ' | b'\t')) {
    index = index.saturating_add(1);
  }
  let quote = *bytes.get(index)?;
  if quote != b'"' && quote != b'\'' {
    return Some(name_only_removal_range(source, attribute.span));
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
  name_span: SourceSpan,
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
  directive_span: SourceSpan,
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

/// Strip a reconstructable `.native` modifier from `@event` / `v-on:event`.
///
/// Vize's directive span is often the `@` / `v-on` prefix, not the full raw
/// name. Reconstruct the contiguous name (`event`, modifiers, optional
/// `[arg]`) from that prefix, then drop `.native`. The handler value is left
/// untouched. Stay incomplete when the prefix does not match `raw_name`, the
/// suffix is not a contiguous name, `.native` is not a distinct modifier
/// token, or the leftover name would be a dangling `@` / `v-on:`. Extra
/// modifiers stay. Bare `v-on` (object listener form) is allowed.
#[must_use]
pub(super) fn strip_native_on_modifier(
  source: &str,
  directive: &TemplateDirectiveFact,
) -> Option<(ByteRange, String)> {
  if directive.name != "on" {
    return None;
  }
  if !directive.modifiers.iter().any(|modifier| modifier == "native") {
    return None;
  }
  let start = directive.span.offset;
  let prefix_end = start.checked_add(directive.span.length)?;
  let prefix = source.get(start..prefix_end)?;
  if prefix != directive.raw_name {
    return None;
  }
  let name_end = scan_on_raw_name_end(source.as_bytes(), prefix_end)?;
  let full_name = source.get(start..name_end)?;
  if !suffix_matches_on_argument(source.get(prefix_end..name_end)?, directive.argument.as_deref()) {
    return None;
  }
  let stripped = strip_native_token(full_name)?;
  if stripped.is_empty() || stripped == full_name {
    return None;
  }
  if !remaining_on_name_is_complete(&stripped) {
    return None;
  }
  Some((ByteRange { offset: start, length: name_end.saturating_sub(start) }, stripped))
}

fn scan_on_raw_name_end(bytes: &[u8], start: usize) -> Option<usize> {
  let mut index = start;
  while let Some(byte) = bytes.get(index) {
    if matches!(byte, b'=' | b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'>') {
      break;
    }
    if !byte.is_ascii_alphanumeric() && !matches!(byte, b':' | b'.' | b'[' | b']' | b'-' | b'_') {
      return None;
    }
    index = index.saturating_add(1);
  }
  Some(index)
}

fn suffix_matches_on_argument(suffix: &str, argument: Option<&str>) -> bool {
  let Some(argument) = argument.filter(|name| !name.is_empty()) else {
    return true;
  };
  let trimmed = suffix.strip_prefix(':').unwrap_or(suffix);
  let trimmed = trimmed.strip_prefix('[').unwrap_or(trimmed);
  let Some(rest) = trimmed.strip_prefix(argument) else {
    return false;
  };
  rest.is_empty() || rest.starts_with('.') || rest.starts_with(']')
}

fn strip_native_token(raw: &str) -> Option<String> {
  let (head, rest) = raw.split_once('.')?;
  if head.is_empty() {
    return None;
  }
  let mut kept = Vec::new();
  let mut removed = false;
  for part in rest.split('.') {
    if part.is_empty() {
      return None;
    }
    if part == "native" {
      removed = true;
      continue;
    }
    kept.push(part);
  }
  if !removed {
    return None;
  }
  if kept.is_empty() { Some(head.to_string()) } else { Some(format!("{head}.{}", kept.join("."))) }
}

fn remaining_on_name_is_complete(name: &str) -> bool {
  if name == "@" || name.ends_with('@') || name.ends_with(':') {
    return false;
  }
  !name.is_empty()
}

/// Rewrite `:arg.sync="expr"` / `v-bind:arg.sync="expr"` to `v-model:arg="expr"`
/// when the quoted value reconstructs exactly.
///
/// Stays incomplete for object `v-bind.sync`, unquoted values, extra modifiers,
/// and dynamic `:[name].sync` (the fact argument is the inner ident, so the
/// source must still start with that ident after `:` / `v-bind`).
pub(super) fn quoted_sync_bind_to_v_model(
  source: &str,
  directive: &TemplateDirectiveFact,
) -> Option<(ByteRange, String)> {
  if directive.name != "bind" {
    return None;
  }
  let argument = directive.argument.as_deref().filter(|name| is_static_prop_name(name))?;
  if !directive.modifiers.iter().any(|modifier| modifier == "sync")
    && !directive.raw_name.contains(".sync")
  {
    return None;
  }
  if directive.modifiers.iter().any(|modifier| modifier != "sync") {
    return None;
  }
  let expression = directive.expression.as_deref()?;
  let bytes = source.as_bytes();
  let mut index = directive.span.offset.saturating_add(directive.span.length);
  index = skip_horizontal_space(bytes, index);
  if bytes.get(index) == Some(&b':') {
    index = skip_horizontal_space(bytes, index.saturating_add(1));
  }
  if !source.get(index..)?.starts_with(argument) {
    return None;
  }
  let after_argument = index.saturating_add(argument.len());
  if bytes
    .get(after_argument)
    .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
  {
    return None;
  }
  index = skip_horizontal_space(bytes, after_argument);
  if !source.get(index..)?.starts_with(".sync") {
    return None;
  }
  index = index.saturating_add(".sync".len());
  if bytes.get(index) == Some(&b'.') {
    return None;
  }
  index = skip_horizontal_space(bytes, index);
  if bytes.get(index) != Some(&b'=') {
    return None;
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
  if parsed != expression {
    return None;
  }
  let end = index.saturating_add(1);
  let range =
    ByteRange { offset: directive.span.offset, length: end.saturating_sub(directive.span.offset) };
  let quote_char = char::from(quote);
  Some((range, format!("v-model:{argument}={quote_char}{expression}{quote_char}")))
}

fn is_static_prop_name(name: &str) -> bool {
  let mut chars = name.chars();
  let Some(first) = chars.next() else {
    return false;
  };
  if !first.is_ascii_alphabetic() && first != '_' {
    return false;
  }
  chars.all(|next| next.is_ascii_alphanumeric() || matches!(next, '-' | '_'))
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

fn name_only_removal_range(source: &str, span: SourceSpan) -> ByteRange {
  expand_leading_space(source, span.offset, span.offset.saturating_add(span.length))
}

#[cfg(test)]
mod tests {
  use super::{
    bound_quoted_value_removal_range, quoted_name_value_removal_range, quoted_sync_bind_to_v_model,
    strip_native_on_modifier,
  };
  use vue_vet_core::{SourceSpan, TemplateDirectiveFact};

  fn name_span(source: &str, name: &str) -> SourceSpan {
    let offset = source.find(name).unwrap_or(0);
    SourceSpan { offset, length: name.len(), line: 1, column: offset.saturating_add(1) }
  }

  #[test]
  #[expect(clippy::panic, reason = "unit test asserts a reconstructed range exists")]
  fn quoted_true_includes_leading_space_and_value() {
    let source = r#"<button aria-hidden="true">Save</button>"#;
    let span = name_span(source, "aria-hidden");
    let Some(range) = quoted_name_value_removal_range(source, span, "true") else {
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
    let Some(range) = bound_quoted_value_removal_range(source, colon, "aria-hidden", "true") else {
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
    let Some(range) = bound_quoted_value_removal_range(source, prefix, "aria-hidden", "true")
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
    assert!(quoted_name_value_removal_range(source, span, "true").is_none());
  }

  fn bind_sync(
    source: &str,
    raw: &str,
    argument: Option<&str>,
    expression: &str,
    modifiers: &[&str],
  ) -> TemplateDirectiveFact {
    let offset = source.find(raw).unwrap_or(0);
    TemplateDirectiveFact {
      name: "bind".into(),
      raw_name: raw.into(),
      argument: argument.map(str::to_string),
      expression: Some(expression.into()),
      modifiers: modifiers.iter().map(|modifier| (*modifier).to_string()).collect(),
      span: SourceSpan { offset, length: raw.len(), line: 1, column: offset.saturating_add(1) },
    }
  }

  #[test]
  #[expect(clippy::panic, reason = "unit test asserts a reconstructed rewrite exists")]
  fn shorthand_sync_rewrites_to_v_model() {
    let source = r#"<Comp :title.sync="title" />"#;
    let directive = bind_sync(source, ":", Some("title"), "title", &["sync"]);
    let Some((range, replacement)) = quoted_sync_bind_to_v_model(source, &directive) else {
      panic!("quoted :title.sync must rewrite");
    };
    let replaced = source.get(range.offset..range.offset.saturating_add(range.length));
    assert_eq!(replaced, Some(r#":title.sync="title""#));
    assert_eq!(replacement, r#"v-model:title="title""#);
  }

  #[test]
  #[expect(clippy::panic, reason = "unit test asserts a reconstructed rewrite exists")]
  fn v_bind_sync_rewrites_to_v_model() {
    let source = r#"<Comp v-bind:open.sync="open" />"#;
    let directive = bind_sync(source, "v-bind", Some("open"), "open", &["sync"]);
    let Some((range, replacement)) = quoted_sync_bind_to_v_model(source, &directive) else {
      panic!("quoted v-bind:open.sync must rewrite");
    };
    let replaced = source.get(range.offset..range.offset.saturating_add(range.length));
    assert_eq!(replaced, Some(r#"v-bind:open.sync="open""#));
    assert_eq!(replacement, r#"v-model:open="open""#);
  }

  #[test]
  fn unquoted_sync_stays_incomplete() {
    let source = "<Comp :title.sync=title />";
    let directive = bind_sync(source, ":", Some("title"), "title", &["sync"]);
    assert!(quoted_sync_bind_to_v_model(source, &directive).is_none());
  }

  #[test]
  fn object_sync_stays_incomplete() {
    let source = r#"<Comp v-bind.sync="state" />"#;
    let directive = bind_sync(source, "v-bind", None, "state", &["sync"]);
    assert!(quoted_sync_bind_to_v_model(source, &directive).is_none());
  }

  #[test]
  fn extra_modifier_stays_incomplete() {
    let source = r#"<Comp :title.sync.foo="title" />"#;
    let directive = bind_sync(source, ":", Some("title"), "title", &["sync", "foo"]);
    assert!(quoted_sync_bind_to_v_model(source, &directive).is_none());
  }

  #[test]
  fn dynamic_argument_stays_incomplete() {
    let source = r#"<Comp :[name].sync="title" />"#;
    let directive = bind_sync(source, ":", Some("name"), "title", &["sync"]);
    assert!(quoted_sync_bind_to_v_model(source, &directive).is_none());
  }

  fn on_native(
    source: &str,
    raw: &str,
    argument: Option<&str>,
    modifiers: &[&str],
  ) -> TemplateDirectiveFact {
    let offset = source.find(raw).unwrap_or(0);
    TemplateDirectiveFact {
      name: "on".into(),
      raw_name: raw.into(),
      argument: argument.map(str::to_string),
      expression: Some("activate".into()),
      modifiers: modifiers.iter().map(|modifier| (*modifier).to_string()).collect(),
      span: SourceSpan { offset, length: raw.len(), line: 1, column: offset.saturating_add(1) },
    }
  }

  #[test]
  #[expect(clippy::panic, reason = "unit test asserts a reconstructed rewrite exists")]
  fn shorthand_native_reconstructs_from_at_prefix() {
    let source = r#"<Widget @click.native="activate" />"#;
    let directive = on_native(source, "@", Some("click"), &["native"]);
    let Some((range, replacement)) = strip_native_on_modifier(source, &directive) else {
      panic!("@click.native must strip from the @ prefix span");
    };
    let replaced = source.get(range.offset..range.offset.saturating_add(range.length));
    assert_eq!(replaced, Some("@click.native"));
    assert_eq!(replacement, "@click");
  }

  #[test]
  #[expect(clippy::panic, reason = "unit test asserts a reconstructed rewrite exists")]
  fn v_on_native_reconstructs_from_v_on_prefix() {
    let source = r#"<Widget v-on:click.native="activate" />"#;
    let directive = on_native(source, "v-on", Some("click"), &["native"]);
    let Some((range, replacement)) = strip_native_on_modifier(source, &directive) else {
      panic!("v-on:click.native must strip from the v-on prefix span");
    };
    let replaced = source.get(range.offset..range.offset.saturating_add(range.length));
    assert_eq!(replaced, Some("v-on:click.native"));
    assert_eq!(replacement, "v-on:click");
  }

  #[test]
  #[expect(clippy::panic, reason = "unit test asserts a reconstructed rewrite exists")]
  fn extra_modifiers_stay_after_native_strip() {
    let source = r#"<Widget @click.native.stop="activate" />"#;
    let directive = on_native(source, "@", Some("click"), &["native", "stop"]);
    let Some((range, replacement)) = strip_native_on_modifier(source, &directive) else {
      panic!("@click.native.stop must keep stop");
    };
    let replaced = source.get(range.offset..range.offset.saturating_add(range.length));
    assert_eq!(replaced, Some("@click.native.stop"));
    assert_eq!(replacement, "@click.stop");
  }

  #[test]
  #[expect(clippy::panic, reason = "unit test asserts a reconstructed rewrite exists")]
  fn native_after_other_modifiers_strips() {
    let source = r#"<Widget @click.prevent.native="activate" />"#;
    let directive = on_native(source, "@", Some("click"), &["prevent", "native"]);
    let Some((_, replacement)) = strip_native_on_modifier(source, &directive) else {
      panic!("@click.prevent.native must strip");
    };
    assert_eq!(replacement, "@click.prevent");
  }

  #[test]
  #[expect(clippy::panic, reason = "unit test asserts a reconstructed rewrite exists")]
  fn dynamic_argument_strips_native() {
    let source = r#"<Widget @[event].native="activate" />"#;
    let directive = on_native(source, "@", Some("event"), &["native"]);
    let Some((range, replacement)) = strip_native_on_modifier(source, &directive) else {
      panic!("@[event].native must strip");
    };
    let replaced = source.get(range.offset..range.offset.saturating_add(range.length));
    assert_eq!(replaced, Some("@[event].native"));
    assert_eq!(replacement, "@[event]");
  }

  #[test]
  fn mismatched_prefix_stays_incomplete() {
    let source = r#"<Widget @click.native="activate" />"#;
    let mut directive = on_native(source, "@", Some("click"), &["native"]);
    directive.raw_name = "v-on".into();
    assert!(
      strip_native_on_modifier(source, &directive).is_none(),
      "a raw_name that does not match the source prefix must stay report-only"
    );
  }

  #[test]
  fn dangling_at_sign_stays_incomplete() {
    let source = r#"<Widget @.native="activate" />"#;
    let directive = on_native(source, "@", None, &["native"]);
    assert!(strip_native_on_modifier(source, &directive).is_none());
  }
}
