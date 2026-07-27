//! Shared helpers for accessible-name content rules.

use vue_vet_core::{ByteRange, TemplateAttributeFact, TemplateElementFact};

#[must_use]
pub(super) const fn is_anchor_like(tag: &str) -> bool {
  tag.eq_ignore_ascii_case("a")
    || tag.eq_ignore_ascii_case("RouterLink")
    || tag.eq_ignore_ascii_case("router-link")
    || tag.eq_ignore_ascii_case("NuxtLink")
    || tag.eq_ignore_ascii_case("nuxt-link")
}

#[must_use]
pub(super) fn is_heading(tag: &str) -> bool {
  matches!(tag.to_ascii_lowercase().as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

#[must_use]
pub(super) fn has_accessible_name_attrs(element: &TemplateElementFact) -> bool {
  element.attribute("aria-label").is_some()
    || element.bound_attribute("aria-label").is_some()
    || element.attribute("aria-labelledby").is_some()
    || element.bound_attribute("aria-labelledby").is_some()
}

#[must_use]
pub(super) fn is_form_control(element: &TemplateElementFact) -> bool {
  match element.tag.to_ascii_lowercase().as_str() {
    "textarea" | "select" | "meter" | "output" | "progress" => true,
    "input" => !input_type_skips_label(element),
    _ => false,
  }
}

fn input_type_skips_label(element: &TemplateElementFact) -> bool {
  let Some(type_name) = element
    .attribute("type")
    .and_then(|attribute| attribute.value.as_deref())
    .map(str::trim)
    .filter(|value| !value.is_empty())
  else {
    // Missing type defaults to text — needs a label.
    return false;
  };
  matches!(
    type_name.to_ascii_lowercase().as_str(),
    "hidden" | "button" | "submit" | "reset" | "image"
  )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum AssocToken<'a> {
  Static(&'a str),
  Expr(&'a str),
}

#[must_use]
pub(super) fn association_token<'a>(
  element: &'a TemplateElementFact,
  name: &str,
) -> Option<AssocToken<'a>> {
  if let Some(attribute) = element.attribute(name) {
    let value = attribute.value.as_deref()?.trim();
    if value.is_empty() {
      return None;
    }
    return Some(AssocToken::Static(value));
  }
  let directive = element.bound_attribute(name)?;
  let expression = directive.expression.as_deref()?.trim();
  if expression.is_empty() {
    return None;
  }
  Some(AssocToken::Expr(expression))
}

/// Static `title="…"` → insert matching `aria-label` after the attribute.
/// Bound `:title` and values containing quotes are left for manual review.
#[must_use]
pub(super) fn title_to_aria_label_edit(
  source: &str,
  element: &TemplateElementFact,
) -> Option<(ByteRange, String)> {
  if element.bound_attribute("title").is_some() {
    return None;
  }
  let title = element.attribute("title")?;
  let (extent, value) = quoted_attribute_extent(source, title)?;
  if value.trim().is_empty() || value.contains('"') || value.contains('\'') {
    return None;
  }
  Some((
    ByteRange { offset: extent.offset.saturating_add(extent.length), length: 0 },
    format!(" aria-label=\"{value}\""),
  ))
}

fn quoted_attribute_extent(
  source: &str,
  attribute: &TemplateAttributeFact,
) -> Option<(ByteRange, String)> {
  let value = attribute.value.as_ref()?.clone();
  let bytes = source.as_bytes();
  let mut index = attribute.span.offset.saturating_add(attribute.span.length);
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
  if parsed != value {
    return None;
  }
  let end = index.saturating_add(1);
  Some((
    ByteRange { offset: attribute.span.offset, length: end.saturating_sub(attribute.span.offset) },
    value,
  ))
}
