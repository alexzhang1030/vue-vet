//! Structured Nuxt config facts. Oxc owns the AST; callers see only this policy.

use std::collections::BTreeSet;

use oxc_allocator::Allocator;
use oxc_ast::{
  AstKind,
  ast::{
    Argument, ArrayExpressionElement, AssignmentTarget, CallExpression, Expression,
    ObjectPropertyKind, Statement,
  },
};
use oxc_parser::Parser;
use oxc_semantic::{Semantic, SemanticBuilder, SymbolId};
use oxc_span::SourceType;
use oxc_syntax::{reference::ReferenceFlags, symbol::SymbolFlags};

const MAX_PEEL_DEPTH: u8 = 8;

/// How a Nuxt config file talks about `@nuxt/content` in `modules`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NuxtContentModulePolicy {
  /// No `modules` property on the config object.
  #[default]
  Absent,
  /// Proven string `'@nuxt/content'` (or equivalent) in `modules`.
  IncludesContent,
  /// Proven empty `modules: []`. Overrides installed-dependency evidence.
  Empty,
  /// Spread, computed key, parse error, or non-literal expression. Does not
  /// register content by itself and does not override package-dependency evidence.
  Unresolved,
}

/// Statically known facts from one `nuxt.config.*` export. Never executes the file.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NuxtConfigFacts {
  pub modules: NuxtContentModulePolicy,
  /// Literal `srcDir` on the exported config, when statically known.
  pub src_dir: Option<String>,
  /// Statically known `extends` specifiers (bare packages or relative paths).
  pub extends: Vec<String>,
}

/// Extract a `modules` policy from `nuxt.config.*` without executing it.
#[must_use]
pub fn nuxt_config_content_modules(path: &str, source: &str) -> NuxtContentModulePolicy {
  parse_nuxt_config(path, source).modules
}

/// Extract exported Nuxt config facts without executing the file.
#[must_use]
pub fn parse_nuxt_config(path: &str, source: &str) -> NuxtConfigFacts {
  let allocator = Allocator::default();
  let source_type = source_type_for_config(path);
  let parsed = Parser::new(&allocator, source, source_type).parse();
  if !parsed.diagnostics.is_empty() {
    return NuxtConfigFacts {
      modules: NuxtContentModulePolicy::Unresolved,
      ..NuxtConfigFacts::default()
    };
  }
  let built = SemanticBuilder::new().with_build_nodes(true).build(&parsed.program);
  if !built.diagnostics.is_empty() {
    return NuxtConfigFacts {
      modules: NuxtContentModulePolicy::Unresolved,
      ..NuxtConfigFacts::default()
    };
  }
  let semantic = built.semantic;
  let Some(exported) = exported_config_expression(parsed.program.body.as_slice()) else {
    return NuxtConfigFacts::default();
  };
  let mut visited = BTreeSet::new();
  facts_from_expression(exported, &semantic, &mut visited, 0).unwrap_or_default()
}

fn source_type_for_config(path: &str) -> SourceType {
  let path = std::path::Path::new(path);
  let ext = path.extension().and_then(|extension| extension.to_str()).unwrap_or("");
  if ext.eq_ignore_ascii_case("mts") || ext.eq_ignore_ascii_case("ts") {
    SourceType::ts()
  } else if ext.eq_ignore_ascii_case("cts") || ext.eq_ignore_ascii_case("cjs") {
    SourceType::cjs()
  } else {
    SourceType::mjs()
  }
}

