//! Closed primitive atoms for computed-identity projections.
//!
//! Vue identity uses Object.is (NaN matches NaN; +0 is distinct from -0).
//! JavaScript `===` is a separate predicate.

use oxc_ast::ast::{Expression, UnaryOperator};

use super::stats::WorkCounter;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum PrimitiveAtom {
  Number { bits: u64 },
  String(String),
  Bool(bool),
  Null,
  Undefined,
}

impl PrimitiveAtom {
  pub(super) fn object_is(&self, other: &Self) -> bool {
    match (self, other) {
      (Self::Number { bits: left }, Self::Number { bits: right }) => {
        let left = f64::from_bits(*left);
        let right = f64::from_bits(*right);
        (left.is_nan() && right.is_nan()) || left.to_bits() == right.to_bits()
      }
      _ => self == other,
    }
  }

  pub(super) fn strict_eq(&self, other: &Self) -> bool {
    match (self, other) {
      (Self::Number { bits: left }, Self::Number { bits: right }) => {
        let left = f64::from_bits(*left);
        let right = f64::from_bits(*right);
        left == right
      }
      _ => self == other,
    }
  }

  pub(super) fn unresolved_global(name: &str) -> Option<Self> {
    match name {
      "undefined" => Some(Self::Undefined),
      "NaN" => Some(Self::from_f64(f64::NAN)),
      "Infinity" => Some(Self::from_f64(f64::INFINITY)),
      _ => None,
    }
  }

  pub(super) const fn as_f64(&self) -> Option<f64> {
    match self {
      Self::Number { bits } => Some(f64::from_bits(*bits)),
      _ => None,
    }
  }

  pub(super) const fn from_f64(value: f64) -> Self {
    Self::Number { bits: value.to_bits() }
  }

  pub(super) fn js_to_string(&self) -> String {
    match self {
      Self::Number { bits } => {
        let value = f64::from_bits(*bits);
        if value.is_nan() {
          "NaN".into()
        } else if value.is_infinite() {
          if value.is_sign_negative() { "-Infinity".into() } else { "Infinity".into() }
        } else if value == 0.0 {
          "0".into()
        } else {
          let mut text = value.to_string();
          if text.ends_with(".0") {
            text.truncate(text.len().saturating_sub(2));
          }
          text
        }
      }
      Self::String(text) => text.clone(),
      Self::Bool(true) => "true".into(),
      Self::Bool(false) => "false".into(),
      Self::Null => "null".into(),
      Self::Undefined => "undefined".into(),
    }
  }
}

pub(super) fn atom_of_expression(
  expression: &Expression<'_>,
  work: &WorkCounter,
  remaining: u8,
) -> Option<PrimitiveAtom> {
  work.add_queries(1);
  if remaining == 0 {
    return None;
  }
  match expression.get_inner_expression() {
    Expression::BooleanLiteral(literal) => Some(PrimitiveAtom::Bool(literal.value)),
    Expression::NumericLiteral(literal) => Some(PrimitiveAtom::from_f64(literal.value)),
    Expression::StringLiteral(literal) => Some(PrimitiveAtom::String(literal.value.to_string())),
    Expression::NullLiteral(_) => Some(PrimitiveAtom::Null),
    Expression::TemplateLiteral(literal) if literal.expressions.is_empty() => {
      let text = literal.quasis.first().map_or_else(String::new, |part| {
        part.value.cooked.as_deref().unwrap_or(part.value.raw.as_str()).to_owned()
      });
      Some(PrimitiveAtom::String(text))
    }
    Expression::UnaryExpression(unary) => {
      let inner = atom_of_expression(&unary.argument, work, remaining.saturating_sub(1))?;
      match unary.operator {
        UnaryOperator::UnaryNegation => inner.as_f64().map(|value| PrimitiveAtom::from_f64(-value)),
        UnaryOperator::UnaryPlus => inner.as_f64().map(PrimitiveAtom::from_f64),
        UnaryOperator::LogicalNot => Some(PrimitiveAtom::Bool(!truthy(&inner))),
        UnaryOperator::Void => Some(PrimitiveAtom::Undefined),
        _ => None,
      }
    }
    _ => None,
  }
}

fn truthy(atom: &PrimitiveAtom) -> bool {
  match atom {
    PrimitiveAtom::Number { bits } => {
      let value = f64::from_bits(*bits);
      value != 0.0 && !value.is_nan()
    }
    PrimitiveAtom::String(text) => !text.is_empty(),
    PrimitiveAtom::Bool(value) => *value,
    PrimitiveAtom::Null | PrimitiveAtom::Undefined => false,
  }
}

pub(super) fn sequences_object_is(
  left: &[PrimitiveAtom],
  right: &[PrimitiveAtom],
  work: &WorkCounter,
) -> bool {
  if left.len() != right.len() {
    work.add_queries(1);
    return false;
  }
  left.iter().zip(right).all(|(a, b)| {
    work.add_queries(1);
    a.object_is(b)
  })
}
