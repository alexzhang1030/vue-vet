use std::collections::BTreeSet;
use std::path::Path;

use thiserror::Error;
use vize_atelier_core::{
  Allocator, ElementNode, ExpressionNode, ForNode, PropNode, TemplateChildNode, parse,
};
use vize_atelier_sfc::{SfcDescriptor, SfcParseOptions, parse_sfc};
use vue_vet_core::{
  Diagnostic, RuleEnvironment, ScriptFacts, ScriptKind, SfcFacts, SourceSpan,
  TemplateAttributeFact, TemplateDirectiveFact, TemplateElementFact, TemplateExpressionFact,
  TemplateFacts,
};
use vue_vet_oxc::{
  AnalyzeScriptError, analyze_script, slot_prop_alias_identifiers,
  template_expression_identifiers_with_shadow, v_for_alias_identifiers,
};
use vue_vet_reactivity::ModuleSource;
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
  /// Preferred script block for cross-module reactivity (`script setup` > `script`).
  pub module_source: Option<ModuleSource>,
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
  let mut analysis = analyze_sfc_facts_with_environment(path, source)?;
  analysis.diagnostics = builtin_registry().run_with_environment(
    path,
    source,
    &analysis.facts.template,
    &analysis.facts.script,
    environment,
  );
  Ok(analysis)
}

/// Extract SFC facts and module identity without running built-in rules.
///
/// Used by the CLI project pass so cross-file module graphs can seed bindings
/// before rule execution.
///
/// # Errors
///
/// Returns the same parse / template / script errors as [`analyze_sfc`].
pub fn analyze_sfc_facts_with_environment(
  path: &Path,
  source: &str,
) -> Result<AnalyzedSfc, AnalyzeError> {
  let descriptor = parse_sfc(source, SfcParseOptions::default())
    .map_err(|error| AnalyzeError::Parse(error.message.into()))?;
  let module_source = preferred_module_source(path, source, &descriptor);
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
  Ok(AnalyzedSfc { diagnostics: Vec::new(), facts: SfcFacts { template, script }, module_source })
}

