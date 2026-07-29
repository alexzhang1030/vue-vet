//! Recognize Vue component render function bodies (JSX / render-fn surfaces).

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::{
  AstKind,
  ast::{
    Argument, BindingPattern, Expression, Function, FunctionBody, ObjectPropertyKind, PropertyKey,
    Statement,
  },
};
use oxc_semantic::NodeId;
use oxc_span::Span;

/// One recognized render function body.
pub(super) struct RenderBody<'a> {
  pub scope_id: NodeId,
  pub body: Option<&'a FunctionBody<'a>>,
  pub span: Span,
}

/// Collect render bodies via structure-first shapes + same-file component factories.
pub(super) fn collect_render_bodies<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> Vec<RenderBody<'a>> {
  let factories = component_factories(semantic, imported_bindings);
  let mut bodies = Vec::new();
  let mut seen = BTreeSet::new();

  for node in semantic.nodes() {
    match node.kind() {
      AstKind::ObjectExpression(object) => {
        collect_options_object(object, &mut bodies, &mut seen);
      }
      AstKind::CallExpression(call) => {
        let Some(callee) = call.callee.get_identifier_reference() else {
          continue;
        };
        if !factories.contains(callee.name.as_str()) {
          continue;
        }
        let Some(first) = call.arguments.first().and_then(Argument::as_expression) else {
          continue;
        };
        match first {
          Expression::ObjectExpression(object) => {
            collect_options_object(object, &mut bodies, &mut seen);
          }
          Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
            if let Some(inner) = setup_returned_render(first) {
              push_unique(&mut bodies, &mut seen, inner);
            }
          }
          _ => {}
        }
      }
      AstKind::Function(function) => {
        if !is_exported_node(semantic, node.id()) {
          continue;
        }
        if let Some(body) = function_declaration_body(function) {
          push_unique(&mut bodies, &mut seen, body);
        }
      }
      AstKind::VariableDeclarator(declarator) => {
        if !is_exported_declarator(semantic, node.id()) {
          continue;
        }
        if let Some(init) = &declarator.init
          && let Some(body) = functional_component_body(init)
        {
          push_unique(&mut bodies, &mut seen, body);
        }
      }
      _ => {}
    }
  }

  bodies
}

fn collect_options_object<'a>(
  object: &'a oxc_ast::ast::ObjectExpression<'a>,
  bodies: &mut Vec<RenderBody<'a>>,
  seen: &mut BTreeSet<u32>,
) {
  for property in &object.properties {
    let ObjectPropertyKind::ObjectProperty(property) = property else {
      continue;
    };
    let Some(name) = property_key_name(&property.key) else {
      continue;
    };
    if name == "render"
      && let Some(body) = function_like_body(&property.value)
    {
      push_unique(bodies, seen, body);
    }
    if name == "setup"
      && let Some(inner) = setup_returned_render(&property.value)
    {
      push_unique(bodies, seen, inner);
    }
  }
}

fn push_unique<'a>(
  bodies: &mut Vec<RenderBody<'a>>,
  seen: &mut BTreeSet<u32>,
  body: RenderBody<'a>,
) {
  if !seen.insert(body.span.start) {
    return;
  }
  bodies.push(body);
}

fn component_factories(
  semantic: &oxc_semantic::Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> BTreeSet<String> {
  let mut factories = BTreeSet::new();
  for (local, (source, imported)) in imported_bindings {
    if imported == "defineComponent" && is_vue_runtime_source(source) {
      factories.insert(local.clone());
    }
  }

  let mut grew = true;
  while grew {
    grew = false;
    for node in semantic.nodes() {
      let AstKind::VariableDeclarator(declarator) = node.kind() else {
        continue;
      };
      let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
        continue;
      };
      let name = binding.name.as_str();
      if factories.contains(name) {
        continue;
      }
      let Some(init) = &declarator.init else {
        continue;
      };
      if is_identity_factory_forward(init, &factories) {
        factories.insert(name.to_owned());
        grew = true;
      }
    }
  }
  factories
}

fn is_vue_runtime_source(source: &str) -> bool {
  matches!(source, "vue" | "@vue/runtime-dom" | "@vue/runtime-core")
}

