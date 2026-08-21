//! Template expression identifier collection (Oxc expression parser).
use std::collections::BTreeSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::{ArrowFunctionExpression, BindingIdentifier, Function, IdentifierReference};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::SourceType;
use oxc_syntax::scope::ScopeFlags;

/// Collect free identifier reads from one template expression surface.
///
/// Uses Oxc's expression parser so static member properties, object keys, and
/// string/number literals are not mistaken for binding reads. Nested arrow /
/// function parameters (and their inner bindings) are filtered so
/// `(item) => item + count` yields only `count`. `v-for` surfaces keep only the
/// iterable source (`item in items` → `items`). On parse failure the result is
/// empty so callers can fall back to a coarser strategy.
///
/// `shadowed` removes template-local aliases (`v-for` / `v-slot` bindings) so
/// `{{ item }}` inside `v-for="item in items"` does not join a script `item`.
#[must_use]
pub fn template_expression_identifiers(expression: &str, surface: &str) -> Vec<String> {
  template_expression_identifiers_with_shadow(expression, surface, &BTreeSet::new())
}

/// Like [`template_expression_identifiers`], with an explicit template-local
/// alias set to exclude from free reads.
#[must_use]
pub fn template_expression_identifiers_with_shadow(
  expression: &str,
  surface: &str,
  shadowed: &BTreeSet<String>,
) -> Vec<String> {
  // Slot prop patterns bind locals; they are not reactive reads themselves.
  if matches!(surface, "slot" | "slot-scope" | "scope") {
    return Vec::new();
  }
  let normalized = normalize_template_expression(expression, surface);
  if normalized.is_empty() {
    return Vec::new();
  }
  let allocator = Allocator::default();
  let Ok(expr) = Parser::new(&allocator, &normalized, SourceType::mjs()).parse_expression() else {
    return Vec::new();
  };
  let mut collector = FreeIdentifierCollector::default();
  collector.visit_expression(&expr);
  collector.names.into_iter().filter(|name| !shadowed.contains(name)).collect()
}

/// Binding names introduced by a Vue `v-for` alias (`item in items` → `item`).
#[must_use]
pub fn v_for_alias_identifiers(expression: &str) -> Vec<String> {
  let trimmed = expression.trim();
  let Some(alias_part) = v_for_alias_part(trimmed) else {
    return Vec::new();
  };
  let mut names = BTreeSet::new();
  for part in split_top_level_aliases(alias_part) {
    for name in binding_pattern_identifiers(part) {
      names.insert(name);
    }
  }
  names.into_iter().collect()
}

/// Binding names introduced by a slot prop pattern (`{ value }` / `slotProps`).
#[must_use]
pub fn slot_prop_alias_identifiers(expression: &str) -> Vec<String> {
  binding_pattern_identifiers(expression.trim())
}

fn normalize_template_expression(expression: &str, surface: &str) -> String {
  let trimmed = expression.trim();
  if surface == "for"
    && let Some(source) = v_for_iterable_source(trimmed)
  {
    return source;
  }
  trimmed.to_owned()
}

/// Vue `v-for` is `alias in|of source`. Only `source` is a reactive read surface.
fn v_for_iterable_source(expression: &str) -> Option<String> {
  v_for_parts(expression).map(|(_, source)| source)
}

fn v_for_alias_part(expression: &str) -> Option<&str> {
  v_for_parts(expression).map(|(alias, _)| alias)
}

fn v_for_parts(expression: &str) -> Option<(&str, String)> {
  for separator in [" in ", " of "] {
    if let Some((alias, source)) = expression.rsplit_once(separator) {
      let alias = alias.trim();
      let source = source.trim();
      if !alias.is_empty() && !source.is_empty() {
        return Some((alias, source.to_owned()));
      }
    }
  }
  None
}