fn preferred_module_source(
  path: &Path,
  sfc_source: &str,
  descriptor: &SfcDescriptor<'_>,
) -> Option<ModuleSource> {
  let id = path.to_string_lossy().replace('\\', "/");
  if let Some(block) = &descriptor.script_setup {
    return Some(ModuleSource::sfc_script(
      id,
      block.content.as_ref(),
      block.lang.as_deref().unwrap_or("js"),
      ScriptKind::Setup,
      block.loc.start,
      sfc_source,
    ));
  }
  if let Some(block) = &descriptor.script {
    return Some(ModuleSource::sfc_script(
      id,
      block.content.as_ref(),
      block.lang.as_deref().unwrap_or("js"),
      ScriptKind::Script,
      block.loc.start,
      sfc_source,
    ));
  }
  None
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

  #[expect(clippy::panic, reason = "an unexpected parser error must fail the test")]
  fn analysis_for_test(path: &Path, source: &str) -> AnalyzedSfc {
    match analyze_sfc_with_facts(path, source) {
      Ok(analysis) => analysis,
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

  #[test]
  fn define_props_computed_and_template_join_end_to_end() {
    let source = r#"<script setup lang="ts">
import { computed } from 'vue'
const props = defineProps<{ count: number; label: string }>()
const doubled = computed(() => props.count * 2)
</script>
<template>
  <p v-if="props.count > 0">{{ props.label }} · {{ doubled }}</p>
</template>"#;
    let facts = facts_for_test(Path::new("PropsCard.vue"), source);
    let Some(graph) = facts.script.blocks.first().map(|block| &block.reactivity_graph) else {
      assert!(!facts.script.blocks.is_empty(), "script setup must be analyzed");
      return;
    };
    assert!(
      graph.bindings.iter().any(|binding| {
        binding.name == "props" && binding.kind == vue_vet_core::ReactiveBindingKind::Reactive
      }),
      "defineProps must seed a reactive props binding"
    );
    assert!(
      graph.scopes.iter().any(|scope| {
        scope.kind == vue_vet_core::TrackingScopeKind::Computed
          && scope
            .reads
            .iter()
            .any(|read| read.binding == "props" && read.property.as_deref() == Some("count"))
      }),
      "computed must track props.count"
    );
    assert!(
      graph.template_reads.iter().any(|read| read.binding == "props" && read.surface == "if"),
      "template v-if must join props"
    );
    assert!(
      graph
        .template_reads
        .iter()
        .any(|read| read.binding == "props" && read.surface == "interpolation"),
      "template must join props member reads onto the props binding"
    );
    assert!(
      graph
        .template_reads
        .iter()
        .any(|read| read.binding == "doubled" && read.surface == "interpolation"),
      "template must join the computed binding"
    );
    assert!(
      graph.edges.iter().any(|edge| {
        edge.kind == vue_vet_core::ReactiveDependencyKind::Template && edge.to == "props"
      }),
      "template edges must target props"
    );
  }

  #[test]
  fn composable_instance_member_joins_template_after_module_seeds() {
    use std::path::PathBuf;
    use vue_vet_project::{ProjectFile, build_project_graph};
    use vue_vet_reactivity::ModuleSource;

    let producer = "import { ref } from 'vue'; export function useSignal() { const signal = ref(0); return { signal }; }";
    let sfc = r#"<script setup lang="ts">
import { watchEffect } from 'vue'
import { useSignal } from './useSignal'
const bag = useSignal()
watchEffect(() => { void bag.signal.value })
</script>
<template>
  <p>{{ bag.signal }}</p>
</template>
"#;
    let analysis = analysis_for_test(Path::new("App.vue"), sfc);
    let files = [
      ProjectFile {
        path: PathBuf::from("useSignal.ts"),
        source_len: producer.len(),
        facts: SfcFacts::default(),
        module_source: Some(ModuleSource::standalone(
          "useSignal.ts",
          producer,
          "ts",
          ScriptKind::Script,
        )),
      },
      ProjectFile {
        path: PathBuf::from("App.vue"),
        source_len: sfc.len(),
        facts: analysis.facts,
        module_source: analysis.module_source,
      },
    ];
    let graph = build_project_graph(&files);
    assert!(
      graph.reactivity_error.is_none(),
      "module tracing must succeed: {:?}",
      graph.reactivity_error
    );
    let app = graph.module_reactivity.iter().find(|module| module.id == "App.vue");
    assert!(
      app.is_some_and(|module| {
        module.graph.composable_instances.contains_key("bag")
          && module.graph.effects.iter().any(|effect| {
            effect.reads.iter().any(|read| {
              read.binding == "signal" && read.kind == vue_vet_core::ReactiveReadKind::Unconditional
            })
          })
          && module
            .graph
            .template_reads
            .iter()
            .any(|read| read.binding == "signal" && read.surface == "interpolation")
          && module.graph.edges.iter().any(|edge| {
            edge.kind == vue_vet_core::ReactiveDependencyKind::Template && edge.to == "signal"
          })
      }),
      "seeded bag.signal must track in effects and join template {{ bag.signal }}; got {:?}",
      app.map(|module| {
        (
          module.graph.composable_instances.clone(),
          module
            .graph
            .effects
            .iter()
            .flat_map(|effect| effect.reads.iter().map(|read| read.binding.clone()))
            .collect::<Vec<_>>(),
          module.graph.template_reads.clone(),
        )
      })
    );
  }

  #[test]
  #[expect(clippy::panic, reason = "missing module source must fail the extraction test")]
  fn exposes_script_setup_module_source_with_sfc_span_mapping() {
    let source = r#"<script setup lang="ts">
import { ref } from 'vue'
const count = ref(0)
</script>
<template>
  <div>{{ count }}</div>
</template>"#;
    let analysis = analysis_for_test(Path::new("pages/Counter.vue"), source);
    let Some(module) = analysis.module_source.as_ref() else {
      panic!("script setup must produce a project module source");
    };
    assert_eq!(module.id, "pages/Counter.vue");
    assert_eq!(module.kind, ScriptKind::Setup);
    assert_eq!(module.language, "ts");
    assert!(module.source.contains("const count = ref(0)"));
    assert!(module.source_offset > 0, "script body offset must be absolute in the SFC");
    assert_eq!(module.span_source, source);
    let body = module
      .span_source
      .get(module.source_offset..module.source_offset.saturating_add(module.source.len()));
    assert_eq!(
      body,
      Some(module.source.as_str()),
      "extracted script body must be an exact slice of the original SFC at source_offset"
    );
  }

  #[test]
  #[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "fixture IO and analysis failures must fail the integration test"
  )]
  fn project_graph_uses_vize_module_source_for_seeds() {
    use std::path::PathBuf;
    use vue_vet_project::{ProjectFile, build_project_graph};
    use vue_vet_reactivity::ModuleSource;

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/projects/module-seeds");
    let app = std::fs::read_to_string(root.join("App.vue")).expect("app fixture");
    let producer =
      std::fs::read_to_string(root.join("composables/useField.ts")).expect("producer fixture");
    let analysis = analysis_for_test(Path::new("App.vue"), &app);
    let Some(module) = analysis.module_source.clone() else {
      panic!("module source missing");
    };
    let files = [
      ProjectFile {
        path: PathBuf::from("App.vue"),
        source_len: app.len(),
        facts: analysis.facts,
        module_source: Some({
          let mut module = module;
          module.id = "App.vue".into();
          module
        }),
      },
      ProjectFile {
        path: PathBuf::from("composables/useField.ts"),
        source_len: producer.len(),
        facts: SfcFacts::default(),
        module_source: Some(ModuleSource::standalone(
          "composables/useField.ts",
          producer,
          "ts",
          ScriptKind::Script,
        )),
      },
    ];
    let graph = build_project_graph(&files);
    let app_mod = graph.module_reactivity.iter().find(|module| module.id == "App.vue");
    assert!(
      app_mod.is_some_and(|module| {
        module.graph.effects.iter().any(|effect| {
          effect.reads.iter().any(|read| {
            read.binding == "title" && read.kind == vue_vet_core::ReactiveReadKind::AfterAwait
          })
        })
      }),
      "Vize module_source through project graph must seed after-await title reads"
    );
  }
}
