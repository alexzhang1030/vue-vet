//! One-pass owner/role summaries for source/view symbols.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::{
  AstKind,
  ast::{
    Argument, CallExpression, Expression, IdentifierReference, ObjectPropertyKind,
    VariableDeclarationKind,
  },
};
use oxc_semantic::{NodeId, Semantic, SymbolId};
use oxc_span::{GetSpan, Span};

use super::super::kinds::resolved_vue_callee;
use super::paths::{collect_assignment_lvalues, peel};
use vue_vet_core::ScriptKind;

#[derive(Clone, Copy, Debug, Default)]
pub struct NotificationWork {
  pub node_visits: usize,
  pub reference_visits: usize,
  pub payload_entries: usize,
  pub aliases: usize,
  pub owners: usize,
  pub events: usize,
  pub queries: usize,
  pub copies: usize,
  pub emissions: usize,
  /// `BTreeMap` get/insert operations. Each is `O(log cardinality)`.
  pub lookups: usize,
  /// Remaining explicit candidate visits inside loops (not indexed misses).
  pub candidate_visits: usize,
  pub payload_prefix_visits: usize,
  pub payload_prefix_removals: usize,
  /// Parent hops while walking enclosing region / use / outermost-member.
  /// Fixtures keep AST depth bounded; hops scale with nodes × that bound.
  pub ancestor_hops: usize,
  pub use_sites: usize,
  pub stop_bucket_visits: usize,
}

impl NotificationWork {
  pub(super) const fn zero() -> Self {
    Self {
      node_visits: 0,
      reference_visits: 0,
      payload_entries: 0,
      aliases: 0,
      owners: 0,
      events: 0,
      queries: 0,
      copies: 0,
      emissions: 0,
      lookups: 0,
      candidate_visits: 0,
      payload_prefix_visits: 0,
      payload_prefix_removals: 0,
      ancestor_hops: 0,
      use_sites: 0,
      stop_bucket_visits: 0,
    }
  }

  #[cfg(test)]
  pub(crate) const fn indexed_work(self) -> usize {
    self
      .lookups
      .saturating_add(self.queries)
      .saturating_add(self.candidate_visits)
      .saturating_add(self.payload_prefix_visits)
      .saturating_add(self.ancestor_hops)
      .saturating_add(self.copies)
      .saturating_add(self.stop_bucket_visits)
      .saturating_add(self.use_sites)
  }
}

#[derive(Clone, Debug)]
pub(super) enum UseRole {
  Read,
  NestedWrite,
  PayloadReplace { path: Vec<String> },
  ClosedAlias,
  VueApiArg,
  HandleStop,
  Invalid,
}

#[derive(Clone, Debug)]
pub(super) struct UseSite {
  pub role: UseRole,
  pub span: Span,
}

pub(super) struct OwnerIndex {
  pub by_symbol: BTreeMap<SymbolId, Vec<UseSite>>,
  pub exported: BTreeSet<SymbolId>,
  pub binding_by_span: BTreeMap<(u32, u32), SymbolId>,
}

pub(super) fn enclosing_region(
  semantic: &Semantic<'_>,
  node_id: NodeId,
  work: &mut NotificationWork,
) -> NodeId {
  for ancestor_id in semantic.nodes().ancestor_ids(node_id) {
    work.ancestor_hops = work.ancestor_hops.saturating_add(1);
    match semantic.nodes().kind(ancestor_id) {
      AstKind::Function(_)
      | AstKind::ArrowFunctionExpression(_)
      | AstKind::Class(_)
      | AstKind::Program(_) => return ancestor_id,
      _ => {}
    }
  }
  node_id
}

fn expression_is_plain_literal(expression: &Expression<'_>) -> bool {
  matches!(
    peel(expression),
    Expression::BooleanLiteral(_)
      | Expression::NumericLiteral(_)
      | Expression::StringLiteral(_)
      | Expression::NullLiteral(_)
      | Expression::BigIntLiteral(_)
      | Expression::UnaryExpression(_)
  )
}

