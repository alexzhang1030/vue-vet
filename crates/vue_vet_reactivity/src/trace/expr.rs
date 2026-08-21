//! Shared expression peel / nesting helpers (no graph logic).
//!
//! Parens and TypeScript wrappers (`as` / `!` / `satisfies` / angle-bracket
//! assertion) are the same under-approx peel everywhere so assignment-only,
//! callback slots, and render factories cannot disagree.

use oxc_ast::{AstKind, ast::Expression};
use oxc_semantic::{NodeId, Semantic};

/// Strip parentheses and TypeScript type wrappers.
pub(super) fn peel_parens<'a>(expression: &'a Expression<'a>) -> &'a Expression<'a> {
  let mut current = expression;
  loop {
    current = match current {
      Expression::ParenthesizedExpression(paren) => &paren.expression,
      Expression::TSAsExpression(assertion) => &assertion.expression,
      Expression::TSTypeAssertion(assertion) => &assertion.expression,
      Expression::TSSatisfiesExpression(satisfies) => &satisfies.expression,
      Expression::TSNonNullExpression(non_null) => &non_null.expression,
      other => return other,
    };
  }
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
