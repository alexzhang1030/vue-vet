//! Class-symbol and member indexes for native private-field receiver proof.
//!
//! Built once per class declaration or const-bound class expression. Collection
//! joins those members with per-object operations; it does not scan
//! class-by-instance-by-method.

use std::collections::{HashMap, HashSet};

use oxc_ast::ast::{
  Argument, AssignmentOperator, AssignmentTarget, BindingPattern, Class, ClassElement, ClassType,
  Expression, Function, MethodDefinition, MethodDefinitionKind, PrivateFieldExpression,
  PropertyKey, SimpleAssignmentTarget, Statement, VariableDeclaration, VariableDeclarationKind,
};
use oxc_semantic::{NodeId, SymbolFlags, SymbolId};
use oxc_span::Span;

use super::stats::WorkCounter;

pub(super) const CLASS_WALK_BUDGET: u8 = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MemberKind {
  Method,
  Getter,
}

#[derive(Clone, Debug)]
pub(super) struct MemberRecord {
  pub span: Span,
  pub kind: MemberKind,
  pub field: String,
  pub private_span: Span,
}

#[derive(Clone, Debug)]
pub(super) struct ClassRecord {
  pub ordinary: bool,
  pub span: Span,
  pub members: HashMap<String, MemberRecord>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ClassNewInfo {
  pub callee: Option<SymbolId>,
  pub argc: u32,
  pub has_spread: bool,
}

pub(super) fn class_binding_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  class: &Class<'_>,
) -> Option<SymbolId> {
  if let Some(id) = &class.id
    && let Some(symbol_id) = id.symbol_id.get()
  {
    return Some(symbol_id);
  }
  if class.r#type != ClassType::ClassExpression {
    return None;
  }
  let mut current = node_id;
  for _ in 0..CLASS_WALK_BUDGET {
    let parent = semantic.nodes().parent_id(current);
    match semantic.nodes().kind(parent) {
      oxc_ast::AstKind::ParenthesizedExpression(_)
      | oxc_ast::AstKind::TSAsExpression(_)
      | oxc_ast::AstKind::TSSatisfiesExpression(_)
      | oxc_ast::AstKind::TSNonNullExpression(_)
      | oxc_ast::AstKind::TSTypeAssertion(_) => current = parent,
      oxc_ast::AstKind::VariableDeclarator(declarator) => {
        let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
          return None;
        };
        let symbol_id = binding.symbol_id.get()?;
        return semantic
          .scoping()
          .symbol_flags(symbol_id)
          .contains(SymbolFlags::ConstVariable)
          .then_some(symbol_id);
      }
      _ => return None,
    }
  }
  None
}