pub(super) fn identifier_symbol(
  semantic: &Semantic<'_>,
  identifier: &IdentifierReference<'_>,
) -> Option<SymbolId> {
  let reference_id = identifier.reference_id.get()?;
  semantic.scoping().get_reference(reference_id).symbol_id()
}

pub(super) fn build_owner_index(
  semantic: &Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_kind: ScriptKind,
  work: &mut NotificationWork,
) -> OwnerIndex {
  let mut binding_by_span = BTreeMap::new();
  for symbol_id in semantic.scoping().symbol_ids() {
    work.node_visits += 1;
    let span = semantic.scoping().symbol_span(symbol_id);
    binding_by_span.insert((span.start, span.end), symbol_id);
  }
  work.owners = binding_by_span.len();
  let exported = collect_exported_symbols(semantic, work);
  let mut by_symbol: BTreeMap<SymbolId, Vec<UseSite>> = BTreeMap::new();
  for (node_id, node) in semantic.nodes().iter_enumerated() {
    work.node_visits += 1;
    match node.kind() {
      AstKind::IdentifierReference(identifier) => {
        work.reference_visits += 1;
        let Some(symbol_id) = identifier_symbol(semantic, identifier) else {
          continue;
        };
        let role = classify_complete_use(semantic, node_id, imported_bindings, script_kind, work);
        by_symbol.entry(symbol_id).or_default().push(UseSite { role, span: identifier.span });
      }
      AstKind::AssignmentExpression(assignment) if !assignment.operator.is_logical() => {
        let mut lvalues = Vec::new();
        collect_assignment_lvalues(&assignment.left, &mut lvalues);
        let rhs_plain = expression_is_plain_literal(&assignment.right);
        for lvalue in lvalues {
          let Some(root) = lvalue.root else {
            continue;
          };
          let Some(symbol_id) = identifier_symbol(semantic, root) else {
            continue;
          };
          let role = if lvalue.patterned || lvalue.path.is_empty() {
            if lvalue.path.is_empty() {
              UseRole::Invalid
            } else {
              UseRole::PayloadReplace { path: lvalue.path }
            }
          } else if rhs_plain {
            UseRole::NestedWrite
          } else {
            UseRole::PayloadReplace { path: lvalue.path }
          };
          by_symbol.entry(symbol_id).or_default().push(UseSite { role, span: lvalue.span });
        }
      }
      _ => {}
    }
  }
  OwnerIndex { by_symbol, exported, binding_by_span }
}

fn collect_exported_symbols(
  semantic: &Semantic<'_>,
  work: &mut NotificationWork,
) -> BTreeSet<SymbolId> {
  let mut exported = BTreeSet::new();
  for node in semantic.nodes() {
    work.node_visits += 1;
    match node.kind() {
      AstKind::ExportNamedDeclaration(declaration) if declaration.source.is_none() => {
        if let Some(decl) = &declaration.declaration {
          collect_exported_from_declaration(semantic, decl, &mut exported, work);
        }
        for specifier in &declaration.specifiers {
          work.lookups = work.lookups.saturating_add(1);
          if let Some(symbol_id) = root_binding(semantic, specifier.local.name().as_str()) {
            exported.insert(symbol_id);
          }
        }
      }
      AstKind::ExportDefaultDeclaration(declaration) => {
        if let oxc_ast::ast::ExportDefaultDeclarationKind::Identifier(identifier) =
          &declaration.declaration
          && let Some(symbol_id) = identifier_symbol(semantic, identifier)
        {
          exported.insert(symbol_id);
        }
      }
      _ => {}
    }
  }
  exported
}

fn root_binding(semantic: &Semantic<'_>, name: &str) -> Option<SymbolId> {
  let scoping = semantic.scoping();
  scoping.get_binding(scoping.root_scope_id(), name.into())
}

