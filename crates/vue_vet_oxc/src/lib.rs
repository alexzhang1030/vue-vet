use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

use oxc_allocator::Allocator;
use oxc_ast::{
  AstKind,
  ast::{
    ArrowFunctionExpression, AssignmentTarget, BindingIdentifier, BindingPattern, Expression,
    Function, IdentifierReference, ImportDeclarationSpecifier, ModuleExportName,
    SimpleAssignmentTarget,
  },
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::{SourceType, Span};
use oxc_syntax::scope::ScopeFlags;
use thiserror::Error;
use vue_vet_core::{
  ScriptBindingFact, ScriptBlockFacts, ScriptCallFact, ScriptDestructureFact, ScriptImportFact,
  ScriptKind, ScriptMemberWriteFact, ScriptOperandFact, SourceSpan, TemplateFacts,
};
use vue_vet_plugins::default_trace_config;
use vue_vet_reactivity::{
  ModuleSummary, prepare_module_summary_with_config, trace_reactivity_with_config,
};

mod jsx;

#[derive(Debug, Error)]
pub enum AnalyzeScriptError {
  #[error("Oxc could not parse the script: {0}")]
  Parse(String),
  #[error("Oxc could not build script semantics: {0}")]
  Semantic(String),
  #[error("unsupported script language `{0}`")]
  UnsupportedLanguage(String),
}

/// Facts produced from one Oxc parse for both file rules and module linking.
#[derive(Debug)]
pub struct ModuleAnalysis {
  pub script_facts: ScriptBlockFacts,
  /// JSX/TSX lowered into template facts (empty when the script has no JSX).
  pub template_facts: TemplateFacts,
  pub module_trace: Arc<ModuleSummary>,
}

/// Analyze one extracted Vue SFC script block and map all facts to original
/// SFC byte offsets.
///
/// # Errors
///
/// Returns a deterministic parser or semantic error for invalid scripts, and
/// rejects script languages outside JavaScript, TypeScript, JSX, and TSX.
pub fn analyze_script(
  sfc_source: &str,
  script_source: &str,
  script_offset: usize,
  language: &str,
  kind: ScriptKind,
) -> Result<ScriptBlockFacts, AnalyzeScriptError> {
  analyze_module_source(sfc_source, script_source, script_offset, language, kind)
    .map(|analysis| analysis.script_facts)
}

/// Analyze one script surface once for file facts and cross-module linking.
///
/// # Errors
///
/// Returns a deterministic parser or semantic error for invalid scripts, and
/// rejects script languages outside JavaScript, TypeScript, JSX, and TSX.
pub fn analyze_module_source(
  sfc_source: &str,
  script_source: &str,
  script_offset: usize,
  language: &str,
  kind: ScriptKind,
) -> Result<ModuleAnalysis, AnalyzeScriptError> {
  let source_type = source_type(language)?;
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, script_source, source_type).parse();
  if !parsed.errors.is_empty() {
    return Err(AnalyzeScriptError::Parse(join_errors(&parsed.errors)));
  }

  let built = SemanticBuilder::new().with_check_syntax_error(true).build(&parsed.program);
  if !built.errors.is_empty() {
    return Err(AnalyzeScriptError::Semantic(join_errors(&built.errors)));
  }
  let semantic = built.semantic;
  let line_index = vue_vet_core::LineIndex::new(sfc_source);
  let (imports, imported_bindings) =
    collect_import_facts(&semantic, &line_index, sfc_source, script_offset);
  let bindings = collect_binding_facts(&semantic, &line_index, sfc_source, script_offset);
  let node_facts =
    collect_node_facts(&semantic, &imported_bindings, &line_index, sfc_source, script_offset)
      .into_source_order();
  // Plain JS/TS has no JSX nodes; skip the AST walk on the CodSpeed hot path.
  let template_facts = if matches!(language, "jsx" | "tsx") {
    jsx::collect_jsx_template_facts(&semantic, &line_index, sfc_source, script_offset)
  } else {
    TemplateFacts::default()
  };

  // Auto-load ecosystem plugins (Nuxt / vue-i18n) at the analysis boundary.
  let trace_config = default_trace_config();
  let reactivity_graph = Arc::new(trace_reactivity_with_config(
    &semantic,
    sfc_source,
    script_offset,
    kind,
    &trace_config,
  ));
  let module_trace = Arc::new(prepare_module_summary_with_config(
    &semantic,
    sfc_source,
    script_offset,
    kind,
    Arc::clone(&reactivity_graph),
    &trace_config,
  ));

  Ok(ModuleAnalysis {
    script_facts: ScriptBlockFacts {
      kind,
      language: language.into(),
      imports,
      bindings,
      calls: node_facts.calls,
      member_writes: node_facts.member_writes,
      destructures: node_facts.destructures,
      top_level_await_ends: node_facts.top_level_await_ends,
      operands: node_facts.operands,
      reactivity_graph,
    },
    template_facts,
    module_trace,
  })
}

