//! Oxc node walks → Vue Vet script facts.
use std::collections::{BTreeMap, BTreeSet, HashSet};

use oxc_ast::{
  AstKind,
  ast::{
    AssignmentTarget, AssignmentTargetMaybeDefault, AssignmentTargetProperty, BindingIdentifier,
    BindingPattern, CallExpression, Declaration, ExportDefaultDeclarationKind, Expression,
    IdentifierReference, ImportDeclarationSpecifier, ImportOrExportKind, ModuleExportName,
    ObjectPropertyKind, SimpleAssignmentTarget,
  },
};
use oxc_semantic::{NodeId, SymbolId};
use oxc_span::{GetSpan, Span};
use vue_vet_core::{
  ScriptBindingFact, ScriptCallFact, ScriptDestructureFact, ScriptImportFact,
  ScriptMemberWriteFact, ScriptOperandFact, SourceSpan,
};

pub struct CollectedNodeFacts {
  pub calls: Vec<ScriptCallFact>,
  pub member_writes: Vec<ScriptMemberWriteFact>,
  pub destructures: Vec<ScriptDestructureFact>,
  pub top_level_await_ends: Vec<usize>,
  pub operands: Vec<ScriptOperandFact>,
}

impl CollectedNodeFacts {
  /// Oxc node iteration is not a source-order guarantee. Sort span-keyed
  /// vectors once here so callers do not grow a parallel laundry list of
  /// `sort_by_key` lines whenever a new fact kind is added.
  pub fn into_source_order(mut self) -> Self {
    self.calls.sort_by_key(|fact| fact.span.offset);
    self.member_writes.sort_by_key(|fact| fact.span.offset);
    self.destructures.sort_by_key(|fact| fact.span.offset);
    self.operands.sort_by_key(|fact| fact.span.offset);
    self.top_level_await_ends.sort_unstable();
    self.top_level_await_ends.dedup();
    self
  }
}

pub fn collect_import_facts(
  semantic: &oxc_semantic::Semantic<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) -> (Vec<ScriptImportFact>, BTreeMap<String, (String, String)>) {
  let mut imports = Vec::new();
  let mut imported_bindings = BTreeMap::new();

  for node in semantic.nodes() {
    match node.kind() {
      AstKind::ImportDeclaration(declaration) => {
        let source = declaration.source.value.to_string();
        let declaration_span = source_span(line_index, sfc_source, script_offset, declaration.span);
        let declaration_type_only = declaration.import_kind == ImportOrExportKind::Type;
        let Some(specifiers) = &declaration.specifiers else {
          imports.push(ScriptImportFact {
            source,
            imported: String::new(),
            local: String::new(),
            span: declaration_span,
            type_only: declaration_type_only,
            declaration_span,
          });
          continue;
        };
        for specifier in specifiers {
          let (imported, local, span, specifier_type_only) = match specifier {
            ImportDeclarationSpecifier::ImportSpecifier(specifier) => (
              module_export_name(&specifier.imported),
              specifier.local.name.to_string(),
              specifier.span,
              specifier.import_kind == ImportOrExportKind::Type,
            ),
            ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
              ("default".into(), specifier.local.name.to_string(), specifier.span, false)
            }
            ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
              ("*".into(), specifier.local.name.to_string(), specifier.span, false)
            }
          };
          imported_bindings.insert(local.clone(), (source.clone(), imported.clone()));
          imports.push(ScriptImportFact {
            source: source.clone(),
            imported,
            local,
            span: source_span(line_index, sfc_source, script_offset, span),
            type_only: declaration_type_only || specifier_type_only,
            declaration_span,
          });
        }
      }
      // Barrel / re-export edges: `export * from './map'` and `export { x } from './x'`.
      // Structural linking treats these like imports so Factory/Composable seeds flow
      // through index barrels (specifier is what ModuleLink resolves).
      AstKind::ExportAllDeclaration(declaration) if declaration.exported.is_none() => {
        let span = source_span(line_index, sfc_source, script_offset, declaration.span);
        imports.push(ScriptImportFact {
          source: declaration.source.value.to_string(),
          imported: "*".into(),
          local: String::new(),
          span,
          type_only: declaration.export_kind == ImportOrExportKind::Type,
          declaration_span: span,
        });
      }
      AstKind::ExportNamedDeclaration(declaration) => {
        let Some(source) = &declaration.source else {
          continue;
        };
        let source = source.value.to_string();
        let declaration_span = source_span(line_index, sfc_source, script_offset, declaration.span);
        let declaration_type_only = declaration.export_kind == ImportOrExportKind::Type;
        if declaration.specifiers.is_empty() {
          imports.push(ScriptImportFact {
            source,
            imported: String::new(),
            local: String::new(),
            span: declaration_span,
            type_only: declaration_type_only,
            declaration_span,
          });
          continue;
        }
        for specifier in &declaration.specifiers {
          imports.push(ScriptImportFact {
            source: source.clone(),
            imported: module_export_name(&specifier.local),
            local: module_export_name(&specifier.exported),
            span: source_span(line_index, sfc_source, script_offset, specifier.span),
            type_only: declaration_type_only || specifier.export_kind == ImportOrExportKind::Type,
            declaration_span,
          });
        }
      }
      _ => {}
    }
  }

  imports.sort_by_key(|fact| fact.span.offset);
  (imports, imported_bindings)
}

