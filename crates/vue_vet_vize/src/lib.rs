use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use thiserror::Error;
use vize_atelier_core::{
  Allocator, ElementNode, ExpressionNode, ForNode, PropNode, TemplateChildNode, parse,
};
use vize_atelier_sfc::{SfcDescriptor, SfcParseOptions, parse_sfc};
use vue_vet_core::{
  ScriptBlockFacts, ScriptFacts, ScriptKind, SfcFacts, SourceSpan, TemplateAttributeFact,
  TemplateDirectiveFact, TemplateElementFact, TemplateExpressionFact, TemplateFacts,
  content_digest,
};
use vue_vet_oxc::{
  AnalyzeScriptError, analyze_module_source, slot_prop_alias_identifiers,
  template_expression_identifiers_with_shadow, v_for_alias_identifiers,
};
use vue_vet_reactivity::ModuleSource;

#[derive(Debug, Error)]
pub enum AnalyzeError {
  #[error("Vize could not parse the SFC: {0}")]
  Parse(String),
  #[error("Vize could not parse the template: {0}")]
  Template(String),
  #[error(transparent)]
  Script(#[from] AnalyzeScriptError),
}

#[derive(Clone, Debug)]
pub struct AnalyzedSfc {
  pub facts: SfcFacts,
  /// Preferred script block for cross-module reactivity (`script setup` > `script`).
  pub module_source: Option<ModuleSource>,
  /// Ordinary `<script>` block when dual-script SFCs also have `<script setup>`.
  /// Id is `{path}#script` so both blocks re-trace with module seeds independently.
  pub ordinary_module_source: Option<ModuleSource>,
  /// Content + absolute span fingerprints for block-level reuse.
  pub revisions: SfcBlockRevisions,
}

/// Fingerprints for SFC blocks that can be reused across edits.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SfcBlockRevisions {
  pub template: Option<BlockFingerprint>,
  pub script: Option<BlockFingerprint>,
  pub script_setup: Option<BlockFingerprint>,
}

/// Content digest plus absolute location — location drift forces re-extract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockFingerprint {
  pub content_digest: String,
  pub start: usize,
  pub end: usize,
}

/// Analyze one Vue SFC and retain its dependency-neutral project facts.
///
/// # Errors
///
/// Returns the same deterministic parse and semantic errors as
/// [`analyze_sfc_facts`].
pub fn analyze_sfc_with_facts(path: &Path, source: &str) -> Result<AnalyzedSfc, AnalyzeError> {
  analyze_sfc_facts(path, source)
}

thread_local! {
  /// Installed for one SFC analysis from a shared [`vue_vet_core::SourceContext`].
  static SFC_LINE_INDEX: RefCell<Option<Arc<vue_vet_core::LineIndex>>> = const { RefCell::new(None) };
}

/// Extract SFC facts and module identity without running built-in rules.
///
/// Used by the CLI project pass so cross-file module graphs can seed bindings
/// before rule execution.
///
/// # Errors
///
/// Returns the same parse / template / script errors as [`analyze_sfc_with_facts`].
pub fn analyze_sfc_facts(path: &Path, source: &str) -> Result<AnalyzedSfc, AnalyzeError> {
  analyze_sfc_facts_reusing(path, source, None)
}

/// Analyze an SFC, reusing unchanged template/script blocks from `previous`.
///
/// Style-only edits that do not move other blocks reuse the prior analysis
/// entirely. Template-only or script-only edits rebuild just the dirty blocks.
///
/// # Errors
///
/// Returns the same parse / template / script errors as [`analyze_sfc_facts`].
pub fn analyze_sfc_facts_reusing(
  path: &Path,
  source: &str,
  previous: Option<&AnalyzedSfc>,
) -> Result<AnalyzedSfc, AnalyzeError> {
  // Index only — avoid `SourceContext::new(&str)` copying the SFC into `Arc<str>`.
  let line_index = Arc::new(vue_vet_core::LineIndex::new(source));
  SFC_LINE_INDEX.with(|slot| {
    *slot.borrow_mut() = Some(line_index);
  });
  let result = analyze_sfc_facts_inner(path, source, previous);
  SFC_LINE_INDEX.with(|slot| {
    *slot.borrow_mut() = None;
  });
  result
}