fn exported_config_expression<'a>(body: &'a [Statement<'a>]) -> Option<&'a Expression<'a>> {
  let mut export_default = None;
  let mut cjs_exports = Vec::new();
  for statement in body {
    match statement {
      Statement::ExportDefaultDeclaration(declaration) => {
        if export_default.is_some() {
          return None;
        }
        export_default = declaration.declaration.as_expression();
      }
      Statement::ExpressionStatement(statement) => {
        if let Expression::AssignmentExpression(assignment) =
          statement.expression.get_inner_expression()
          && is_module_exports(&assignment.left)
        {
          cjs_exports.push(&assignment.right);
        }
      }
      _ => {}
    }
  }
  if cjs_exports.len() > 1 {
    return None;
  }
  match (export_default, cjs_exports.first().copied()) {
    (Some(_), Some(_)) | (None, None) => None,
    (Some(exported), None) | (None, Some(exported)) => Some(exported),
  }
}

fn is_define_nuxt_config(call: &CallExpression<'_>, semantic: &Semantic<'_>) -> bool {
  match call.callee.get_inner_expression() {
    Expression::Identifier(identifier) if identifier.name == "defineNuxtConfig" => {
      define_nuxt_config_is_unshadowed(identifier, semantic)
    }
    _ => false,
  }
}

fn define_nuxt_config_is_unshadowed(
  identifier: &oxc_ast::ast::IdentifierReference<'_>,
  semantic: &Semantic<'_>,
) -> bool {
  let symbol_id = identifier
    .reference_id
    .get()
    .and_then(|id| semantic.scoping().get_reference(id).symbol_id())
    .or_else(|| {
      semantic.scoping().find_binding(semantic.scoping().root_scope_id(), "defineNuxtConfig".into())
    });
  symbol_id.is_none_or(|symbol_id| {
    semantic
      .scoping()
      .symbol_flags(symbol_id)
      .intersects(SymbolFlags::Import | SymbolFlags::TypeImport)
  })
}

fn is_module_exports(target: &AssignmentTarget<'_>) -> bool {
  match target {
    AssignmentTarget::StaticMemberExpression(member) => {
      matches!(
        member.object.get_inner_expression(),
        Expression::Identifier(object) if object.name == "module"
      ) && member.property.name == "exports"
    }
    _ => false,
  }
}

fn facts_from_expression<'a>(
  expression: &'a Expression<'a>,
  semantic: &Semantic<'a>,
  visited: &mut BTreeSet<SymbolId>,
  depth: u8,
) -> Option<NuxtConfigFacts> {
  if depth > MAX_PEEL_DEPTH {
    return Some(NuxtConfigFacts {
      modules: NuxtContentModulePolicy::Unresolved,
      ..NuxtConfigFacts::default()
    });
  }
  let inner = peel(expression, semantic, visited, depth)?;
  match inner {
    Expression::CallExpression(call) if is_define_nuxt_config(call, semantic) => {
      call.arguments.first().and_then(|argument| match argument {
        Argument::SpreadElement(_) => Some(NuxtConfigFacts {
          modules: NuxtContentModulePolicy::Unresolved,
          ..NuxtConfigFacts::default()
        }),
        other => other
          .as_expression()
          .and_then(|expr| facts_from_expression(expr, semantic, visited, depth.saturating_add(1))),
      })
    }
    Expression::ObjectExpression(object) => {
      Some(facts_from_object(object, semantic, visited, depth))
    }
    _ => Some(NuxtConfigFacts {
      modules: NuxtContentModulePolicy::Unresolved,
      ..NuxtConfigFacts::default()
    }),
  }
}

fn peel<'a>(
  expression: &'a Expression<'a>,
  semantic: &Semantic<'a>,
  visited: &mut BTreeSet<SymbolId>,
  depth: u8,
) -> Option<&'a Expression<'a>> {
  if depth > MAX_PEEL_DEPTH {
    return None;
  }
  let inner = expression.get_inner_expression();
  match inner {
    Expression::Identifier(identifier) => {
      let (symbol_id, init) = local_const_init(identifier, semantic)?;
      if !visited.insert(symbol_id) {
        return None;
      }
      let peeled = peel(init, semantic, visited, depth.saturating_add(1));
      visited.remove(&symbol_id);
      peeled
    }
    other => Some(other),
  }
}

