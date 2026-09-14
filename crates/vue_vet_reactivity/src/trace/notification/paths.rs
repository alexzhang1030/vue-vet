//! Static member-path walks for notification provenance (no dynamic keys).

use oxc_ast::{
  AstKind,
  ast::{
    AssignmentTarget, Expression, IdentifierReference, SimpleAssignmentTarget,
    StaticMemberExpression,
  },
};
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};

use super::super::expr;

pub(super) fn peel<'a>(expression: &'a Expression<'a>) -> &'a Expression<'a> {
  expr::peel_parens(expression)
}

pub(super) fn static_key_name(key: &oxc_ast::ast::PropertyKey<'_>) -> Option<String> {
  match key {
    oxc_ast::ast::PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.to_string()),
    oxc_ast::ast::PropertyKey::StringLiteral(literal) => Some(literal.value.to_string()),
    oxc_ast::ast::PropertyKey::NumericLiteral(literal) => Some(literal.raw.as_ref()?.to_string()),
    _ => None,
  }
}

pub(super) fn expression_member_chain<'a>(
  expression: &'a Expression<'a>,
) -> Option<(&'a IdentifierReference<'a>, Vec<String>, Span)> {
  let mut segments = Vec::new();
  let mut current = peel(expression);
  let span = current.span();
  loop {
    match current {
      Expression::StaticMemberExpression(member) => {
        if member.optional {
          return None;
        }
        segments.push(member.property.name.to_string());
        current = peel(&member.object);
      }
      Expression::ComputedMemberExpression(member) => {
        if member.optional {
          return None;
        }
        segments.push(member.static_property_name()?.to_string());
        current = peel(&member.object);
      }
      Expression::Identifier(identifier) => {
        segments.reverse();
        return Some((identifier, segments, span));
      }
      _ => return None,
    }
  }
}

pub(super) fn assignment_member_chain<'a>(
  target: &'a AssignmentTarget<'a>,
) -> Option<(&'a IdentifierReference<'a>, Vec<String>, Span)> {
  match target {
    AssignmentTarget::StaticMemberExpression(member) => static_member_chain(member),
    AssignmentTarget::ComputedMemberExpression(member) => {
      if member.optional {
        return None;
      }
      let key = member.static_property_name()?.to_string();
      let (root, mut path, _) = expression_member_chain(&member.object)?;
      path.push(key);
      Some((root, path, member.span))
    }
    _ => None,
  }
}

/// Direct member writes plus assignment-pattern members (`{ x: state.value } =`).
pub(super) fn collect_assignment_lvalues<'a>(
  target: &'a AssignmentTarget<'a>,
  out: &mut Vec<AssignmentLvalue<'a>>,
) {
  match target {
    AssignmentTarget::StaticMemberExpression(member) => {
      if let Some((root, path, span)) = static_member_chain(member) {
        out.push(AssignmentLvalue { root: Some(root), path, span, patterned: false });
      }
    }
    AssignmentTarget::ComputedMemberExpression(member) => {
      if member.optional {
        out.push(AssignmentLvalue {
          root: None,
          path: Vec::new(),
          span: member.span,
          patterned: true,
        });
        return;
      }
      let Some(key) = member.static_property_name() else {
        out.push(AssignmentLvalue {
          root: None,
          path: Vec::new(),
          span: member.span,
          patterned: true,
        });
        return;
      };
      if let Some((root, mut path, _)) = expression_member_chain(&member.object) {
        path.push(key.to_string());
        out.push(AssignmentLvalue { root: Some(root), path, span: member.span, patterned: false });
      } else {
        out.push(AssignmentLvalue {
          root: None,
          path: Vec::new(),
          span: member.span,
          patterned: true,
        });
      }
    }
    AssignmentTarget::ArrayAssignmentTarget(array) => {
      for element in array.elements.iter().flatten() {
        collect_maybe_default_lvalues(element, out);
      }
      if let Some(rest) = &array.rest {
        collect_assignment_lvalues(&rest.target, out);
      }
    }
    AssignmentTarget::ObjectAssignmentTarget(object) => {
      for property in &object.properties {
        match property {
          oxc_ast::ast::AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) => {
            collect_maybe_default_lvalues(&property.binding, out);
          }
          oxc_ast::ast::AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(_) => {
            out.push(AssignmentLvalue {
              root: None,
              path: Vec::new(),
              span: object.span,
              patterned: true,
            });
          }
        }
      }
      if let Some(rest) = &object.rest {
        collect_assignment_lvalues(&rest.target, out);
      }
    }
    AssignmentTarget::TSAsExpression(inner) => {
      collect_expression_lvalues(&inner.expression, out);
    }
    AssignmentTarget::TSSatisfiesExpression(inner) => {
      collect_expression_lvalues(&inner.expression, out);
    }
    AssignmentTarget::TSNonNullExpression(inner) => {
      collect_expression_lvalues(&inner.expression, out);
    }
    AssignmentTarget::TSTypeAssertion(inner) => {
      collect_expression_lvalues(&inner.expression, out);
    }
    AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
      out.push(AssignmentLvalue {
        root: Some(identifier),
        path: Vec::new(),
        span: identifier.span,
        patterned: false,
      });
    }
    AssignmentTarget::PrivateFieldExpression(_) => {}
  }
}