pub fn collect_binding_facts(
  semantic: &oxc_semantic::Semantic<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) -> Vec<ScriptBindingFact> {
  let exported_symbols = collect_exported_symbol_ids(semantic);
  let escaped_symbols = collect_escaped_symbol_ids(semantic);
  let scoping = semantic.scoping();
  let mut bindings = scoping
    .symbol_ids()
    .map(|symbol_id| {
      let references = scoping.get_resolved_references(symbol_id);
      let (reads, writes) = references.fold((0_usize, 0_usize), |(reads, writes), reference| {
        (
          reads.saturating_add(usize::from(reference.is_read())),
          writes.saturating_add(usize::from(reference.is_write())),
        )
      });
      let exported = exported_symbols.contains(&symbol_id);
      ScriptBindingFact {
        name: scoping.symbol_name(symbol_id).into(),
        reads,
        writes,
        span: source_span(line_index, sfc_source, script_offset, scoping.symbol_span(symbol_id)),
        exported,
        plain_initializer: symbol_has_plain_initializer(semantic, symbol_id),
        escaped: exported || escaped_symbols.contains(&symbol_id),
      }
    })
    .collect::<Vec<_>>();
  // Symbol iteration order is not a source-order contract.
  bindings.sort_by_key(|fact| fact.span.offset);
  bindings
}

fn collect_escaped_symbol_ids(semantic: &oxc_semantic::Semantic<'_>) -> BTreeSet<SymbolId> {
  let mut symbols = BTreeSet::new();
  for node in semantic.nodes() {
    match node.kind() {
      AstKind::ObjectProperty(property) => {
        if let Some(identifier) = property.value.get_identifier_reference() {
          collect_referenced_symbol_id(semantic, identifier, &mut symbols);
        }
      }
      AstKind::ArrayExpression(array) => {
        for element in &array.elements {
          let Some(expression) = element.as_expression() else {
            continue;
          };
          if let Some(identifier) = expression.get_identifier_reference() {
            collect_referenced_symbol_id(semantic, identifier, &mut symbols);
          }
        }
      }
      AstKind::ReturnStatement(statement) => {
        let Some(argument) = &statement.argument else {
          continue;
        };
        if let Some(identifier) = argument.get_identifier_reference() {
          collect_referenced_symbol_id(semantic, identifier, &mut symbols);
        }
      }
      _ => {}
    }
  }
  symbols
}

fn symbol_has_plain_initializer(
  semantic: &oxc_semantic::Semantic<'_>,
  symbol_id: SymbolId,
) -> bool {
  let declaration = semantic.symbol_declaration(symbol_id);
  let declarator = match declaration.kind() {
    AstKind::VariableDeclarator(declarator) => declarator,
    AstKind::BindingIdentifier(_) => match semantic.nodes().parent_kind(declaration.id()) {
      AstKind::VariableDeclarator(declarator) => declarator,
      _ => return false,
    },
    _ => return false,
  };
  declarator
    .init
    .as_ref()
    .is_none_or(|initializer| expression_is_plain_primitive(semantic, initializer))
}