fn local_const_init<'a>(
  identifier: &oxc_ast::ast::IdentifierReference<'a>,
  semantic: &Semantic<'a>,
) -> Option<(SymbolId, &'a Expression<'a>)> {
  let reference_id = identifier.reference_id.get()?;
  let symbol_id = semantic.scoping().get_reference(reference_id).symbol_id()?;
  if !semantic.scoping().symbol_flags(symbol_id).contains(SymbolFlags::ConstVariable) {
    return None;
  }
  if !const_initializer_is_immutable(semantic, symbol_id) {
    return None;
  }
  let declaration = semantic.symbol_declaration(symbol_id);
  let init = match declaration.kind() {
    AstKind::VariableDeclarator(declarator) => declarator.init.as_ref()?,
    AstKind::BindingIdentifier(_) => match semantic.nodes().parent_kind(declaration.id()) {
      AstKind::VariableDeclarator(declarator) => declarator.init.as_ref()?,
      _ => return None,
    },
    _ => return None,
  };
  Some((symbol_id, init))
}

fn const_initializer_is_immutable(semantic: &Semantic<'_>, symbol_id: SymbolId) -> bool {
  semantic.symbol_references(symbol_id).all(|reference| {
    if reference.flags().intersects(ReferenceFlags::Write | ReferenceFlags::MemberWriteTarget) {
      return false;
    }
    !reference_can_mutate_or_escape(semantic, reference, symbol_id)
  })
}

fn reference_can_mutate_or_escape(
  semantic: &Semantic<'_>,
  reference: &oxc_semantic::Reference,
  symbol_id: SymbolId,
) -> bool {
  let parent = semantic.nodes().parent_kind(reference.node_id());
  match parent {
    AstKind::StaticMemberExpression(_)
    | AstKind::PrivateFieldExpression(_)
    | AstKind::ComputedMemberExpression(_)
    | AstKind::NewExpression(_)
    | AstKind::SpreadElement(_)
    | AstKind::VariableDeclarator(_)
    | AstKind::ReturnStatement(_) => true,
    AstKind::CallExpression(call) => {
      if callee_is_symbol(call, symbol_id, semantic) {
        return true;
      }
      !is_define_nuxt_config(call, semantic)
    }
    AstKind::AssignmentExpression(assignment) => !is_module_exports(&assignment.left),
    _ => false,
  }
}

fn callee_is_symbol(
  call: &CallExpression<'_>,
  symbol_id: SymbolId,
  semantic: &Semantic<'_>,
) -> bool {
  match call.callee.get_inner_expression() {
    Expression::Identifier(identifier) => {
      identifier.reference_id.get().and_then(|id| semantic.scoping().get_reference(id).symbol_id())
        == Some(symbol_id)
    }
    _ => false,
  }
}

fn facts_from_object(
  object: &oxc_ast::ast::ObjectExpression<'_>,
  semantic: &Semantic<'_>,
  visited: &mut BTreeSet<SymbolId>,
  depth: u8,
) -> NuxtConfigFacts {
  let mut facts = NuxtConfigFacts::default();
  for property in &object.properties {
    match property {
      ObjectPropertyKind::SpreadProperty(_) => {
        facts.modules = NuxtContentModulePolicy::Unresolved;
        facts.src_dir = None;
        facts.extends.clear();
      }
      ObjectPropertyKind::ObjectProperty(property) => {
        let Some(name) = property.key.static_name() else {
          facts.modules = NuxtContentModulePolicy::Unresolved;
          facts.src_dir = None;
          facts.extends.clear();
          continue;
        };
        match name.as_ref() {
          "modules" => {
            facts.modules = policy_from_modules_value(
              property.value.get_inner_expression(),
              semantic,
              visited,
              depth,
            );
          }
          "srcDir" => {
            facts.src_dir =
              string_literal(property.value.get_inner_expression(), semantic, visited, depth);
          }
          "extends" => {
            facts.extends =
              extends_from_value(property.value.get_inner_expression(), semantic, visited, depth);
          }
          _ => {}
        }
      }
    }
  }
  facts
}