pub(super) fn analyze_class(class: &Class<'_>, work: &WorkCounter) -> ClassRecord {
  work.add_object_entries(1);
  let mut ordinary = class.decorators.is_empty()
    && class.super_class.is_none()
    && !class.r#abstract
    && !class.declare;
  let mut privates = HashMap::new();
  let mut constructor = None;
  let mut methods = Vec::new();
  let mut own_fields = HashSet::new();
  let mut field_inits = Vec::new();
  for element in &class.body.body {
    work.add_object_entries(1);
    match element {
      ClassElement::StaticBlock(_) | ClassElement::AccessorProperty(_) => ordinary = false,
      ClassElement::TSIndexSignature(_) => {}
      ClassElement::PropertyDefinition(prop) => {
        if !prop.decorators.is_empty() {
          ordinary = false;
          continue;
        }
        if !prop.r#static && skips_reactive_proxy(&prop.key) {
          ordinary = false;
        }
        match &prop.key {
          PropertyKey::PrivateIdentifier(id) if !prop.r#static => {
            work.add_key_copies(1);
            privates.insert(id.name.to_string(), id.span);
          }
          _ if !prop.r#static => {
            if let Some(name) = own_instance_name(&prop.key) {
              work.add_key_copies(1);
              own_fields.insert(name.to_string());
            }
            if let Some(init) = &prop.value {
              field_inits.push(init);
            }
          }
          _ => {}
        }
      }
      ClassElement::MethodDefinition(method) => {
        if !method.decorators.is_empty() {
          ordinary = false;
          continue;
        }
        if !method.r#static && skips_reactive_proxy(&method.key) {
          ordinary = false;
        }
        match method.kind {
          MethodDefinitionKind::Constructor => constructor = Some(method.as_ref()),
          MethodDefinitionKind::Method | MethodDefinitionKind::Get
            if !method.r#static && !method.computed =>
          {
            methods.push(method.as_ref());
          }
          MethodDefinitionKind::Method | MethodDefinitionKind::Get | MethodDefinitionKind::Set => {}
        }
      }
    }
  }
  let mut remaining = CLASS_WALK_BUDGET;
  for init in field_inits {
    collect_this_assigns(init, &mut own_fields, work, remaining);
    remaining = remaining.saturating_sub(1);
  }
  let method_names: HashSet<&str> =
    methods.iter().filter_map(|method| static_method_name(method)).collect();
  let mut bound = HashSet::new();
  if let Some(ctor) = constructor {
    match constructor_effects(&ctor.value, &method_names, work) {
      ConstructorEffects::Replacement | ConstructorEffects::Uncertain => ordinary = false,
      ConstructorEffects::Ordinary { bound_methods } => bound = bound_methods,
    }
  }
  let mut members = HashMap::new();
  if ordinary && !privates.is_empty() {
    for method in methods {
      let Some(name) = static_method_name(method) else {
        continue;
      };
      work.add_key_lookups(1);
      if bound.contains(name) || own_fields.contains(name) {
        continue;
      }
      let kind = match method.kind {
        MethodDefinitionKind::Get => MemberKind::Getter,
        _ => MemberKind::Method,
      };
      let Some((field, private_span)) = proven_this_private(&method.value, &privates, work) else {
        continue;
      };
      work.add_key_copies(1);
      members
        .insert(name.to_string(), MemberRecord { span: method.span, kind, field, private_span });
    }
  }
  ClassRecord { ordinary, span: class.span, members }
}

pub(super) fn class_new_info(
  expression: &oxc_ast::ast::NewExpression<'_>,
  callee: Option<SymbolId>,
) -> ClassNewInfo {
  let has_spread = expression.arguments.iter().any(Argument::is_spread);
  ClassNewInfo {
    callee,
    argc: u32::try_from(expression.arguments.len()).unwrap_or(u32::MAX),
    has_spread,
  }
}

enum ConstructorEffects {
  Ordinary { bound_methods: HashSet<String> },
  Replacement,
  Uncertain,
}

fn constructor_effects(
  function: &Function<'_>,
  method_names: &HashSet<&str>,
  work: &WorkCounter,
) -> ConstructorEffects {
  let Some(body) = function.body.as_ref() else {
    return ConstructorEffects::Uncertain;
  };
  if function.generator {
    return ConstructorEffects::Uncertain;
  }
  let mut bound_methods = HashSet::new();
  let mut remaining = CLASS_WALK_BUDGET;
  for statement in &body.statements {
    work.add_queries(1);
    if remaining == 0 {
      return ConstructorEffects::Uncertain;
    }
    remaining = remaining.saturating_sub(1);
    match statement {
      Statement::EmptyStatement(_) => {}
      Statement::ExpressionStatement(statement) => {
        match constructor_expression(
          &statement.expression,
          method_names,
          &mut bound_methods,
          work,
          remaining,
        ) {
          ConstructorEffects::Ordinary { .. } => {}
          other => return other,
        }
      }
      Statement::ReturnStatement(statement) => match statement.argument.as_ref() {
        None => {}
        Some(argument)
          if matches!(argument.get_inner_expression(), Expression::ThisExpression(_)) => {}
        Some(argument) if matches!(argument.get_inner_expression(), Expression::NewTarget(_)) => {
          return ConstructorEffects::Uncertain;
        }
        Some(_) => return ConstructorEffects::Replacement,
      },
      _ => return ConstructorEffects::Uncertain,
    }
  }
  ConstructorEffects::Ordinary { bound_methods }
}

