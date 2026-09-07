use std::collections::HashMap;

use oxc_ast::{
  AstKind,
  ast::{
    Argument, BindingPattern, CallExpression, Expression, Function, ImportDeclarationSpecifier,
    ImportOrExportKind, ModuleExportName,
  },
};
use oxc_semantic::{IsGlobalReference, NodeId, SymbolId};
use oxc_span::Span;
use vue_vet_core::WatcherApiKind;

const ALIAS_BOUND: usize = 8;

#[derive(Clone, Copy)]
pub(super) struct FunctionRef {
  pub node_id: NodeId,
  pub span: Span,
  pub is_async: bool,
  pub is_generator: bool,
}

enum ResolveOutcome {
  Found(FunctionRef),
  Unknown,
  Budget,
}

pub(super) struct FunctionResolver<'src, 'ast> {
  semantic: &'src oxc_semantic::Semantic<'ast>,
  cache: HashMap<SymbolId, Option<FunctionRef>>,
  write_cache: HashMap<SymbolId, bool>,
}

impl<'src, 'ast> FunctionResolver<'src, 'ast> {
  pub(super) fn new(semantic: &'src oxc_semantic::Semantic<'ast>) -> Self {
    Self { semantic, cache: HashMap::new(), write_cache: HashMap::new() }
  }

  pub(super) fn symbol_has_write(&mut self, symbol_id: SymbolId) -> bool {
    if let Some(&cached) = self.write_cache.get(&symbol_id) {
      return cached;
    }
    let written = symbol_has_write(self.semantic, symbol_id);
    self.write_cache.insert(symbol_id, written);
    written
  }

  pub(super) fn resolve_expression(&mut self, expression: &Expression<'_>) -> Option<FunctionRef> {
    match self.resolve_expression_bound(expression, 0, &mut Vec::new()) {
      ResolveOutcome::Found(function) => Some(function),
      ResolveOutcome::Unknown | ResolveOutcome::Budget => None,
    }
  }

  pub(super) fn resolve_symbol_id(&mut self, symbol_id: SymbolId) -> Option<FunctionRef> {
    match self.resolve_symbol_bound(symbol_id, 0, &mut Vec::new()) {
      ResolveOutcome::Found(function) => Some(function),
      ResolveOutcome::Unknown | ResolveOutcome::Budget => None,
    }
  }

  fn resolve_expression_bound(
    &mut self,
    expression: &Expression<'_>,
    depth: usize,
    stack: &mut Vec<SymbolId>,
  ) -> ResolveOutcome {
    match expression.get_inner_expression() {
      Expression::ArrowFunctionExpression(arrow) => ResolveOutcome::Found(FunctionRef {
        node_id: arrow.node_id.get(),
        span: arrow.span,
        is_async: arrow.r#async,
        is_generator: false,
      }),
      Expression::FunctionExpression(function) => {
        function_ref_from_function(function, function.node_id.get())
          .map_or(ResolveOutcome::Unknown, ResolveOutcome::Found)
      }
      Expression::Identifier(identifier) => {
        let Some(symbol_id) = referenced_symbol(self.semantic, identifier) else {
          return ResolveOutcome::Unknown;
        };
        self.resolve_symbol_bound(symbol_id, depth, stack)
      }
      _ => ResolveOutcome::Unknown,
    }
  }

  fn resolve_symbol_bound(
    &mut self,
    symbol_id: SymbolId,
    depth: usize,
    stack: &mut Vec<SymbolId>,
  ) -> ResolveOutcome {
    if depth >= ALIAS_BOUND {
      return ResolveOutcome::Budget;
    }
    if let Some(&cached) = self.cache.get(&symbol_id) {
      return cached.map_or(ResolveOutcome::Unknown, ResolveOutcome::Found);
    }
    if stack.contains(&symbol_id) {
      self.cache.insert(symbol_id, None);
      return ResolveOutcome::Unknown;
    }
    stack.push(symbol_id);
    let outcome = self.resolve_symbol(symbol_id, depth, stack);
    stack.pop();
    match outcome {
      ResolveOutcome::Found(function) => {
        self.cache.insert(symbol_id, Some(function));
        ResolveOutcome::Found(function)
      }
      ResolveOutcome::Unknown => {
        self.cache.insert(symbol_id, None);
        ResolveOutcome::Unknown
      }
      ResolveOutcome::Budget => ResolveOutcome::Budget,
    }
  }

