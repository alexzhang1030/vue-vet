//! Callback-specific watch contract interpretation (once/immediate and root identity).

use oxc_ast::ast::{
  Argument, BindingPattern, CallExpression, Expression, FormalParameter, FunctionBody, Statement,
  UnaryOperator,
};
use oxc_semantic::SymbolId;
use oxc_span::{GetSpan, Span};
use oxc_syntax::operator::BinaryOperator;
use vue_vet_core::{WatchCallbackContractFact, WatchCallbackContractReason};

use super::index::CallInfo;
use super::shape::{Shape, ShapeHint, is_ref_api, span_key};
use super::{Collector, MAX_DEPTH};

#[derive(Clone, Copy)]
struct WatchOptions {
  once: bool,
  immediate: bool,
}

#[derive(Clone, Copy)]
enum WatchSourceKind {
  Single,
  ReactiveRoot,
}

#[derive(Clone, Copy)]
enum GuardKind {
  UndefinedOld,
  ParamIdentity,
}

struct Callback<'a> {
  new_param: SymbolId,
  old_param: SymbolId,
  new_name: &'a str,
  old_name: &'a str,
  body: &'a FunctionBody<'a>,
}

struct GuardPattern {
  kind: GuardKind,
  guard_span: Span,
}

enum LaterWork {
  Consumes,
  Absent,
  Uncertain,
}