fn constructor_expression(
  expression: &Expression<'_>,
  method_names: &HashSet<&str>,
  bound: &mut HashSet<String>,
  work: &WorkCounter,
  remaining: u8,
) -> ConstructorEffects {
  work.add_queries(1);
  if remaining == 0 {
    return ConstructorEffects::Uncertain;
  }
  match expression.get_inner_expression() {
    Expression::ThisExpression(_)
    | Expression::Identifier(_)
    | Expression::BooleanLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::NullLiteral(_) => ConstructorEffects::Ordinary { bound_methods: HashSet::new() },
    Expression::AssignmentExpression(assignment) => {
      if assignment.operator != AssignmentOperator::Assign {
        return ConstructorEffects::Uncertain;
      }
      match this_assign_target(&assignment.left) {
        Some(ThisAssign::Private) => ConstructorEffects::Ordinary { bound_methods: HashSet::new() },
        Some(ThisAssign::Public(name)) => {
          if is_bound_method_rhs(&assignment.right, name) {
            work.add_key_copies(1);
            bound.insert(name.to_string());
            ConstructorEffects::Ordinary { bound_methods: HashSet::new() }
          } else if method_names.contains(name) {
            ConstructorEffects::Uncertain
          } else {
            ConstructorEffects::Ordinary { bound_methods: HashSet::new() }
          }
        }
        None => ConstructorEffects::Uncertain,
      }
    }
    Expression::SequenceExpression(sequence) => {
      let mut remaining = remaining.saturating_sub(1);
      for nested in &sequence.expressions {
        match constructor_expression(nested, method_names, bound, work, remaining) {
          ConstructorEffects::Ordinary { .. } => remaining = remaining.saturating_sub(1),
          other => return other,
        }
        if remaining == 0 {
          return ConstructorEffects::Uncertain;
        }
      }
      ConstructorEffects::Ordinary { bound_methods: HashSet::new() }
    }
    _ => ConstructorEffects::Uncertain,
  }
}

enum ThisAssign<'a> {
  Public(&'a str),
  Private,
}

fn this_assign_target<'a>(target: &'a AssignmentTarget<'a>) -> Option<ThisAssign<'a>> {
  match target {
    AssignmentTarget::StaticMemberExpression(member) => {
      matches!(member.object.get_inner_expression(), Expression::ThisExpression(_))
        .then_some(ThisAssign::Public(member.property.name.as_str()))
    }
    AssignmentTarget::PrivateFieldExpression(member) => {
      matches!(member.object.get_inner_expression(), Expression::ThisExpression(_))
        .then_some(ThisAssign::Private)
    }
    _ => None,
  }
}

fn this_member_name<'a>(target: &'a AssignmentTarget<'a>) -> Option<&'a str> {
  match this_assign_target(target) {
    Some(ThisAssign::Public(name)) => Some(name),
    _ => None,
  }
}

fn is_bound_method_rhs(expression: &Expression<'_>, name: &str) -> bool {
  match expression.get_inner_expression() {
    Expression::ArrowFunctionExpression(_) => true,
    Expression::CallExpression(call) => {
      let Expression::StaticMemberExpression(bind) = call.callee.get_inner_expression() else {
        return false;
      };
      if bind.property.name.as_str() != "bind"
        || call.arguments.len() != 1
        || call.arguments.iter().any(Argument::is_spread)
      {
        return false;
      }
      let Some(this_arg) = call.arguments.first().and_then(Argument::as_expression) else {
        return false;
      };
      if !matches!(this_arg.get_inner_expression(), Expression::ThisExpression(_)) {
        return false;
      }
      let Expression::StaticMemberExpression(method) = bind.object.get_inner_expression() else {
        return false;
      };
      matches!(method.object.get_inner_expression(), Expression::ThisExpression(_))
        && method.property.name.as_str() == name
    }
    _ => false,
  }
}

fn static_method_name<'a>(method: &'a MethodDefinition<'a>) -> Option<&'a str> {
  match &method.key {
    PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
    _ => None,
  }
}

fn own_instance_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
  match key {
    PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
    PropertyKey::StringLiteral(literal) => Some(literal.value.as_str()),
    _ => None,
  }
}

fn skips_reactive_proxy(key: &PropertyKey<'_>) -> bool {
  if is_symbol_tostringtag(key) {
    return true;
  }
  matches!(own_instance_name(key), Some("__v_skip" | "__v_raw"))
}

fn is_symbol_tostringtag(key: &PropertyKey<'_>) -> bool {
  let PropertyKey::StaticMemberExpression(member) = key else {
    return false;
  };
  member.property.name.as_str() == "toStringTag"
    && matches!(
      member.object.get_inner_expression(),
      Expression::Identifier(object) if object.name.as_str() == "Symbol"
    )
}