fn analyze_sfc_facts_inner(
  path: &Path,
  source: &str,
  previous: Option<&AnalyzedSfc>,
) -> Result<AnalyzedSfc, AnalyzeError> {
  let descriptor = parse_sfc(source, SfcParseOptions::default())
    .map_err(|error| AnalyzeError::Parse(error.message.into()))?;
  let revisions = revisions_from_descriptor(&descriptor);
  if let Some(previous) = previous
    && previous.revisions == revisions
  {
    // Style-only (or identical) edit: every tracked block fingerprint matches.
    let (module_source, ordinary_module_source) = dual_module_sources(path, source, &descriptor);
    let module_source = attach_reused_summaries(module_source, previous.module_source.as_ref());
    let ordinary_module_source =
      attach_reused_summaries(ordinary_module_source, previous.ordinary_module_source.as_ref());
    return Ok(AnalyzedSfc {
      facts: previous.facts.clone(),
      module_source,
      ordinary_module_source,
      revisions,
    });
  }

  let (mut module_source, mut ordinary_module_source) =
    dual_module_sources(path, source, &descriptor);
  let has_script_setup = descriptor.script_setup.is_some();
  let reuse_template = previous.is_some_and(|prev| prev.revisions.template == revisions.template);
  let reuse_script = previous.is_some_and(|prev| prev.revisions.script == revisions.script);
  let reuse_setup =
    previous.is_some_and(|prev| prev.revisions.script_setup == revisions.script_setup);

  // Reuse a prior template only when script blocks are also reused. JSX from
  // script is merged into `template`; a script rebuild must start from a clean
  // Vize-only surface so stale JSX facts are not retained.
  let mut template = if reuse_template && reuse_script && reuse_setup {
    previous.map(|prev| prev.facts.template.clone()).unwrap_or_default()
  } else if let Some(template) = descriptor.template {
    // Vize already supplies template content + absolute SFC content offsets.
    extract_template_facts(source, &template.content, template.loc.start)?
  } else {
    TemplateFacts::default()
  };

  let mut script = ScriptFacts::default();
  let mut script_rebuilt = false;
  if let Some(block) = descriptor.script {
    let lang = block.lang.as_deref().unwrap_or("js");
    let can_reuse_script = (reuse_template || !matches!(lang, "jsx" | "tsx")) && reuse_script;
    let (script_facts, summary) = if can_reuse_script
      && let Some(previous) = previous
      && let Some(facts) = previous_script_block(&previous.facts, ScriptKind::Script)
    {
      let summary = if has_script_setup {
        previous.ordinary_module_source.as_ref().and_then(ModuleSource::module_summary)
      } else {
        previous.module_source.as_ref().and_then(ModuleSource::module_summary)
      };
      (facts.clone(), summary)
    } else {
      script_rebuilt = true;
      // `block.loc.start/end` are absolute offsets into the original SFC source.
      let analysis =
        analyze_module_source(source, &block.content, block.loc.start, lang, ScriptKind::Script)?;
      merge_jsx_template_facts(&mut template, analysis.template_facts);
      (analysis.script_facts, Some(analysis.module_trace))
    };
    let target = if has_script_setup { &mut ordinary_module_source } else { &mut module_source };
    if let Some(module) = target.take() {
      *target = Some(match summary {
        Some(summary) => module.with_module_summary(summary),
        None => module,
      });
    }
    script.blocks.push(script_facts);
  }
  if let Some(block) = descriptor.script_setup {
    let lang = block.lang.as_deref().unwrap_or("js");
    let can_reuse_setup = (reuse_template || !matches!(lang, "jsx" | "tsx")) && reuse_setup;
    let (script_facts, summary) = if can_reuse_setup
      && let Some(previous) = previous
      && let Some(facts) = previous_script_block(&previous.facts, ScriptKind::Setup)
    {
      (facts.clone(), previous.module_source.as_ref().and_then(ModuleSource::module_summary))
    } else {
      script_rebuilt = true;
      let analysis =
        analyze_module_source(source, &block.content, block.loc.start, lang, ScriptKind::Setup)?;
      merge_jsx_template_facts(&mut template, analysis.template_facts);
      (analysis.script_facts, Some(analysis.module_trace))
    };
    if let Some(module) = module_source.take() {
      module_source = Some(match summary {
        Some(summary) => module.with_module_summary(summary),
        None => module,
      });
    }
    script.blocks.push(script_facts);
  }

  // Join when template or any script block was rebuilt; full reuse already joined.
  let needs_join = !reuse_template || script_rebuilt;
  if needs_join {
    let module_id = path.to_string_lossy().replace('\\', "/");
    for block in &mut script.blocks {
      let graph = Arc::make_mut(&mut block.reactivity_graph);
      graph.join_template_reads(&template);
      graph.set_module_id(module_id.clone());
    }
  }
  Ok(AnalyzedSfc {
    facts: SfcFacts { template, script },
    module_source,
    ordinary_module_source,
    revisions,
  })
}