impl Collector<'_> {
  pub(super) fn collect_watch_callback_contracts(
    &mut self,
    call: &CallExpression<'_>,
    info: CallInfo,
  ) {
    if info.has_spread {
      return;
    }
    let Some(source_expr) = call.arguments.first().and_then(Argument::as_expression) else {
      return;
    };
    let Some(callback_expr) = call.arguments.get(1).and_then(Argument::as_expression) else {
      return;
    };
    let Some(callback) = simple_callback(callback_expr) else {
      return;
    };
    self.indexes.note_query();
    if self.indexes.reassigned.contains(&callback.new_param)
      || self.indexes.reassigned.contains(&callback.old_param)
    {
      return;
    }
    let Some(options) = self.closed_watch_options(call) else {
      return;
    };
    let Some((kind, source_span)) = self.classify_watch_source(source_expr) else {
      return;
    };
    let Some(pattern) = self.callback_guard_pattern(&callback) else {
      return;
    };
    let reason = match (kind, options, pattern.kind) {
      (_, WatchOptions { once: true, immediate: true }, GuardKind::UndefinedOld)
        if matches!(kind, WatchSourceKind::Single | WatchSourceKind::ReactiveRoot) =>
      {
        Some(WatchCallbackContractReason::OnceImmediateUndefinedGuard)
      }
      (
        WatchSourceKind::ReactiveRoot,
        WatchOptions { immediate: false, once: false },
        GuardKind::ParamIdentity,
      ) => Some(WatchCallbackContractReason::ReactiveRootIdentityGuard),
      _ => None,
    };
    let Some(reason) = reason else {
      return;
    };
    self.facts.watch_callback_contracts.push(WatchCallbackContractFact {
      watch_span: self.span(call.span),
      source_span: self.span(source_span),
      guard_span: self.span(pattern.guard_span),
      reason,
    });
  }

  fn classify_watch_source(&mut self, source: &Expression<'_>) -> Option<(WatchSourceKind, Span)> {
    let inner = source.get_inner_expression();
    match inner {
      Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
        Some((WatchSourceKind::Single, source.span()))
      }
      Expression::Identifier(identifier) => {
        let shape = self.classify_identifier(identifier, MAX_DEPTH);
        match shape {
          Shape::RefLike | Shape::Function => Some((WatchSourceKind::Single, source.span())),
          Shape::DeepProxy | Shape::ShallowProxy
            if self.identifier_is_closed_reactive_root(identifier) =>
          {
            Some((WatchSourceKind::ReactiveRoot, source.span()))
          }
          _ => None,
        }
      }
      Expression::CallExpression(call) => {
        self.indexes.note_query();
        let info = self.indexes.calls.get(&span_key(call.span)).copied()?;
        if info.has_spread {
          return None;
        }
        match info.api {
          Some(api) if is_ref_api(api) => Some((WatchSourceKind::Single, source.span())),
          Some("reactive" | "shallowReactive") => {
            let argument = info.first_arg?;
            if self.closed_construction(argument, MAX_DEPTH) {
              Some((WatchSourceKind::ReactiveRoot, source.span()))
            } else {
              None
            }
          }
          _ => None,
        }
      }
      _ => None,
    }
  }

  fn identifier_is_closed_reactive_root(
    &mut self,
    identifier: &oxc_ast::ast::IdentifierReference<'_>,
  ) -> bool {
    self.indexes.note_query();
    let Some(symbol_id) = self.reference_symbol(identifier) else {
      return false;
    };
    let root = self.indexes.root_of(symbol_id);
    if self.indexes.construction_mutated(root) {
      return false;
    }
    let Some(init_span) = self.indexes.init_span.get(&root).copied() else {
      return false;
    };
    self.indexes.note_query();
    let Some(hint) = self.indexes.hints.get(&span_key(init_span)).copied() else {
      return false;
    };
    let ShapeHint::Call(call_span) = hint else {
      return false;
    };
    self.indexes.note_query();
    let Some(info) = self.indexes.calls.get(&span_key(call_span)).copied() else {
      return false;
    };
    if !matches!(info.api, Some("reactive" | "shallowReactive")) || info.has_spread {
      return false;
    }
    let Some(argument) = info.first_arg else {
      return false;
    };
    self.closed_construction(argument, MAX_DEPTH)
  }

  fn closed_construction(&mut self, span: Span, remaining: u8) -> bool {
    self.indexes.note_query();
    if remaining == 0 {
      return false;
    }
    if let Some(closed) = self.indexes.closed_object_literal(span) {
      return closed;
    }
    if self.indexes.is_array_literal(span) {
      return true;
    }
    self.indexes.note_query();
    let Some(hint) = self.indexes.hints.get(&span_key(span)).copied() else {
      return false;
    };
    match hint {
      ShapeHint::Identifier(Some(symbol_id), _) => {
        let root = self.indexes.root_of(symbol_id);
        if self.indexes.construction_mutated(root) {
          return false;
        }
        let Some(init_span) = self.indexes.init_span.get(&root).copied() else {
          return false;
        };
        self.closed_construction(init_span, remaining.saturating_sub(1))
      }
      _ => false,
    }
  }

  fn closed_watch_options(&self, call: &CallExpression<'_>) -> Option<WatchOptions> {
    self.indexes.note_query();
    let Some(options_expr) = call.arguments.get(2).and_then(Argument::as_expression) else {
      return Some(WatchOptions { once: false, immediate: false });
    };
    let Expression::ObjectExpression(object) = options_expr.get_inner_expression() else {
      return None;
    };
    let mut once = None;
    let mut immediate = None;
    for property in &object.properties {
      self.indexes.note_query();
      match property {
        oxc_ast::ast::ObjectPropertyKind::SpreadProperty(_) => return None,
        oxc_ast::ast::ObjectPropertyKind::ObjectProperty(prop) => {
          if prop.kind != oxc_ast::ast::PropertyKind::Init || prop.method || prop.shorthand {
            return None;
          }
          let name = prop.key.static_name()?;
          if name == "once" || name == "immediate" {
            let Expression::BooleanLiteral(literal) = prop.value.get_inner_expression() else {
              return None;
            };
            if name == "once" {
              if once.is_some() {
                return None;
              }
              once = Some(literal.value);
            } else {
              if immediate.is_some() {
                return None;
              }
              immediate = Some(literal.value);
            }
          }
        }
      }
    }
    Some(WatchOptions { once: once.unwrap_or(false), immediate: immediate.unwrap_or(false) })
  }

  fn callback_guard_pattern(&self, callback: &Callback<'_>) -> Option<GuardPattern> {
    let statements = callback.body.statements.as_slice();
    let first = statements.first()?;
    if statements.iter().any(|statement| matches!(statement, Statement::TryStatement(_))) {
      self.indexes.add_queries(statements.len() as u64);
      return None;
    }
    self.indexes.note_query();
    let Statement::IfStatement(if_stmt) = first else {
      return None;
    };
    if if_stmt.alternate.is_some() {
      return None;
    }
    if !is_bare_return(&if_stmt.consequent) {
      return None;
    }
    let test = if_stmt.test.get_inner_expression();
    let Expression::BinaryExpression(binary) = test else {
      return None;
    };
    if binary.operator != BinaryOperator::StrictEquality {
      return None;
    }
    let left = binary.left.get_inner_expression();
    let right = binary.right.get_inner_expression();
    let kind = if (is_param(left, callback.old_name, callback.old_param, self)
      && is_pure_undefined(right, self))
      || (is_pure_undefined(left, self)
        && is_param(right, callback.old_name, callback.old_param, self))
    {
      GuardKind::UndefinedOld
    } else if (is_param(left, callback.new_name, callback.new_param, self)
      && is_param(right, callback.old_name, callback.old_param, self))
      || (is_param(left, callback.old_name, callback.old_param, self)
        && is_param(right, callback.new_name, callback.new_param, self))
    {
      GuardKind::ParamIdentity
    } else {
      return None;
    };
    let rest = statements.get(1..)?;
    if rest.is_empty() {
      return None;
    }
    match later_work_consumes(rest, callback.new_param, callback.new_name, self, MAX_DEPTH) {
      LaterWork::Consumes => Some(GuardPattern { kind, guard_span: if_stmt.span }),
      LaterWork::Absent | LaterWork::Uncertain => None,
    }
  }
}

