use std::path::Path;

use thiserror::Error;
use vize_atelier_sfc::{SfcParseOptions, parse_sfc};
use vue_vet_core::{Diagnostic, Severity, SourceSpan};

#[derive(Debug, Error)]
pub enum AnalyzeError {
  #[error("Vize could not parse the SFC: {0}")]
  Parse(String),
}

pub fn analyze_sfc(path: &Path, source: &str) -> Result<Vec<Diagnostic>, AnalyzeError> {
  let descriptor = parse_sfc(source, SfcParseOptions::default())
    .map_err(|error| AnalyzeError::Parse(error.message.into()))?;
  let mut diagnostics = Vec::new();

  if let Some(template) = descriptor.template {
    for relative_offset in attribute_offsets(&template.content, "v-html") {
      let offset = template.loc.start + relative_offset;
      let (line, column) = line_column(source, offset);
      diagnostics.push(Diagnostic {
                rule_id: "vue-vet/security/no-v-html".into(),
                category: "security".into(),
                severity: Severity::Warning,
                message: "`v-html` can render untrusted HTML into the page".into(),
                help: Some(
                    "Prefer normal template interpolation. If raw HTML is required, sanitize it at the trust boundary."
                        .into(),
                ),
                file: path.to_path_buf(),
                span: SourceSpan { offset, length: "v-html".len(), line, column },
            });
    }
  }

  Ok(diagnostics)
}

fn attribute_offsets(template: &str, attribute: &str) -> Vec<usize> {
  template
    .match_indices(attribute)
    .filter_map(|(offset, _)| {
      let before = template[..offset].chars().next_back();
      let after = template[offset + attribute.len()..].chars().next();
      let boundary_before =
        before.is_some_and(|character| character.is_ascii_whitespace() || character == '<');
      let boundary_after = after.is_none_or(|character| {
        character.is_ascii_whitespace() || matches!(character, '=' | '.' | '>' | '/')
      });

      let opening = template[..offset].rfind('<')?;
      let closing = template[..offset].rfind('>');
      let inside_tag = closing.is_none_or(|closing| opening > closing);
      let tag_tail = &template[opening + 1..offset];
      let is_start_tag =
        tag_tail.chars().next().is_none_or(|character| !matches!(character, '!' | '/' | '?'));
      let comment_start = template[..offset].rfind("<!--");
      let comment_end = template[..offset].rfind("-->");
      let inside_comment =
        comment_start.is_some_and(|start| comment_end.is_none_or(|end| start > end));

      (boundary_before && boundary_after && inside_tag && is_start_tag && !inside_comment)
        .then_some(offset)
    })
    .collect()
}

fn line_column(source: &str, offset: usize) -> (usize, usize) {
  let prefix = &source[..offset.min(source.len())];
  let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
  let column = prefix.rsplit_once('\n').map_or(prefix.len() + 1, |(_, tail)| tail.len() + 1);
  (line, column)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reports_v_html_at_the_source_location() {
    let source = "<template>\n  <div v-html=\"html\" />\n</template>";
    let diagnostics = analyze_sfc(Path::new("Unsafe.vue"), source).unwrap();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].rule_id, "vue-vet/security/no-v-html");
    assert_eq!(diagnostics[0].span.line, 2);
    assert_eq!(diagnostics[0].span.column, 8);
  }

  #[test]
  fn ignores_the_same_text_outside_the_template() {
    let source = "<script setup>\nconst note = 'v-html'\n</script>\n<template><div /></template>";
    let diagnostics = analyze_sfc(Path::new("Safe.vue"), source).unwrap();

    assert!(diagnostics.is_empty());
  }

  #[test]
  fn ignores_comments_text_and_similar_attribute_names() {
    let source = r#"<template>
  <!-- <div v-html="html" /> -->
  <p>write v-html only when content is trusted</p>
  <div data-v-html="html" />
</template>"#;
    let diagnostics = analyze_sfc(Path::new("Safe.vue"), source).unwrap();

    assert!(diagnostics.is_empty());
  }
}
