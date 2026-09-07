//! Bounded demand reachability, assignment-target role, and key/capability policy.

use oxc_ast::{
  AstKind,
  ast::{
    AssignmentOperator, AssignmentTarget, BindingPattern, Expression, FormalParameters,
    FunctionBody, IdentifierReference, PropertyKey, SimpleAssignmentTarget, Statement,
    UnaryOperator, UpdateOperator,
  },
};
use oxc_semantic::{NodeId, SymbolId};
use oxc_span::{GetSpan, Span};

use super::stats::WorkCounter;

pub(super) const ANCESTOR_BUDGET: u8 = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Reach {
  Straight,
  Guarded,
  Unknown,
}

impl Reach {
  pub(super) const fn is_straight(self) -> bool {
    matches!(self, Self::Straight)
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DemandRole {
  Read,
  Write,
  ReadWrite,
  Delete,
  Other,
}

impl DemandRole {
  pub(super) const fn needs_get(self) -> bool {
    matches!(self, Self::Read | Self::ReadWrite)
  }

  pub(super) const fn needs_set(self) -> bool {
    matches!(self, Self::Write | Self::ReadWrite)
  }
}

/// Construction / run / demand owner: the function (or program) plus its
/// straight-line body region. Innermost blocks stay on `Owner.block` for
/// source5 statement sites.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DemandOrigin {
  pub callable: Option<NodeId>,
  pub region: NodeId,
  pub offset: usize,
}

/// Local callable body vs the `customRef` impl receiver (`this`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReceiverEffect {
  Closed,
  Uncertain,
}

pub(super) fn classify_reach(
  semantic: &oxc_semantic::Semantic<'_>,
  mut node_id: NodeId,
  work: &WorkCounter,
) -> Reach {
  for _ in 0..ANCESTOR_BUDGET {
    work.add_queries(1);
    let parent = semantic.nodes().parent_id(node_id);
    let current_span = semantic.nodes().kind(node_id).span();
    match semantic.nodes().kind(parent) {
      AstKind::Program(_) | AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
        return Reach::Straight;
      }
      AstKind::LogicalExpression(logical) => {
        if span_covers(logical.left.span(), current_span) {
          node_id = parent;
          continue;
        }
        return Reach::Guarded;
      }
      AstKind::ConditionalExpression(conditional) => {
        if span_covers(conditional.test.span(), current_span) {
          node_id = parent;
          continue;
        }
        return Reach::Guarded;
      }
      AstKind::IfStatement(statement) => {
        if span_covers(statement.test.span(), current_span) {
          node_id = parent;
          continue;
        }
        return Reach::Guarded;
      }
      AstKind::ForStatement(_)
      | AstKind::ForInStatement(_)
      | AstKind::ForOfStatement(_)
      | AstKind::WhileStatement(_)
      | AstKind::DoWhileStatement(_)
      | AstKind::SwitchStatement(_)
      | AstKind::TryStatement(_)
      | AstKind::AwaitExpression(_)
      | AstKind::YieldExpression(_)
      | AstKind::ChainExpression(_)
      | AstKind::AssignmentPattern(_) => return Reach::Guarded,
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_)
      | AstKind::ExpressionStatement(_)
      | AstKind::VariableDeclarator(_)
      | AstKind::VariableDeclaration(_)
      | AstKind::BlockStatement(_)
      | AstKind::FunctionBody(_)
      | AstKind::StaticMemberExpression(_)
      | AstKind::ComputedMemberExpression(_)
      | AstKind::CallExpression(_)
      | AstKind::NewExpression(_)
      | AstKind::UnaryExpression(_)
      | AstKind::UpdateExpression(_)
      | AstKind::AssignmentExpression(_)
      | AstKind::SequenceExpression(_)
      | AstKind::ArrayExpression(_)
      | AstKind::ObjectExpression(_)
      | AstKind::SpreadElement(_)
      | AstKind::ReturnStatement(_) => {
        node_id = parent;
      }
      _ => return Reach::Unknown,
    }
  }
  Reach::Unknown
}

pub(super) fn classify_role(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  work: &WorkCounter,
) -> DemandRole {
  work.add_queries(1);
  let parent = skip_ts_parent(semantic, node_id, work);
  match semantic.nodes().kind(parent) {
    AstKind::AssignmentExpression(assignment) => {
      if assignment_left_is_node(&assignment.left, semantic.nodes().kind(node_id).span()) {
        if assignment.operator == AssignmentOperator::Assign {
          DemandRole::Write
        } else {
          DemandRole::ReadWrite
        }
      } else {
        DemandRole::Read
      }
    }
    AstKind::UpdateExpression(update) => match update.operator {
      UpdateOperator::Increment | UpdateOperator::Decrement => DemandRole::ReadWrite,
    },
    AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::Delete => {
      DemandRole::Delete
    }
    _ => DemandRole::Read,
  }
}