fn expression_is_plain_primitive(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
) -> bool {
  match peel_ts_parens(expression) {
    Expression::StringLiteral(_)
    | Expression::NumericLiteral(_)
    | Expression::BooleanLiteral(_)
    | Expression::NullLiteral(_)
    | Expression::BigIntLiteral(_)
    | Expression::RegExpLiteral(_) => true,
    Expression::TemplateLiteral(literal) => literal.expressions.is_empty(),
    Expression::UnaryExpression(unary) => expression_is_plain_primitive(semantic, &unary.argument),
    Expression::Identifier(identifier) => identifier_is_global_undefined(semantic, identifier),
    _ => false,
  }
}

fn identifier_is_global_undefined(
  semantic: &oxc_semantic::Semantic<'_>,
  identifier: &IdentifierReference<'_>,
) -> bool {
  if identifier.name.as_str() != "undefined" {
    return false;
  }
  let Some(reference_id) = identifier.reference_id.get() else {
    return false;
  };
  semantic.scoping().get_reference(reference_id).symbol_id().is_none()
}

fn collect_exported_symbol_ids(semantic: &oxc_semantic::Semantic<'_>) -> BTreeSet<SymbolId> {
  let mut symbols = BTreeSet::new();
  for node in semantic.nodes() {
    match node.kind() {
      AstKind::ExportNamedDeclaration(declaration) if declaration.source.is_none() => {
        if let Some(inner) = &declaration.declaration {
          collect_declaration_symbol_ids(inner, &mut symbols);
        }
        for specifier in &declaration.specifiers {
          if let Some(symbol_id) = export_local_symbol_id(semantic, &specifier.local) {
            symbols.insert(symbol_id);
          }
        }
      }
      AstKind::ExportDefaultDeclaration(declaration) => {
        collect_default_export_symbol_ids(semantic, &declaration.declaration, &mut symbols);
      }
      _ => {}
    }
  }
  symbols
}

fn collect_declaration_symbol_ids(declaration: &Declaration<'_>, symbols: &mut BTreeSet<SymbolId>) {
  match declaration {
    Declaration::VariableDeclaration(variable) => {
      for declarator in &variable.declarations {
        collect_pattern_symbol_ids(&declarator.id, symbols);
      }
    }
    Declaration::FunctionDeclaration(function) => {
      collect_binding_identifier_symbol(function.id.as_ref(), symbols);
    }
    Declaration::ClassDeclaration(class) => {
      collect_binding_identifier_symbol(class.id.as_ref(), symbols);
    }
    _ => {}
  }
}

fn collect_default_export_symbol_ids(
  semantic: &oxc_semantic::Semantic<'_>,
  declaration: &ExportDefaultDeclarationKind<'_>,
  symbols: &mut BTreeSet<SymbolId>,
) {
  match declaration {
    ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
      collect_binding_identifier_symbol(function.id.as_ref(), symbols);
    }
    ExportDefaultDeclarationKind::ClassDeclaration(class) => {
      collect_binding_identifier_symbol(class.id.as_ref(), symbols);
    }
    other => {
      if let Some(identifier) = other.as_expression().and_then(Expression::get_identifier_reference)
      {
        collect_referenced_symbol_id(semantic, identifier, symbols);
      }
    }
  }
}

fn collect_pattern_symbol_ids(pattern: &BindingPattern<'_>, symbols: &mut BTreeSet<SymbolId>) {
  match pattern {
    BindingPattern::BindingIdentifier(identifier) => {
      collect_binding_identifier_symbol(Some(identifier), symbols);
    }
    BindingPattern::ObjectPattern(object) => {
      for property in &object.properties {
        collect_pattern_symbol_ids(&property.value, symbols);
      }
      if let Some(rest) = &object.rest {
        collect_pattern_symbol_ids(&rest.argument, symbols);
      }
    }
    BindingPattern::ArrayPattern(array) => {
      for element in array.elements.iter().flatten() {
        collect_pattern_symbol_ids(element, symbols);
      }
      if let Some(rest) = &array.rest {
        collect_pattern_symbol_ids(&rest.argument, symbols);
      }
    }
    BindingPattern::AssignmentPattern(assignment) => {
      collect_pattern_symbol_ids(&assignment.left, symbols);
    }
  }
}

