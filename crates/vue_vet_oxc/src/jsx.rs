//! Lower Oxc JSX/TSX into Vue Vet [`TemplateFacts`] (Vue JSX semantics, not React).

use oxc_ast::{
  AstKind,
  ast::{
    Expression, JSXAttribute, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXChild,
    JSXElement, JSXElementName, JSXExpression, JSXFragment, JSXMemberExpression,
  },
};
use oxc_span::Span;
use vue_vet_core::{
  TemplateAttributeFact, TemplateDirectiveFact, TemplateElementFact, TemplateExpressionFact,
  TemplateFacts,
};

use crate::source_span;

pub fn collect_jsx_template_facts(
  semantic: &oxc_semantic::Semantic<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) -> TemplateFacts {
  let mut facts = TemplateFacts::default();
  for node in semantic.nodes() {
    match node.kind() {
      AstKind::JSXElement(element) => {
        // Nested elements are also visited as their own nodes; collect each once.
        push_jsx_element(&mut facts, element, line_index, sfc_source, script_offset);
      }
      AstKind::JSXFragment(fragment) => {
        push_jsx_fragment_expressions(&mut facts, fragment, line_index, sfc_source, script_offset);
      }
      _ => {}
    }
  }
  facts.elements.sort_by_key(|element| element.span.offset);
  facts.expressions.sort_by_key(|expression| expression.span.offset);
  facts
}

fn push_jsx_element(
  facts: &mut TemplateFacts,
  element: &JSXElement<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) {
  let tag = jsx_element_name(&element.opening_element.name);
  let mut attributes = Vec::new();
  let mut directives = Vec::new();
  for item in &element.opening_element.attributes {
    let JSXAttributeItem::Attribute(attribute) = item else {
      continue;
    };
    classify_jsx_attribute(
      attribute,
      &mut attributes,
      &mut directives,
      facts,
      line_index,
      sfc_source,
      script_offset,
    );
  }

  let has_children = !element.children.is_empty();
  let mut has_accessible_content = directives.iter().any(|directive| {
    matches!(directive.name.as_str(), "html" | "text")
      || (directive.name == "bind"
        && directive
          .argument
          .as_deref()
          .is_some_and(|argument| argument.eq_ignore_ascii_case("alt")))
  });
  for child in &element.children {
    match child {
      JSXChild::Text(text) if !text.value.as_str().trim().is_empty() => {
        has_accessible_content = true;
      }
      JSXChild::ExpressionContainer(container) => {
        has_accessible_content = true;
        push_jsx_expression(
          facts,
          "jsx",
          &container.expression,
          container.span,
          line_index,
          sfc_source,
          script_offset,
        );
      }
      // Nested custom components often own the accessible name (Vue JSX).
      JSXChild::Element(child_element)
        if jsx_tag_is_vue_component(&jsx_element_name(&child_element.opening_element.name)) =>
      {
        has_accessible_content = true;
      }
      _ => {}
    }
  }

  facts.elements.push(TemplateElementFact {
    tag,
    span: source_span(line_index, sfc_source, script_offset, element.span),
    attributes,
    directives,
    has_children,
    has_accessible_content,
    has_labelable_descendant: false,
    has_label_ancestor: false,
    has_accessible_name_ancestor: false,
  });
}

fn push_jsx_fragment_expressions(
  facts: &mut TemplateFacts,
  fragment: &JSXFragment<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) {
  for child in &fragment.children {
    if let JSXChild::ExpressionContainer(container) = child {
      push_jsx_expression(
        facts,
        "jsx",
        &container.expression,
        container.span,
        line_index,
        sfc_source,
        script_offset,
      );
    }
  }
}