fn collect_this_assigns(
  expression: &Expression<'_>,
  names: &mut HashSet<String>,
  work: &WorkCounter,
  remaining: u8,
) {
  work.add_queries(1);
  if remaining == 0 {
    return;
  }
  let remaining = remaining.saturating_sub(1);
  match expression.get_inner_expression() {
    Expression::AssignmentExpression(assignment) => {
      if let Some(name) = this_member_name(&assignment.left) {
        work.add_key_copies(1);
        names.insert(name.to_string());
      }
      collect_this_assigns(&assignment.right, names, work, remaining);
    }
    Expression::SequenceExpression(sequence) => {
      for nested in &sequence.expressions {
        collect_this_assigns(nested, names, work, remaining);
      }
    }
    Expression::ParenthesizedExpression(inner) => {
      collect_this_assigns(&inner.expression, names, work, remaining);
    }
    Expression::TSAsExpression(inner) => {
      collect_this_assigns(&inner.expression, names, work, remaining);
    }
    Expression::TSSatisfiesExpression(inner) => {
      collect_this_assigns(&inner.expression, names, work, remaining);
    }
    Expression::TSNonNullExpression(inner) => {
      collect_this_assigns(&inner.expression, names, work, remaining);
    }
    Expression::TSTypeAssertion(inner) => {
      collect_this_assigns(&inner.expression, names, work, remaining);
    }
    _ => {}
  }
}

enum Access {
  Hit { field: String, span: Span },
  None,
  Unknown,
}

fn proven_this_private(
  function: &Function<'_>,
  privates: &HashMap<String, Span>,
  work: &WorkCounter,
) -> Option<(String, Span)> {
  let body = function.body.as_ref()?;
  if function.generator || function.r#async {
    return None;
  }
  let mut alias = None;
  let mut found = None;
  let mut remaining = CLASS_WALK_BUDGET;
  for statement in &body.statements {
    work.add_queries(1);
    if remaining == 0 {
      return None;
    }
    remaining = remaining.saturating_sub(1);
    match statement {
      Statement::EmptyStatement(_) => {}
      Statement::VariableDeclaration(declaration) => {
        if let Some(name) = this_alias_name(declaration) {
          if alias.is_some() {
            return None;
          }
          alias = Some(name);
          continue;
        }
        for declarator in &declaration.declarations {
          if !matches!(declarator.id, BindingPattern::BindingIdentifier(_)) {
            return None;
          }
          let Some(init) = &declarator.init else {
            return None;
          };
          match walk_access(init, alias, privates, work, remaining, false) {
            Access::Hit { field, span } => found = Some(earliest(found, field, span)),
            Access::None => {}
            Access::Unknown => return None,
          }
        }
      }
      Statement::ExpressionStatement(statement) => {
        match walk_access(&statement.expression, alias, privates, work, remaining, false) {
          Access::Hit { field, span } => found = Some(earliest(found, field, span)),
          Access::None => {}
          Access::Unknown => return None,
        }
      }
      Statement::ReturnStatement(statement) => {
        let Some(argument) = &statement.argument else {
          continue;
        };
        match walk_access(argument, alias, privates, work, remaining, false) {
          Access::Hit { field, span } => found = Some(earliest(found, field, span)),
          Access::None => {}
          Access::Unknown => return None,
        }
      }
      _ => return None,
    }
  }
  found
}

fn this_alias_name<'a>(declaration: &'a VariableDeclaration<'a>) -> Option<&'a str> {
  if declaration.kind != VariableDeclarationKind::Const || declaration.declarations.len() != 1 {
    return None;
  }
  let declarator = declaration.declarations.first()?;
  let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
    return None;
  };
  let init = declarator.init.as_ref()?;
  matches!(init.get_inner_expression(), Expression::ThisExpression(_))
    .then_some(binding.name.as_str())
}

fn earliest(found: Option<(String, Span)>, field: String, span: Span) -> (String, Span) {
  match found {
    Some((current_field, current_span)) if current_span.start <= span.start => {
      (current_field, current_span)
    }
    _ => (field, span),
  }
}