fn collect_exported_from_declaration(
  semantic: &Semantic<'_>,
  declaration: &oxc_ast::ast::Declaration<'_>,
  exported: &mut BTreeSet<SymbolId>,
  work: &mut NotificationWork,
) {
  match declaration {
    oxc_ast::ast::Declaration::VariableDeclaration(variables) => {
      for declarator in &variables.declarations {
        if let oxc_ast::ast::BindingPattern::BindingIdentifier(identifier) = &declarator.id {
          work.lookups = work.lookups.saturating_add(1);
          if let Some(symbol_id) = root_binding(semantic, identifier.name.as_str()) {
            exported.insert(symbol_id);
          }
        }
      }
    }
    oxc_ast::ast::Declaration::FunctionDeclaration(function) => {
      if let Some(id) = &function.id {
        work.lookups = work.lookups.saturating_add(1);
        if let Some(symbol_id) = root_binding(semantic, id.name.as_str()) {
          exported.insert(symbol_id);
        }
      }
    }
    _ => {}
  }
}

fn classify_complete_use(
  semantic: &Semantic<'_>,
  ident_id: NodeId,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_kind: ScriptKind,
  work: &mut NotificationWork,
) -> UseRole {
  let span = semantic.nodes().kind(ident_id).span();
  let mut current = span;
  let mut path = Vec::new();
  for ancestor_id in semantic.nodes().ancestor_ids(ident_id) {
    work.ancestor_hops = work.ancestor_hops.saturating_add(1);
    match semantic.nodes().kind(ancestor_id) {
      AstKind::ParenthesizedExpression(_)
      | AstKind::TSAsExpression(_)
      | AstKind::TSTypeAssertion(_)
      | AstKind::TSSatisfiesExpression(_)
      | AstKind::TSNonNullExpression(_) => {}
      AstKind::StaticMemberExpression(member)
        if member.object.span() == current && !member.optional =>
      {
        if member.property.name.as_str() == "stop" && path.is_empty() {
          return UseRole::HandleStop;
        }
        path.push(member.property.name.to_string());
        current = member.span;
      }
      AstKind::ComputedMemberExpression(member)
        if member.object.span() == current && !member.optional =>
      {
        let Some(key) = member.static_property_name() else {
          return UseRole::Invalid;
        };
        path.push(key.to_string());
        current = member.span;
      }
      AstKind::VariableDeclarator(declarator) => {
        if path.is_empty() && declarator.init.as_ref().is_some_and(|init| peel(init).span() == span)
        {
          return closed_alias_role(semantic, ancestor_id, declarator);
        }
        return UseRole::Invalid;
      }
      AstKind::AssignmentExpression(assignment) => {
        if assignment.left.span() == current || assignment.left.span() == span {
          if path.is_empty() {
            return UseRole::Invalid;
          }
          return UseRole::NestedWrite;
        }
        return UseRole::Invalid;
      }
      AstKind::AssignmentTargetPropertyProperty(_)
      | AstKind::ArrayAssignmentTarget(_)
      | AstKind::ObjectAssignmentTarget(_) => {
        if path.is_empty() {
          return UseRole::Invalid;
        }
        return UseRole::PayloadReplace { path };
      }
      AstKind::CallExpression(call) => {
        if peel(&call.callee).span() == current || peel(&call.callee).span() == span {
          return UseRole::HandleStop;
        }
        if path.is_empty() {
          return vue_api_or_invalid(semantic, call, imported_bindings, script_kind);
        }
        return UseRole::Invalid;
      }
      AstKind::NewExpression(_)
      | AstKind::ReturnStatement(_)
      | AstKind::SpreadElement(_)
      | AstKind::ArrayExpression(_)
      | AstKind::ObjectProperty(_)
      | AstKind::ExportNamedDeclaration(_)
      | AstKind::ExportDefaultDeclaration(_) => return UseRole::Invalid,
      _ => {
        return if path.is_empty() { UseRole::Invalid } else { UseRole::Read };
      }
    }
  }
  if path.is_empty() { UseRole::Invalid } else { UseRole::Read }
}