fn revisions_from_descriptor(descriptor: &SfcDescriptor<'_>) -> SfcBlockRevisions {
  SfcBlockRevisions {
    template: descriptor
      .template
      .as_ref()
      .map(|block| fingerprint(block.content.as_ref(), block.loc.start, block.loc.end)),
    script: descriptor
      .script
      .as_ref()
      .map(|block| fingerprint(block.content.as_ref(), block.loc.start, block.loc.end)),
    script_setup: descriptor
      .script_setup
      .as_ref()
      .map(|block| fingerprint(block.content.as_ref(), block.loc.start, block.loc.end)),
  }
}

fn fingerprint(content: &str, start: usize, end: usize) -> BlockFingerprint {
  BlockFingerprint { content_digest: content_digest(content.as_bytes()), start, end }
}

fn previous_script_block(facts: &SfcFacts, kind: ScriptKind) -> Option<&ScriptBlockFacts> {
  facts.script.blocks.iter().find(|block| block.kind == kind)
}

fn attach_reused_summaries(
  fresh: Option<ModuleSource>,
  previous: Option<&ModuleSource>,
) -> Option<ModuleSource> {
  match (fresh, previous.and_then(ModuleSource::module_summary)) {
    (Some(module), Some(summary)) => Some(module.with_module_summary(summary)),
    (module, _) => module,
  }
}

/// Primary (`script setup` preferred) and optional ordinary dual companion.
fn dual_module_sources(
  path: &Path,
  sfc_source: &str,
  descriptor: &SfcDescriptor<'_>,
) -> (Option<ModuleSource>, Option<ModuleSource>) {
  let id = path.to_string_lossy().replace('\\', "/");
  let sfc_source = Arc::<str>::from(sfc_source);
  let setup = descriptor.script_setup.as_ref().map(|block| {
    ModuleSource::sfc_script(
      id.clone(),
      block.content.as_ref(),
      block.lang.as_deref().unwrap_or("js"),
      ScriptKind::Setup,
      block.loc.start,
      Arc::clone(&sfc_source),
    )
  });
  let ordinary = descriptor.script.as_ref().map(|block| {
    ModuleSource::sfc_script(
      // Dual companion id so both blocks re-trace with seeds independently.
      if setup.is_some() { format!("{id}#script") } else { id.clone() },
      block.content.as_ref(),
      block.lang.as_deref().unwrap_or("js"),
      ScriptKind::Script,
      block.loc.start,
      Arc::clone(&sfc_source),
    )
  });
  match (setup, ordinary) {
    (Some(setup), Some(ordinary)) => (Some(setup), Some(ordinary)),
    (Some(setup), None) => (Some(setup), None),
    (None, Some(ordinary)) => (Some(ordinary), None),
    (None, None) => (None, None),
  }
}

