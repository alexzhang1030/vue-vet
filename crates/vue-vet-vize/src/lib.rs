use std::collections::BTreeSet;
use std::path::Path;

use thiserror::Error;
use vize_atelier_core::{
  Allocator, ElementNode, ExpressionNode, ForNode, PropNode, TemplateChildNode, parse,
};
use vize_atelier_sfc::{SfcParseOptions, parse_sfc};
use vue_vet_core::{
  Diagnostic, RuleEnvironment, ScriptFacts, ScriptKind, SfcFacts, SourceSpan,
  TemplateAttributeFact, TemplateDirectiveFact, TemplateElementFact, TemplateExpressionFact,
  TemplateFacts,
};
use vue_vet_oxc::{
  AnalyzeScriptError, analyze_script, slot_prop_alias_identifiers,
  template_expression_identifiers_with_shadow, v_for_alias_identifiers,
};
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

pub struct AnalyzedSfc {
  pub diagnostics: Vec<Diagnostic>,
  pub facts: SfcFacts,
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
  analyze_sfc_with_facts(path, source).map(|analysis| analysis.diagnostics)
}

/// Analyze one Vue SFC and retain its dependency-neutral project facts.
///
/// # Errors
///
/// Returns the same deterministic parse and semantic errors as [`analyze_sfc`].
pub fn analyze_sfc_with_facts(path: &Path, source: &str) -> Result<AnalyzedSfc, AnalyzeError> {
  analyze_sfc_with_environment(path, source, RuleEnvironment::default())
}

/// Analyze one Vue SFC with project capability information.
///
/// # Errors
///
/// Returns the same deterministic parse and semantic errors as [`analyze_sfc`].
pub fn analyze_sfc_with_environment(
  path: &Path,
  source: &str,
  environment: RuleEnvironment,
) -> Result<AnalyzedSfc, AnalyzeError> {
  let descriptor = parse_sfc(source, SfcParseOptions::default())
    .map_err(|error| AnalyzeError::Parse(error.message.into()))?;
  let template = if let Some(template) = descriptor.template {
    // Vize already supplies template content + absolute SFC content offsets.
    extract_template_facts(source, &template.content, template.loc.start)?
  } else {
    TemplateFacts::default()
  };
  let mut script = ScriptFacts::default();
  if let Some(block) = descriptor.script {
    // `block.loc.start/end` are absolute offsets into the original SFC source.
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
  // Join Vize template expressions onto Oxc script reactive bindings.
  for block in &mut script.blocks {
    block.reactivity_graph.join_template_reads(&template);
  }
  let diagnostics =
    builtin_registry().run_with_environment(path, source, &template, &script, environment);
  Ok(AnalyzedSfc { diagnostics, facts: SfcFacts { template, script } })
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
  let mut scopes = TemplateAliasScopes::default();
  collect_children(source, template_offset, &root.children, &mut facts, &mut scopes);
  facts.expressions.sort_by_key(|expression| expression.span.offset);
  Ok(facts)
}

/// Stack of template-local aliases (`v-for` / `v-slot`) that shadow script bindings.
#[derive(Default)]
struct TemplateAliasScopes {
  stack: Vec<BTreeSet<String>>,
}

impl TemplateAliasScopes {
  fn push(&mut self, aliases: BTreeSet<String>) {
    if !aliases.is_empty() {
      self.stack.push(aliases);
    }
  }

  fn pop_if(&mut self, aliases: &BTreeSet<String>) {
    if !aliases.is_empty() {
      self.stack.pop();
    }
  }

  fn shadowed(&self) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for scope in &self.stack {
      names.extend(scope.iter().cloned());
    }
    names
  }
}