fn collect_maybe_default_lvalues<'a>(
  target: &'a oxc_ast::ast::AssignmentTargetMaybeDefault<'a>,
  out: &mut Vec<AssignmentLvalue<'a>>,
) {
  match target {
    oxc_ast::ast::AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(with_default) => {
      collect_assignment_lvalues(&with_default.binding, out);
      if let Some(lvalue) = out.last_mut() {
        lvalue.patterned = true;
      }
    }
    other => {
      if let Some(assignment_target) = other.as_assignment_target() {
        let start = out.len();
        collect_assignment_lvalues(assignment_target, out);
        for lvalue in out.iter_mut().skip(start) {
          lvalue.patterned = true;
        }
      }
    }
  }
}

fn collect_expression_lvalues<'a>(
  expression: &'a Expression<'a>,
  out: &mut Vec<AssignmentLvalue<'a>>,
) {
  match peel(expression) {
    Expression::StaticMemberExpression(member) => {
      if let Some((root, path, span)) = static_member_chain(member) {
        out.push(AssignmentLvalue { root: Some(root), path, span, patterned: false });
      }
    }
    Expression::ComputedMemberExpression(member) => {
      if let Some(key) = member.static_property_name()
        && let Some((root, mut path, _)) = expression_member_chain(&member.object)
      {
        path.push(key.to_string());
        out.push(AssignmentLvalue { root: Some(root), path, span: member.span, patterned: false });
      }
    }
    Expression::Identifier(identifier) => {
      out.push(AssignmentLvalue {
        root: Some(identifier),
        path: Vec::new(),
        span: identifier.span,
        patterned: false,
      });
    }
    _ => {}
  }
}

pub(super) struct AssignmentLvalue<'a> {
  pub root: Option<&'a IdentifierReference<'a>>,
  pub path: Vec<String>,
  pub span: Span,
  pub patterned: bool,
}

fn static_member_chain<'a>(
  member: &'a StaticMemberExpression<'a>,
) -> Option<(&'a IdentifierReference<'a>, Vec<String>, Span)> {
  if member.optional {
    return None;
  }
  let (root, mut path, _) = expression_member_chain(&member.object)?;
  path.push(member.property.name.to_string());
  Some((root, path, member.span))
}

pub(super) fn simple_assignment_member_chain<'a>(
  target: &'a SimpleAssignmentTarget<'a>,
) -> Option<(&'a IdentifierReference<'a>, Vec<String>, Span)> {
  match target {
    SimpleAssignmentTarget::StaticMemberExpression(member) => static_member_chain(member),
    SimpleAssignmentTarget::ComputedMemberExpression(member) => {
      if member.optional {
        return None;
      }
      let key = member.static_property_name()?.to_string();
      let (root, mut path, _) = expression_member_chain(&member.object)?;
      path.push(key);
      Some((root, path, member.span))
    }
    _ => None,
  }
}

pub(super) fn member_node_chain<'a>(
  semantic: &'a Semantic<'a>,
  node_id: NodeId,
) -> Option<(&'a IdentifierReference<'a>, Vec<String>, Span)> {
  match semantic.nodes().kind(node_id) {
    AstKind::StaticMemberExpression(member) => {
      if member.optional {
        return None;
      }
      let (root, mut path, _) = expression_member_chain(&member.object)?;
      path.push(member.property.name.to_string());
      Some((root, path, member.span))
    }
    AstKind::ComputedMemberExpression(member) => {
      if member.optional {
        return None;
      }
      let key = member.static_property_name()?.to_string();
      let (root, mut path, _) = expression_member_chain(&member.object)?;
      path.push(key);
      Some((root, path, member.span))
    }
    _ => None,
  }
}

pub(super) fn is_outermost_member(
  semantic: &Semantic<'_>,
  node_id: NodeId,
  work: &super::uses::WorkCounter,
) -> bool {
  let span = semantic.nodes().kind(node_id).span();
  for ancestor_id in semantic.nodes().ancestor_ids(node_id) {
    work.add_ancestor_hops(1);
    match semantic.nodes().kind(ancestor_id) {
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSTypeAssertion(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_) => {}
      AstKind::StaticMemberExpression(member) => {
        return member.object.span() != span;
      }
      AstKind::ComputedMemberExpression(member) => {
        return member.object.span() != span;
      }
      _ => return true,
    }
  }
  true
}