fn collect_binding_identifier_symbol(
  identifier: Option<&BindingIdentifier<'_>>,
  symbols: &mut BTreeSet<SymbolId>,
) {
  if let Some(symbol_id) = identifier.and_then(|identifier| identifier.symbol_id.get()) {
    symbols.insert(symbol_id);
  }
}

fn collect_referenced_symbol_id(
  semantic: &oxc_semantic::Semantic<'_>,
  identifier: &IdentifierReference<'_>,
  symbols: &mut BTreeSet<SymbolId>,
) {
  let Some(reference_id) = identifier.reference_id.get() else {
    return;
  };
  if let Some(symbol_id) = semantic.scoping().get_reference(reference_id).symbol_id() {
    symbols.insert(symbol_id);
  }
}

fn export_local_symbol_id(
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

pub fn collect_node_facts(
  semantic: &oxc_semantic::Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) -> CollectedNodeFacts {
  let mut calls = Vec::new();
  let mut member_writes = Vec::new();
  let mut destructures = Vec::new();
  let mut top_level_await_ends = Vec::new();
  let mut operands = Vec::new();
  let self_scheduling = self_scheduling_callback_symbols(semantic);
  for (node_id, node) in semantic.nodes().iter_enumerated() {
    match node.kind() {
      AstKind::CallExpression(call) => {
        let Some(callee) = call_callee_name(&call.callee) else {
          continue;
        };
        let parent = semantic.nodes().parent_kind(node_id);
        let assigned_to = call_assigned_to(parent);
        if let AstKind::VariableDeclarator(declarator) = parent
          && matches!(
            &declarator.id,
            BindingPattern::ObjectPattern(_) | BindingPattern::ArrayPattern(_)
          )
        {
          destructures.push(ScriptDestructureFact {
            source_call: callee.clone(),
            span: source_span(line_index, sfc_source, script_offset, declarator.id.span()),
          });
        }
        let resolved_import =
          if callee.contains('.') { None } else { imported_bindings.get(&callee).cloned() };
        let (flush_option, flush_option_unresolved) = call_flush_option(call, imported_bindings);
        calls.push(ScriptCallFact {
          assigned_to,
          resolved_import,
          argument_identifiers: expression_argument_identifiers(call.arguments.iter()),
          enclosing_callees: enclosing_call_callees(semantic, node_id),
          has_function_argument: arguments_include_function(call.arguments.iter()),
          callback_reschedules_self: first_callback_symbol(semantic, call)
            .is_some_and(|symbol| self_scheduling.contains(&symbol)),
          flush_option,
          flush_option_unresolved,
          callee,
          span: source_span(line_index, sfc_source, script_offset, call.span),
        });
      }
      AstKind::NewExpression(expression) => {
        let Some(callee) = call_callee_name(&expression.callee) else {
          continue;
        };
        let parent = semantic.nodes().parent_kind(node_id);
        let assigned_to = call_assigned_to(parent);
        let resolved_import =
          if callee.contains('.') { None } else { imported_bindings.get(&callee).cloned() };
        calls.push(ScriptCallFact {
          assigned_to,
          resolved_import,
          argument_identifiers: expression_argument_identifiers(expression.arguments.iter()),
          enclosing_callees: enclosing_call_callees(semantic, node_id),
          has_function_argument: arguments_include_function(expression.arguments.iter()),
          callback_reschedules_self: false,
          flush_option: None,
          flush_option_unresolved: false,
          callee,
          span: source_span(line_index, sfc_source, script_offset, expression.span),
        });
      }
      AstKind::AssignmentExpression(assignment) => {
        collect_assignment_member_writes(
          &assignment.left,
          line_index,
          sfc_source,
          script_offset,
          &mut member_writes,
        );
      }
      AstKind::UpdateExpression(update) => {
        if let Some(write) = update_member(&update.argument, line_index, sfc_source, script_offset)
        {
          member_writes.push(write);
        }
      }
      AstKind::AwaitExpression(await_expression) => {
        if is_module_top_level_await(semantic, node_id) {
          let span = source_span(line_index, sfc_source, script_offset, await_expression.span);
          top_level_await_ends.push(span.offset.saturating_add(span.length));
        }
      }
      AstKind::BinaryExpression(binary) => {
        push_operand_identifier(
          &mut operands,
          semantic,
          &binary.left,
          line_index,
          sfc_source,
          script_offset,
        );
        push_operand_identifier(
          &mut operands,
          semantic,
          &binary.right,
          line_index,
          sfc_source,
          script_offset,
        );
      }
      AstKind::LogicalExpression(logical) => {
        push_operand_identifier(
          &mut operands,
          semantic,
          &logical.left,
          line_index,
          sfc_source,
          script_offset,
        );
        push_operand_identifier(
          &mut operands,
          semantic,
          &logical.right,
          line_index,
          sfc_source,
          script_offset,
        );
      }
      AstKind::UnaryExpression(unary) => {
        push_operand_identifier(
          &mut operands,
          semantic,
          &unary.argument,
          line_index,
          sfc_source,
          script_offset,
        );
      }
      _ => {}
    }
  }
  CollectedNodeFacts { calls, member_writes, destructures, top_level_await_ends, operands }
}

fn module_export_name(name: &ModuleExportName<'_>) -> String {
  match name {
    ModuleExportName::IdentifierName(name) => name.name.to_string(),
    ModuleExportName::IdentifierReference(name) => name.name.to_string(),
    ModuleExportName::StringLiteral(name) => name.value.to_string(),
  }
}

fn collect_assignment_member_writes(
  target: &AssignmentTarget<'_>,
  index: &vue_vet_core::LineIndex,
  source: &str,
  offset: usize,
  writes: &mut Vec<ScriptMemberWriteFact>,
) {
  match target {
    AssignmentTarget::StaticMemberExpression(member) => {
      if let Some(write) = member_write(
        &member.object,
        Some(member.property.name.as_str()),
        member.span,
        index,
        source,
        offset,
      ) {
        writes.push(write);
      }
    }
    AssignmentTarget::ComputedMemberExpression(member) => {
      if let Some(write) = member_write(
        &member.object,
        member.static_property_name().as_deref(),
        member.span,
        index,
        source,
        offset,
      ) {
        writes.push(write);
      }
    }
    AssignmentTarget::ArrayAssignmentTarget(array) => {
      for element in array.elements.iter().flatten() {
        collect_maybe_default_member_writes(element, index, source, offset, writes);
      }
      if let Some(rest) = &array.rest {
        collect_assignment_member_writes(&rest.target, index, source, offset, writes);
      }
    }
    AssignmentTarget::ObjectAssignmentTarget(object) => {
      for property in &object.properties {
        if let AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) = property {
          collect_maybe_default_member_writes(&property.binding, index, source, offset, writes);
        }
      }
      if let Some(rest) = &object.rest {
        collect_assignment_member_writes(&rest.target, index, source, offset, writes);
      }
    }
    AssignmentTarget::TSAsExpression(inner) => {
      collect_expression_member_write(&inner.expression, index, source, offset, writes);
    }
    AssignmentTarget::TSSatisfiesExpression(inner) => {
      collect_expression_member_write(&inner.expression, index, source, offset, writes);
    }
    AssignmentTarget::TSNonNullExpression(inner) => {
      collect_expression_member_write(&inner.expression, index, source, offset, writes);
    }
    AssignmentTarget::TSTypeAssertion(inner) => {
      collect_expression_member_write(&inner.expression, index, source, offset, writes);
    }
    _ => {}
  }
}

