//! Closed evaluation grammar for customRef track/trigger proofs.
//!
//! Storage, setter-identity, and subscribed-prefix proofs share the canonical
//! executed forms below. Every unsupported evaluation shape is Unknown: do not
//! collect a parent node while dropping its evaluated children. `classify_reach`
//! stays in `proof` as the ancestor-kind walk.

use oxc_ast::{
  AstKind,
  ast::{
    AssignmentTarget, AssignmentTargetMaybeDefault, AssignmentTargetProperty, BindingPattern,
    Expression, FunctionBody, IdentifierReference, LogicalOperator, ObjectProperty,
    ObjectPropertyKind, PropertyKind, SimpleAssignmentTarget, Statement, UnaryOperator,
  },
};
use oxc_semantic::{NodeId, SymbolId};
use oxc_span::{GetSpan, Span};

use super::stats::WorkCounter;

const SUSPEND_BUDGET: u8 = 64;
const GRAMMAR_BUDGET: u8 = 64;

fn identifier_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  identifier: &IdentifierReference<'_>,
) -> Option<SymbolId> {
  let reference_id = identifier.reference_id.get()?;
  semantic.scoping().get_reference(reference_id).symbol_id()
}

const fn consume_grammar(budget: &mut u8) -> bool {
  if *budget == 0 {
    return false;
  }
  *budget = budget.saturating_sub(1);
  true
}

/// Proven boolean used to take one conditional/logical arm. Sequences yield the
/// last expression; anything else is Unknown.
pub(super) fn proven_bool(expression: &Expression<'_>, work: &WorkCounter) -> Option<bool> {
  let mut budget = GRAMMAR_BUDGET;
  proven_bool_inner(expression, work, &mut budget)
}

fn proven_bool_inner(
  expression: &Expression<'_>,
  work: &WorkCounter,
  budget: &mut u8,
) -> Option<bool> {
  if !consume_grammar(budget) {
    return None;
  }
  work.add_nodes(1);
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(literal) => Some(literal.value),
    Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
      proven_bool_inner(&unary.argument, work, budget).map(|value| !value)
    }
    Expression::SequenceExpression(sequence) => {
      sequence.expressions.last().and_then(|item| proven_bool_inner(item, work, budget))
    }
    _ => None,
  }
}

fn proven_nullish(
  expression: &Expression<'_>,
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
) -> Option<bool> {
  let mut budget = GRAMMAR_BUDGET;
  proven_nullish_inner(expression, semantic, work, &mut budget)
}

