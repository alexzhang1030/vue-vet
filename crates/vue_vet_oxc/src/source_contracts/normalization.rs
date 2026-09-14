//! toRef ignored-key and effectScope callback-argument facts.

use oxc_ast::ast::{CallExpression, Expression};
use oxc_span::{GetSpan, Span};

use super::index::CallInfo;
use super::{Collector, MAX_DEPTH};
use vue_vet_core::{SourceContractSiteFact, ToRefIgnoredKeyFact, ToRefIgnoredKeyReason};

impl Collector<'_> {
  pub(super) fn collect_toref(&mut self, call: &CallExpression<'_>, info: CallInfo) {
    let Some(source_span) = info.first_arg else {
      return;
    };
    let Some(key_span) = info.second_arg else {
      return;
    };
    let Some(key_expr) = positional_arg(call, 1, key_span) else {
      return;
    };
    if self.is_language_undefined(key_expr) {
      return;
    }
    let Some(key) = static_key(key_expr) else {
      return;
    };
    if self.toref_source_unproven(source_span) {
      return;
    }
    let shape = self.classify_span(source_span, MAX_DEPTH);
    let Some(reason) = shape.toref_ignored_key_reason() else {
      return;
    };
    if reason == ToRefIgnoredKeyReason::Ref && key.is_value {
      return;
    }
    self.facts.toref_ignored_key.push(ToRefIgnoredKeyFact { span: self.span(key.span), reason });
  }

  pub(super) fn collect_effect_scope(&mut self, info: CallInfo) {
    let Some(argument) = info.first_arg else {
      return;
    };
    if self.classify_span(argument, MAX_DEPTH) != super::shape::Shape::Function {
      return;
    }
    self
      .facts
      .effect_scope_callback
      .push(SourceContractSiteFact { span: self.span(argument), api: "effectScope".into() });
  }

  fn toref_source_unproven(&self, span: Span) -> bool {
    let Some(hint) = self.indexes.hints.get(&super::shape::span_key(span)).copied() else {
      return true;
    };
    match hint {
      super::shape::ShapeHint::Identifier(Some(symbol_id), false) => {
        self.indexes.toref_identity_unproven(symbol_id)
      }
      super::shape::ShapeHint::Identifier(None, false) | super::shape::ShapeHint::Unknown => true,
      _ => false,
    }
  }

  fn is_language_undefined(&self, expression: &Expression<'_>) -> bool {
    let Some(identifier) = expression.get_inner_expression().get_identifier_reference() else {
      return false;
    };
    identifier.name.as_str() == "undefined" && self.reference_symbol(identifier).is_none()
  }
}

struct StaticKey {
  span: Span,
  is_value: bool,
}

fn positional_arg<'a>(
  call: &'a CallExpression<'a>,
  index: usize,
  span: Span,
) -> Option<&'a Expression<'a>> {
  call.arguments.get(index).and_then(oxc_ast::ast::Argument::as_expression).filter(|expression| {
    expression.span() == span || expression.get_inner_expression().span() == span
  })
}

fn static_key(expression: &Expression<'_>) -> Option<StaticKey> {
  let inner = expression.get_inner_expression();
  match inner {
    Expression::StringLiteral(literal) => {
      Some(StaticKey { span: inner.span(), is_value: literal.value.as_str() == "value" })
    }
    Expression::NumericLiteral(_) => Some(StaticKey { span: inner.span(), is_value: false }),
    Expression::TemplateLiteral(literal) if literal.expressions.is_empty() => {
      let cooked =
        literal.quasis.first().and_then(|quasi| quasi.value.cooked.as_deref()).unwrap_or("");
      Some(StaticKey { span: inner.span(), is_value: cooked == "value" })
    }
    _ => None,
  }
}