pub(super) fn is_global_undefined(
  identifier: &IdentifierReference<'_>,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
) -> bool {
  identifier.name.as_str() == "undefined" && symbol_of(identifier).is_none()
}

pub(super) fn is_object_prototype_key(key: &str) -> bool {
  matches!(
    key,
    "constructor"
      | "hasOwnProperty"
      | "isPrototypeOf"
      | "propertyIsEnumerable"
      | "toLocaleString"
      | "toString"
      | "valueOf"
      | "__defineGetter__"
      | "__defineSetter__"
      | "__lookupGetter__"
      | "__lookupSetter__"
      | "__proto__"
  )
}

pub(super) fn is_custom_prototype_key(key: &str) -> bool {
  key == "__proto__"
}

fn assignment_left_is_node(left: &AssignmentTarget<'_>, node_span: Span) -> bool {
  left.span() == node_span || span_covers(left.span(), node_span)
}

const fn span_covers(outer: Span, inner: Span) -> bool {
  outer.start <= inner.start && inner.end <= outer.end
}

fn skip_ts_parent(
  semantic: &oxc_semantic::Semantic<'_>,
  mut node_id: NodeId,
  work: &WorkCounter,
) -> NodeId {
  for _ in 0..ANCESTOR_BUDGET {
    work.add_queries(1);
    let parent = semantic.nodes().parent_id(node_id);
    match semantic.nodes().kind(parent) {
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::TSTypeAssertion(_) => node_id = parent,
      _ => return parent,
    }
  }
  node_id
}

pub(super) fn expression_is_noncallable_literal(
  expression: &Expression<'_>,
  symbol_of: impl Fn(&IdentifierReference<'_>) -> Option<SymbolId>,
) -> bool {
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::NullLiteral(_) => true,
    Expression::Identifier(ident) => is_global_undefined(ident, symbol_of),
    _ => false,
  }
}

/// Conservative closed-body check: receiver mutation, computed receiver
/// mutation, receiver escape, unknown receiver calls/tags, and helper
/// delegation are Uncertain. Ordinary local reads/writes stay Closed.
/// Exhausted budget fails closed-to-Uncertain.
pub(super) fn callable_receiver_effect(
  expression: &Expression<'_>,
  work: &WorkCounter,
) -> ReceiverEffect {
  match expression.get_inner_expression() {
    Expression::ArrowFunctionExpression(arrow) => {
      let params = walk_params(&arrow.params, work, ANCESTOR_BUDGET);
      if params == ReceiverEffect::Uncertain {
        return ReceiverEffect::Uncertain;
      }
      walk_body(&arrow.body, work, ANCESTOR_BUDGET.saturating_sub(1))
    }
    Expression::FunctionExpression(function) => {
      let Some(body) = function.body.as_ref() else {
        return ReceiverEffect::Uncertain;
      };
      let params = walk_params(&function.params, work, ANCESTOR_BUDGET);
      if params == ReceiverEffect::Uncertain {
        return ReceiverEffect::Uncertain;
      }
      walk_body(body, work, ANCESTOR_BUDGET.saturating_sub(1))
    }
    _ => ReceiverEffect::Uncertain,
  }
}

fn walk_body(body: &FunctionBody<'_>, work: &WorkCounter, remaining: u8) -> ReceiverEffect {
  let mut remaining = remaining;
  for statement in &body.statements {
    match walk_statement(statement, work, remaining) {
      ReceiverEffect::Closed => remaining = remaining.saturating_sub(1),
      ReceiverEffect::Uncertain => return ReceiverEffect::Uncertain,
    }
    if remaining == 0 {
      return ReceiverEffect::Uncertain;
    }
  }
  ReceiverEffect::Closed
}

