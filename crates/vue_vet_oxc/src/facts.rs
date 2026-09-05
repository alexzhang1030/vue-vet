//! Oxc node walks → Vue Vet script facts.
use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::{
  AstKind,
  ast::{
    AssignmentTarget, BindingIdentifier, BindingPattern, Declaration, ExportDefaultDeclarationKind,
    Expression, IdentifierReference, ImportDeclarationSpecifier, ModuleExportName,
    SimpleAssignmentTarget,
  },
};
use oxc_semantic::SymbolId;
use oxc_span::Span;
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
        let Some(specifiers) = &declaration.specifiers else {
          imports.push(ScriptImportFact {
            source,
            imported: String::new(),
            local: String::new(),
            span: source_span(line_index, sfc_source, script_offset, declaration.span),
          });
          continue;
        };
        for specifier in specifiers {
          let (imported, local, span) = match specifier {
            ImportDeclarationSpecifier::ImportSpecifier(specifier) => (
              module_export_name(&specifier.imported),
              specifier.local.name.to_string(),
              specifier.span,
            ),
            ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
              ("default".into(), specifier.local.name.to_string(), specifier.span)
            }
            ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
              ("*".into(), specifier.local.name.to_string(), specifier.span)
            }
          };
          imported_bindings.insert(local.clone(), (source.clone(), imported.clone()));
          imports.push(ScriptImportFact {
            source: source.clone(),
            imported,
            local,
            span: source_span(line_index, sfc_source, script_offset, span),
          });
        }
      }
      // Barrel / re-export edges: `export * from './map'` and `export { x } from './x'`.
      // Structural linking treats these like imports so Factory/Composable seeds flow
      // through index barrels (specifier is what ModuleLink resolves).
      AstKind::ExportAllDeclaration(declaration) if declaration.exported.is_none() => {
        imports.push(ScriptImportFact {
          source: declaration.source.value.to_string(),
          imported: "*".into(),
          local: String::new(),
          span: source_span(line_index, sfc_source, script_offset, declaration.span),
        });
      }
      AstKind::ExportNamedDeclaration(declaration) => {
        let Some(source) = &declaration.source else {
          continue;
        };
        let source = source.value.to_string();
        if declaration.specifiers.is_empty() {
          imports.push(ScriptImportFact {
            source,
            imported: String::new(),
            local: String::new(),
            span: source_span(line_index, sfc_source, script_offset, declaration.span),
          });
          continue;
        }
        for specifier in &declaration.specifiers {
          imports.push(ScriptImportFact {
            source: source.clone(),
            imported: module_export_name(&specifier.local),
            local: module_export_name(&specifier.exported),
            span: source_span(line_index, sfc_source, script_offset, specifier.span),
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
      ScriptBindingFact {
        name: scoping.symbol_name(symbol_id).into(),
        reads,
        writes,
        span: source_span(line_index, sfc_source, script_offset, scoping.symbol_span(symbol_id)),
        exported: exported_symbols.contains(&symbol_id),
      }
    })
    .collect::<Vec<_>>();
  // Symbol iteration order is not a source-order contract.
  bindings.sort_by_key(|fact| fact.span.offset);
  bindings
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
  for (node_id, node) in semantic.nodes().iter_enumerated() {
    match node.kind() {
      AstKind::CallExpression(call) => {
        let Some(callee) = call_callee_name(&call.callee) else {
          continue;
        };
        let parent = semantic.nodes().parent_kind(node_id);
        let assigned_to = call_assigned_to(parent);
        if matches!(
          callee.as_str(),
          "defineProps"
            | "reactive"
            | "shallowReactive"
            | "toRefs"
            | "storeToRefs"
            | "useRoute"
            | "useRouter"
        ) && let AstKind::VariableDeclarator(declarator) = parent
          && let BindingPattern::ObjectPattern(pattern) = &declarator.id
        {
          destructures.push(ScriptDestructureFact {
            source_call: callee.clone(),
            span: source_span(line_index, sfc_source, script_offset, pattern.span),
          });
        }
        let resolved_import =
          if callee.contains('.') { None } else { imported_bindings.get(&callee).cloned() };
        calls.push(ScriptCallFact {
          assigned_to,
          resolved_import,
          argument_identifiers: expression_argument_identifiers(call.arguments.iter()),
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
          callee,
          span: source_span(line_index, sfc_source, script_offset, expression.span),
        });
      }
      AstKind::AssignmentExpression(assignment) => {
        if let Some(write) =
          assignment_member(&assignment.left, line_index, sfc_source, script_offset)
        {
          member_writes.push(write);
        }
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
        push_operand_identifier(&mut operands, &binary.left, line_index, sfc_source, script_offset);
        push_operand_identifier(
          &mut operands,
          &binary.right,
          line_index,
          sfc_source,
          script_offset,
        );
      }
      AstKind::LogicalExpression(logical) => {
        push_operand_identifier(
          &mut operands,
          &logical.left,
          line_index,
          sfc_source,
          script_offset,
        );
        push_operand_identifier(
          &mut operands,
          &logical.right,
          line_index,
          sfc_source,
          script_offset,
        );
      }
      AstKind::UnaryExpression(unary) => {
        push_operand_identifier(
          &mut operands,
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

fn assignment_member(
  target: &AssignmentTarget<'_>,
  index: &vue_vet_core::LineIndex,
  source: &str,
  offset: usize,
) -> Option<ScriptMemberWriteFact> {
  match target {
    AssignmentTarget::StaticMemberExpression(member) => member_write(
      &member.object,
      Some(member.property.name.as_str()),
      member.span,
      index,
      source,
      offset,
    ),
    AssignmentTarget::ComputedMemberExpression(member) => member_write(
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
  });
}

fn call_callee_name(callee: &Expression<'_>) -> Option<String> {
  if let Some(identifier) = callee.get_identifier_reference() {
    return Some(identifier.name.to_string());
  }
  match callee {
    Expression::StaticMemberExpression(member) => {
      let object = member.object.get_identifier_reference()?;
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

fn expression_argument_identifiers<'a, I>(arguments: I) -> Vec<String>
where
  I: Iterator<Item = &'a oxc_ast::ast::Argument<'a>>,
{
  arguments
    .filter_map(|argument| {
      argument.as_expression()?.get_identifier_reference().map(|id| id.name.to_string())
    })
    .collect()
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
