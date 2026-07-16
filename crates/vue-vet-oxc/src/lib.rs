use std::collections::BTreeMap;

use oxc_allocator::Allocator;
use oxc_ast::{
  AstKind,
  ast::{
    Argument, AssignmentTarget, BindingPattern, Expression, ImportDeclarationSpecifier,
    ModuleExportName, SimpleAssignmentTarget, Statement,
  },
};
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::{GetSpan, SourceType, Span};
use thiserror::Error;
use vue_vet_core::{
  ReactiveBindingFact, ReactiveBindingKind, ReactiveReadFact, ReactivityEffectFact,
  ReactivityGraph, ScriptBindingFact, ScriptBlockFacts, ScriptCallFact, ScriptDestructureFact,
  ScriptImportFact, ScriptKind, ScriptMemberWriteFact, SourceSpan,
};

#[derive(Debug, Error)]
pub enum AnalyzeScriptError {
  #[error("Oxc could not parse the script: {0}")]
  Parse(String),
  #[error("Oxc could not build script semantics: {0}")]
  Semantic(String),
  #[error("unsupported script language `{0}`")]
  UnsupportedLanguage(String),
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
  let mut imports = Vec::new();
  let mut imported_bindings = BTreeMap::new();

  for node in semantic.nodes() {
    let AstKind::ImportDeclaration(declaration) = node.kind() else {
      continue;
    };
    let source = declaration.source.value.to_string();
    let Some(specifiers) = &declaration.specifiers else {
      imports.push(ScriptImportFact {
        source,
        imported: String::new(),
        local: String::new(),
        span: source_span(sfc_source, script_offset, declaration.span),
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
        span: source_span(sfc_source, script_offset, span),
      });
    }
  }