fn walk_statement(statement: &Statement<'_>, work: &WorkCounter, remaining: u8) -> ReceiverEffect {
  work.add_queries(1);
  if remaining == 0 {
    return ReceiverEffect::Uncertain;
  }
  match statement {
    Statement::BlockStatement(block) => {
      let mut remaining = remaining.saturating_sub(1);
      for nested in &block.body {
        match walk_statement(nested, work, remaining) {
          ReceiverEffect::Closed => remaining = remaining.saturating_sub(1),
          ReceiverEffect::Uncertain => return ReceiverEffect::Uncertain,
        }
        if remaining == 0 {
          return ReceiverEffect::Uncertain;
        }
      }
      ReceiverEffect::Closed
    }
    Statement::ExpressionStatement(statement) => {
      walk_expr(&statement.expression, work, remaining.saturating_sub(1))
    }
    Statement::ReturnStatement(statement) => {
      statement.argument.as_ref().map_or(ReceiverEffect::Closed, |argument| {
        walk_expr(argument, work, remaining.saturating_sub(1))
      })
    }
    Statement::VariableDeclaration(declaration) => {
      let mut remaining = remaining.saturating_sub(1);
      for declarator in &declaration.declarations {
        match walk_binding_pattern(&declarator.id, work, remaining) {
          ReceiverEffect::Closed => remaining = remaining.saturating_sub(1),
          ReceiverEffect::Uncertain => return ReceiverEffect::Uncertain,
        }
        if remaining == 0 {
          return ReceiverEffect::Uncertain;
        }
        if let Some(init) = &declarator.init {
          match walk_expr(init, work, remaining) {
            ReceiverEffect::Closed => remaining = remaining.saturating_sub(1),
            ReceiverEffect::Uncertain => return ReceiverEffect::Uncertain,
          }
          if remaining == 0 {
            return ReceiverEffect::Uncertain;
          }
        }
      }
      ReceiverEffect::Closed
    }
    Statement::FunctionDeclaration(_) | Statement::EmptyStatement(_) => ReceiverEffect::Closed,
    _ => ReceiverEffect::Uncertain,
  }
}

fn walk_expr(expression: &Expression<'_>, work: &WorkCounter, remaining: u8) -> ReceiverEffect {
  work.add_queries(1);
  if remaining == 0 {
    return ReceiverEffect::Uncertain;
  }
  let remaining = remaining.saturating_sub(1);
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::NullLiteral(_)
    | Expression::RegExpLiteral(_)
    | Expression::Identifier(_)
    | Expression::Super(_)
    | Expression::ImportMeta(_)
    | Expression::NewTarget(_) => ReceiverEffect::Closed,
    Expression::UnaryExpression(unary) => walk_expr(&unary.argument, work, remaining),
    Expression::UpdateExpression(update) => {
      simple_assignment_target_effect(&update.argument, work, remaining)
    }
    Expression::BinaryExpression(binary) => join_effect(
      walk_expr(&binary.left, work, remaining),
      walk_expr(&binary.right, work, remaining),
    ),
    Expression::LogicalExpression(logical) => join_effect(
      walk_expr(&logical.left, work, remaining),
      walk_expr(&logical.right, work, remaining),
    ),
    Expression::AssignmentExpression(assignment) => join_effect(
      assignment_target_effect(&assignment.left, work, remaining),
      walk_expr(&assignment.right, work, remaining),
    ),
    Expression::StaticMemberExpression(member) => walk_expr(&member.object, work, remaining),
    Expression::ComputedMemberExpression(member) => join_effect(
      walk_expr(&member.object, work, remaining),
      walk_expr(&member.expression, work, remaining),
    ),
    Expression::PrivateFieldExpression(member) => walk_expr(&member.object, work, remaining),
    Expression::ConditionalExpression(conditional) => join_effect(
      walk_expr(&conditional.test, work, remaining),
      join_effect(
        walk_expr(&conditional.consequent, work, remaining),
        walk_expr(&conditional.alternate, work, remaining),
      ),
    ),
    Expression::SequenceExpression(sequence) => {
      walk_expr_list(&sequence.expressions, work, remaining)
    }
    Expression::ArrayExpression(array) => {
      let mut remaining = remaining;
      for element in &array.elements {
        let Some(expr) = element.as_expression() else {
          return ReceiverEffect::Uncertain;
        };
        match walk_expr(expr, work, remaining) {
          ReceiverEffect::Closed => remaining = remaining.saturating_sub(1),
          ReceiverEffect::Uncertain => return ReceiverEffect::Uncertain,
        }
        if remaining == 0 {
          return ReceiverEffect::Uncertain;
        }
      }
      ReceiverEffect::Closed
    }
    Expression::ObjectExpression(object) => {
      let mut remaining = remaining;
      for property in &object.properties {
        match property {
          oxc_ast::ast::ObjectPropertyKind::SpreadProperty(spread) => {
            match walk_expr(&spread.argument, work, remaining) {
              ReceiverEffect::Closed => remaining = remaining.saturating_sub(1),
              ReceiverEffect::Uncertain => return ReceiverEffect::Uncertain,
            }
          }
          oxc_ast::ast::ObjectPropertyKind::ObjectProperty(prop) => {
            let key_effect = walk_computed_key(&prop.key, prop.computed, work, remaining);
            match join_effect(key_effect, walk_expr(&prop.value, work, remaining)) {
              ReceiverEffect::Closed => remaining = remaining.saturating_sub(1),
              ReceiverEffect::Uncertain => return ReceiverEffect::Uncertain,
            }
          }
        }
        if remaining == 0 {
          return ReceiverEffect::Uncertain;
        }
      }
      ReceiverEffect::Closed
    }
    Expression::ParenthesizedExpression(inner) => walk_expr(&inner.expression, work, remaining),
    Expression::TSAsExpression(inner) => walk_expr(&inner.expression, work, remaining),
    Expression::TSSatisfiesExpression(inner) => walk_expr(&inner.expression, work, remaining),
    Expression::TSNonNullExpression(inner) => walk_expr(&inner.expression, work, remaining),
    Expression::TSTypeAssertion(inner) => walk_expr(&inner.expression, work, remaining),
    Expression::TSInstantiationExpression(inner) => walk_expr(&inner.expression, work, remaining),
    Expression::TemplateLiteral(template) => walk_expr_list(&template.expressions, work, remaining),
    _ => ReceiverEffect::Uncertain,
  }
}