fn walk_access(
  expression: &Expression<'_>,
  alias: Option<&str>,
  privates: &HashMap<String, Span>,
  work: &WorkCounter,
  remaining: u8,
  guarded: bool,
) -> Access {
  work.add_queries(1);
  if remaining == 0 {
    return Access::Unknown;
  }
  let remaining = remaining.saturating_sub(1);
  match expression.get_inner_expression() {
    Expression::ThisExpression(_)
    | Expression::Identifier(_)
    | Expression::BooleanLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::NullLiteral(_)
    | Expression::RegExpLiteral(_)
    | Expression::Super(_)
    | Expression::PrivateInExpression(_) => Access::None,
    Expression::PrivateFieldExpression(member) => {
      private_field_hit(member, alias, privates, work, remaining, guarded)
    }
    Expression::ConditionalExpression(conditional) => {
      match walk_access(&conditional.test, alias, privates, work, remaining, guarded) {
        Access::Unknown => Access::Unknown,
        Access::Hit { field, span } => {
          let right = walk_access(&conditional.consequent, alias, privates, work, remaining, true);
          let alt = walk_access(&conditional.alternate, alias, privates, work, remaining, true);
          if matches!(right, Access::Unknown) || matches!(alt, Access::Unknown) {
            Access::Unknown
          } else {
            Access::Hit { field, span }
          }
        }
        Access::None => {
          let right = walk_access(&conditional.consequent, alias, privates, work, remaining, true);
          let alt = walk_access(&conditional.alternate, alias, privates, work, remaining, true);
          if matches!(right, Access::Unknown) || matches!(alt, Access::Unknown) {
            Access::Unknown
          } else {
            Access::None
          }
        }
      }
    }
    Expression::LogicalExpression(logical) => {
      match walk_access(&logical.left, alias, privates, work, remaining, guarded) {
        Access::Unknown => Access::Unknown,
        Access::Hit { field, span } => {
          match walk_access(&logical.right, alias, privates, work, remaining, true) {
            Access::Unknown => Access::Unknown,
            _ => Access::Hit { field, span },
          }
        }
        Access::None => match walk_access(&logical.right, alias, privates, work, remaining, true) {
          Access::Unknown => Access::Unknown,
          _ => Access::None,
        },
      }
    }
    Expression::BinaryExpression(binary) => merge_unconditional(
      walk_access(&binary.left, alias, privates, work, remaining, guarded),
      walk_access(&binary.right, alias, privates, work, remaining, guarded),
    ),
    Expression::SequenceExpression(sequence) => {
      let mut result = Access::None;
      for nested in &sequence.expressions {
        result = merge_unconditional(
          result,
          walk_access(nested, alias, privates, work, remaining, guarded),
        );
        if matches!(result, Access::Unknown) {
          return Access::Unknown;
        }
      }
      result
    }
    Expression::UnaryExpression(unary) => {
      walk_access(&unary.argument, alias, privates, work, remaining, guarded)
    }
    Expression::UpdateExpression(update) => match &update.argument {
      SimpleAssignmentTarget::PrivateFieldExpression(member) => {
        private_field_hit(member, alias, privates, work, remaining, guarded)
      }
      _ => Access::Unknown,
    },
    Expression::StaticMemberExpression(member) => {
      walk_access(&member.object, alias, privates, work, remaining, guarded)
    }
    Expression::ComputedMemberExpression(member) => merge_unconditional(
      walk_access(&member.object, alias, privates, work, remaining, guarded),
      walk_access(&member.expression, alias, privates, work, remaining, guarded),
    ),
    Expression::AssignmentExpression(assignment) => {
      let left = match &assignment.left {
        AssignmentTarget::PrivateFieldExpression(member) => {
          private_field_hit(member, alias, privates, work, remaining, guarded)
        }
        _ => Access::Unknown,
      };
      if matches!(left, Access::Unknown) {
        return Access::Unknown;
      }
      merge_unconditional(
        left,
        walk_access(&assignment.right, alias, privates, work, remaining, guarded),
      )
    }
    Expression::ArrayExpression(array) => {
      let mut result = Access::None;
      for element in &array.elements {
        let Some(expr) = element.as_expression() else {
          return Access::Unknown;
        };
        result =
          merge_unconditional(result, walk_access(expr, alias, privates, work, remaining, guarded));
        if matches!(result, Access::Unknown) {
          return Access::Unknown;
        }
      }
      result
    }
    Expression::TemplateLiteral(literal) => {
      let mut result = Access::None;
      for nested in &literal.expressions {
        result = merge_unconditional(
          result,
          walk_access(nested, alias, privates, work, remaining, guarded),
        );
        if matches!(result, Access::Unknown) {
          return Access::Unknown;
        }
      }
      result
    }
    Expression::ParenthesizedExpression(inner) => {
      walk_access(&inner.expression, alias, privates, work, remaining, guarded)
    }
    Expression::TSAsExpression(inner) => {
      walk_access(&inner.expression, alias, privates, work, remaining, guarded)
    }
    Expression::TSSatisfiesExpression(inner) => {
      walk_access(&inner.expression, alias, privates, work, remaining, guarded)
    }
    Expression::TSNonNullExpression(inner) => {
      walk_access(&inner.expression, alias, privates, work, remaining, guarded)
    }
    Expression::TSTypeAssertion(inner) => {
      walk_access(&inner.expression, alias, privates, work, remaining, guarded)
    }
    Expression::TSInstantiationExpression(inner) => {
      walk_access(&inner.expression, alias, privates, work, remaining, guarded)
    }
    Expression::CallExpression(call) => {
      if call.optional {
        return Access::Unknown;
      }
      let callee = match call.callee.get_inner_expression() {
        Expression::StaticMemberExpression(member) => {
          walk_access(&member.object, alias, privates, work, remaining, guarded)
        }
        Expression::ComputedMemberExpression(member) => merge_unconditional(
          walk_access(&member.object, alias, privates, work, remaining, guarded),
          walk_access(&member.expression, alias, privates, work, remaining, guarded),
        ),
        Expression::PrivateFieldExpression(member) => {
          private_field_hit(member, alias, privates, work, remaining, guarded)
        }
        _ => Access::None,
      };
      let mut result = match callee {
        Access::Unknown => Access::None,
        other => other,
      };
      for argument in &call.arguments {
        if argument.is_spread() {
          return Access::Unknown;
        }
        let Some(expr) = argument.as_expression() else {
          return Access::Unknown;
        };
        result =
          merge_unconditional(result, walk_access(expr, alias, privates, work, remaining, guarded));
        if matches!(result, Access::Unknown) {
          return Access::Unknown;
        }
      }
      match result {
        Access::None => Access::Unknown,
        other => other,
      }
    }
    _ => Access::Unknown,
  }
}