fn closed_alias_role(
  semantic: &Semantic<'_>,
  declarator_id: NodeId,
  declarator: &oxc_ast::ast::VariableDeclarator<'_>,
) -> UseRole {
  let oxc_ast::ast::BindingPattern::BindingIdentifier(_) = &declarator.id else {
    return UseRole::Invalid;
  };
  let parent_id = semantic.nodes().parent_id(declarator_id);
  let AstKind::VariableDeclaration(declaration) = semantic.nodes().kind(parent_id) else {
    return UseRole::Invalid;
  };
  if declaration.kind == VariableDeclarationKind::Const {
    UseRole::ClosedAlias
  } else {
    UseRole::Invalid
  }
}

fn vue_api_or_invalid(
  semantic: &Semantic<'_>,
  call: &CallExpression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_kind: ScriptKind,
) -> UseRole {
  let Some(callee) = resolved_vue_callee(semantic, &call.callee, imported_bindings, script_kind)
  else {
    return UseRole::Invalid;
  };
  if matches!(
    callee.as_str(),
    "toRaw"
      | "triggerRef"
      | "watchSyncEffect"
      | "watchEffect"
      | "watchPostEffect"
      | "watch"
      | "unref"
      | "toValue"
  ) {
    UseRole::VueApiArg
  } else {
    UseRole::Invalid
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FlushKind {
  Sync,
  Post,
  Unknown,
}

pub(super) fn watch_effect_flush(call: &CallExpression<'_>) -> FlushKind {
  if call.arguments.iter().any(Argument::is_spread) {
    return FlushKind::Unknown;
  }
  let Some(last) = call.arguments.last().and_then(Argument::as_expression) else {
    return FlushKind::Sync;
  };
  if matches!(
    peel(last),
    Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
  ) {
    return FlushKind::Sync;
  }
  object_flush(last)
}

fn object_flush(expression: &Expression<'_>) -> FlushKind {
  let Expression::ObjectExpression(object) = peel(expression) else {
    return FlushKind::Unknown;
  };
  let mut flush = FlushKind::Sync;
  for property in &object.properties {
    match property {
      ObjectPropertyKind::SpreadProperty(_) => return FlushKind::Unknown,
      ObjectPropertyKind::ObjectProperty(property) => {
        if super::paths::static_key_name(&property.key).as_deref() != Some("flush") {
          continue;
        }
        flush = match peel(&property.value) {
          Expression::StringLiteral(literal) if literal.value.as_str() == "post" => FlushKind::Post,
          Expression::StringLiteral(literal)
            if matches!(literal.value.as_str(), "pre" | "sync") =>
          {
            FlushKind::Sync
          }
          _ => return FlushKind::Unknown,
        };
      }
    }
  }
  flush
}

pub(super) fn call_is_unconditional(
  semantic: &Semantic<'_>,
  call_id: NodeId,
  work: &mut NotificationWork,
) -> bool {
  for ancestor_id in semantic.nodes().ancestor_ids(call_id) {
    work.ancestor_hops = work.ancestor_hops.saturating_add(1);
    match semantic.nodes().kind(ancestor_id) {
      AstKind::IfStatement(_)
      | AstKind::ConditionalExpression(_)
      | AstKind::LogicalExpression(_)
      | AstKind::SwitchStatement(_)
      | AstKind::ForStatement(_)
      | AstKind::ForInStatement(_)
      | AstKind::ForOfStatement(_)
      | AstKind::WhileStatement(_)
      | AstKind::DoWhileStatement(_)
      | AstKind::AwaitExpression(_) => return false,
      AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
        return true;
      }
      _ => {}
    }
  }
  true
}

pub(super) fn binding_symbol_at(
  index: &OwnerIndex,
  span: Span,
  work: &mut NotificationWork,
) -> Option<SymbolId> {
  work.lookups = work.lookups.saturating_add(1);
  index.binding_by_span.get(&(span.start, span.end)).copied()
}