fn collect_maybe_default_member_writes(
  target: &AssignmentTargetMaybeDefault<'_>,
  index: &vue_vet_core::LineIndex,
  source: &str,
  offset: usize,
  writes: &mut Vec<ScriptMemberWriteFact>,
) {
  match target {
    AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(with_default) => {
      collect_assignment_member_writes(&with_default.binding, index, source, offset, writes);
    }
    other => {
      if let Some(assignment_target) = other.as_assignment_target() {
        collect_assignment_member_writes(assignment_target, index, source, offset, writes);
      }
    }
  }
}

fn collect_expression_member_write(
  expression: &Expression<'_>,
  index: &vue_vet_core::LineIndex,
  source: &str,
  offset: usize,
  writes: &mut Vec<ScriptMemberWriteFact>,
) {
  match peel_ts_parens(expression) {
    Expression::StaticMemberExpression(member) => {
      if let Some(write) = member_write(
        &member.object,
        Some(member.property.name.as_str()),
        member.span,
        index,
        source,
        offset,
      ) {
        writes.push(write);
      }
    }
    Expression::ComputedMemberExpression(member) => {
      if let Some(write) = member_write(
        &member.object,
        member.static_property_name().as_deref(),
        member.span,
        index,
        source,
        offset,
      ) {
        writes.push(write);
      }
    }
    _ => {}
  }
}