fn walk_params(params: &FormalParameters<'_>, work: &WorkCounter, remaining: u8) -> ReceiverEffect {
  work.add_queries(1);
  if remaining == 0 {
    return ReceiverEffect::Uncertain;
  }
  let mut remaining = remaining.saturating_sub(1);
  for item in &params.items {
    match walk_binding_pattern(&item.pattern, work, remaining) {
      ReceiverEffect::Closed => remaining = remaining.saturating_sub(1),
      ReceiverEffect::Uncertain => return ReceiverEffect::Uncertain,
    }
    if remaining == 0 {
      return ReceiverEffect::Uncertain;
    }
    if let Some(init) = &item.initializer {
      match walk_expr(init, work, remaining) {
        ReceiverEffect::Closed => remaining = remaining.saturating_sub(1),
        ReceiverEffect::Uncertain => return ReceiverEffect::Uncertain,
      }
      if remaining == 0 {
        return ReceiverEffect::Uncertain;
      }
    }
  }
  params.rest.as_ref().map_or(ReceiverEffect::Closed, |rest| {
    walk_binding_pattern(&rest.rest.argument, work, remaining)
  })
}

fn walk_binding_pattern(
  pattern: &BindingPattern<'_>,
  work: &WorkCounter,
  remaining: u8,
) -> ReceiverEffect {
  work.add_queries(1);
  if remaining == 0 {
    return ReceiverEffect::Uncertain;
  }
  let remaining = remaining.saturating_sub(1);
  match pattern {
    BindingPattern::BindingIdentifier(_) => ReceiverEffect::Closed,
    BindingPattern::AssignmentPattern(assign) => join_effect(
      walk_binding_pattern(&assign.left, work, remaining),
      walk_expr(&assign.right, work, remaining),
    ),
    BindingPattern::ObjectPattern(object) => {
      let mut remaining = remaining;
      for property in &object.properties {
        let key_effect = walk_computed_key(&property.key, property.computed, work, remaining);
        match join_effect(key_effect, walk_binding_pattern(&property.value, work, remaining)) {
          ReceiverEffect::Closed => remaining = remaining.saturating_sub(1),
          ReceiverEffect::Uncertain => return ReceiverEffect::Uncertain,
        }
        if remaining == 0 {
          return ReceiverEffect::Uncertain;
        }
      }
      object.rest.as_ref().map_or(ReceiverEffect::Closed, |rest| {
        walk_binding_pattern(&rest.argument, work, remaining)
      })
    }
    BindingPattern::ArrayPattern(array) => {
      let mut remaining = remaining;
      for element in &array.elements {
        let Some(nested) = element else {
          continue;
        };
        match walk_binding_pattern(nested, work, remaining) {
          ReceiverEffect::Closed => remaining = remaining.saturating_sub(1),
          ReceiverEffect::Uncertain => return ReceiverEffect::Uncertain,
        }
        if remaining == 0 {
          return ReceiverEffect::Uncertain;
        }
      }
      array.rest.as_ref().map_or(ReceiverEffect::Closed, |rest| {
        walk_binding_pattern(&rest.argument, work, remaining)
      })
    }
  }
}

fn walk_computed_key(
  key: &PropertyKey<'_>,
  computed: bool,
  work: &WorkCounter,
  remaining: u8,
) -> ReceiverEffect {
  if !computed {
    return ReceiverEffect::Closed;
  }
  key
    .as_expression()
    .map_or(ReceiverEffect::Uncertain, |expression| walk_expr(expression, work, remaining))
}

