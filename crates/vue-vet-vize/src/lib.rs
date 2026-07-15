use std::path::Path;

use thiserror::Error;
use vize_atelier_sfc::{SfcParseOptions, parse_sfc};
use vue_vet_core::{Diagnostic, Severity, SourceSpan};

#[derive(Debug, Error)]
pub enum AnalyzeError {
  #[error("Vize could not parse the SFC: {0}")]
  Parse(String),
}

/// Analyze one Vue single-file component.
///
/// # Errors
///
/// Returns [`AnalyzeError::Parse`] when Vize cannot parse the component.
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
      let prefix = template.get(..offset)?;
      let suffix = template.get(offset + attribute.len()..)?;
      let before = prefix.chars().next_back();
      let after = suffix.chars().next();
      let boundary_before =
        before.is_some_and(|character| character.is_ascii_whitespace() || character == '<');
      let boundary_after = after.is_none_or(|character| {
        character.is_ascii_whitespace() || matches!(character, '=' | '.' | '>' | '/')
      });

      let opening = prefix.rfind('<')?;
      let closing = prefix.rfind('>');
      let inside_tag = closing.is_none_or(|closing| opening > closing);
      let tag_tail = template.get(opening + 1..offset)?;
      let is_start_tag =
        tag_tail.chars().next().is_none_or(|character| !matches!(character, '!' | '/' | '?'));
      let comment_start = prefix.rfind("<!--");
      let comment_end = prefix.rfind("-->");
      let inside_comment =
        comment_start.is_some_and(|start| comment_end.is_none_or(|end| start > end));

      (boundary_before && boundary_after && inside_tag && is_start_tag && !inside_comment)
        .then_some(offset)
    })
    .collect()
}

fn line_column(source: &str, offset: usize) -> (usize, usize) {
  let bytes = source.as_bytes();
  let prefix = bytes.get(..offset.min(bytes.len())).unwrap_or(bytes);
  let line =
    prefix.iter().fold(1_usize, |line, byte| line.saturating_add(usize::from(*byte == b'\n')));
  let column = prefix
    .iter()
    .rposition(|byte| *byte == b'\n')
    .map_or_else(|| prefix.len().saturating_add(1), |newline| prefix.len().saturating_sub(newline));
  (line, column)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[expect(clippy::panic, reason = "an unexpected parser error must fail the test")]
  fn analyze_for_test(path: &Path, source: &str) -> Vec<Diagnostic> {
    match analyze_sfc(path, source) {
      Ok(diagnostics) => diagnostics,
      Err(error) => panic!("analysis unexpectedly failed: {error}"),
    }
  }

  #[test]
  fn reports_v_html_at_the_source_location() {
    let source = "<template>\n  <div v-html=\"html\" />\n</template>";
    let diagnostics = analyze_for_test(Path::new("Unsafe.vue"), source);

    assert_eq!(diagnostics.len(), 1, "expected exactly one v-html diagnostic");
    assert_eq!(
      diagnostics.first().map(|diagnostic| diagnostic.rule_id.as_str()),
      Some("vue-vet/security/no-v-html"),
      "expected the stable no-v-html rule ID"
    );
    assert_eq!(diagnostics.first().map(|diagnostic| diagnostic.span.line), Some(2));
    assert_eq!(diagnostics.first().map(|diagnostic| diagnostic.span.column), Some(8));
  }

  #[test]
  fn ignores_the_same_text_outside_the_template() {
    let source = "<script setup>\nconst note = 'v-html'\n</script>\n<template><div /></template>";
    let diagnostics = analyze_for_test(Path::new("Safe.vue"), source);

    assert!(diagnostics.is_empty(), "script text must not be treated as a template directive");
  }

  #[test]
  fn ignores_comments_text_and_similar_attribute_names() {
    let source = r#"<template>
  <!-- <div v-html="html" /> -->
  <p>write v-html only when content is trusted</p>
  <div data-v-html="html" />
</template>"#;
    let diagnostics = analyze_for_test(Path::new("Safe.vue"), source);

    assert!(diagnostics.is_empty(), "non-directive text and attributes must not produce findings");
  }
}