fn simple_callback<'a>(expression: &'a Expression<'a>) -> Option<Callback<'a>> {
  match expression.get_inner_expression() {
    Expression::ArrowFunctionExpression(arrow) => {
      if arrow.expression || arrow.r#async {
        return None;
      }
      let (new_param, old_param, new_name, old_name) = two_simple_params(&arrow.params.items)?;
      if arrow.params.rest.is_some() {
        return None;
      }
      Some(Callback { new_param, old_param, new_name, old_name, body: &arrow.body })
    }
    Expression::FunctionExpression(function) => {
      if function.r#async || function.generator || function.params.rest.is_some() {
        return None;
      }
      let (new_param, old_param, new_name, old_name) = two_simple_params(&function.params.items)?;
      let body = function.body.as_ref()?;
      Some(Callback { new_param, old_param, new_name, old_name, body })
    }
    _ => None,
  }
}

fn two_simple_params<'a>(
  items: &'a oxc_allocator::Vec<'a, FormalParameter<'a>>,
) -> Option<(SymbolId, SymbolId, &'a str, &'a str)> {
  if items.len() != 2 {
    return None;
  }
  let first = simple_param(items.first()?)?;
  let second = simple_param(items.get(1)?)?;
  Some((first.0, second.0, first.1, second.1))
}

fn simple_param<'a>(parameter: &'a FormalParameter<'a>) -> Option<(SymbolId, &'a str)> {
  if parameter.initializer.is_some() || parameter.optional {
    return None;
  }
  match &parameter.pattern {
    BindingPattern::BindingIdentifier(binding) => {
      let symbol_id = binding.symbol_id.get()?;
      Some((symbol_id, binding.name.as_str()))
    }
    _ => None,
  }
}

fn is_bare_return(statement: &Statement<'_>) -> bool {
  match statement {
    Statement::ReturnStatement(ret) => ret.argument.is_none(),
    Statement::BlockStatement(block) if block.body.len() == 1 => {
      matches!(block.body.first(), Some(Statement::ReturnStatement(ret)) if ret.argument.is_none())
    }
    _ => false,
  }
}

fn is_param(
  expression: &Expression<'_>,
  name: &str,
  symbol_id: SymbolId,
  collector: &Collector<'_>,
) -> bool {
  let Some(identifier) = expression.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  collector.indexes.note_query();
  identifier.name.as_str() == name && collector.reference_symbol(identifier) == Some(symbol_id)
}

fn is_pure_undefined(expression: &Expression<'_>, collector: &Collector<'_>) -> bool {
  match expression.get_inner_expression() {
    Expression::Identifier(identifier) => {
      collector.indexes.note_query();
      identifier.name.as_str() == "undefined" && collector.reference_symbol(identifier).is_none()
    }
    Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::Void => {
      is_pure_void_operand(unary.argument.get_inner_expression(), collector)
    }
    _ => false,
  }
}

fn is_pure_void_operand(expression: &Expression<'_>, collector: &Collector<'_>) -> bool {
  collector.indexes.note_query();
  match expression.get_inner_expression() {
    Expression::NumericLiteral(_)
    | Expression::BooleanLiteral(_)
    | Expression::StringLiteral(_)
    | Expression::NullLiteral(_)
    | Expression::BigIntLiteral(_) => true,
    Expression::Identifier(identifier) => {
      identifier.name.as_str() == "undefined" && collector.reference_symbol(identifier).is_none()
    }
    _ => false,
  }
}