fn policy_from_modules_value(
  value: &Expression<'_>,
  semantic: &Semantic<'_>,
  visited: &mut BTreeSet<SymbolId>,
  depth: u8,
) -> NuxtContentModulePolicy {
  let Some(inner) = peel_value(value, semantic, visited, depth) else {
    return NuxtContentModulePolicy::Unresolved;
  };
  let Expression::ArrayExpression(array) = inner else {
    return NuxtContentModulePolicy::Unresolved;
  };
  if array.elements.is_empty() {
    return NuxtContentModulePolicy::Empty;
  }
  let mut includes = false;
  let mut unknown = false;
  for element in &array.elements {
    match element {
      ArrayExpressionElement::SpreadElement(_) => unknown = true,
      ArrayExpressionElement::Elision(_) => {}
      other => {
        match other.as_expression().and_then(|expr| peel_value(expr, semantic, visited, depth)) {
          Some(Expression::StringLiteral(literal)) => {
            if literal.value == "@nuxt/content" {
              includes = true;
            }
          }
          Some(Expression::TemplateLiteral(literal))
            if literal.expressions.is_empty() && literal.quasis.len() == 1 =>
          {
            if literal.quasis.first().is_some_and(|quasi| {
              quasi.value.cooked.as_deref().unwrap_or(quasi.value.raw.as_str()) == "@nuxt/content"
            }) {
              includes = true;
            }
          }
          _ => unknown = true,
        }
      }
    }
  }
  if includes {
    NuxtContentModulePolicy::IncludesContent
  } else if unknown {
    NuxtContentModulePolicy::Unresolved
  } else {
    NuxtContentModulePolicy::Empty
  }
}

fn extends_from_value(
  value: &Expression<'_>,
  semantic: &Semantic<'_>,
  visited: &mut BTreeSet<SymbolId>,
  depth: u8,
) -> Vec<String> {
  let Some(inner) = peel_value(value, semantic, visited, depth) else {
    return Vec::new();
  };
  if let Some(single) = string_from_expression(inner) {
    return vec![single];
  }
  let Expression::ArrayExpression(array) = inner else {
    return Vec::new();
  };
  let mut specifiers = Vec::new();
  for element in &array.elements {
    let Some(expr) = element.as_expression() else {
      continue;
    };
    let Some(inner) = peel_value(expr, semantic, visited, depth) else {
      continue;
    };
    if let Some(specifier) = string_from_expression(inner) {
      specifiers.push(specifier);
    }
  }
  specifiers
}

fn string_literal(
  value: &Expression<'_>,
  semantic: &Semantic<'_>,
  visited: &mut BTreeSet<SymbolId>,
  depth: u8,
) -> Option<String> {
  peel_value(value, semantic, visited, depth).and_then(string_from_expression)
}

fn string_from_expression(value: &Expression<'_>) -> Option<String> {
  match value {
    Expression::StringLiteral(literal) => Some(literal.value.to_string()),
    Expression::TemplateLiteral(literal)
      if literal.expressions.is_empty() && literal.quasis.len() == 1 =>
    {
      literal
        .quasis
        .first()
        .map(|quasi| quasi.value.cooked.as_deref().unwrap_or(quasi.value.raw.as_str()).to_owned())
    }
    _ => None,
  }
}

fn peel_value<'a>(
  expression: &'a Expression<'a>,
  semantic: &Semantic<'a>,
  visited: &mut BTreeSet<SymbolId>,
  depth: u8,
) -> Option<&'a Expression<'a>> {
  if depth > MAX_PEEL_DEPTH {
    return None;
  }
  peel(expression, semantic, visited, depth)
}
