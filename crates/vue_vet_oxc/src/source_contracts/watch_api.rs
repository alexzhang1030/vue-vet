//! Watch-family option and signature contracts (issue #224).
//!
//! Collection stays linear over proven calls and the already-indexed options
//! object. `watchEffect` / `watchPostEffect` / `watchSyncEffect` are
//! `ContractSink::WatchEffectFamily` APIs; this module records option/slot
//! facts after the shared sink table admits the call. Both predicates read a
//! second argument (`nth_expr(..., 1)` for signature mismatch, options slot 1
//! for effect ignored keys). Effect-only files whose proven calls have fewer
//! than two arguments keep source indexes empty.

use std::collections::HashSet;

use oxc_ast::ast::{Argument, CallExpression, Expression, ObjectExpression, ObjectPropertyKind};
use oxc_span::{GetSpan, Span};
use vue_vet_core::{
  WatchIgnoredOptionFact, WatchIgnoredOptionReason, WatchSignatureMismatchFact,
  WatchSignatureMismatchReason,
};

use super::index::{CallInfo, ObjectEntry};
use super::shape::{Shape, span_key};
use super::stats::WorkCounter;
use super::{Collector, MAX_DEPTH};

fn is_effect_api(api: &str) -> bool {
  matches!(api, "watchEffect" | "watchPostEffect" | "watchSyncEffect")
}

impl Collector<'_> {
  pub(super) fn collect_watch_api(&mut self, call: &CallExpression<'_>, info: CallInfo) {
    let Some(api) = info.api else {
      return;
    };
    if self.collect_signature_mismatch(call, api) {
      return;
    }
    self.collect_ignored_options(call, api);
  }

  fn collect_signature_mismatch(&mut self, call: &CallExpression<'_>, api: &str) -> bool {
    let Some(argument) = nth_expr(call, 1) else {
      return false;
    };
    let inner = argument.get_inner_expression();
    if api == "watch" {
      if is_array_literal(inner) || is_fresh_function(inner) || is_falsy_callback(inner) {
        return false;
      }
      if is_plain_object(inner) || is_truthy_primitive(inner) {
        self.facts.watch_signature_mismatch.push(WatchSignatureMismatchFact {
          span: self.span(argument.span()),
          api: api.into(),
          reason: WatchSignatureMismatchReason::WatchNonFunctionCallback,
        });
        return true;
      }
      return false;
    }
    if !is_effect_api(api) || !is_fresh_function(inner) {
      return false;
    }
    let Some(first) = nth_expr(call, 0) else {
      return false;
    };
    let first_inner = first.get_inner_expression();
    let reason = if is_fresh_function(first_inner) {
      WatchSignatureMismatchReason::EffectFunctionAsOptions
    } else if self.classify_span(first.span(), MAX_DEPTH) == Shape::RefLike {
      WatchSignatureMismatchReason::EffectRefWithCallback
    } else {
      return false;
    };
    self.facts.watch_signature_mismatch.push(WatchSignatureMismatchFact {
      span: self.span(argument.span()),
      api: api.into(),
      reason,
    });
    true
  }

  fn collect_ignored_options(&mut self, call: &CallExpression<'_>, api: &str) {
    let slot = if api == "watch" { 2 } else { 1 };
    let Some(argument) = nth_expr(call, slot) else {
      return;
    };
    let Expression::ObjectExpression(object) = argument.get_inner_expression() else {
      return;
    };
    let Some(keys) = unique_static_option_keys(
      object,
      self.indexes.objects.get(&span_key(object.span)),
      self.indexes.work(),
    ) else {
      return;
    };
    let effect = is_effect_api(api);
    for (name, key, value) in keys {
      self.indexes.note_query();
      let reason = if name == "equals" && self.value_is_function(value) {
        Some(WatchIgnoredOptionReason::Equals)
      } else if effect {
        match name {
          "immediate" if self.value_is_proven_non_undefined(value) => {
            Some(WatchIgnoredOptionReason::Immediate)
          }
          "deep" if self.value_is_proven_non_undefined(value) => {
            Some(WatchIgnoredOptionReason::Deep)
          }
          "once" if self.value_is_proven_non_undefined(value) => {
            Some(WatchIgnoredOptionReason::Once)
          }
          _ => None,
        }
      } else {
        None
      };
      if let Some(reason) = reason {
        self.facts.watch_ignored_option.push(WatchIgnoredOptionFact {
          span: self.span(key),
          api: api.into(),
          reason,
        });
      }
    }
  }

  fn value_is_function(&self, span: Span) -> bool {
    matches!(self.indexes.hints.get(&span_key(span)), Some(super::shape::ShapeHint::Function))
  }

  fn value_is_proven_non_undefined(&self, span: Span) -> bool {
    matches!(
      self.indexes.hints.get(&span_key(span)),
      Some(super::shape::ShapeHint::Primitive | super::shape::ShapeHint::Nullish)
    )
  }
}