  fn resolve_symbol(
    &mut self,
    symbol_id: SymbolId,
    depth: usize,
    stack: &mut Vec<SymbolId>,
  ) -> ResolveOutcome {
    if self.symbol_has_write(symbol_id) {
      return ResolveOutcome::Unknown;
    }
    let declaration_id = self.semantic.scoping().symbol_declaration(symbol_id);
    match self.semantic.nodes().kind(declaration_id) {
      AstKind::Function(function) => function_ref_from_function(function, declaration_id)
        .map_or(ResolveOutcome::Unknown, ResolveOutcome::Found),
      AstKind::VariableDeclarator(declarator) => {
        let Some(init) = &declarator.init else {
          return ResolveOutcome::Unknown;
        };
        self.resolve_expression_bound(init, depth.saturating_add(1), stack)
      }
      _ => ResolveOutcome::Unknown,
    }
  }
}

const fn function_ref_from_function(
  function: &Function<'_>,
  node_id: NodeId,
) -> Option<FunctionRef> {
  if function.generator {
    return None;
  }
  Some(FunctionRef {
    node_id,
    span: function.span,
    is_async: function.r#async,
    is_generator: false,
  })
}

pub(super) fn vue_export_symbols(
  semantic: &oxc_semantic::Semantic<'_>,
) -> HashMap<SymbolId, String> {
  let mut exports = HashMap::new();
  for node in semantic.nodes() {
    let AstKind::ImportDeclaration(declaration) = node.kind() else {
      continue;
    };
    if declaration.import_kind == ImportOrExportKind::Type {
      continue;
    }
    if !is_vue_package(declaration.source.value.as_str()) {
      continue;
    }
    let Some(specifiers) = &declaration.specifiers else {
      continue;
    };
    for specifier in specifiers {
      let ImportDeclarationSpecifier::ImportSpecifier(specifier) = specifier else {
        continue;
      };
      if specifier.import_kind == ImportOrExportKind::Type {
        continue;
      }
      let Some(symbol_id) = specifier.local.symbol_id.get() else {
        continue;
      };
      exports.insert(symbol_id, module_export_name(&specifier.imported));
    }
  }
  exports
}

fn is_vue_package(source: &str) -> bool {
  matches!(source, "vue" | "@vue/runtime-core" | "@vue/runtime-dom" | "@vue/reactivity")
}

fn module_export_name(name: &ModuleExportName<'_>) -> String {
  match name {
    ModuleExportName::IdentifierName(name) => name.name.to_string(),
    ModuleExportName::IdentifierReference(name) => name.name.to_string(),
    ModuleExportName::StringLiteral(name) => name.value.to_string(),
  }
}

pub(super) fn watcher_api(
  semantic: &oxc_semantic::Semantic<'_>,
  call: &CallExpression<'_>,
  vue_exports: &HashMap<SymbolId, String>,
) -> Option<WatcherApiKind> {
  match vue_callee_export(semantic, &call.callee, vue_exports)? {
    "watch" => Some(WatcherApiKind::Watch),
    "watchEffect" => Some(WatcherApiKind::WatchEffect),
    "watchPostEffect" => Some(WatcherApiKind::WatchPostEffect),
    "watchSyncEffect" => Some(WatcherApiKind::WatchSyncEffect),
    _ => None,
  }
}

pub(super) fn vue_callee_export<'a>(
  semantic: &oxc_semantic::Semantic<'_>,
  callee: &Expression<'a>,
  vue_exports: &'a HashMap<SymbolId, String>,
) -> Option<&'a str> {
  let identifier = callee.get_inner_expression().get_identifier_reference()?;
  let symbol_id = referenced_symbol(semantic, identifier)?;
  vue_exports.get(&symbol_id).map(String::as_str)
}

pub(super) fn referenced_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  identifier: &oxc_ast::ast::IdentifierReference<'_>,
) -> Option<SymbolId> {
  let reference_id = identifier.reference_id.get()?;
  semantic.scoping().get_reference(reference_id).symbol_id()
}

pub(super) const fn callback_argument_index(api: WatcherApiKind) -> usize {
  match api {
    WatcherApiKind::Watch => 1,
    WatcherApiKind::WatchEffect
    | WatcherApiKind::WatchPostEffect
    | WatcherApiKind::WatchSyncEffect => 0,
  }
}

pub(super) const fn cleanup_parameter_index(api: WatcherApiKind) -> usize {
  match api {
    WatcherApiKind::Watch => 2,
    WatcherApiKind::WatchEffect
    | WatcherApiKind::WatchPostEffect
    | WatcherApiKind::WatchSyncEffect => 0,
  }
}

pub(super) fn argument_expression<'a>(argument: &'a Argument<'a>) -> Option<&'a Expression<'a>> {
  Some(argument.as_expression()?.get_inner_expression())
}