fn peel_ts_parens<'a>(expression: &'a Expression<'a>) -> &'a Expression<'a> {
  expression.get_inner_expression()
}

fn update_member(
  target: &SimpleAssignmentTarget<'_>,
  index: &vue_vet_core::LineIndex,
  source: &str,
  offset: usize,
) -> Option<ScriptMemberWriteFact> {
  match target {
    SimpleAssignmentTarget::StaticMemberExpression(member) => member_write(
      &member.object,
      Some(member.property.name.as_str()),
      member.span,
      index,
      source,
      offset,
    ),
    SimpleAssignmentTarget::ComputedMemberExpression(member) => member_write(
      &member.object,
      member.static_property_name().as_deref(),
      member.span,
      index,
      source,
      offset,
    ),
    _ => None,
  }
}

fn member_write(
  object: &Expression<'_>,
  property: Option<&str>,
  span: Span,
  index: &vue_vet_core::LineIndex,
  source: &str,
  offset: usize,
) -> Option<ScriptMemberWriteFact> {
  let object = object.get_identifier_reference()?.name.to_string();
  Some(ScriptMemberWriteFact {
    object,
    property: property.map(str::to_owned),
    span: source_span(index, source, offset, span),
  })
}

fn is_module_top_level_await(
  semantic: &oxc_semantic::Semantic<'_>,
  await_id: oxc_semantic::NodeId,
) -> bool {
  !semantic.nodes().ancestor_ids(await_id).any(|ancestor_id| {
    matches!(
      semantic.nodes().kind(ancestor_id),
      AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Class(_)
    )
  })
}

fn push_operand_identifier(
  operands: &mut Vec<ScriptOperandFact>,
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) {
  let Expression::Identifier(identifier) = expression else {
    return;
  };
  operands.push(ScriptOperandFact {
    name: identifier.name.to_string(),
    span: source_span(line_index, sfc_source, script_offset, identifier.span),
    binding_span: identifier_binding_span(
      semantic,
      identifier,
      line_index,
      sfc_source,
      script_offset,
    ),
  });
}

fn identifier_binding_span(
  semantic: &oxc_semantic::Semantic<'_>,
  identifier: &IdentifierReference<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) -> Option<SourceSpan> {
  let reference_id = identifier.reference_id.get()?;
  let symbol_id = semantic.scoping().get_reference(reference_id).symbol_id()?;
  Some(source_span(
    line_index,
    sfc_source,
    script_offset,
    semantic.scoping().symbol_span(symbol_id),
  ))
}

fn call_callee_name(callee: &Expression<'_>) -> Option<String> {
  let callee = callee.get_inner_expression();
  if let Some(identifier) = callee.get_identifier_reference() {
    return Some(identifier.name.to_string());
  }
  match callee {
    Expression::StaticMemberExpression(member) => {
      let object = member.object.get_inner_expression().get_identifier_reference()?;
      Some(format!("{}.{}", object.name, member.property.name))
    }
    _ => None,
  }
}