fn nth_expr<'a>(call: &'a CallExpression<'a>, index: usize) -> Option<&'a Expression<'a>> {
  call.arguments.get(index).and_then(Argument::as_expression)
}

fn is_fresh_function(expression: &Expression<'_>) -> bool {
  matches!(
    expression.get_inner_expression(),
    Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
  )
}

fn is_array_literal(expression: &Expression<'_>) -> bool {
  matches!(expression.get_inner_expression(), Expression::ArrayExpression(_))
}

fn is_plain_object(expression: &Expression<'_>) -> bool {
  matches!(expression.get_inner_expression(), Expression::ObjectExpression(_))
}

fn is_falsy_callback(expression: &Expression<'_>) -> bool {
  match expression.get_inner_expression() {
    Expression::NullLiteral(_) => true,
    Expression::BooleanLiteral(literal) => !literal.value,
    Expression::NumericLiteral(literal) => literal.value == 0.0,
    Expression::BigIntLiteral(literal) => bigint_is_zero(literal.value.as_str()),
    Expression::StringLiteral(literal) => literal.value.is_empty(),
    Expression::TemplateLiteral(literal)
      if literal.expressions.is_empty()
        && literal
          .quasis
          .first()
          .is_none_or(|part| part.value.cooked.as_deref().is_none_or(str::is_empty)) =>
    {
      true
    }
    Expression::Identifier(identifier) if identifier.name.as_str() == "undefined" => true,
    _ => false,
  }
}

fn is_truthy_primitive(expression: &Expression<'_>) -> bool {
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(literal) => literal.value,
    Expression::NumericLiteral(literal) => literal.value != 0.0 && !literal.value.is_nan(),
    Expression::BigIntLiteral(literal) => !bigint_is_zero(literal.value.as_str()),
    Expression::StringLiteral(literal) => !literal.value.is_empty(),
    Expression::TemplateLiteral(literal)
      if literal.expressions.is_empty()
        && literal
          .quasis
          .first()
          .and_then(|part| part.value.cooked.as_deref())
          .is_some_and(|cooked| !cooked.is_empty()) =>
    {
      true
    }
    _ => false,
  }
}

fn bigint_is_zero(digits: &str) -> bool {
  let stripped = digits.strip_prefix('+').unwrap_or(digits);
  !stripped.is_empty() && stripped.bytes().all(|byte| byte == b'0')
}

fn unique_static_option_keys(
  object: &ObjectExpression<'_>,
  indexed: Option<&Vec<ObjectEntry>>,
  work: &WorkCounter,
) -> Option<Vec<(&'static str, Span, Span)>> {
  let entries = indexed?;
  if object.properties.len() != entries.len() {
    return None;
  }
  let mut seen = HashSet::new();
  let mut keys = Vec::new();
  for (property, entry) in object.properties.iter().zip(entries) {
    work.add_object_entries(1);
    if matches!(property, ObjectPropertyKind::ObjectProperty(prop) if prop.computed) {
      return None;
    }
    match entry {
      ObjectEntry::Spread | ObjectEntry::Computed | ObjectEntry::Accessor { .. } => return None,
      ObjectEntry::Data { name, key, value } | ObjectEntry::Method { name, key, value } => {
        if name == "__proto__" || !seen.insert(name.as_str()) {
          return None;
        }
        let interned = match name.as_str() {
          "equals" => "equals",
          "immediate" => "immediate",
          "deep" => "deep",
          "once" => "once",
          _ => continue,
        };
        keys.push((interned, *key, *value));
      }
    }
  }
  Some(keys)
}