pub(super) fn call_has_spread(call: &CallExpression<'_>) -> bool {
  call.arguments.iter().any(|argument| matches!(argument, Argument::SpreadElement(_)))
}

pub(super) fn symbol_has_write(semantic: &oxc_semantic::Semantic<'_>, symbol_id: SymbolId) -> bool {
  semantic.scoping().get_resolved_references(symbol_id).any(oxc_semantic::Reference::is_write)
}

pub(super) fn enclosing_function(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  memo: &mut HashMap<NodeId, Option<NodeId>>,
) -> Option<NodeId> {
  if let Some(&cached) = memo.get(&node_id) {
    return cached;
  }
  let parent = semantic.nodes().parent_id(node_id);
  let result = if parent == node_id {
    None
  } else {
    match semantic.nodes().kind(parent) {
      AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => Some(parent),
      AstKind::Program(_) => None,
      _ => enclosing_function(semantic, parent, memo),
    }
  };
  memo.insert(node_id, result);
  result
}

pub(super) fn uncertain_to_function(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
  function_id: NodeId,
) -> bool {
  let mut current = node_id;
  loop {
    let parent = semantic.nodes().parent_id(current);
    if parent == current {
      return false;
    }
    if parent == function_id {
      return false;
    }
    match semantic.nodes().kind(parent) {
      AstKind::IfStatement(_)
      | AstKind::ForStatement(_)
      | AstKind::ForInStatement(_)
      | AstKind::ForOfStatement(_)
      | AstKind::WhileStatement(_)
      | AstKind::DoWhileStatement(_)
      | AstKind::SwitchStatement(_)
      | AstKind::TryStatement(_)
      | AstKind::ConditionalExpression(_)
      | AstKind::LogicalExpression(_)
      | AstKind::Function(_)
      | AstKind::ArrowFunctionExpression(_) => return true,
      _ => current = parent,
    }
  }
}

pub(super) fn is_global_host(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
) -> bool {
  let Some(identifier) = expression.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  identifier.is_global_reference(semantic.scoping())
    && matches!(identifier.name.as_str(), "window" | "globalThis" | "self")
}

pub(super) fn is_global_name(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  name: &str,
) -> bool {
  let Some(identifier) = expression.get_inner_expression().get_identifier_reference() else {
    return false;
  };
  identifier.name.as_str() == name && identifier.is_global_reference(semantic.scoping())
}

pub(super) fn is_proven_promise(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
) -> bool {
  match expression.get_inner_expression() {
    Expression::Identifier(identifier) => {
      identifier.name.as_str() == "Promise" && identifier.is_global_reference(semantic.scoping())
    }
    Expression::CallExpression(call) => {
      let Expression::StaticMemberExpression(member) = call.callee.get_inner_expression() else {
        return false;
      };
      is_global_name(semantic, &member.object, "Promise")
        && matches!(member.property.name.as_str(), "resolve" | "reject")
    }
    Expression::NewExpression(expression) => {
      is_global_name(semantic, &expression.callee, "Promise")
    }
    _ => false,
  }
}

pub(super) fn callback_parameter_at(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  index: usize,
) -> Option<SymbolId> {
  let pattern = match semantic.nodes().kind(function_id) {
    AstKind::Function(function) => &function.params.items.get(index)?.pattern,
    AstKind::ArrowFunctionExpression(arrow) => &arrow.params.items.get(index)?.pattern,
    _ => return None,
  };
  simple_binding_symbol(pattern)
}

fn simple_binding_symbol(pattern: &BindingPattern<'_>) -> Option<SymbolId> {
  match pattern {
    BindingPattern::BindingIdentifier(identifier) => identifier.symbol_id.get(),
    BindingPattern::AssignmentPattern(assignment) => simple_binding_symbol(&assignment.left),
    _ => None,
  }
}

pub(super) fn export_local_symbol_id(
  semantic: &oxc_semantic::Semantic<'_>,
  name: &ModuleExportName<'_>,
) -> Option<SymbolId> {
  let scoping = semantic.scoping();
  match name {
    ModuleExportName::IdentifierReference(identifier) => {
      let reference_id = identifier.reference_id.get()?;
      scoping.get_reference(reference_id).symbol_id()
    }
    ModuleExportName::IdentifierName(identifier) => {
      scoping.get_binding(scoping.root_scope_id(), identifier.name)
    }
    ModuleExportName::StringLiteral(literal) => {
      scoping.get_binding(scoping.root_scope_id(), literal.value.as_str().into())
    }
  }
}