fn call_assigned_to(parent: AstKind<'_>) -> Option<String> {
  match parent {
    AstKind::VariableDeclarator(declarator) => match &declarator.id {
      BindingPattern::BindingIdentifier(binding) => Some(binding.name.to_string()),
      _ => None,
    },
    AstKind::AssignmentExpression(assignment) => match &assignment.left {
      AssignmentTarget::AssignmentTargetIdentifier(binding) => Some(binding.name.to_string()),
      _ => None,
    },
    _ => None,
  }
}

fn argument_expression<'a>(argument: &'a oxc_ast::ast::Argument<'a>) -> Option<&'a Expression<'a>> {
  Some(argument.as_expression()?.get_inner_expression())
}

fn expression_argument_identifiers<'a, I>(arguments: I) -> Vec<String>
where
  I: Iterator<Item = &'a oxc_ast::ast::Argument<'a>>,
{
  arguments
    .filter_map(|argument| {
      argument_expression(argument)?.get_identifier_reference().map(|id| id.name.to_string())
    })
    .collect()
}

fn arguments_include_function<'a, I>(mut arguments: I) -> bool
where
  I: Iterator<Item = &'a oxc_ast::ast::Argument<'a>>,
{
  arguments.any(|argument| {
    matches!(
      argument_expression(argument),
      Some(Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_))
    )
  })
}

fn enclosing_call_callees(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: oxc_semantic::NodeId,
) -> Vec<String> {
  semantic
    .nodes()
    .ancestor_kinds(node_id)
    .filter_map(|kind| match kind {
      AstKind::CallExpression(call) => call_callee_name(&call.callee),
      AstKind::NewExpression(expression) => call_callee_name(&expression.callee),
      _ => None,
    })
    .collect()
}

fn is_request_animation_frame(callee: &Expression<'_>) -> bool {
  call_callee_name(callee)
    .is_some_and(|name| name == "requestAnimationFrame" || name.ends_with(".requestAnimationFrame"))
}

fn self_scheduling_callback_symbols(semantic: &oxc_semantic::Semantic<'_>) -> HashSet<SymbolId> {
  let mut symbols = HashSet::new();
  for (node_id, node) in semantic.nodes().iter_enumerated() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    if !is_request_animation_frame(&call.callee) {
      continue;
    }
    let Some(argument_symbol) = first_argument_symbol(semantic, call) else {
      continue;
    };
    if nearest_enclosing_callback_symbol(semantic, node_id) == Some(argument_symbol) {
      symbols.insert(argument_symbol);
    }
  }
  symbols
}

fn nearest_enclosing_callback_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
) -> Option<SymbolId> {
  for ancestor_id in semantic.nodes().ancestor_ids(node_id) {
    if ancestor_id == node_id {
      continue;
    }
    match semantic.nodes().kind(ancestor_id) {
      AstKind::Function(function) => {
        return function
          .id
          .as_ref()
          .and_then(|id| id.symbol_id.get())
          .or_else(|| enclosing_assigned_symbol(semantic, ancestor_id));
      }
      AstKind::ArrowFunctionExpression(_) => {
        return enclosing_assigned_symbol(semantic, ancestor_id);
      }
      _ => {}
    }
  }
  None
}

fn first_callback_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  call: &CallExpression<'_>,
) -> Option<SymbolId> {
  let argument = argument_expression(call.arguments.first()?)?;
  match argument {
    Expression::FunctionExpression(function) => {
      function.id.as_ref().and_then(|id| id.symbol_id.get())
    }
    _ => argument.get_identifier_reference().and_then(|identifier| {
      let reference_id = identifier.reference_id.get()?;
      semantic.scoping().get_reference(reference_id).symbol_id()
    }),
  }
}

fn first_argument_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  call: &CallExpression<'_>,
) -> Option<SymbolId> {
  let argument = argument_expression(call.arguments.first()?)?;
  let identifier = argument.get_identifier_reference()?;
  let reference_id = identifier.reference_id.get()?;
  semantic.scoping().get_reference(reference_id).symbol_id()
}