/// Split `item, index` / `(item, index)` / `({ a }, i)` into top-level alias parts.
fn split_top_level_aliases(alias: &str) -> Vec<&str> {
  let mut inner = alias.trim();
  if inner.starts_with('(') && inner.ends_with(')') && inner.len() >= 2 {
    inner = inner.get(1..inner.len().saturating_sub(1)).unwrap_or(inner).trim();
  }
  let mut parts = Vec::new();
  let mut start = 0_usize;
  let mut depth = 0_i32;
  for (idx, character) in inner.char_indices() {
    match character {
      '(' | '[' | '{' => depth = depth.saturating_add(1),
      ')' | ']' | '}' => depth = depth.saturating_sub(1),
      ',' if depth == 0 => {
        if let Some(part) = inner.get(start..idx).map(str::trim).filter(|part| !part.is_empty()) {
          parts.push(part);
        }
        start = idx.saturating_add(character.len_utf8());
      }
      _ => {}
    }
  }
  if let Some(part) = inner.get(start..).map(str::trim).filter(|part| !part.is_empty()) {
    parts.push(part);
  }
  parts
}

/// Collect binding identifiers from a Vue alias / slot prop pattern.
fn binding_pattern_identifiers(pattern: &str) -> Vec<String> {
  let trimmed = pattern.trim();
  if trimmed.is_empty() {
    return Vec::new();
  }
  if is_simple_identifier(trimmed) {
    return vec![trimmed.to_owned()];
  }
  let source = format!("let {trimmed} = null");
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, &source, SourceType::mjs()).parse();
  if !parsed.diagnostics.is_empty() {
    // Best-effort: pull simple identifier tokens from the pattern text.
    return template_expression_identifiers(trimmed, "bind");
  }
  let mut collector = BindingIdentifierCollector::default();
  collector.visit_program(&parsed.program);
  collector.names.into_iter().collect()
}

fn is_simple_identifier(text: &str) -> bool {
  let mut chars = text.chars();
  let Some(first) = chars.next() else {
    return false;
  };
  if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
    return false;
  }
  chars.all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '$')
}

#[derive(Default)]
struct BindingIdentifierCollector {
  names: BTreeSet<String>,
}

impl<'a> Visit<'a> for BindingIdentifierCollector {
  fn visit_binding_identifier(&mut self, identifier: &BindingIdentifier<'a>) {
    self.names.insert(identifier.name.to_string());
  }
}

#[derive(Default)]
struct FreeIdentifierCollector {
  names: BTreeSet<String>,
  /// Nested function / arrow scopes; identifiers bound here are not free reads.
  bound_stack: Vec<BTreeSet<String>>,
}

impl FreeIdentifierCollector {
  fn is_bound(&self, name: &str) -> bool {
    self.bound_stack.iter().any(|scope| scope.contains(name))
  }

  fn push_scope(&mut self) {
    self.bound_stack.push(BTreeSet::new());
  }

  fn pop_scope(&mut self) {
    self.bound_stack.pop();
  }

  fn bind(&mut self, name: &str) {
    if let Some(scope) = self.bound_stack.last_mut() {
      scope.insert(name.to_owned());
    }
  }
}

impl<'a> Visit<'a> for FreeIdentifierCollector {
  fn visit_identifier_reference(&mut self, identifier: &IdentifierReference<'a>) {
    let name = identifier.name.as_str();
    if !self.is_bound(name) {
      self.names.insert(name.to_owned());
    }
  }

  fn visit_binding_identifier(&mut self, identifier: &BindingIdentifier<'a>) {
    self.bind(identifier.name.as_str());
  }

  fn visit_arrow_function_expression(&mut self, expr: &ArrowFunctionExpression<'a>) {
    self.push_scope();
    walk::walk_arrow_function_expression(self, expr);
    self.pop_scope();
  }

  fn visit_function(&mut self, func: &Function<'a>, flags: ScopeFlags) {
    self.push_scope();
    walk::walk_function(self, func, flags);
    self.pop_scope();
  }
}