fn classify_jsx_attribute(
  attribute: &JSXAttribute<'_>,
  attributes: &mut Vec<TemplateAttributeFact>,
  directives: &mut Vec<TemplateDirectiveFact>,
  facts: &mut TemplateFacts,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) {
  let raw_name = jsx_attribute_name(&attribute.name);
  let span = source_span(line_index, sfc_source, script_offset, attribute.span);
  let (expression_text, expression_span, identifiers) =
    jsx_attribute_value(attribute, line_index, sfc_source, script_offset);

  // Vue JSX: `v-html` / `innerHTML` / `domPropsInnerHTML` → html directive for rule reuse.
  if raw_name == "v-html"
    || raw_name.eq_ignore_ascii_case("innerHTML")
    || raw_name.eq_ignore_ascii_case("domPropsInnerHTML")
  {
    if let Some((surface_span, expression, identifiers)) =
      expression_payload(expression_text.as_ref(), expression_span, identifiers, span)
    {
      facts.expressions.push(TemplateExpressionFact {
        surface: "html".into(),
        expression,
        span: surface_span,
        identifiers: Some(identifiers),
      });
    }
    directives.push(TemplateDirectiveFact {
      name: "html".into(),
      raw_name,
      argument: None,
      expression: expression_text,
      modifiers: Vec::new(),
      span,
    });
    return;
  }

  if let Some(rest) = raw_name.strip_prefix("v-") {
    let (name, argument, modifiers) = parse_vue_jsx_directive(rest);
    if let Some((surface_span, expression, identifiers)) =
      expression_payload(expression_text.as_ref(), expression_span, identifiers, span)
    {
      facts.expressions.push(TemplateExpressionFact {
        surface: name.clone(),
        expression,
        span: surface_span,
        identifiers: Some(identifiers),
      });
    }
    directives.push(TemplateDirectiveFact {
      name,
      raw_name,
      argument,
      expression: expression_text,
      modifiers,
      span,
    });
    return;
  }

  if let Some(event) = raw_name
    .strip_prefix("on")
    .filter(|rest| rest.as_bytes().first().is_some_and(u8::is_ascii_uppercase))
  {
    let event = event.to_ascii_lowercase();
    if let Some((surface_span, expression, identifiers)) =
      expression_payload(expression_text.as_ref(), expression_span, identifiers, span)
    {
      facts.expressions.push(TemplateExpressionFact {
        surface: "on".into(),
        expression,
        span: surface_span,
        identifiers: Some(identifiers),
      });
    }
    directives.push(TemplateDirectiveFact {
      name: "on".into(),
      raw_name,
      argument: Some(event),
      expression: expression_text,
      modifiers: Vec::new(),
      span,
    });
    return;
  }

  // Dynamic JSX prop `{expr}` without `v-` → bind-like surface for template join.
  if expression_text.is_some()
    && !matches!(attribute.value, Some(JSXAttributeValue::StringLiteral(_)))
  {
    if let Some((surface_span, expression, identifiers)) =
      expression_payload(expression_text.as_ref(), expression_span, identifiers, span)
    {
      facts.expressions.push(TemplateExpressionFact {
        surface: "bind".into(),
        expression,
        span: surface_span,
        identifiers: Some(identifiers),
      });
    }
    directives.push(TemplateDirectiveFact {
      name: "bind".into(),
      raw_name: format!(":{raw_name}"),
      argument: Some(raw_name),
      expression: expression_text,
      modifiers: Vec::new(),
      span,
    });
    return;
  }

  attributes.push(TemplateAttributeFact { name: raw_name, value: expression_text, span });
}

fn expression_payload(
  expression_text: Option<&String>,
  expression_span: Option<vue_vet_core::SourceSpan>,
  identifiers: Vec<String>,
  fallback_span: vue_vet_core::SourceSpan,
) -> Option<(vue_vet_core::SourceSpan, String, Vec<String>)> {
  let expression = expression_text.cloned()?;
  Some((expression_span.unwrap_or(fallback_span), expression, identifiers))
}

