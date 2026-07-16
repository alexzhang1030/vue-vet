use std::path::Path;

use thiserror::Error;
use vize_atelier_core::{
  Allocator, ElementNode, ExpressionNode, PropNode, TemplateChildNode, parse,
};
use vize_atelier_sfc::{SfcParseOptions, parse_sfc};
use vue_vet_core::{
  Diagnostic, ScriptFacts, ScriptKind, SourceSpan, TemplateAttributeFact, TemplateDirectiveFact,
  TemplateElementFact, TemplateFacts,
};
use vue_vet_oxc::{AnalyzeScriptError, analyze_script};
use vue_vet_rules::builtin_registry;

#[derive(Debug, Error)]
pub enum AnalyzeError {
  #[error("Vize could not parse the SFC: {0}")]
  Parse(String),
  #[error("Vize could not parse the template: {0}")]
  Template(String),
  #[error(transparent)]
  Script(#[from] AnalyzeScriptError),
}

/// Analyze one Vue single-file component.
///
/// # Errors
///
/// Returns [`AnalyzeError::Parse`] when Vize cannot parse the component or
/// [`AnalyzeError::Template`] when its template contains a fatal parse error,
/// or [`AnalyzeError::Script`] when an embedded JavaScript or TypeScript block
/// cannot be analyzed.
pub fn analyze_sfc(path: &Path, source: &str) -> Result<Vec<Diagnostic>, AnalyzeError> {
  let descriptor = parse_sfc(source, SfcParseOptions::default())
    .map_err(|error| AnalyzeError::Parse(error.message.into()))?;
  let template = if let Some(template) = descriptor.template {
    extract_template_facts(source, &template.content, template.loc.start)?
  } else {
    TemplateFacts::default()
  };
  let mut script = ScriptFacts::default();
  if let Some(block) = descriptor.script {
    script.blocks.push(analyze_script(
      source,
      &block.content,
      block.loc.start,
      block.lang.as_deref().unwrap_or("js"),
      ScriptKind::Script,
    )?);
  }
  if let Some(block) = descriptor.script_setup {
    script.blocks.push(analyze_script(
      source,
      &block.content,
      block.loc.start,
      block.lang.as_deref().unwrap_or("js"),
      ScriptKind::Setup,
    )?);
  }
  Ok(builtin_registry().run(path, source, &template, &script))
}

fn extract_template_facts(
  source: &str,
  template: &str,
  template_offset: usize,
) -> Result<TemplateFacts, AnalyzeError> {
  let allocator = Allocator::default();
  let (root, errors) = parse(allocator.as_bump(), template);
  if let Some(error) = errors.iter().find(|error| !error.is_recoverable()) {
    return Err(AnalyzeError::Template(error.to_string()));
  }

  let mut facts = TemplateFacts::default();
  collect_children(source, template_offset, &root.children, &mut facts);
  Ok(facts)
}

fn collect_children(
  source: &str,
  template_offset: usize,
  children: &[TemplateChildNode<'_>],
  facts: &mut TemplateFacts,
) {
  for child in children {
    if let TemplateChildNode::Element(element) = child {
      collect_element(source, template_offset, element, facts);
    }
  }
}

fn collect_element(
  source: &str,
  template_offset: usize,
  element: &ElementNode<'_>,
  facts: &mut TemplateFacts,
) {
  let offset = template_offset.saturating_add(position_offset(element.loc.start.offset));
  let end = template_offset.saturating_add(position_offset(element.loc.end.offset));
  let mut attributes = Vec::new();
  let mut directives = Vec::new();
  for prop in &element.props {
    match prop {
      PropNode::Attribute(attribute) => {
        let offset =
          template_offset.saturating_add(position_offset(attribute.name_loc.start.offset));
        attributes.push(TemplateAttributeFact {
          name: attribute.name.to_string(),
          value: attribute.value.as_ref().map(|value| value.content.to_string()),
          span: source_span(source, offset, attribute.name.len()),
        });
      }
      PropNode::Directive(directive) => {
        let raw_name = directive
          .raw_name
          .as_ref()
          .map_or_else(|| format!("v-{}", directive.name), ToString::to_string);
        let offset = template_offset.saturating_add(position_offset(directive.loc.start.offset));
        let argument = directive.arg.as_ref().map(expression_text);
        let expression = directive.exp.as_ref().map(expression_text);
        let modifiers = directive
          .modifiers
          .iter()
          .map(|modifier| modifier.content.to_string())
          .collect::<Vec<_>>();
        directives.push(TemplateDirectiveFact {
          name: directive.name.to_string(),
          argument,
          expression,
          modifiers,
          span: source_span(source, offset, raw_name.len()),
          raw_name,
        });
      }
    }
  }

  facts.elements.push(TemplateElementFact {
    tag: element.tag.to_string(),
    span: source_span(source, offset, end.saturating_sub(offset)),
    attributes,
    directives,
    has_children: !element.children.is_empty(),
  });
  collect_children(source, template_offset, &element.children, facts);
}

fn expression_text(expression: &ExpressionNode<'_>) -> String {
  match expression {
    ExpressionNode::Simple(expression) => expression.content.to_string(),
    ExpressionNode::Compound(expression) => expression.loc.source.to_string(),
  }
}

fn position_offset(offset: u32) -> usize {
  usize::try_from(offset).unwrap_or(usize::MAX)
}

fn source_span(source: &str, offset: usize, length: usize) -> SourceSpan {
  let (line, column) = line_column(source, offset);
  SourceSpan { offset, length, line, column }
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