fn private_field_hit(
  member: &PrivateFieldExpression<'_>,
  alias: Option<&str>,
  privates: &HashMap<String, Span>,
  work: &WorkCounter,
  remaining: u8,
  guarded: bool,
) -> Access {
  if member.optional {
    return Access::Unknown;
  }
  work.add_key_lookups(1);
  let field = member.field.name.as_str();
  if !privates.contains_key(field) {
    return Access::None;
  }
  if is_this_or_alias(&member.object, alias) {
    if guarded {
      Access::None
    } else {
      work.add_key_copies(1);
      Access::Hit { field: field.to_string(), span: member.span }
    }
  } else {
    match walk_access(&member.object, alias, privates, work, remaining, guarded) {
      Access::Unknown => Access::Unknown,
      Access::Hit { field, span } if !guarded => Access::Hit { field, span },
      _ => Access::None,
    }
  }
}

fn merge_unconditional(left: Access, right: Access) -> Access {
  match (left, right) {
    (Access::Unknown, _) | (_, Access::Unknown) => Access::Unknown,
    (Access::Hit { field, span }, Access::Hit { field: other, span: other_span }) => {
      if span.start <= other_span.start {
        Access::Hit { field, span }
      } else {
        Access::Hit { field: other, span: other_span }
      }
    }
    (Access::Hit { field, span }, _) | (_, Access::Hit { field, span }) => {
      Access::Hit { field, span }
    }
    (Access::None, Access::None) => Access::None,
  }
}

fn is_this_or_alias(expression: &Expression<'_>, alias: Option<&str>) -> bool {
  match expression.get_inner_expression() {
    Expression::ThisExpression(_) => true,
    Expression::Identifier(identifier) => {
      alias.is_some_and(|name| identifier.name.as_str() == name)
    }
    _ => false,
  }
}