  let scoping = semantic.scoping();
  let bindings = scoping
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
        span: source_span(sfc_source, script_offset, scoping.symbol_span(symbol_id)),
      }
    })
    .collect();

  let mut calls = Vec::new();
  let mut member_writes = Vec::new();
  let mut destructures = Vec::new();
  for (node_id, node) in semantic.nodes().iter_enumerated() {
    match node.kind() {
      AstKind::CallExpression(call) => {
        if let Some(identifier) = call.callee.get_identifier_reference() {
          let callee = identifier.name.to_string();
          let assigned_to = match semantic.nodes().parent_kind(node_id) {
            AstKind::VariableDeclarator(declarator) => match &declarator.id {
              BindingPattern::BindingIdentifier(binding) => Some(binding.name.to_string()),
              BindingPattern::ObjectPattern(pattern) => {
                if callee == "defineProps" {
                  destructures.push(ScriptDestructureFact {
                    source_call: callee.clone(),
                    span: source_span(sfc_source, script_offset, pattern.span),
                  });
                }
                None
              }
              _ => None,
            },
            _ => None,
          };
          calls.push(ScriptCallFact {
            assigned_to,
            resolved_import: imported_bindings.get(&callee).cloned(),
            callee,
            span: source_span(sfc_source, script_offset, call.span),
          });
        }
      }
      AstKind::AssignmentExpression(assignment) => {
        if let Some(write) = assignment_member(&assignment.left, sfc_source, script_offset) {
          member_writes.push(write);
        }
      }
      AstKind::UpdateExpression(update) => {
        if let Some(write) = update_member(&update.argument, sfc_source, script_offset) {
          member_writes.push(write);
        }
      }
      _ => {}
    }
  }

  let mut reactive_bindings = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(identifier) = call.callee.get_identifier_reference() else {
      continue;
    };
    let local = identifier.name.as_str();
    let imported = imported_bindings
      .get(local)
      .filter(|(source, _)| source == "vue")
      .map(|(_, imported)| imported.as_str())
      .unwrap_or(local);
    let binding_kind = match imported {
      "ref" => Some(ReactiveBindingKind::Ref),
      "shallowRef" => Some(ReactiveBindingKind::ShallowRef),
      "computed" => Some(ReactiveBindingKind::Computed),
      "reactive" => Some(ReactiveBindingKind::Reactive),
      "shallowReactive" => Some(ReactiveBindingKind::ShallowReactive),
      _ => None,
    };
    let Some(binding_kind) = binding_kind else {
      continue;
    };
    let AstKind::VariableDeclarator(declarator) =
      semantic.nodes().parent_kind(call.node_id.get())
    else {
      continue;
    };
    let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
      continue;
    };
    reactive_bindings.push(ReactiveBindingFact {
      name: binding.name.to_string(),
      kind: binding_kind,
      initialized_with_null: call
        .arguments
        .first()
        .is_some_and(|argument| matches!(argument, Argument::NullLiteral(_))),
      span: source_span(sfc_source, script_offset, binding.span),
    });
  }

  let mut effects = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(identifier) = call.callee.get_identifier_reference() else {
      continue;
    };
    let callee = identifier.name.as_str();
    let imported = imported_bindings
      .get(callee)
      .filter(|(source, _)| source == "vue")
      .map(|(_, imported)| imported.as_str())
      .unwrap_or(callee);
    if !matches!(imported, "watchEffect" | "watchPostEffect" | "watchSyncEffect") {
      continue;
    }
    let Some(Argument::ArrowFunctionExpression(callback)) = call.arguments.first() else {
      continue;
    };
    let callback_id = callback.node_id.get();
    let mut reads = Vec::new();
    for statement in &callback.body.statements {
      let Statement::IfStatement(guard) = statement else {
        continue;
      };
      if guard.alternate.is_some() || !is_early_return(&guard.consequent) {
        continue;
      }
      let collect_reads = |range: Span| {
        semantic
          .nodes()
          .iter_enumerated()
          .filter_map(|(member_id, member_node)| {
            let (object, property, member_span) = match member_node.kind() {
              AstKind::StaticMemberExpression(member) => (
                member.object.get_identifier_reference()?.name.as_str(),
                Some(member.property.name.to_string()),
                member.span,
              ),
              AstKind::ComputedMemberExpression(member) => (
                member.object.get_identifier_reference()?.name.as_str(),
                member.static_property_name().map(|name| name.to_string()),
                member.span,
              ),
              _ => return None,
            };
            if member_span.start < range.start || member_span.end > range.end {
              return None;
            }
            let mut nested_function = false;
            let mut write_only = false;
            for ancestor_id in semantic.nodes().ancestor_ids(member_id) {
              if ancestor_id == callback_id {
                break;
              }
              match semantic.nodes().kind(ancestor_id) {
                AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                  nested_function = true;
                  break;
                }
                AstKind::AssignmentExpression(assignment) => {
                  let target = assignment.left.span();
                  write_only = target.start <= member_span.start && target.end >= member_span.end;
                }
                _ => {}
              }
            }
            if nested_function || write_only {
              return None;
            }
            let binding = reactive_bindings.iter().find(|binding| {
              binding.name == object
                && match binding.kind {
                  ReactiveBindingKind::Reactive | ReactiveBindingKind::ShallowReactive => true,
                  ReactiveBindingKind::Ref
                  | ReactiveBindingKind::ShallowRef
                  | ReactiveBindingKind::Computed => property.as_deref() == Some("value"),
                }
            })?;
            Some((binding.name.clone(), member_span))
          })
          .collect::<Vec<_>>()
      };
      let guard_reads = collect_reads(guard.test.span());
      let before_reads = collect_reads(Span::new(callback.body.span.start, guard.span.start));
      let after_reads = collect_reads(Span::new(guard.span.end, callback.body.span.end));
      let guarded_by = guard_reads.first().map(|(binding, _)| binding.clone());
      let Some(guarded_by) = guarded_by else {
        continue;
      };
      for (binding, span) in after_reads {
        if binding == guarded_by
          || before_reads.iter().any(|(read, _)| read == &binding)
          || guard_reads.iter().any(|(read, _)| read == &binding)
          || reads.iter().any(|read: &ReactiveReadFact| read.binding == binding)
        {
          continue;
        }
        reads.push(ReactiveReadFact {
          binding,
          guarded_by: Some(guarded_by.clone()),
          span: source_span(sfc_source, script_offset, span),
        });
      }
      break;
    }
    if !reads.is_empty() {
      effects.push(ReactivityEffectFact {
        callee: imported.into(),
        span: source_span(sfc_source, script_offset, call.span),
        reads,
      });
    }
  }

  imports.sort_by_key(|fact| fact.span.offset);
  calls.sort_by_key(|fact| fact.span.offset);
  member_writes.sort_by_key(|fact| fact.span.offset);
  destructures.sort_by_key(|fact| fact.span.offset);
  reactive_bindings.sort_by_key(|fact| fact.span.offset);
  effects.sort_by_key(|fact| fact.span.offset);
  Ok(ScriptBlockFacts {
    kind,
    language: language.into(),
    imports,
    bindings,
    calls,
    member_writes,
    destructures,
    reactivity_graph: ReactivityGraph { bindings: reactive_bindings, effects },
  })
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

