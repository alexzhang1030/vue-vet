//! Shared expression peel / nesting helpers (no graph logic).
//!
//! Parens and TypeScript wrappers (`as` / `!` / `satisfies` / angle-bracket
//! assertion) are the same under-approx peel everywhere so assignment-only,
//! callback slots, and render factories cannot disagree.

use oxc_ast::{AstKind, ast::Expression};
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};

/// Strip parentheses and TypeScript type wrappers.
pub(super) fn peel_parens<'a>(expression: &'a Expression<'a>) -> &'a Expression<'a> {
  expression.get_inner_expression()
}

/// True when `node_id` is the assigned location of `=` (not a nested receiver/key).
///
/// `[target.value] = …` and `{ field: target.value } = …` treat `target.value` as
/// write-only. Nested `draft.value` in `draft.value.params.x = …` stays a get.
/// Compound `+=` / logical assigns are not write-only (they may read).
pub(super) fn member_is_write_only_assignment(semantic: &Semantic<'_>, node_id: NodeId) -> bool {
  let node_span = semantic.nodes().kind(node_id).span();
  for ancestor_id in semantic.nodes().ancestor_ids(node_id) {
    match semantic.nodes().kind(ancestor_id) {
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSTypeAssertion(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_)
      | AstKind::ArrayAssignmentTarget(_)
      | AstKind::ObjectAssignmentTarget(_)
      | AstKind::AssignmentTargetRest(_) => {}
      AstKind::AssignmentTargetWithDefault(with_default) => {
        if span_contains(with_default.init.span(), node_span) {
          return false;
        }
      }
      AstKind::AssignmentTargetPropertyProperty(property) => {
        if span_contains(property.name.span(), node_span) {
          return false;
        }
      }
      AstKind::AssignmentTargetPropertyIdentifier(property) => {
        if property.init.as_ref().is_some_and(|init| span_contains(init.span(), node_span)) {
          return false;
        }
      }
      AstKind::AssignmentExpression(assignment) => {
        return assignment.operator.is_assign() && span_contains(assignment.left.span(), node_span);
      }
      _ => return false,
    }
  }
  false
}

const fn span_contains(outer: Span, inner: Span) -> bool {
  inner.start >= outer.start && inner.end <= outer.end
}

/// True when `node_id` sits inside a nested `function` / arrow (not the root).
pub(super) fn is_nested_in_function(semantic: &Semantic<'_>, node_id: NodeId) -> bool {
  semantic.nodes().ancestor_ids(node_id).any(|ancestor_id| {
    matches!(
      semantic.nodes().kind(ancestor_id),
      AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
    )
  })
}

#[cfg(test)]
mod tests {
  use oxc_allocator::Allocator;
  use oxc_parser::Parser;
  use oxc_span::SourceType;

  use super::peel_parens;

  #[test]
  fn peel_parens_strips_ts_wrappers() {
    let allocator = Allocator::default();
    let source = "const x = ((value as number)!);";
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "parse: {:?}", parsed.diagnostics);
    let peeled = parsed.program.body.first().and_then(|stmt| match stmt {
      oxc_ast::ast::Statement::VariableDeclaration(decl) => {
        decl.declarations.first()?.init.as_ref().map(peel_parens)
      }
      _ => None,
    });
    assert!(
      peeled.is_some_and(|expr| matches!(expr, oxc_ast::ast::Expression::Identifier(_))),
      "expected identifier after peel; got {peeled:?}"
    );
  }
}