fn parse_vue_jsx_directive(rest: &str) -> (String, Option<String>, Vec<String>) {
  // `model:arg_modifier` / `model_modifier` / `show`
  let (head, modifiers) = match rest.split_once('_') {
    Some((head, modifier_list)) => {
      (head, modifier_list.split('_').filter(|part| !part.is_empty()).map(str::to_owned).collect())
    }
    None => (rest, Vec::new()),
  };
  match head.split_once(':') {
    Some((name, argument)) => (name.to_owned(), Some(argument.to_owned()), modifiers),
    None => (head.to_owned(), None, modifiers),
  }
}

fn jsx_attribute_value(
  attribute: &JSXAttribute<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) -> (Option<String>, Option<vue_vet_core::SourceSpan>, Vec<String>) {
  let Some(value) = &attribute.value else {
    return (None, None, Vec::new());
  };
  match value {
    JSXAttributeValue::StringLiteral(literal) => {
      (Some(literal.value.to_string()), None, Vec::new())
    }
    JSXAttributeValue::ExpressionContainer(container) => {
      let span = source_span(line_index, sfc_source, script_offset, container.span);
      let text = slice_span(sfc_source, script_offset, container.span);
      let identifiers = expression_identifiers(&container.expression);
      (Some(text), Some(span), identifiers)
    }
    JSXAttributeValue::Element(_) | JSXAttributeValue::Fragment(_) => {
      (Some(String::new()), None, Vec::new())
    }
  }
}

fn push_jsx_expression(
  facts: &mut TemplateFacts,
  surface: &str,
  expression: &JSXExpression<'_>,
  span: Span,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) {
  if matches!(expression, JSXExpression::EmptyExpression(_)) {
    return;
  }
  let text = slice_span(sfc_source, script_offset, span);
  if text.trim().is_empty() {
    return;
  }
  facts.expressions.push(TemplateExpressionFact {
    surface: surface.into(),
    expression: text,
    span: source_span(line_index, sfc_source, script_offset, span),
    identifiers: Some(expression_identifiers(expression)),
  });
}

fn expression_identifiers(expression: &JSXExpression<'_>) -> Vec<String> {
  let mut names = Vec::new();
  // `JSXExpression` inherits `Expression` variants; reuse Expression matching via ref cast
  // only for the shared arms we care about.
  match expression {
    JSXExpression::Identifier(identifier) => names.push(identifier.name.to_string()),
    JSXExpression::StaticMemberExpression(member) => {
      collect_identifiers_expr(&member.object, &mut names);
    }
    JSXExpression::ComputedMemberExpression(member) => {
      collect_identifiers_expr(&member.object, &mut names);
      collect_identifiers_expr(&member.expression, &mut names);
    }
    JSXExpression::CallExpression(call) => {
      collect_identifiers_expr(&call.callee, &mut names);
      for argument in &call.arguments {
        if let Some(argument) = argument.as_expression() {
          collect_identifiers_expr(argument, &mut names);
        }
      }
    }
    JSXExpression::BinaryExpression(binary) => {
      collect_identifiers_expr(&binary.left, &mut names);
      collect_identifiers_expr(&binary.right, &mut names);
    }
    JSXExpression::LogicalExpression(logical) => {
      collect_identifiers_expr(&logical.left, &mut names);
      collect_identifiers_expr(&logical.right, &mut names);
    }
    JSXExpression::ConditionalExpression(conditional) => {
      collect_identifiers_expr(&conditional.test, &mut names);
      collect_identifiers_expr(&conditional.consequent, &mut names);
      collect_identifiers_expr(&conditional.alternate, &mut names);
    }
    JSXExpression::ParenthesizedExpression(paren) => {
      collect_identifiers_expr(&paren.expression, &mut names);
    }
    JSXExpression::TSAsExpression(as_expr) => {
      collect_identifiers_expr(&as_expr.expression, &mut names);
    }
    JSXExpression::TSSatisfiesExpression(satisfies) => {
      collect_identifiers_expr(&satisfies.expression, &mut names);
    }
    JSXExpression::TemplateLiteral(template) => {
      for inner in &template.expressions {
        collect_identifiers_expr(inner, &mut names);
      }
    }
    _ => {}
  }
  names.sort_unstable();
  names.dedup();
  names
}