fn is_early_return(statement: &Statement<'_>) -> bool {
  match statement {
    Statement::ReturnStatement(_) => true,
    Statement::BlockStatement(block) => {
      matches!(block.body.as_slice(), [Statement::ReturnStatement(_)])
    }
    _ => false,
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
  source: &str,
  offset: usize,
) -> Option<ScriptMemberWriteFact> {
  match target {
    AssignmentTarget::StaticMemberExpression(member) => {
      member_write(&member.object, Some(member.property.name.as_str()), member.span, source, offset)
    }
    AssignmentTarget::ComputedMemberExpression(member) => member_write(
      &member.object,
      member.static_property_name().as_deref(),
      member.span,
      source,
      offset,
    ),
    _ => None,
  }
}

fn update_member(
  target: &SimpleAssignmentTarget<'_>,
  source: &str,
  offset: usize,
) -> Option<ScriptMemberWriteFact> {
  match target {
    SimpleAssignmentTarget::StaticMemberExpression(member) => {
      member_write(&member.object, Some(member.property.name.as_str()), member.span, source, offset)
    }
    SimpleAssignmentTarget::ComputedMemberExpression(member) => member_write(
      &member.object,
      member.static_property_name().as_deref(),
      member.span,
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
  source: &str,
  offset: usize,
) -> Option<ScriptMemberWriteFact> {
  let object = object.get_identifier_reference()?.name.to_string();
  Some(ScriptMemberWriteFact {
    object,
    property: property.map(str::to_owned),
    span: source_span(source, offset, span),
  })
}

fn source_span(source: &str, base: usize, span: Span) -> SourceSpan {
  let offset = base.saturating_add(usize::try_from(span.start).unwrap_or(usize::MAX));
  let end = base.saturating_add(usize::try_from(span.end).unwrap_or(usize::MAX));
  let bytes = source.as_bytes();
  let prefix = bytes.get(..offset.min(bytes.len())).unwrap_or(bytes);
  let line =
    prefix.iter().fold(1_usize, |line, byte| line.saturating_add(usize::from(*byte == b'\n')));
  let column = prefix
    .iter()
    .rposition(|byte| *byte == b'\n')
    .map_or_else(|| prefix.len().saturating_add(1), |newline| prefix.len().saturating_sub(newline));
  SourceSpan { offset, length: end.saturating_sub(offset), line, column }
}

fn join_errors(errors: &[impl ToString]) -> String {
  errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[expect(clippy::panic, reason = "unexpected Oxc errors must fail adapter tests")]
  fn analyze(source: &str, language: &str) -> ScriptBlockFacts {
    match analyze_script(source, source, 0, language, ScriptKind::Setup) {
      Ok(facts) => facts,
      Err(error) => panic!("script analysis unexpectedly failed: {error}"),
    }
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
        .map(|read| (read.binding.as_str(), read.guarded_by.as_deref()))
        .collect::<Vec<_>>(),
      [("value", Some("ready"))]
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
  fn supports_js_ts_jsx_and_tsx() {
    for language in ["js", "ts", "jsx", "tsx"] {
      let facts = analyze("const value = 1", language);
      assert_eq!(facts.language, language, "language selection must stay stable");
    }
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