fn is_identity_factory_forward(expression: &Expression<'_>, factories: &BTreeSet<String>) -> bool {
  match expression {
    Expression::ArrowFunctionExpression(arrow) => {
      let Some(param) = arrow.params.items.first() else {
        return false;
      };
      let BindingPattern::BindingIdentifier(param) = &param.pattern else {
        return false;
      };
      let param_name = param.name.as_str();
      if arrow.expression {
        let Some(Statement::ExpressionStatement(statement)) = arrow.body.statements.first() else {
          return false;
        };
        return is_factory_call_forwarding(&statement.expression, param_name, factories);
      }
      let Some(Statement::ReturnStatement(ret)) = arrow.body.statements.first() else {
        return false;
      };
      let Some(argument) = &ret.argument else {
        return false;
      };
      is_factory_call_forwarding(argument, param_name, factories)
    }
    Expression::FunctionExpression(function) => {
      let Some(param) = function.params.items.first() else {
        return false;
      };
      let BindingPattern::BindingIdentifier(param) = &param.pattern else {
        return false;
      };
      let param_name = param.name.as_str();
      let Some(body) = function.body.as_ref() else {
        return false;
      };
      let Some(Statement::ReturnStatement(ret)) = body.statements.first() else {
        return false;
      };
      let Some(argument) = &ret.argument else {
        return false;
      };
      is_factory_call_forwarding(argument, param_name, factories)
    }
    _ => false,
  }
}

fn is_factory_call_forwarding(
  expression: &Expression<'_>,
  param_name: &str,
  factories: &BTreeSet<String>,
) -> bool {
  let Expression::CallExpression(call) = expression else {
    return false;
  };
  let Some(callee) = call.callee.get_identifier_reference() else {
    return false;
  };
  if !factories.contains(callee.name.as_str()) || call.arguments.len() != 1 {
    return false;
  }
  call
    .arguments
    .first()
    .and_then(Argument::as_expression)
    .and_then(Expression::get_identifier_reference)
    .is_some_and(|id| id.name.as_str() == param_name)
}

fn setup_returned_render<'a>(expression: &'a Expression<'a>) -> Option<RenderBody<'a>> {
  match expression {
    Expression::ArrowFunctionExpression(arrow) => {
      if arrow.expression {
        let Statement::ExpressionStatement(statement) = arrow.body.statements.first()? else {
          return None;
        };
        return function_like_body(&statement.expression);
      }
      for statement in &arrow.body.statements {
        if let Statement::ReturnStatement(ret) = statement
          && let Some(argument) = &ret.argument
        {
          return function_like_body(argument);
        }
      }
      None
    }
    Expression::FunctionExpression(function) => {
      let body = function.body.as_ref()?;
      for statement in &body.statements {
        if let Statement::ReturnStatement(ret) = statement
          && let Some(argument) = &ret.argument
        {
          return function_like_body(argument);
        }
      }
      None
    }
    _ => None,
  }
}

fn function_like_body<'a>(expression: &'a Expression<'a>) -> Option<RenderBody<'a>> {
  match expression {
    Expression::ArrowFunctionExpression(arrow) => {
      Some(RenderBody { scope_id: arrow.node_id.get(), body: Some(&*arrow.body), span: arrow.span })
    }
    Expression::FunctionExpression(function) => Some(RenderBody {
      scope_id: function.node_id.get(),
      body: function.body.as_deref(),
      span: function.span,
    }),
    _ => None,
  }
}

fn functional_component_body<'a>(expression: &'a Expression<'a>) -> Option<RenderBody<'a>> {
  let body = function_like_body(expression)?;
  function_body_returns_jsx(body.body).then_some(body)
}

fn function_declaration_body<'a>(function: &'a Function<'a>) -> Option<RenderBody<'a>> {
  let body = function.body.as_deref();
  function_body_returns_jsx(body).then(|| RenderBody {
    scope_id: function.node_id.get(),
    body,
    span: function.span,
  })
}

fn function_body_returns_jsx(body: Option<&FunctionBody<'_>>) -> bool {
  let Some(body) = body else {
    return false;
  };
  body.statements.iter().any(|statement| match statement {
    Statement::ReturnStatement(ret) => {
      ret.argument.as_ref().is_some_and(|argument| expression_is_jsx(argument))
    }
    Statement::ExpressionStatement(statement) => expression_is_jsx(&statement.expression),
    _ => false,
  })
}

fn expression_is_jsx(expression: &Expression<'_>) -> bool {
  match expression {
    Expression::JSXElement(_) | Expression::JSXFragment(_) => true,
    Expression::ParenthesizedExpression(paren) => expression_is_jsx(&paren.expression),
    _ => false,
  }
}

fn property_key_name(key: &PropertyKey<'_>) -> Option<String> {
  match key {
    PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.to_string()),
    PropertyKey::StringLiteral(literal) => Some(literal.value.to_string()),
    _ => None,
  }
}

fn is_exported_node(semantic: &oxc_semantic::Semantic<'_>, node_id: NodeId) -> bool {
  semantic.nodes().ancestor_ids(node_id).any(|ancestor| {
    matches!(
      semantic.nodes().kind(ancestor),
      AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_)
    )
  })
}

fn is_exported_declarator(semantic: &oxc_semantic::Semantic<'_>, node_id: NodeId) -> bool {
  is_exported_node(semantic, node_id)
}