fn collect_children(
  source: &str,
  template_offset: usize,
  children: &[TemplateChildNode<'_>],
  facts: &mut TemplateFacts,
  scopes: &mut TemplateAliasScopes,
) {
  for child in children {
    match child {
      TemplateChildNode::Element(element) => {
        collect_element(source, template_offset, element, facts, scopes);
      }
      TemplateChildNode::Interpolation(interpolation) => {
        push_expression_fact(
          source,
          template_offset,
          "interpolation",
          &interpolation.content,
          facts,
          scopes,
        );
      }
      TemplateChildNode::If(if_node) => {
        for branch in &if_node.branches {
          if let Some(condition) = &branch.condition {
            push_expression_fact(source, template_offset, "if", condition, facts, scopes);
          }
          collect_children(source, template_offset, &branch.children, facts, scopes);
        }
      }
      TemplateChildNode::For(for_node) => {
        // Transform-time structural For nodes (raw parse keeps v-for on Element props).
        let aliases = structural_for_aliases(for_node);
        push_expression_fact(source, template_offset, "for", &for_node.source, facts, scopes);
        scopes.push(aliases.clone());
        collect_children(source, template_offset, &for_node.children, facts, scopes);
        scopes.pop_if(&aliases);
      }
      TemplateChildNode::IfBranch(branch) => {
        if let Some(condition) = &branch.condition {
          push_expression_fact(source, template_offset, "if", condition, facts, scopes);
        }
        collect_children(source, template_offset, &branch.children, facts, scopes);
      }
      TemplateChildNode::Text(_)
      | TemplateChildNode::Comment(_)
      | TemplateChildNode::TextCall(_)
      | TemplateChildNode::CompoundExpression(_)
      | TemplateChildNode::Hoisted(_) => {}
    }
  }
}

fn collect_element(
  source: &str,
  template_offset: usize,
  element: &ElementNode<'_>,
  facts: &mut TemplateFacts,
  scopes: &mut TemplateAliasScopes,
) {
  let offset = template_offset.saturating_add(position_offset(element.loc.start.offset));
  let end = template_offset.saturating_add(position_offset(element.loc.end.offset));
  let mut attributes = Vec::new();
  let mut directives = Vec::new();

  // v-for / v-slot aliases scope the element's own props and descendants.
  let local_aliases = element_local_aliases(element);
  scopes.push(local_aliases.clone());

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
          argument: argument.clone(),
          expression: expression.clone(),
          modifiers,
          span: source_span(source, offset, raw_name.len()),
          raw_name,
        });
        if let Some(exp) = &directive.exp {
          let surface = if directive.name == "bind" {
            argument.unwrap_or_else(|| "bind".into())
          } else {
            directive.name.to_string()
          };
          // For the for-source expression, outer aliases may still apply; this
          // element's own for aliases are already on the stack (and only affect
          // non-source free ids because source extraction drops the alias side).
          push_expression_fact(source, template_offset, &surface, exp, facts, scopes);
        }
        if let Some(arg) = &directive.arg {
          // Dynamic argument only: v-bind:[foo]. Static `:title` args are not reads.
          if !expression_is_static(arg) {
            push_expression_fact(source, template_offset, "bind-arg", arg, facts, scopes);
          }
        }
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
  collect_children(source, template_offset, &element.children, facts, scopes);
  scopes.pop_if(&local_aliases);
}

fn element_local_aliases(element: &ElementNode<'_>) -> BTreeSet<String> {
  let mut aliases = BTreeSet::new();
  for prop in &element.props {
    let PropNode::Directive(directive) = prop else {
      continue;
    };
    let Some(exp) = directive.exp.as_ref().map(expression_text) else {
      continue;
    };
    match directive.name.as_str() {
      "for" => {
        for name in v_for_alias_identifiers(&exp) {
          aliases.insert(name);
        }
      }
      "slot" | "slot-scope" | "scope" => {
        for name in slot_prop_alias_identifiers(&exp) {
          aliases.insert(name);
        }
      }
      _ => {}
    }
  }
  aliases
}

fn structural_for_aliases(for_node: &ForNode<'_>) -> BTreeSet<String> {
  let mut aliases = BTreeSet::new();
  for expression in
    [&for_node.value_alias, &for_node.key_alias, &for_node.object_index_alias].into_iter().flatten()
  {
    for name in slot_prop_alias_identifiers(&expression_text(expression)) {
      aliases.insert(name);
    }
  }
  aliases
}

fn push_expression_fact(
  source: &str,
  template_offset: usize,
  surface: &str,
  expression: &ExpressionNode<'_>,
  facts: &mut TemplateFacts,
  scopes: &TemplateAliasScopes,
) {
  let text = expression_text(expression);
  if text.trim().is_empty() {
    return;
  }
  let loc = expression.loc();
  let offset = template_offset.saturating_add(position_offset(loc.start.offset));
  let end = template_offset.saturating_add(position_offset(loc.end.offset));
  let length = end.saturating_sub(offset).max(text.len());
  let shadowed = scopes.shadowed();
  // `Some` even when empty: empty means resolved-no-reads, not “unknown”.
  let identifiers = Some(template_expression_identifiers_with_shadow(&text, surface, &shadowed));
  facts.expressions.push(TemplateExpressionFact {
    surface: surface.into(),
    expression: text,
    span: source_span(source, offset, length),
    identifiers,
  });
}

fn expression_text(expression: &ExpressionNode<'_>) -> String {
  match expression {
    ExpressionNode::Simple(expression) => expression.content.to_string(),
    ExpressionNode::Compound(expression) => expression.loc.source.to_string(),
  }
}