fn enclosing_assigned_symbol(
  semantic: &oxc_semantic::Semantic<'_>,
  node_id: NodeId,
) -> Option<SymbolId> {
  for ancestor_id in semantic.nodes().ancestor_ids(node_id) {
    if ancestor_id == node_id {
      continue;
    }
    match semantic.nodes().kind(ancestor_id) {
      AstKind::VariableDeclarator(declarator) => {
        return match &declarator.id {
          BindingPattern::BindingIdentifier(identifier) => identifier.symbol_id.get(),
          _ => None,
        };
      }
      AstKind::AssignmentExpression(assignment) => {
        return match &assignment.left {
          AssignmentTarget::AssignmentTargetIdentifier(identifier) => {
            let reference_id = identifier.reference_id.get()?;
            semantic.scoping().get_reference(reference_id).symbol_id()
          }
          _ => None,
        };
      }
      AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Class(_) => {
        return None;
      }
      _ => {}
    }
  }
  None
}

fn call_flush_option(
  call: &CallExpression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> (Option<String>, bool) {
  let Some(index) = watcher_options_argument_index(&call.callee, imported_bindings) else {
    return (None, false);
  };
  for argument in call.arguments.iter().take(index.saturating_add(1)) {
    if argument.is_spread() {
      return (None, true);
    }
  }
  let Some(argument) = call.arguments.get(index).and_then(argument_expression) else {
    return (None, false);
  };
  let Expression::ObjectExpression(object) = argument.get_inner_expression() else {
    return (None, true);
  };
  let mut unresolved = false;
  let mut flush = None;
  for property in &object.properties {
    match property {
      ObjectPropertyKind::SpreadProperty(_) => {
        flush = None;
        unresolved = true;
      }
      ObjectPropertyKind::ObjectProperty(property) => {
        let Some(name) = property.key.static_name() else {
          flush = None;
          unresolved = true;
          continue;
        };
        if name != "flush" {
          continue;
        }
        match property.value.get_inner_expression() {
          Expression::StringLiteral(literal)
            if matches!(literal.value.as_str(), "pre" | "post" | "sync") =>
          {
            flush = Some(literal.value.to_string());
            unresolved = false;
          }
          _ => {
            flush = None;
            unresolved = true;
          }
        }
      }
    }
  }
  (flush, unresolved)
}

fn watcher_options_argument_index(
  callee: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> Option<usize> {
  match watcher_api_name(callee, imported_bindings)? {
    "watchEffect" | "watchPostEffect" | "watchSyncEffect" => Some(1),
    "watch" => Some(2),
    _ => None,
  }
}

fn watcher_api_name(
  callee: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> Option<&'static str> {
  let callee = callee.get_inner_expression();
  if let Some(identifier) = callee.get_identifier_reference() {
    let local = identifier.name.as_str();
    if let Some((source, imported)) = imported_bindings.get(local) {
      return if is_vue_watch_source(source) { watcher_export(imported) } else { None };
    }
    return watcher_export(local);
  }
  let Expression::StaticMemberExpression(member) = callee else {
    return None;
  };
  let object = member.object.get_inner_expression().get_identifier_reference()?;
  let property = member.property.name.as_str();
  if let Some((source, imported)) = imported_bindings.get(object.name.as_str()) {
    return if is_vue_watch_source(source) && matches!(imported.as_str(), "*" | "default") {
      watcher_export(property)
    } else {
      None
    };
  }
  watcher_export(property)
}

fn is_vue_watch_source(source: &str) -> bool {
  source == "vue" || source == "vue-demi" || source == "#imports" || source.starts_with("@vue/")
}

fn watcher_export(name: &str) -> Option<&'static str> {
  match name {
    "watchEffect" => Some("watchEffect"),
    "watchPostEffect" => Some("watchPostEffect"),
    "watchSyncEffect" => Some("watchSyncEffect"),
    "watch" => Some("watch"),
    _ => None,
  }
}

pub fn source_span(
  index: &vue_vet_core::LineIndex,
  _source: &str,
  base: usize,
  span: Span,
) -> SourceSpan {
  let offset = base.saturating_add(usize::try_from(span.start).unwrap_or(usize::MAX));
  let end = base.saturating_add(usize::try_from(span.end).unwrap_or(usize::MAX));
  let (line, column) = index.byte_to_line_column(offset);
  SourceSpan { offset, length: end.saturating_sub(offset), line, column }
}
