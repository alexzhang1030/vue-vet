//! `<style>` `v-bind(ident)` under-approx → template expression facts.
use vize_croquis::sfc::SfcDescriptor;
use vue_vet_core::{TemplateExpressionFact, TemplateFacts};

use crate::span::source_span;

/// Replace `surface == "style"` expressions with a fresh under-approx scan of
/// `<style>` `v-bind(ident)` / quoted ident. Returns whether the style set changed.
pub fn refresh_style_v_bind_expressions(
  source: &str,
  descriptor: &SfcDescriptor<'_>,
  facts: &mut TemplateFacts,
) -> bool {
  let before: Vec<TemplateExpressionFact> =
    facts.expressions.iter().filter(|expression| expression.surface == "style").cloned().collect();
  facts.expressions.retain(|expression| expression.surface != "style");
  extract_style_v_bind_expressions(source, descriptor, facts);
  let after: Vec<TemplateExpressionFact> =
    facts.expressions.iter().filter(|expression| expression.surface == "style").cloned().collect();
  before != after
}

fn extract_style_v_bind_expressions(
  source: &str,
  descriptor: &SfcDescriptor<'_>,
  facts: &mut TemplateFacts,
) {
  for style in &descriptor.styles {
    for found in scan_style_v_bind_idents(style.content.as_ref()) {
      let offset = style.loc.start.saturating_add(found.byte_offset);
      facts.expressions.push(TemplateExpressionFact {
        surface: "style".into(),
        expression: found.ident.clone(),
        span: source_span(source, offset, found.ident.len()),
        identifiers: Some(vec![found.ident]),
      });
    }
  }
}

struct StyleVBindIdent {
  ident: String,
  byte_offset: usize,
}

/// Under-approx: `v-bind(ident)`, `v-bind('ident')`, `v-bind("ident")`.
/// Skip members, calls, and other expressions (`height + 'px'`, `theme.color`).
fn scan_style_v_bind_idents(content: &str) -> Vec<StyleVBindIdent> {
  let bytes = content.as_bytes();
  let mut found = Vec::new();
  let mut index = 0;
  while let Some(rel) = content.get(index..).and_then(|rest| rest.find("v-bind")) {
    let start = index.saturating_add(rel);
    if !v_bind_keyword_at(bytes, start) {
      index = start.saturating_add(1);
      continue;
    }
    let Some(ident) = parse_v_bind_simple_ident(content, bytes, start.saturating_add(6)) else {
      index = start.saturating_add(6);
      continue;
    };
    found.push(ident);
    index = start.saturating_add(6);
  }
  found
}

fn v_bind_keyword_at(bytes: &[u8], start: usize) -> bool {
  if start > 0
    && bytes.get(start.saturating_sub(1)).is_some_and(|byte| {
      byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-' || *byte == b'$'
    })
  {
    return false;
  }
  bytes.get(start..start.saturating_add(6)) == Some(b"v-bind".as_slice())
}

fn parse_v_bind_simple_ident(
  content: &str,
  bytes: &[u8],
  after_keyword: usize,
) -> Option<StyleVBindIdent> {
  let mut cursor = skip_ascii_ws(bytes, after_keyword);
  if bytes.get(cursor).copied() != Some(b'(') {
    return None;
  }
  cursor = skip_ascii_ws(bytes, cursor.saturating_add(1));
  let (ident_start, ident_end) = match bytes.get(cursor).copied() {
    Some(quote @ (b'\'' | b'"')) => {
      cursor = cursor.saturating_add(1);
      let start = cursor;
      while bytes.get(cursor).is_some_and(|byte| *byte != quote) {
        if !is_js_ident_byte(*bytes.get(cursor)?, cursor == start) {
          return None;
        }
        cursor = cursor.saturating_add(1);
      }
      if bytes.get(cursor).copied() != Some(quote) || start == cursor {
        return None;
      }
      let end = cursor;
      cursor = cursor.saturating_add(1);
      (start, end)
    }
    Some(byte) if is_js_ident_byte(byte, true) => {
      let start = cursor;
      cursor = cursor.saturating_add(1);
      while bytes.get(cursor).is_some_and(|next| is_js_ident_byte(*next, false)) {
        cursor = cursor.saturating_add(1);
      }
      (start, cursor)
    }
    _ => return None,
  };
  cursor = skip_ascii_ws(bytes, cursor);
  if bytes.get(cursor).copied() != Some(b')') {
    return None;
  }
  let ident = content.get(ident_start..ident_end)?.to_string();
  Some(StyleVBindIdent { ident, byte_offset: ident_start })
}

fn skip_ascii_ws(bytes: &[u8], mut cursor: usize) -> usize {
  while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
    cursor = cursor.saturating_add(1);
  }
  cursor
}

const fn is_js_ident_byte(byte: u8, first: bool) -> bool {
  byte == b'_' || byte == b'$' || byte.is_ascii_alphabetic() || (!first && byte.is_ascii_digit())
}