fn collect_identifiers_expr(expression: &Expression<'_>, names: &mut Vec<String>) {
  match expression {
    Expression::Identifier(identifier) => names.push(identifier.name.to_string()),
    Expression::StaticMemberExpression(member) => collect_identifiers_expr(&member.object, names),
    Expression::ComputedMemberExpression(member) => {
      collect_identifiers_expr(&member.object, names);
      collect_identifiers_expr(&member.expression, names);
    }
    Expression::CallExpression(call) => {
      collect_identifiers_expr(&call.callee, names);
      for argument in &call.arguments {
        if let Some(argument) = argument.as_expression() {
          collect_identifiers_expr(argument, names);
        }
      }
    }
    Expression::BinaryExpression(binary) => {
      collect_identifiers_expr(&binary.left, names);
      collect_identifiers_expr(&binary.right, names);
    }
    Expression::LogicalExpression(logical) => {
      collect_identifiers_expr(&logical.left, names);
      collect_identifiers_expr(&logical.right, names);
    }
    Expression::ConditionalExpression(conditional) => {
      collect_identifiers_expr(&conditional.test, names);
      collect_identifiers_expr(&conditional.consequent, names);
      collect_identifiers_expr(&conditional.alternate, names);
    }
    Expression::ParenthesizedExpression(paren) => {
      collect_identifiers_expr(&paren.expression, names);
    }
    Expression::TSAsExpression(as_expr) => collect_identifiers_expr(&as_expr.expression, names),
    Expression::TSSatisfiesExpression(satisfies) => {
      collect_identifiers_expr(&satisfies.expression, names);
    }
    Expression::TemplateLiteral(template) => {
      for inner in &template.expressions {
        collect_identifiers_expr(inner, names);
      }
    }
    _ => {}
  }
}

fn jsx_element_name(name: &JSXElementName<'_>) -> String {
  match name {
    JSXElementName::Identifier(identifier) => identifier.name.to_string(),
    JSXElementName::IdentifierReference(identifier) => identifier.name.to_string(),
    JSXElementName::NamespacedName(namespaced) => {
      format!("{}:{}", namespaced.namespace.name, namespaced.name.name)
    }
    JSXElementName::MemberExpression(member) => jsx_member_name(member),
    JSXElementName::ThisExpression(_) => "this".into(),
  }
}

/// `PascalCase` / kebab-case multi-word tags are Vue components (see vize).
fn jsx_tag_is_vue_component(tag: &str) -> bool {
  if tag.is_empty() {
    return false;
  }
  tag.chars().any(|ch| ch.is_ascii_uppercase()) || tag.contains('-')
}

fn jsx_member_name(member: &JSXMemberExpression<'_>) -> String {
  let property = member.property.name.as_str();
  match &member.object {
    oxc_ast::ast::JSXMemberExpressionObject::IdentifierReference(identifier) => {
      format!("{}.{}", identifier.name, property)
    }
    oxc_ast::ast::JSXMemberExpressionObject::MemberExpression(inner) => {
      format!("{}.{}", jsx_member_name(inner), property)
    }
    oxc_ast::ast::JSXMemberExpressionObject::ThisExpression(_) => format!("this.{property}"),
  }
}

fn jsx_attribute_name(name: &JSXAttributeName<'_>) -> String {
  match name {
    JSXAttributeName::Identifier(identifier) => identifier.name.to_string(),
    JSXAttributeName::NamespacedName(namespaced) => {
      format!("{}:{}", namespaced.namespace.name, namespaced.name.name)
    }
  }
}

fn slice_span(sfc_source: &str, script_offset: usize, span: Span) -> String {
  let start = script_offset.saturating_add(usize::try_from(span.start).unwrap_or(usize::MAX));
  let end = script_offset.saturating_add(usize::try_from(span.end).unwrap_or(usize::MAX));
  sfc_source.get(start..end.min(sfc_source.len())).unwrap_or("").to_owned()
}