struct CollectedNodeFacts {
  calls: Vec<ScriptCallFact>,
  member_writes: Vec<ScriptMemberWriteFact>,
  destructures: Vec<ScriptDestructureFact>,
  top_level_await_ends: Vec<usize>,
  operands: Vec<ScriptOperandFact>,
}

impl CollectedNodeFacts {
  /// Oxc node iteration is not a source-order guarantee. Sort span-keyed
  /// vectors once here so callers do not grow a parallel laundry list of
  /// `sort_by_key` lines whenever a new fact kind is added.
  fn into_source_order(mut self) -> Self {
    self.calls.sort_by_key(|fact| fact.span.offset);
    self.member_writes.sort_by_key(|fact| fact.span.offset);
    self.destructures.sort_by_key(|fact| fact.span.offset);
    self.operands.sort_by_key(|fact| fact.span.offset);
    self.top_level_await_ends.sort_unstable();
    self.top_level_await_ends.dedup();
    self
  }
}

fn collect_import_facts(
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

fn collect_binding_facts(
  semantic: &oxc_semantic::Semantic<'_>,
  line_index: &vue_vet_core::LineIndex,
  sfc_source: &str,
  script_offset: usize,
) -> Vec<ScriptBindingFact> {
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
      }
    })
    .collect::<Vec<_>>();
  // Symbol iteration order is not a source-order contract.
  bindings.sort_by_key(|fact| fact.span.offset);
  bindings
}