fn walk_expr_list(
  expressions: &[Expression<'_>],
  work: &WorkCounter,
  mut remaining: u8,
) -> ReceiverEffect {
  for expr in expressions {
    match walk_expr(expr, work, remaining) {
      ReceiverEffect::Closed => remaining = remaining.saturating_sub(1),
      ReceiverEffect::Uncertain => return ReceiverEffect::Uncertain,
    }
    if remaining == 0 {
      return ReceiverEffect::Uncertain;
    }
  }
  ReceiverEffect::Closed
}

fn assignment_target_effect(
  target: &AssignmentTarget<'_>,
  work: &WorkCounter,
  remaining: u8,
) -> ReceiverEffect {
  work.add_queries(1);
  if remaining == 0 {
    return ReceiverEffect::Uncertain;
  }
  match target {
    AssignmentTarget::AssignmentTargetIdentifier(_) => ReceiverEffect::Closed,
    AssignmentTarget::TSAsExpression(inner) => {
      walk_expr(&inner.expression, work, remaining.saturating_sub(1))
    }
    AssignmentTarget::TSSatisfiesExpression(inner) => {
      walk_expr(&inner.expression, work, remaining.saturating_sub(1))
    }
    AssignmentTarget::TSNonNullExpression(inner) => {
      walk_expr(&inner.expression, work, remaining.saturating_sub(1))
    }
    AssignmentTarget::TSTypeAssertion(inner) => {
      walk_expr(&inner.expression, work, remaining.saturating_sub(1))
    }
    AssignmentTarget::StaticMemberExpression(member) => {
      member_object_effect(&member.object, work, remaining)
    }
    AssignmentTarget::ComputedMemberExpression(member) => join_effect(
      member_object_effect(&member.object, work, remaining),
      walk_expr(&member.expression, work, remaining.saturating_sub(1)),
    ),
    AssignmentTarget::PrivateFieldExpression(member) => {
      member_object_effect(&member.object, work, remaining)
    }
    AssignmentTarget::ArrayAssignmentTarget(_) | AssignmentTarget::ObjectAssignmentTarget(_) => {
      ReceiverEffect::Uncertain
    }
  }
}

fn simple_assignment_target_effect(
  target: &SimpleAssignmentTarget<'_>,
  work: &WorkCounter,
  remaining: u8,
) -> ReceiverEffect {
  work.add_queries(1);
  if remaining == 0 {
    return ReceiverEffect::Uncertain;
  }
  match target {
    SimpleAssignmentTarget::AssignmentTargetIdentifier(_) => ReceiverEffect::Closed,
    SimpleAssignmentTarget::TSAsExpression(inner) => {
      walk_expr(&inner.expression, work, remaining.saturating_sub(1))
    }
    SimpleAssignmentTarget::TSSatisfiesExpression(inner) => {
      walk_expr(&inner.expression, work, remaining.saturating_sub(1))
    }
    SimpleAssignmentTarget::TSNonNullExpression(inner) => {
      walk_expr(&inner.expression, work, remaining.saturating_sub(1))
    }
    SimpleAssignmentTarget::TSTypeAssertion(inner) => {
      walk_expr(&inner.expression, work, remaining.saturating_sub(1))
    }
    SimpleAssignmentTarget::StaticMemberExpression(member) => {
      member_object_effect(&member.object, work, remaining)
    }
    SimpleAssignmentTarget::ComputedMemberExpression(member) => join_effect(
      member_object_effect(&member.object, work, remaining),
      walk_expr(&member.expression, work, remaining.saturating_sub(1)),
    ),
    SimpleAssignmentTarget::PrivateFieldExpression(member) => {
      member_object_effect(&member.object, work, remaining)
    }
  }
}

fn member_object_effect(
  object: &Expression<'_>,
  work: &WorkCounter,
  remaining: u8,
) -> ReceiverEffect {
  if is_this_expr(object) {
    ReceiverEffect::Uncertain
  } else {
    walk_expr(object, work, remaining.saturating_sub(1))
  }
}

const fn join_effect(left: ReceiverEffect, right: ReceiverEffect) -> ReceiverEffect {
  match (left, right) {
    (ReceiverEffect::Closed, ReceiverEffect::Closed) => ReceiverEffect::Closed,
    _ => ReceiverEffect::Uncertain,
  }
}

fn is_this_expr(expression: &Expression<'_>) -> bool {
  matches!(expression.get_inner_expression(), Expression::ThisExpression(_))
}