fn expression_is_static(expression: &ExpressionNode<'_>) -> bool {
  match expression {
    ExpressionNode::Simple(expression) => expression.is_static,
    ExpressionNode::Compound(_) => false,
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

  #[expect(clippy::panic, reason = "an unexpected parser error must fail the test")]
  fn facts_for_test(path: &Path, source: &str) -> SfcFacts {
    match analyze_sfc_with_facts(path, source) {
      Ok(analysis) => analysis.facts,
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

  #[test]
  fn joins_template_interpolation_and_directives_onto_script_bindings() {
    let source = r#"<script setup lang="ts">
import { ref } from 'vue'
const count = ref(0)
const label = ref('x')
const user = ref({ name: 'a' })
const items = ref([1])
const name = ref('shadow')
const item = ref('script-item')
</script>
<template>
  <div v-if="count > 0" :title="user.name">{{ label }}</div>
  <li v-for="item in items" :key="item">{{ item }}</li>
  <p>{{ item }}</p>
  <template #default="{ value }">
    <span>{{ value }} · {{ label }}</span>
  </template>
</template>"#;
    let facts = facts_for_test(Path::new("Join.vue"), source);
    let Some(graph) = facts.script.blocks.first().map(|block| &block.reactivity_graph) else {
      assert!(!facts.script.blocks.is_empty(), "script setup block must be analyzed");
      return;
    };

    assert!(
      facts.template.expressions.iter().any(|expression| expression.surface == "interpolation"),
      "Vize interpolations must be extracted as expression surfaces"
    );
    assert!(
      facts.template.expressions.iter().any(|expression| {
        expression.surface == "title"
          && expression
            .identifiers
            .as_ref()
            .is_some_and(|identifiers| identifiers.iter().any(|identifier| identifier == "user"))
          && expression
            .identifiers
            .as_ref()
            .is_some_and(|identifiers| !identifiers.iter().any(|identifier| identifier == "name"))
      }),
      "Oxc AST extraction must keep member objects and drop static property names"
    );
    assert!(
      graph.template_reads.iter().any(|read| read.binding == "count" && read.surface == "if"),
      "v-if expression must join onto the count binding"
    );
    assert!(
      graph
        .template_reads
        .iter()
        .any(|read| read.binding == "label" && read.surface == "interpolation"),
      "mustache interpolation must join onto the label binding"
    );
    assert!(
      graph.template_reads.iter().any(|read| read.binding == "user" && read.surface == "title"),
      "v-bind member expression must join the object binding"
    );
    assert!(
      !graph.template_reads.iter().any(|read| read.binding == "name"),
      "static property `name` must not join a same-named reactive binding"
    );
    assert!(
      graph.template_reads.iter().any(|read| read.binding == "items" && read.surface == "for"),
      "v-for iterable source must join onto items"
    );
    // Inside v-for, `item` is a template-local alias even when script also has `item`.
    assert!(
      !graph
        .template_reads
        .iter()
        .any(|read| read.binding == "item" && matches!(read.surface.as_str(), "key" | "for")),
      "v-for alias uses must not join the script item binding"
    );
    assert!(
      facts.template.expressions.iter().any(|expression| {
        expression.surface == "key"
          && expression.identifiers.as_ref().is_some_and(std::vec::Vec::is_empty)
      }),
      "`:key=\"item\"` free reads must resolve empty under the v-for alias scope"
    );
    let item_interpolation_joins = graph
      .template_reads
      .iter()
      .filter(|read| read.binding == "item" && read.surface == "interpolation")
      .count();
    assert_eq!(
      item_interpolation_joins, 1,
      "only the outer `{{{{ item }}}}` outside v-for should join the script item binding"
    );
    assert!(
      !graph.template_reads.iter().any(|read| read.binding == "value"),
      "slot prop aliases must not join script bindings"
    );
    assert!(
      facts.template.expressions.iter().any(|expression| {
        expression.surface == "interpolation"
          && expression.identifiers.as_ref().is_some_and(|identifiers| {
            identifiers.iter().any(|identifier| identifier == "label")
              && !identifiers.iter().any(|identifier| identifier == "value")
          })
      }),
      "slot body may read script bindings while dropping slot prop aliases"
    );
    // Expression spans must be absolute SFC offsets (not template-relative zeros).
    assert!(
      facts.template.expressions.iter().all(|expression| expression.span.offset > 0),
      "expression spans must use original SFC offsets via template.loc.start + expr.loc"
    );
  }
}
