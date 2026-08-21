//! Vize template walk → [`vue_vet_core::TemplateFacts`].
use std::collections::BTreeSet;

use vize_atelier_core::{
  Allocator, CompoundExpressionChild, ElementNode, ExpressionNode, ForNode, PropNode,
  TemplateChildNode, parse,
};
use vue_vet_core::{
  TemplateAttributeFact, TemplateDirectiveFact, TemplateElementFact, TemplateExpressionFact,
  TemplateFacts,
};
use vue_vet_oxc::{
  slot_prop_alias_identifiers, template_expression_identifiers_with_shadow, v_for_alias_identifiers,
};

use crate::AnalyzeError;
use crate::span::{position_offset, source_span};

pub fn extract_template_facts(
  source: &str,
  template: &str,
  template_offset: usize,
) -> Result<TemplateFacts, AnalyzeError> {
  let allocator = Allocator::default();
  let (root, errors) = parse(&allocator, template);
  if let Some(error) = errors.iter().find(|error| !error.is_recoverable()) {
    return Err(AnalyzeError::Template(error.to_string()));
  }

  let mut facts = TemplateFacts::default();
  let mut scopes = TemplateAliasScopes::default();
  collect_children(source, template_offset, &root.children, &mut facts, &mut scopes, 0, 0);
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
  name_depth: usize,
) -> SubtreeSummary {
  let mut summary = SubtreeSummary::default();
  for child in children {
    match child {
      TemplateChildNode::Element(element) => {
        summary = summary.or(collect_element(
          source,
          template_offset,
          element,
          facts,
          scopes,
          label_depth,
          name_depth,
        ));
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
            name_depth,
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
          name_depth,
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
          name_depth,
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
  name_depth: usize,
) -> SubtreeSummary {
  let offset = template_offset.saturating_add(position_offset(element.loc.span.start));
  let end = template_offset.saturating_add(position_offset(element.loc.span.end));
  let mut attributes = Vec::new();
  let mut directives = Vec::new();

  // v-for / v-slot aliases scope the element's own props and descendants.
  let local_aliases = element_local_aliases(element);
  scopes.push(local_aliases.clone());

  for prop in &element.props {
    match prop {
      PropNode::Attribute(attribute) => {
        let offset = template_offset.saturating_add(position_offset(attribute.name_loc.span.start));
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
        let offset = template_offset.saturating_add(position_offset(directive.loc.span.start));
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

  let child_label_depth = if element.tag.eq_ignore_ascii_case("label") {
    label_depth.saturating_add(1)
  } else {
    label_depth
  };
  // `CommonTooltip :content` / menu wrappers name their default-slot controls.
  let child_name_depth =
    if component_provides_slot_name(element) { name_depth.saturating_add(1) } else { name_depth };
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
    has_accessible_name_ancestor: name_depth > 0,
  });
  let child_summary = collect_children(
    source,
    template_offset,
    &element.children,
    facts,
    scopes,
    child_label_depth,
    child_name_depth,
  );
  let content_directive = element_has_content_directive(element);
  // Own content only: children / v-text / v-html. Do not treat the control itself
  // as content just because it is a component (`<NuxtLink />` stays empty).
  let has_accessible_content = content_directive || child_summary.accessible_content;
  if let Some(fact) = facts.elements.get_mut(element_index) {
    fact.has_accessible_content = has_accessible_content;
    fact.has_labelable_descendant = child_summary.labelable_control;
  }
  scopes.pop_if(&local_aliases);

  // Parents skip aria-hidden subtrees for accessible-content propagation.
  // Custom Vue components often render text/aria names we cannot see statically
  // (`AccountInfo`, `CommonDropdownItem :text`) — they still count for parents.
  let propagate_accessible = if element_is_aria_hidden(element) {
    false
  } else {
    element_provides_alt_name(element)
      || content_directive
      || child_summary.accessible_content
      || tag_is_vue_component(element.tag)
  };
  SubtreeSummary {
    accessible_content: propagate_accessible,
    labelable_control: is_labelable_control_tag(element.tag) || child_summary.labelable_control,
  }
}

/// Vue SFC convention: `PascalCase` or kebab-case multi-word tags are components.
///
/// Under-approx for a11y: a component child may still be decorative, but real-app
/// FPs from nested text/menu components dominate empty-icon false reports.
fn tag_is_vue_component(tag: &str) -> bool {
  if tag.is_empty() {
    return false;
  }
  // `RouterLink` / `NuxtLink` / `AccountInfo`
  if tag.chars().any(|ch| ch.is_ascii_uppercase()) {
    return true;
  }
  // `common-dropdown-item` / `nuxt-link` (when used as a child, not the control itself)
  tag.contains('-')
}

/// Component publishes a name-like prop for its default slot (`:content`, `title`, …).
fn component_provides_slot_name(element: &ElementNode<'_>) -> bool {
  if !tag_is_vue_component(element.tag) {
    return false;
  }
  for prop in &element.props {
    match prop {
      PropNode::Attribute(attribute) if is_slot_name_prop(attribute.name) => {
        if attribute.value.as_ref().is_some_and(|value| !value.content.trim().is_empty()) {
          return true;
        }
      }
      PropNode::Directive(directive)
        if directive.name == "bind"
          && directive
            .arg
            .as_ref()
            .is_some_and(|argument| is_slot_name_prop(expression_text(argument).as_str()))
          && directive.exp.as_ref().is_some_and(|exp| !expression_text(exp).trim().is_empty()) =>
      {
        return true;
      }
      _ => {}
    }
  }
  false
}

fn is_slot_name_prop(name: &str) -> bool {
  matches!(
    name.to_ascii_lowercase().as_str(),
    "content" | "title" | "label" | "text" | "aria-label" | "aria-labelledby"
  )
}

fn element_has_content_directive(element: &ElementNode<'_>) -> bool {
  element.props.iter().any(|prop| {
    matches!(prop, PropNode::Directive(directive) if matches!(directive.name, "text" | "html"))
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
    match directive.name {
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
  let offset = template_offset.saturating_add(position_offset(loc.span.start));
  let end = template_offset.saturating_add(position_offset(loc.span.end));
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
    ExpressionNode::Compound(expression) => compound_expression_text(expression),
  }
}

fn compound_expression_text(expression: &vize_atelier_core::CompoundExpressionNode<'_>) -> String {
  expression
    .children
    .iter()
    .map(|child| match child {
      CompoundExpressionChild::Simple(node) => node.content.to_string(),
      CompoundExpressionChild::Compound(node) => compound_expression_text(node),
      CompoundExpressionChild::Interpolation(node) => expression_text(&node.content),
      CompoundExpressionChild::Text(node) => node.content.to_string(),
      CompoundExpressionChild::String(text) => (*text).to_string(),
      CompoundExpressionChild::Symbol(_) => String::new(),
    })
    .collect()
}

fn expression_is_static(expression: &ExpressionNode<'_>) -> bool {
  match expression {
    ExpressionNode::Simple(expression) => expression.is_static,
    ExpressionNode::Compound(_) => false,
  }
}