fn collect_node_facts(
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

/// Analyze a standalone JavaScript or TypeScript module.
///
/// # Errors
///
/// Returns a deterministic parser, semantic, or language-selection error.
pub fn analyze_module(
  source: &str,
  language: &str,
) -> Result<ScriptBlockFacts, AnalyzeScriptError> {
  analyze_script(source, source, 0, language, ScriptKind::Script)
}

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
  if !parsed.errors.is_empty() {
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

fn source_type(language: &str) -> Result<SourceType, AnalyzeScriptError> {
  match language {
    "js" | "javascript" => Ok(SourceType::mjs()),
    "jsx" => Ok(SourceType::jsx()),
    "ts" | "typescript" => Ok(SourceType::ts()),
    "tsx" => Ok(SourceType::tsx()),
    other => Err(AnalyzeScriptError::UnsupportedLanguage(other.into())),
  }
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

pub(crate) fn source_span(
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

fn join_errors(errors: &[impl ToString]) -> String {
  errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
}

#[cfg(test)]
mod tests {
  use super::*;
  use vue_vet_core::ReactiveReadKind;

  #[expect(clippy::panic, reason = "unexpected Oxc errors must fail adapter tests")]
  fn analyze(source: &str, language: &str) -> ScriptBlockFacts {
    match analyze_script(source, source, 0, language, ScriptKind::Setup) {
      Ok(facts) => facts,
      Err(error) => panic!("script analysis unexpectedly failed: {error}"),
    }
  }

  #[test]
  fn records_new_expressions_as_call_facts() {
    let facts = analyze(
      "const io = new IntersectionObserver(() => {});\
       const ro = new ResizeObserver(() => {}); io.disconnect();",
      "ts",
    );
    assert!(
      facts.calls.iter().any(|call| {
        call.callee == "IntersectionObserver" && call.assigned_to.as_deref() == Some("io")
      }),
      "new IntersectionObserver must become a ScriptCallFact; got {:?}",
      facts.calls
    );
    assert!(
      facts.calls.iter().any(|call| {
        call.callee == "ResizeObserver" && call.assigned_to.as_deref() == Some("ro")
      }),
      "new ResizeObserver must become a ScriptCallFact; got {:?}",
      facts.calls
    );
    assert!(
      facts.calls.iter().any(callee_is_disconnect),
      "member disconnect calls must remain queryable"
    );
  }

  fn callee_is_disconnect(call: &ScriptCallFact) -> bool {
    call.callee == "disconnect" || call.callee.ends_with(".disconnect")
  }

  #[test]
  fn records_member_callees_assignment_targets_and_identifier_args() {
    let facts = analyze(
      "let timer; clearTimeout(timer); timer = setTimeout(() => {}, 0);\
       window.addEventListener('resize', () => {});",
      "ts",
    );
    assert!(
      facts.calls.iter().any(|call| {
        call.callee == "setTimeout" && call.assigned_to.as_deref() == Some("timer")
      }),
      "assignment targets must populate ScriptCallFact.assigned_to"
    );
    assert!(
      facts.calls.iter().any(|call| {
        call.callee == "clearTimeout"
          && call.argument_identifiers.iter().any(|name| name == "timer")
      }),
      "identifier call arguments must remain queryable without exposing Oxc nodes"
    );
    assert!(
      facts.calls.iter().any(|call| call.callee == "window.addEventListener"),
      "static member callees must remain queryable without exposing Oxc nodes"
    );
  }

  #[test]
  fn resolves_aliased_vue_calls_and_member_writes() {
    let facts = analyze(
      "import { ref as makeRef } from 'vue';\
       const props = defineProps(); const x = makeRef(0); props.count += 1;",
      "ts",
    );
    assert!(
      facts.calls.iter().any(|call| {
        call.callee == "makeRef"
          && call
            .resolved_import
            .as_ref()
            .is_some_and(|(source, imported)| source == "vue" && imported == "ref")
      }),
      "aliased Vue imports must resolve at the fact boundary"
    );
    assert_eq!(
      facts
        .calls
        .iter()
        .find(|call| call.callee == "defineProps")
        .and_then(|call| call.assigned_to.as_deref()),
      Some("props"),
      "the identifier assigned from a compiler macro must remain queryable"
    );
    assert!(
      facts
        .member_writes
        .iter()
        .any(|write| { write.object == "props" && write.property.as_deref() == Some("count") }),
      "member writes must be queryable without exposing Oxc AST nodes"
    );
  }

  #[test]
  fn builds_conditional_watch_effect_edges_without_nested_callbacks() {
    let facts = analyze(
      "import { computed, ref, watchEffect } from 'vue';\
       const ready = computed(() => true); const value = ref(0); const nested = ref(0);\
       watchEffect(() => { if (!ready.value) return; console.log(value.value);\
         const later = () => nested.value; void later; });",
      "ts",
    );
    let effect = facts.reactivity_graph.effects.first();
    assert_eq!(effect.map(|effect| effect.callee.as_str()), Some("watchEffect"));
    assert_eq!(
      effect
        .into_iter()
        .flat_map(|effect| &effect.reads)
        .map(|read| (read.binding.as_str(), read.kind, read.guarded_by.as_deref()))
        .collect::<Vec<_>>(),
      [
        ("ready", ReactiveReadKind::Unconditional, None),
        ("value", ReactiveReadKind::Conditional, Some("ready")),
      ]
    );
  }

  #[test]
  fn records_props_destructures_and_null_template_refs() {
    let facts = analyze(
      "import { ref } from 'vue'; const { title } = defineProps(); const input = ref(null);",
      "ts",
    );
    assert_eq!(facts.destructures.len(), 1);
    assert!(
      facts
        .reactivity_graph
        .bindings
        .iter()
        .any(|binding| binding.name == "input" && binding.initialized_with_null)
    );
  }

  #[test]
  fn template_expression_identifiers_use_oxc_ast_not_property_names() {
    assert_eq!(
      template_expression_identifiers("user.name + count", "interpolation"),
      vec!["count".to_owned(), "user".to_owned()],
      "static member properties must not be collected as free reads"
    );
    assert_eq!(
      template_expression_identifiers("item in items", "for"),
      vec!["items".to_owned()],
      "v-for must join only the iterable source, not the alias"
    );
    assert_eq!(
      template_expression_identifiers("(item, index) of list", "for"),
      vec!["list".to_owned()],
      "destructured v-for aliases must not appear as free reads"
    );
    assert_eq!(
      template_expression_identifiers("(item) => item + count", "on"),
      vec!["count".to_owned()],
      "handler parameters must not be treated as free template reads"
    );
    assert_eq!(
      template_expression_identifiers(
        "(item) => { const local = item; return local + total }",
        "on"
      ),
      vec!["total".to_owned()],
      "inner let/const bindings must be filtered from free reads"
    );
    assert_eq!(
      v_for_alias_identifiers("item in items"),
      vec!["item".to_owned()],
      "simple v-for aliases must be recovered"
    );
    assert_eq!(
      v_for_alias_identifiers("(item, index) of list"),
      vec!["index".to_owned(), "item".to_owned()],
      "paired v-for aliases must be recovered"
    );
    assert_eq!(
      v_for_alias_identifiers("({ id, label }, index) in rows"),
      vec!["id".to_owned(), "index".to_owned(), "label".to_owned()],
      "destructured v-for aliases must be recovered"
    );
    assert_eq!(
      slot_prop_alias_identifiers("{ value, meta }"),
      vec!["meta".to_owned(), "value".to_owned()],
      "slot prop destructuring must bind locals"
    );
    let shadowed = BTreeSet::from(["item".to_owned()]);
    assert_eq!(
      template_expression_identifiers_with_shadow("item + count", "interpolation", &shadowed),
      vec!["count".to_owned()],
      "template-local aliases must not appear as free reads"
    );
    assert!(
      template_expression_identifiers("{ value }", "slot").is_empty(),
      "slot prop patterns are bindings, not free reads"
    );
    assert!(
      template_expression_identifiers("??? not expression", "if").is_empty(),
      "parse failures stay quiet so callers can fall back"
    );
  }

  #[test]
  fn supports_js_ts_jsx_and_tsx() {
    for language in ["js", "ts", "jsx", "tsx"] {
      let facts = analyze("const value = 1", language);
      assert_eq!(facts.language, language, "language selection must stay stable");
    }
  }

  #[test]
  #[expect(clippy::panic, reason = "unexpected Oxc errors must fail adapter tests")]
  fn lowers_vue_jsx_v_html_and_inner_html_to_template_facts() {
    let source = "export function Comp() { return <div v-html={html} innerHTML={raw} /> }";
    let analysis = match analyze_module_source(source, source, 0, "tsx", ScriptKind::Script) {
      Ok(analysis) => analysis,
      Err(error) => panic!("tsx analysis failed: {error}"),
    };
    assert!(
      analysis.template_facts.elements.iter().any(|element| {
        element.tag == "div"
          && element.directive("html").is_some()
          && element.directives.iter().filter(|directive| directive.name == "html").count() >= 2
      }),
      "v-html and innerHTML must lower to html directives; got {:?}",
      analysis.template_facts.elements
    );
  }

  #[test]
  fn retains_block_kind_and_original_sfc_offsets() {
    let sfc = "<script>const value = run()</script>";
    let script = "const value = run()";
    let offset = sfc.find(script).unwrap_or_default();
    let facts = analyze_script(sfc, script, offset, "js", ScriptKind::Script);
    assert!(facts.is_ok(), "a normal script block must be analyzable");
    if let Ok(facts) = facts {
      assert_eq!(facts.kind, ScriptKind::Script, "the SFC block kind must be retained");
      assert_eq!(
        facts.calls.first().map(|call| call.span.offset),
        sfc.find("run()"),
        "Oxc spans must map back to the original SFC source"
      );
    }
  }

  #[test]
  fn retains_side_effect_imports_for_project_edges() {
    let facts = analyze("import './setup'", "ts");
    assert_eq!(
      facts.imports.first().map(|import| import.source.as_str()),
      Some("./setup"),
      "side-effect imports must remain visible to the project graph"
    );
  }
}