fn merge_jsx_template_facts(target: &mut TemplateFacts, jsx: TemplateFacts) {
  if jsx.elements.is_empty() && jsx.expressions.is_empty() {
    return;
  }
  target.elements.extend(jsx.elements);
  target.expressions.extend(jsx.expressions);
  target.elements.sort_by_key(|element| element.span.offset);
  target.expressions.sort_by_key(|expression| expression.span.offset);
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
  collect_children(source, template_offset, &root.children, &mut facts, &mut scopes, 0);
  // Elements follow document-order DFS; expressions are gathered from mixed
  // surfaces and need an explicit source-order pass.
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

/// Bottom-up subtree flags — each template node is visited once.
#[derive(Clone, Copy, Debug, Default)]
struct SubtreeSummary {
  accessible_content: bool,
  labelable_control: bool,
}

impl SubtreeSummary {
  const fn or(self, other: Self) -> Self {
    Self {
      accessible_content: self.accessible_content || other.accessible_content,
      labelable_control: self.labelable_control || other.labelable_control,
    }
  }
}

fn collect_children(
  source: &str,
  template_offset: usize,
  children: &[TemplateChildNode<'_>],
  facts: &mut TemplateFacts,
  scopes: &mut TemplateAliasScopes,
  label_depth: usize,
) -> SubtreeSummary {
  let mut summary = SubtreeSummary::default();
  for child in children {
    match child {
      TemplateChildNode::Element(element) => {
        summary =
          summary.or(collect_element(source, template_offset, element, facts, scopes, label_depth));
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
        summary.accessible_content = true;
      }
      TemplateChildNode::Text(text) if !text.content.trim().is_empty() => {
        summary.accessible_content = true;
      }
      TemplateChildNode::TextCall(_) | TemplateChildNode::CompoundExpression(_) => {
        summary.accessible_content = true;
      }
      TemplateChildNode::If(if_node) => {
        for branch in &if_node.branches {
          if let Some(condition) = &branch.condition {
            push_expression_fact(source, template_offset, "if", condition, facts, scopes);
          }
          summary = summary.or(collect_children(
            source,
            template_offset,
            &branch.children,
            facts,
            scopes,
            label_depth,
          ));
        }
      }
      TemplateChildNode::For(for_node) => {
        // Transform-time structural For nodes (raw parse keeps v-for on Element props).
        let aliases = structural_for_aliases(for_node);
        push_expression_fact(source, template_offset, "for", &for_node.source, facts, scopes);
        scopes.push(aliases.clone());
        summary = summary.or(collect_children(
          source,
          template_offset,
          &for_node.children,
          facts,
          scopes,
          label_depth,
        ));
        scopes.pop_if(&aliases);
      }
      TemplateChildNode::IfBranch(branch) => {
        if let Some(condition) = &branch.condition {
          push_expression_fact(source, template_offset, "if", condition, facts, scopes);
        }
        summary = summary.or(collect_children(
          source,
          template_offset,
          &branch.children,
          facts,
          scopes,
          label_depth,
        ));
      }
      TemplateChildNode::Text(_)
      | TemplateChildNode::Comment(_)
      | TemplateChildNode::Hoisted(_) => {}
    }
  }
  summary
}

fn collect_element(
  source: &str,
  template_offset: usize,
  element: &ElementNode<'_>,
  facts: &mut TemplateFacts,
  scopes: &mut TemplateAliasScopes,
  label_depth: usize,
) -> SubtreeSummary {
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

  let child_label_depth = if element.tag.as_str().eq_ignore_ascii_case("label") {
    label_depth.saturating_add(1)
  } else {
    label_depth
  };
  // Preserve parent-before-child element order for deterministic fixtures.
  let element_index = facts.elements.len();
  facts.elements.push(TemplateElementFact {
    tag: element.tag.to_string(),
    span: source_span(source, offset, end.saturating_sub(offset)),
    attributes,
    directives,
    has_children: !element.children.is_empty(),
    has_accessible_content: false,
    has_labelable_descendant: false,
    has_label_ancestor: label_depth > 0,
  });
  let child_summary =
    collect_children(source, template_offset, &element.children, facts, scopes, child_label_depth);
  let content_directive = element_has_content_directive(element);
  let has_accessible_content = content_directive || child_summary.accessible_content;
  if let Some(fact) = facts.elements.get_mut(element_index) {
    fact.has_accessible_content = has_accessible_content;
    fact.has_labelable_descendant = child_summary.labelable_control;
  }
  scopes.pop_if(&local_aliases);

  // Parents skip aria-hidden subtrees for accessible-content propagation.
  let propagate_accessible = if element_is_aria_hidden(element) {
    false
  } else {
    element_provides_alt_name(element) || content_directive || child_summary.accessible_content
  };
  SubtreeSummary {
    accessible_content: propagate_accessible,
    labelable_control: is_labelable_control_tag(element.tag.as_str())
      || child_summary.labelable_control,
  }
}

fn element_has_content_directive(element: &ElementNode<'_>) -> bool {
  element.props.iter().any(|prop| {
    matches!(prop, PropNode::Directive(directive) if matches!(directive.name.as_str(), "text" | "html"))
  })
}

fn element_is_aria_hidden(element: &ElementNode<'_>) -> bool {
  for prop in &element.props {
    match prop {
      PropNode::Attribute(attribute) if attribute.name.eq_ignore_ascii_case("aria-hidden") => {
        return attribute.value.as_ref().is_none_or(|value| {
          let content = value.content.trim();
          content.is_empty() || content.eq_ignore_ascii_case("true")
        });
      }
      PropNode::Directive(directive)
        if directive.name == "bind"
          && directive.arg.as_ref().is_some_and(|argument| {
            expression_text(argument).eq_ignore_ascii_case("aria-hidden")
          }) =>
      {
        // Bound visibility is unknown statically; treat as hidden so we do not
        // accept decorative icon trees that toggle aria-hidden at runtime.
        return true;
      }
      _ => {}
    }
  }
  false
}

fn is_labelable_control_tag(tag: &str) -> bool {
  matches!(
    tag.to_ascii_lowercase().as_str(),
    "input" | "textarea" | "select" | "button" | "meter" | "output" | "progress"
  )
}

fn element_provides_alt_name(element: &ElementNode<'_>) -> bool {
  if !element.tag.eq_ignore_ascii_case("img") && !element.tag.eq_ignore_ascii_case("area") {
    return false;
  }
  for prop in &element.props {
    match prop {
      PropNode::Attribute(attribute) if attribute.name.eq_ignore_ascii_case("alt") => {
        return attribute.value.as_ref().is_some_and(|value| !value.content.trim().is_empty());
      }
      PropNode::Directive(directive)
        if directive.name == "bind"
          && directive
            .arg
            .as_ref()
            .is_some_and(|argument| expression_text(argument).eq_ignore_ascii_case("alt"))
          && directive.exp.is_some() =>
      {
        return true;
      }
      _ => {}
    }
  }
  false
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
  SFC_LINE_INDEX.with(|slot| {
    slot.borrow().as_ref().map_or_else(
      || vue_vet_core::LineIndex::new(source).byte_to_line_column(offset),
      |index| index.as_ref().byte_to_line_column(offset),
    )
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use vue_vet_core::{Diagnostic, RuleEnvironment, RuleRegistry};
  use vue_vet_practice::practice_rules;
  use vue_vet_rules::builtin_rules;

  #[expect(clippy::panic, reason = "an unexpected parser error must fail the test")]
  fn analyze_for_test(path: &Path, source: &str) -> Vec<Diagnostic> {
    match analyze_sfc_with_facts(path, source) {
      Ok(analysis) => {
        let mut rules = builtin_rules();
        rules.extend(practice_rules());
        RuleRegistry::new(rules).run_with_environment(
          path,
          source,
          &analysis.facts.template,
          &analysis.facts.script,
          RuleEnvironment::default(),
        )
      }
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
  fn style_only_edit_reuses_template_and_script_blocks() {
    let path = Path::new("Reuse.vue");
    let base = concat!(
      "<script setup lang=\"ts\">\nimport { ref } from 'vue'\nconst count = ref(0)\n</script>\n",
      "<template><p>{{ count }}</p></template>\n",
      "<style>.a { color: red; }</style>\n",
    );
    let first = analysis_for_test(path, base);
    let style_only = concat!(
      "<script setup lang=\"ts\">\nimport { ref } from 'vue'\nconst count = ref(0)\n</script>\n",
      "<template><p>{{ count }}</p></template>\n",
      "<style>.a { color: blue; }</style>\n",
    );
    let second = analysis_reusing_for_test(path, style_only, &first);
    assert_eq!(first.revisions, second.revisions);
    assert_eq!(first.facts, second.facts);
    assert!(
      second.module_source.as_ref().and_then(ModuleSource::module_summary).is_some(),
      "reused analysis must keep module summary"
    );
  }

  #[test]
  fn template_only_edit_keeps_script_fingerprint() {
    let path = Path::new("Tpl.vue");
    let base = concat!(
      "<script setup lang=\"ts\">\nimport { ref } from 'vue'\nconst count = ref(0)\n</script>\n",
      "<template><p>{{ count }}</p></template>\n",
    );
    let first = analysis_for_test(path, base);
    let template_only = concat!(
      "<script setup lang=\"ts\">\nimport { ref } from 'vue'\nconst count = ref(0)\n</script>\n",
      "<template><span>{{ count }}</span></template>\n",
    );
    let second = analysis_reusing_for_test(path, template_only, &first);
    assert_eq!(first.revisions.script_setup, second.revisions.script_setup);
    assert_ne!(first.revisions.template, second.revisions.template);
    assert_ne!(first.facts.template, second.facts.template);
  }

  #[expect(clippy::panic, reason = "an unexpected parser error must fail the test")]
  fn analysis_reusing_for_test(path: &Path, source: &str, previous: &AnalyzedSfc) -> AnalyzedSfc {
    match analyze_sfc_facts_reusing(path, source, Some(previous)) {
      Ok(analysis) => analysis,
      Err(error) => panic!("analysis unexpectedly failed: {error}"),
    }
  }

  #[test]
  fn label_facts_mark_nested_labelable_controls() {
    let source = "<template>\n  <label>\n    <input type=\"text\">\n  </label>\n  <label>Name</label>\n</template>";
    let facts = facts_for_test(Path::new("Label.vue"), source);
    let labels = facts.template.elements.iter().filter(|el| el.tag == "label").collect::<Vec<_>>();
    assert_eq!(labels.len(), 2, "expected two label elements");
    if let [nested, text_only] = labels.as_slice() {
      assert!(nested.has_labelable_descendant, "nested input must set has_labelable_descendant");
      assert!(!text_only.has_labelable_descendant, "text-only label must stay clear");
    }
    let input = facts.template.elements.iter().find(|el| el.tag == "input");
    assert!(
      input.is_some_and(|el| el.has_label_ancestor),
      "nested input must set has_label_ancestor"
    );
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
  fn dual_script_emits_setup_and_ordinary_module_sources() {
    let source = r#"<script lang="ts">
import { ref } from 'vue'
export function useShared() {
  const shared = ref(0)
  return { shared }
}
</script>
<script setup lang="ts">
import { watchEffect } from 'vue'
import { useShared } from './unused'
const bag = useShared()
watchEffect(() => { void bag.shared.value })
</script>
<template><p>{{ bag.shared }}</p></template>
"#;
    let analysis = analysis_for_test(Path::new("Dual.vue"), source);
    assert!(
      analysis.module_source.as_ref().is_some_and(|module| {
        module.kind == ScriptKind::Setup && module.id == "Dual.vue" && module.source.contains("bag")
      }),
      "primary module source must be script setup"
    );
    assert!(
      analysis.ordinary_module_source.as_ref().is_some_and(|module| {
        module.kind == ScriptKind::Script
          && module.id == "Dual.vue#script"
          && module.source.contains("useShared")
      }),
      "dual ordinary companion must use #script id; got {:?}",
      analysis.ordinary_module_source.as_ref().map(|module| (
        &module.id,
        module.kind,
        &module.source
      ))
    );
    assert_eq!(
      analysis.facts.script.blocks.len(),
      2,
      "both script blocks must be analyzed for rules"
    );
  }

  #[test]
  #[expect(clippy::panic, reason = "test setup failures must fail the integration test")]
  fn same_file_composable_instance_tracks_and_joins_template_in_sfc() {
    use std::path::PathBuf;
    use vue_vet_project::{ProjectFile, build_project_graph};

    let sfc = r#"<script setup lang="ts">
import { ref, watchEffect } from 'vue'
function useSignal() {
  const signal = ref(0)
  return { signal }
}
const bag = useSignal()
watchEffect(() => { void bag.signal.value })
</script>
<template>
  <p>{{ bag.signal }}</p>
</template>
"#;
    let temp = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .join("../../target")
      .join(format!("vize-same-file-{}", std::process::id()));
    let _ignored = std::fs::remove_dir_all(&temp);
    if let Err(error) = std::fs::create_dir_all(&temp) {
      panic!("failed to create temp project: {error}");
    }
    if let Err(error) = std::fs::write(temp.join("LocalBag.vue"), sfc) {
      panic!("failed to write LocalBag.vue: {error}");
    }
    // Shipped Vize path: analyze_sfc_with_facts → per-block graph + template join.
    let analysis = analysis_for_test(Path::new("LocalBag.vue"), sfc);
    let vize_ok = analysis.facts.script.blocks.first().is_some_and(|block| {
      let graph = &block.reactivity_graph;
      graph.composable_instances.contains_key("bag")
        && graph.effects.iter().any(|effect| {
          effect.reads.iter().any(|read| {
            read.binding == "signal" && read.kind == vue_vet_core::ReactiveReadKind::Unconditional
          })
        })
        && graph
          .template_reads
          .iter()
          .any(|read| read.binding == "signal" && read.surface == "interpolation")
    });
    assert!(
      vize_ok,
      "Vize SFC path must track same-file bag.signal and join template; blocks={:?}",
      analysis
        .facts
        .script
        .blocks
        .iter()
        .map(|block| {
          (
            block.reactivity_graph.composable_instances.clone(),
            block
              .reactivity_graph
              .effects
              .iter()
              .flat_map(|effect| effect.reads.iter().map(|read| read.binding.clone()))
              .collect::<Vec<_>>(),
            block.reactivity_graph.template_reads.clone(),
          )
        })
        .collect::<Vec<_>>()
    );

    // Project re-trace path (single module, no cross-file seed).
    assert!(analysis.module_source.is_some(), "script setup must produce a project module source");
    if let Some(mut module) = analysis.module_source {
      module.id = "LocalBag.vue".into();
      let files = [ProjectFile {
        path: PathBuf::from("LocalBag.vue").into(),
        source_len: sfc.len(),
        facts: analysis.facts.into(),
        module_source: Some(module),
        ordinary_module_source: None,
      }];
      let project = build_project_graph(&temp, &files);
      let _ignored = std::fs::remove_dir_all(&temp);
      let page = project.module_reactivity.iter().find(|module| module.id == "LocalBag.vue");
      assert!(
        page.is_some_and(|module| {
          module.graph.composable_instances.contains_key("bag")
            && module
              .graph
              .effects
              .iter()
              .any(|effect| effect.reads.iter().any(|read| read.binding == "signal"))
            && module
              .graph
              .template_reads
              .iter()
              .any(|read| read.binding == "signal" && read.surface == "interpolation")
        }),
        "project re-trace must keep same-file instance + template join; got {:?}",
        page.map(|module| {
          (module.graph.composable_instances.clone(), module.graph.template_reads.clone())
        })
      );
    }
  }

  #[test]
  #[expect(clippy::panic, reason = "test setup failures must fail the integration test")]
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
  <p>{{ bag?.signal }}</p>
</template>
"#;
    let temp = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .join("../../target")
      .join(format!("vize-composable-{}", std::process::id()));
    let _ignored = std::fs::remove_dir_all(&temp);
    if let Err(error) = std::fs::create_dir_all(&temp) {
      panic!("failed to create temp project: {error}");
    }
    if let Err(error) = std::fs::write(temp.join("useSignal.ts"), producer) {
      panic!("failed to write useSignal.ts: {error}");
    }
    if let Err(error) = std::fs::write(temp.join("App.vue"), sfc) {
      panic!("failed to write App.vue: {error}");
    }
    let analysis = analysis_for_test(Path::new("App.vue"), sfc);
    let files = [
      ProjectFile {
        path: PathBuf::from("useSignal.ts").into(),
        source_len: producer.len(),
        facts: SfcFacts::default().into(),
        module_source: Some(ModuleSource::standalone(
          "useSignal.ts",
          producer,
          "ts",
          ScriptKind::Script,
        )),
        ordinary_module_source: None,
      },
      ProjectFile {
        path: PathBuf::from("App.vue").into(),
        source_len: sfc.len(),
        facts: analysis.facts.into(),
        module_source: analysis.module_source,
        ordinary_module_source: None,
      },
    ];
    let graph = build_project_graph(&temp, &files);
    let _ignored = std::fs::remove_dir_all(&temp);
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
    assert_eq!(module.span_source.as_ref(), source);
    let body = module
      .span_source
      .get(module.source_offset..module.source_offset.saturating_add(module.source.len()));
    assert_eq!(
      body,
      Some(module.source.as_ref()),
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
        path: PathBuf::from("App.vue").into(),
        source_len: app.len(),
        facts: analysis.facts.into(),
        module_source: Some({
          let mut module = module;
          module.id = "App.vue".into();
          module
        }),
        ordinary_module_source: None,
      },
      ProjectFile {
        path: PathBuf::from("composables/useField.ts").into(),
        source_len: producer.len(),
        facts: SfcFacts::default().into(),
        module_source: Some(ModuleSource::standalone(
          "composables/useField.ts",
          producer,
          "ts",
          ScriptKind::Script,
        )),
        ordinary_module_source: None,
      },
    ];
    let graph = build_project_graph(&root, &files);
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

  #[test]
  #[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "fixture IO and analysis failures must fail the integration test"
  )]
  fn prop_flow_fixture_joins_parent_binding_onto_child_props() {
    use std::path::PathBuf;
    use vue_vet_core::ReactiveDependencyKind;
    use vue_vet_project::{ProjectFile, build_project_graph};

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/projects/prop-flow");
    let mut files = Vec::new();
    for name in ["Parent.vue", "Child.vue", "MultiHop.vue"] {
      let source = std::fs::read_to_string(root.join(name)).expect("fixture");
      let analysis = analysis_for_test(Path::new(name), &source);
      let Some(module) = analysis.module_source.clone() else {
        panic!("module source missing for {name}");
      };
      files.push(ProjectFile {
        path: PathBuf::from(name).into(),
        source_len: source.len(),
        facts: analysis.facts.into(),
        module_source: Some({
          let mut module = module;
          module.id = name.into();
          module
        }),
        ordinary_module_source: None,
      });
    }
    let graph = build_project_graph(&root, &files);
    let child = graph.module_reactivity.iter().find(|module| module.id == "Child.vue");
    let props = child.map(|module| {
      module
        .graph
        .edges
        .iter()
        .filter(|edge| edge.kind == ReactiveDependencyKind::Prop)
        .map(|edge| (edge.property.as_deref(), edge.to.as_str()))
        .collect::<Vec<_>>()
    });
    assert!(
      props.as_ref().is_some_and(|props| {
        props.contains(&(Some("title"), "label"))
          && props.contains(&(Some("modelValue"), "msg"))
          && props.contains(&(Some("subtitle"), "bag"))
          && props.contains(&(Some("count"), "msg"))
      }),
      "prop-flow fixture must emit title/v-model/member/.value Prop edges; got {props:?}"
    );
    assert!(
      child.is_some_and(|module| {
        module.graph.edges.iter().any(|edge| {
          edge.kind == ReactiveDependencyKind::Prop
            && edge.property.as_deref() == Some("subtitle")
            && edge.to == "bag"
            && edge.to_id.as_deref().is_some_and(|id| id.starts_with("MultiHop.vue:bag@"))
        })
      }),
      "MultiHop.vue multi-hop chain must join root binding onto Child props"
    );
    assert!(
      child.is_some_and(|module| {
        module.graph.edges.iter().any(|edge| {
          edge.kind == ReactiveDependencyKind::Prop
            && edge.property.as_deref() == Some("title")
            && edge.to == "bag"
            && edge.to_id.as_deref().is_some_and(|id| id.starts_with("MultiHop.vue:bag@"))
        })
      }),
      "MultiHop.vue optional chain must join root binding onto Child props"
    );
  }
}