fn later_work_consumes(
  statements: &[Statement<'_>],
  new_param: SymbolId,
  new_name: &str,
  collector: &Collector<'_>,
  remaining: u8,
) -> LaterWork {
  if remaining == 0 {
    return LaterWork::Uncertain;
  }
  collector.indexes.add_queries(statements.len() as u64);
  for statement in statements {
    match statement {
      Statement::EmptyStatement(_)
      | Statement::DebuggerStatement(_)
      | Statement::FunctionDeclaration(_)
      | Statement::ClassDeclaration(_)
      | Statement::VariableDeclaration(_) => {}
      Statement::ExpressionStatement(expr) => {
        if expression_consumes(
          expr.expression.get_inner_expression(),
          new_param,
          new_name,
          collector,
          remaining,
        ) {
          return LaterWork::Consumes;
        }
      }
      Statement::ReturnStatement(ret) => {
        if let Some(argument) = &ret.argument
          && expression_consumes(
            argument.get_inner_expression(),
            new_param,
            new_name,
            collector,
            remaining,
          )
        {
          return LaterWork::Consumes;
        }
        return LaterWork::Absent;
      }
      _ => return LaterWork::Uncertain,
    }
  }
  LaterWork::Absent
}

fn expression_consumes(
  expression: &Expression<'_>,
  new_param: SymbolId,
  new_name: &str,
  collector: &Collector<'_>,
  remaining: u8,
) -> bool {
  if remaining == 0 {
    return false;
  }
  collector.indexes.note_query();
  match expression.get_inner_expression() {
    Expression::CallExpression(call) => call.arguments.iter().any(|argument| {
      argument.as_expression().is_some_and(|expr| {
        mentions_param(expr, new_param, new_name, collector, remaining.saturating_sub(1))
      })
    }),
    Expression::NewExpression(new_expr) => new_expr.arguments.iter().any(|argument| {
      argument.as_expression().is_some_and(|expr| {
        mentions_param(expr, new_param, new_name, collector, remaining.saturating_sub(1))
      })
    }),
    Expression::AssignmentExpression(assign) => {
      mentions_param(&assign.right, new_param, new_name, collector, remaining.saturating_sub(1))
    }
    Expression::SequenceExpression(seq) => seq.expressions.iter().any(|expr| {
      expression_consumes(expr, new_param, new_name, collector, remaining.saturating_sub(1))
    }),
    _ => false,
  }
}

fn mentions_param(
  expression: &Expression<'_>,
  new_param: SymbolId,
  new_name: &str,
  collector: &Collector<'_>,
  remaining: u8,
) -> bool {
  if remaining == 0 {
    return false;
  }
  collector.indexes.note_query();
  match expression.get_inner_expression() {
    Expression::Identifier(identifier) => {
      identifier.name.as_str() == new_name
        && collector.reference_symbol(identifier) == Some(new_param)
    }
    Expression::StaticMemberExpression(member) => {
      mentions_param(&member.object, new_param, new_name, collector, remaining.saturating_sub(1))
    }
    Expression::CallExpression(call) => {
      mentions_param(&call.callee, new_param, new_name, collector, remaining.saturating_sub(1))
        || call.arguments.iter().any(|argument| {
          argument.as_expression().is_some_and(|expr| {
            mentions_param(expr, new_param, new_name, collector, remaining.saturating_sub(1))
          })
        })
    }
    Expression::ParenthesizedExpression(inner) => {
      mentions_param(&inner.expression, new_param, new_name, collector, remaining.saturating_sub(1))
    }
    Expression::TSAsExpression(inner) => {
      mentions_param(&inner.expression, new_param, new_name, collector, remaining.saturating_sub(1))
    }
    Expression::TSSatisfiesExpression(inner) => {
      mentions_param(&inner.expression, new_param, new_name, collector, remaining.saturating_sub(1))
    }
    Expression::TSNonNullExpression(inner) => {
      mentions_param(&inner.expression, new_param, new_name, collector, remaining.saturating_sub(1))
    }
    Expression::TSTypeAssertion(inner) => {
      mentions_param(&inner.expression, new_param, new_name, collector, remaining.saturating_sub(1))
    }
    _ => false,
  }
}