fn proven_nullish_inner(
  expression: &Expression<'_>,
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
  budget: &mut u8,
) -> Option<bool> {
  if !consume_grammar(budget) {
    return None;
  }
  work.add_nodes(1);
  match expression.get_inner_expression() {
    Expression::NullLiteral(_) => Some(true),
    Expression::Identifier(identifier)
      if identifier.name.as_str() == "undefined"
        && identifier_symbol(semantic, identifier).is_none() =>
    {
      Some(true)
    }
    Expression::BooleanLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::RegExpLiteral(_) => Some(false),
    Expression::SequenceExpression(sequence) => sequence
      .expressions
      .last()
      .and_then(|item| proven_nullish_inner(item, semantic, work, budget)),
    _ => None,
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TakenBranch {
  Consequent,
  Alternate,
  Unknown,
}

pub(super) fn taken_conditional_branch(test: &Expression<'_>, work: &WorkCounter) -> TakenBranch {
  match proven_bool(test, work) {
    Some(true) => TakenBranch::Consequent,
    Some(false) => TakenBranch::Alternate,
    None => TakenBranch::Unknown,
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TakenLogical {
  LeftOnly,
  Both,
  Unknown,
}

pub(super) fn taken_logical_side(
  operator: LogicalOperator,
  left: &Expression<'_>,
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
) -> TakenLogical {
  match operator {
    LogicalOperator::And => match proven_bool(left, work) {
      Some(true) => TakenLogical::Both,
      Some(false) => TakenLogical::LeftOnly,
      None => TakenLogical::Unknown,
    },
    LogicalOperator::Or => match proven_bool(left, work) {
      Some(true) => TakenLogical::LeftOnly,
      Some(false) => TakenLogical::Both,
      None => TakenLogical::Unknown,
    },
    LogicalOperator::Coalesce => match proven_nullish(left, semantic, work) {
      Some(true) => TakenLogical::Both,
      Some(false) => TakenLogical::LeftOnly,
      None => TakenLogical::Unknown,
    },
  }
}

/// Function/arrow bodies have their own owner. Class evaluation is eager
/// (heritage, computed keys, static blocks) and stays Unknown in this grammar.
pub(super) fn is_nested_callable(expression: &Expression<'_>) -> bool {
  matches!(
    expression.get_inner_expression(),
    Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
  )
}

pub(super) fn object_value_is_eager(property: &ObjectProperty<'_>) -> bool {
  property.kind == PropertyKind::Init && !property.method && !is_nested_callable(&property.value)
}

pub(super) fn computed_key_expression<'a>(
  property: &'a ObjectProperty<'a>,
) -> Option<&'a Expression<'a>> {
  if property.computed { property.key.as_expression() } else { None }
}

pub(super) fn callable_body<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  node_id: NodeId,
) -> Option<&'a FunctionBody<'a>> {
  match semantic.nodes().kind(node_id) {
    AstKind::Function(function) => function.body.as_deref(),
    AstKind::ArrowFunctionExpression(arrow) => Some(&arrow.body),
    _ => None,
  }
}

/// True when a prior return/throw/unknown statement makes `read` unexecuted.
pub(super) fn preceding_statement_blocks_read(
  statements: &[Statement<'_>],
  read: Span,
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
) -> bool {
  preceding_execution_blocks_read(statements, read, semantic, work)
}

/// True when a prior exit, unknown control, or executed await/yield ends the
/// subscribed synchronous prefix before `read`. Nested function bodies are not
/// executed here; source5 reach classification stays on ancestor kinds.
fn preceding_execution_blocks_read(
  statements: &[Statement<'_>],
  read: Span,
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
) -> bool {
  for statement in statements {
    work.add_nodes(1);
    let span = statement.span();
    if span.start <= read.start && read.end <= span.end {
      return statement_is_unknown_control(statement)
        || statement_suspends_before(statement, read, semantic, work);
    }
    if span.end <= read.start {
      let mut exit_budget = GRAMMAR_BUDGET;
      if statement_exits_or_unknown(statement, work, &mut exit_budget)
        || statement_suspends(statement, semantic, work)
      {
        return true;
      }
    }
  }
  true
}

fn statement_exits_or_unknown(
  statement: &Statement<'_>,
  work: &WorkCounter,
  budget: &mut u8,
) -> bool {
  if !consume_grammar(budget) {
    return true;
  }
  work.add_nodes(1);
  match statement {
    Statement::BlockStatement(block) => {
      block.body.iter().any(|inner| statement_exits_or_unknown(inner, work, budget))
    }
    Statement::ReturnStatement(_)
    | Statement::ThrowStatement(_)
    | Statement::BreakStatement(_)
    | Statement::ContinueStatement(_)
    | Statement::IfStatement(_)
    | Statement::ForStatement(_)
    | Statement::ForInStatement(_)
    | Statement::ForOfStatement(_)
    | Statement::WhileStatement(_)
    | Statement::DoWhileStatement(_)
    | Statement::SwitchStatement(_)
    | Statement::TryStatement(_)
    | Statement::WithStatement(_)
    | Statement::LabeledStatement(_) => true,
    _ => false,
  }
}

const fn statement_is_unknown_control(statement: &Statement<'_>) -> bool {
  matches!(
    statement,
    Statement::IfStatement(_)
      | Statement::ForStatement(_)
      | Statement::ForInStatement(_)
      | Statement::ForOfStatement(_)
      | Statement::WhileStatement(_)
      | Statement::DoWhileStatement(_)
      | Statement::SwitchStatement(_)
      | Statement::TryStatement(_)
      | Statement::WithStatement(_)
      | Statement::LabeledStatement(_)
  )
}

fn statement_suspends(
  statement: &Statement<'_>,
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
) -> bool {
  let mut budget = SUSPEND_BUDGET;
  statement_has_suspension(statement, None, semantic, work, &mut budget)
}

fn statement_suspends_before(
  statement: &Statement<'_>,
  read: Span,
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
) -> bool {
  let mut budget = SUSPEND_BUDGET;
  statement_has_suspension(statement, Some(read), semantic, work, &mut budget)
}

fn statement_has_suspension(
  statement: &Statement<'_>,
  read: Option<Span>,
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
  budget: &mut u8,
) -> bool {
  work.add_nodes(1);
  match statement {
    Statement::ExpressionStatement(statement) => {
      expression_suspends(&statement.expression, read, semantic, work, budget)
    }
    Statement::VariableDeclaration(declaration) => {
      declaration.declarations.iter().any(|declarator| {
        binding_pattern_suspends(&declarator.id, read, semantic, work, budget)
          || declarator
            .init
            .as_ref()
            .is_some_and(|init| expression_suspends(init, read, semantic, work, budget))
      })
    }
    Statement::ReturnStatement(ret) => ret
      .argument
      .as_ref()
      .is_some_and(|argument| expression_suspends(argument, read, semantic, work, budget)),
    Statement::BlockStatement(block) => block.body.iter().any(|inner| match read {
      Some(read) if inner.span().end <= read.start => {
        let mut exit_budget = GRAMMAR_BUDGET;
        statement_exits_or_unknown(inner, work, &mut exit_budget)
          || statement_has_suspension(inner, None, semantic, work, budget)
      }
      Some(read) if inner.span().start <= read.start && read.end <= inner.span().end => {
        statement_has_suspension(inner, Some(read), semantic, work, budget)
      }
      Some(_) => false,
      None => {
        let mut exit_budget = GRAMMAR_BUDGET;
        statement_exits_or_unknown(inner, work, &mut exit_budget)
          || statement_has_suspension(inner, None, semantic, work, budget)
      }
    }),
    _ => false,
  }
}

fn expression_suspends(
  expression: &Expression<'_>,
  read: Option<Span>,
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
  budget: &mut u8,
) -> bool {
  if *budget == 0 {
    return true;
  }
  *budget = budget.saturating_sub(1);
  work.add_nodes(1);
  if let Some(read) = read
    && expression.span().end <= read.start
  {
    return expression_suspends(expression, None, semantic, work, budget);
  }
  if let Some(read) = read
    && expression.span().start >= read.end
  {
    return false;
  }
  #[expect(
    clippy::match_same_arms,
    reason = "await/yield are named canonical suspends; the wildcard is every unsupported form"
  )]
  match expression.get_inner_expression() {
    Expression::AwaitExpression(_) | Expression::YieldExpression(_) => true,
    Expression::BooleanLiteral(_)
    | Expression::NullLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::RegExpLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::Identifier(_)
    | Expression::ThisExpression(_)
    | Expression::Super(_)
    | Expression::ImportMeta(_)
    | Expression::NewTarget(_)
    | Expression::ArrowFunctionExpression(_)
    | Expression::FunctionExpression(_) => false,
    Expression::SequenceExpression(sequence) => sequence
      .expressions
      .iter()
      .any(|item| expression_suspends(item, read, semantic, work, budget)),
    Expression::UnaryExpression(unary) => {
      expression_suspends(&unary.argument, read, semantic, work, budget)
    }
    Expression::UpdateExpression(update) => {
      simple_assignment_target_suspends(&update.argument, read, semantic, work, budget)
    }
    Expression::BinaryExpression(binary) => {
      expression_suspends(&binary.left, read, semantic, work, budget)
        || expression_suspends(&binary.right, read, semantic, work, budget)
    }
    Expression::LogicalExpression(logical) => {
      expression_suspends(&logical.left, read, semantic, work, budget)
        || match taken_logical_side(logical.operator, &logical.left, semantic, work) {
          TakenLogical::Both => expression_suspends(&logical.right, read, semantic, work, budget),
          TakenLogical::LeftOnly => false,
          TakenLogical::Unknown => true,
        }
    }
    Expression::ConditionalExpression(conditional) => {
      expression_suspends(&conditional.test, read, semantic, work, budget)
        || match taken_conditional_branch(&conditional.test, work) {
          TakenBranch::Consequent => {
            expression_suspends(&conditional.consequent, read, semantic, work, budget)
          }
          TakenBranch::Alternate => {
            expression_suspends(&conditional.alternate, read, semantic, work, budget)
          }
          TakenBranch::Unknown => true,
        }
    }
    Expression::AssignmentExpression(assignment) => {
      assignment_target_suspends(&assignment.left, read, semantic, work, budget)
        || expression_suspends(&assignment.right, read, semantic, work, budget)
    }
    Expression::CallExpression(call) => {
      expression_suspends(&call.callee, read, semantic, work, budget)
        || call.arguments.iter().any(|argument| match argument {
          oxc_ast::ast::Argument::SpreadElement(spread) => {
            expression_suspends(&spread.argument, read, semantic, work, budget)
          }
          other => other
            .as_expression()
            .is_some_and(|expr| expression_suspends(expr, read, semantic, work, budget)),
        })
    }
    Expression::NewExpression(new_expr) => {
      expression_suspends(&new_expr.callee, read, semantic, work, budget)
        || new_expr.arguments.iter().any(|argument| match argument {
          oxc_ast::ast::Argument::SpreadElement(spread) => {
            expression_suspends(&spread.argument, read, semantic, work, budget)
          }
          other => other
            .as_expression()
            .is_some_and(|expr| expression_suspends(expr, read, semantic, work, budget)),
        })
    }
    Expression::StaticMemberExpression(member) => {
      expression_suspends(&member.object, read, semantic, work, budget)
    }
    Expression::ComputedMemberExpression(member) => {
      expression_suspends(&member.object, read, semantic, work, budget)
        || expression_suspends(&member.expression, read, semantic, work, budget)
    }
    Expression::PrivateFieldExpression(member) => {
      expression_suspends(&member.object, read, semantic, work, budget)
    }
    Expression::ArrayExpression(array) => array.elements.iter().any(|element| match element {
      oxc_ast::ast::ArrayExpressionElement::SpreadElement(spread) => {
        expression_suspends(&spread.argument, read, semantic, work, budget)
      }
      oxc_ast::ast::ArrayExpressionElement::Elision(_) => false,
      other => other
        .as_expression()
        .is_some_and(|expr| expression_suspends(expr, read, semantic, work, budget)),
    }),
    Expression::ObjectExpression(object) => {
      object.properties.iter().any(|property| match property {
        ObjectPropertyKind::ObjectProperty(property) => {
          let key_suspends = match computed_key_expression(property) {
            Some(key) => expression_suspends(key, read, semantic, work, budget),
            None if property.computed => true,
            None => false,
          };
          key_suspends
            || (object_value_is_eager(property)
              && expression_suspends(&property.value, read, semantic, work, budget))
        }
        ObjectPropertyKind::SpreadProperty(spread) => {
          expression_suspends(&spread.argument, read, semantic, work, budget)
        }
      })
    }
    Expression::TemplateLiteral(template) => template
      .expressions
      .iter()
      .any(|item| expression_suspends(item, read, semantic, work, budget)),
    Expression::ParenthesizedExpression(inner) => {
      expression_suspends(&inner.expression, read, semantic, work, budget)
    }
    _ => true,
  }
}

fn simple_assignment_target_suspends(
  target: &SimpleAssignmentTarget<'_>,
  read: Option<Span>,
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
  budget: &mut u8,
) -> bool {
  match target {
    SimpleAssignmentTarget::AssignmentTargetIdentifier(_) => false,
    SimpleAssignmentTarget::TSAsExpression(inner) => {
      expression_suspends(&inner.expression, read, semantic, work, budget)
    }
    SimpleAssignmentTarget::TSSatisfiesExpression(inner) => {
      expression_suspends(&inner.expression, read, semantic, work, budget)
    }
    SimpleAssignmentTarget::TSNonNullExpression(inner) => {
      expression_suspends(&inner.expression, read, semantic, work, budget)
    }
    SimpleAssignmentTarget::TSTypeAssertion(inner) => {
      expression_suspends(&inner.expression, read, semantic, work, budget)
    }
    SimpleAssignmentTarget::StaticMemberExpression(member) => {
      expression_suspends(&member.object, read, semantic, work, budget)
    }
    SimpleAssignmentTarget::ComputedMemberExpression(member) => {
      expression_suspends(&member.object, read, semantic, work, budget)
        || expression_suspends(&member.expression, read, semantic, work, budget)
    }
    SimpleAssignmentTarget::PrivateFieldExpression(member) => {
      expression_suspends(&member.object, read, semantic, work, budget)
    }
  }
}

fn assignment_target_suspends(
  target: &AssignmentTarget<'_>,
  read: Option<Span>,
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
  budget: &mut u8,
) -> bool {
  match target {
    AssignmentTarget::AssignmentTargetIdentifier(_) => false,
    AssignmentTarget::TSAsExpression(inner) => {
      expression_suspends(&inner.expression, read, semantic, work, budget)
    }
    AssignmentTarget::TSSatisfiesExpression(inner) => {
      expression_suspends(&inner.expression, read, semantic, work, budget)
    }
    AssignmentTarget::TSNonNullExpression(inner) => {
      expression_suspends(&inner.expression, read, semantic, work, budget)
    }
    AssignmentTarget::TSTypeAssertion(inner) => {
      expression_suspends(&inner.expression, read, semantic, work, budget)
    }
    AssignmentTarget::StaticMemberExpression(member) => {
      expression_suspends(&member.object, read, semantic, work, budget)
    }
    AssignmentTarget::ComputedMemberExpression(member) => {
      expression_suspends(&member.object, read, semantic, work, budget)
        || expression_suspends(&member.expression, read, semantic, work, budget)
    }
    AssignmentTarget::PrivateFieldExpression(member) => {
      expression_suspends(&member.object, read, semantic, work, budget)
    }
    AssignmentTarget::ArrayAssignmentTarget(array) => {
      array
        .elements
        .iter()
        .flatten()
        .any(|element| assignment_maybe_default_suspends(element, read, semantic, work, budget))
        || array.rest.as_ref().is_some_and(|rest| {
          assignment_target_suspends(&rest.target, read, semantic, work, budget)
        })
    }
    AssignmentTarget::ObjectAssignmentTarget(object) => {
      object.properties.iter().any(|property| match property {
        AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(property) => property
          .init
          .as_ref()
          .is_some_and(|init| expression_suspends(init, read, semantic, work, budget)),
        AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) => {
          let key_suspends = if property.computed {
            property
              .name
              .as_expression()
              .is_none_or(|key| expression_suspends(key, read, semantic, work, budget))
          } else {
            false
          };
          key_suspends
            || assignment_maybe_default_suspends(&property.binding, read, semantic, work, budget)
        }
      }) || object
        .rest
        .as_ref()
        .is_some_and(|rest| assignment_target_suspends(&rest.target, read, semantic, work, budget))
    }
  }
}

fn assignment_maybe_default_suspends(
  target: &AssignmentTargetMaybeDefault<'_>,
  read: Option<Span>,
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
  budget: &mut u8,
) -> bool {
  match target {
    AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(inner) => {
      assignment_target_suspends(&inner.binding, read, semantic, work, budget)
        || expression_suspends(&inner.init, read, semantic, work, budget)
    }
    other => other
      .as_assignment_target()
      .is_some_and(|target| assignment_target_suspends(target, read, semantic, work, budget)),
  }
}

fn binding_pattern_suspends(
  pattern: &BindingPattern<'_>,
  read: Option<Span>,
  semantic: &oxc_semantic::Semantic<'_>,
  work: &WorkCounter,
  budget: &mut u8,
) -> bool {
  work.add_nodes(1);
  match pattern {
    BindingPattern::BindingIdentifier(_) => false,
    BindingPattern::AssignmentPattern(inner) => {
      binding_pattern_suspends(&inner.left, read, semantic, work, budget)
        || expression_suspends(&inner.right, read, semantic, work, budget)
    }
    BindingPattern::ObjectPattern(object) => {
      object.properties.iter().any(|property| {
        let key_suspends = if property.computed {
          property
            .key
            .as_expression()
            .is_none_or(|key| expression_suspends(key, read, semantic, work, budget))
        } else {
          false
        };
        key_suspends || binding_pattern_suspends(&property.value, read, semantic, work, budget)
      }) || object
        .rest
        .as_ref()
        .is_some_and(|rest| binding_pattern_suspends(&rest.argument, read, semantic, work, budget))
    }
    BindingPattern::ArrayPattern(array) => {
      array
        .elements
        .iter()
        .flatten()
        .any(|element| binding_pattern_suspends(element, read, semantic, work, budget))
        || array.rest.as_ref().is_some_and(|rest| {
          binding_pattern_suspends(&rest.argument, read, semantic, work, budget)
        })
    }
  }
}
