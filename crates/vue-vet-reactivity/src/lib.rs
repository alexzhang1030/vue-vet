use std::collections::BTreeMap;

use oxc_ast::{
  AstKind,
  ast::{
    Argument, BindingPattern, Expression, FunctionBody, ImportDeclarationSpecifier,
    ModuleExportName, Statement,
  },
};
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};
use vue_vet_core::{
  ReactiveBindingFact, ReactiveBindingKind, ReactiveGuardFact, ReactiveReadFact, ReactiveReadKind,
  ReactivityEffectFact, ReactivityGraph, ScriptKind, SourceSpan,
};

/// Trace Vue reactive bindings and effect dependencies from an Oxc semantic model.
///
/// The returned graph contains only Vue Vet-owned serializable facts. Oxc nodes
/// remain an implementation detail of this crate.
///
/// # Panics
///
/// This function does not panic for valid Oxc semantic models.
pub fn trace_reactivity(
  semantic: &Semantic<'_>,
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
) -> ReactivityGraph {
  trace_reactivity_seeded(semantic, sfc_source, script_offset, script_kind, &[])
}

fn trace_reactivity_seeded(
  semantic: &Semantic<'_>,
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
  seed_bindings: &[ReactiveBindingFact],
) -> ReactivityGraph {
  let imported_bindings = collect_imported_bindings(semantic);
  let mut bindings =
    collect_reactive_bindings(semantic, &imported_bindings, sfc_source, script_offset, script_kind);
  for binding in seed_bindings {
    if !bindings.iter().any(|local| local.name == binding.name) {
      bindings.push(binding.clone());
    }
  }
  let mut effects =
    collect_effects(semantic, &imported_bindings, &bindings, sfc_source, script_offset);
  bindings.sort_by_key(|fact| fact.span.offset);
  effects.sort_by_key(|fact| fact.span.offset);
  ReactivityGraph { bindings, effects }
}

fn collect_imported_bindings(semantic: &Semantic<'_>) -> BTreeMap<String, (String, String)> {
  let mut imported_bindings = BTreeMap::new();
  for node in semantic.nodes() {
    let AstKind::ImportDeclaration(declaration) = node.kind() else {
      continue;
    };
    let Some(specifiers) = &declaration.specifiers else {
      continue;
    };
    let source = declaration.source.value.to_string();
    for specifier in specifiers {
      let (imported, local) = match specifier {
        ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
          (module_export_name(&specifier.imported), specifier.local.name.to_string())
        }
        ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
          ("default".into(), specifier.local.name.to_string())
        }
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
          ("*".into(), specifier.local.name.to_string())
        }
      };
      imported_bindings.insert(local, (source.clone(), imported));
    }
  }
  imported_bindings
}

fn module_export_name(name: &ModuleExportName<'_>) -> String {
  match name {
    ModuleExportName::IdentifierName(name) => name.name.to_string(),
    ModuleExportName::IdentifierReference(name) => name.name.to_string(),
    ModuleExportName::StringLiteral(name) => name.value.to_string(),
  }
}

fn resolved_vue_callee(
  callee: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  kind: ScriptKind,
) -> Option<String> {
  if let Some(identifier) = callee.get_identifier_reference() {
    let local = identifier.name.as_str();
    if local == "defineModel" && kind == ScriptKind::Setup && !imported_bindings.contains_key(local)
    {
      return Some(local.into());
    }
    return imported_bindings
      .get(local)
      .filter(|(source, _)| matches!(source.as_str(), "vue" | "#imports"))
      .map(|(_, imported)| imported.clone());
  }

  let (namespace, property) = match callee {
    Expression::StaticMemberExpression(member) => {
      (member.object.get_identifier_reference()?.name.as_str(), member.property.name.to_string())
    }
    Expression::ComputedMemberExpression(member) => (
      member.object.get_identifier_reference()?.name.as_str(),
      member.static_property_name()?.to_string(),
    ),
    _ => return None,
  };
  imported_bindings
    .get(namespace)
    .filter(|(source, imported)| matches!(source.as_str(), "vue" | "#imports") && imported == "*")
    .map(|_| property)
}

fn reactive_binding_kind(callee: &str) -> Option<ReactiveBindingKind> {
  match callee {
    "ref" => Some(ReactiveBindingKind::Ref),
    "shallowRef" => Some(ReactiveBindingKind::ShallowRef),
    "computed" => Some(ReactiveBindingKind::Computed),
    "reactive" => Some(ReactiveBindingKind::Reactive),
    "shallowReactive" => Some(ReactiveBindingKind::ShallowReactive),
    "readonly" => Some(ReactiveBindingKind::Readonly),
    "shallowReadonly" => Some(ReactiveBindingKind::ShallowReadonly),
    "customRef" => Some(ReactiveBindingKind::CustomRef),
    "toRef" | "toRefs" => Some(ReactiveBindingKind::ToRef),
    "useTemplateRef" => Some(ReactiveBindingKind::TemplateRef),
    "defineModel" => Some(ReactiveBindingKind::ModelRef),
    _ => None,
  }
}

fn collect_binding_identifiers(
  pattern: &BindingPattern<'_>,
  identifiers: &mut Vec<(String, Span)>,
) {
  match pattern {
    BindingPattern::BindingIdentifier(identifier) => {
      identifiers.push((identifier.name.to_string(), identifier.span));
    }
    BindingPattern::ObjectPattern(object) => {
      for property in &object.properties {
        collect_binding_identifiers(&property.value, identifiers);
      }
      if let Some(rest) = &object.rest {
        collect_binding_identifiers(&rest.argument, identifiers);
      }
    }
    BindingPattern::ArrayPattern(array) => {
      for element in array.elements.iter().flatten() {
        collect_binding_identifiers(element, identifiers);
      }
      if let Some(rest) = &array.rest {
        collect_binding_identifiers(&rest.argument, identifiers);
      }
    }
    BindingPattern::AssignmentPattern(assignment) => {
      collect_binding_identifiers(&assignment.left, identifiers);
    }
  }
}

fn collect_reactive_bindings(
  semantic: &oxc_semantic::Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
) -> Vec<ReactiveBindingFact> {
  let mut reactive_bindings = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = resolved_vue_callee(&call.callee, imported_bindings, script_kind) else {
      continue;
    };
    let Some(binding_kind) = reactive_binding_kind(&callee) else {
      continue;
    };
    let AstKind::VariableDeclarator(declarator) = semantic.nodes().parent_kind(call.node_id.get())
    else {
      continue;
    };

    let mut identifiers = Vec::new();
    if callee == "toRefs" {
      if matches!(&declarator.id, BindingPattern::ObjectPattern(_)) {
        collect_binding_identifiers(&declarator.id, &mut identifiers);
      }
    } else if let BindingPattern::BindingIdentifier(identifier) = &declarator.id {
      identifiers.push((identifier.name.to_string(), identifier.span));
    }

    let initialized_with_null =
      call.arguments.first().is_some_and(|argument| matches!(argument, Argument::NullLiteral(_)));
    for (name, span) in identifiers {
      reactive_bindings.push(ReactiveBindingFact {
        name,
        kind: binding_kind,
        initialized_with_null,
        span: source_span(sfc_source, script_offset, span),
      });
    }
  }

  reactive_bindings
}

#[derive(Clone, Debug)]
struct RawReactiveRead {
  node_id: NodeId,
  binding: String,
  property: Option<String>,
  span: Span,
}

const fn is_ref_like(kind: ReactiveBindingKind) -> bool {
  matches!(
    kind,
    ReactiveBindingKind::Ref
      | ReactiveBindingKind::ShallowRef
      | ReactiveBindingKind::Computed
      | ReactiveBindingKind::CustomRef
      | ReactiveBindingKind::ToRef
      | ReactiveBindingKind::TemplateRef
      | ReactiveBindingKind::ModelRef
  )
}

const fn span_contains(outer: Span, inner: Span) -> bool {
  outer.start <= inner.start && outer.end >= inner.end
}

fn collect_callback_reads(
  semantic: &oxc_semantic::Semantic<'_>,
  callback_id: NodeId,
  reactive_bindings: &[ReactiveBindingFact],
) -> Vec<RawReactiveRead> {
  let mut reads = semantic
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

      let mut reached_callback = false;
      let mut nested_function = false;
      let mut write_only = false;
      for ancestor_id in semantic.nodes().ancestor_ids(member_id) {
        if ancestor_id == callback_id {
          reached_callback = true;
          break;
        }
        match semantic.nodes().kind(ancestor_id) {
          AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
            nested_function = true;
            break;
          }
          AstKind::AssignmentExpression(assignment)
            if assignment.operator.is_assign()
              && span_contains(assignment.left.span(), member_span) =>
          {
            write_only = true;
          }
          _ => {}
        }
      }
      if !reached_callback || nested_function || write_only {
        return None;
      }

      let binding = reactive_bindings.iter().find(|binding| {
        binding.name == object
          && (!is_ref_like(binding.kind) || property.as_deref() == Some("value"))
      })?;
      Some(RawReactiveRead {
        node_id: member_id,
        binding: binding.name.clone(),
        property,
        span: member_span,
      })
    })
    .collect::<Vec<_>>();
  reads.sort_by_key(|read| read.span.start);
  reads
}

fn push_guards_in_span(guards: &mut Vec<RawReactiveRead>, reads: &[RawReactiveRead], span: Span) {
  for read in reads.iter().filter(|read| span_contains(span, read.span)) {
    if !guards.iter().any(|guard| {
      guard.binding == read.binding
        && guard.property == read.property
        && guard.span.start == read.span.start
        && guard.span.end == read.span.end
    }) {
      guards.push(read.clone());
    }
  }
}

fn path_guards(
  semantic: &oxc_semantic::Semantic<'_>,
  callback_id: NodeId,
  body: &FunctionBody<'_>,
  reads: &[RawReactiveRead],
  read: &RawReactiveRead,
) -> Vec<RawReactiveRead> {
  let mut guards = Vec::new();

  for statement in &body.statements {
    let Statement::IfStatement(guard) = statement else {
      continue;
    };
    if guard.span.end > read.span.start
      || guard.alternate.is_some()
      || !is_early_return(&guard.consequent)
    {
      continue;
    }
    push_guards_in_span(&mut guards, reads, guard.test.span());
  }

  for ancestor_id in semantic.nodes().ancestor_ids(read.node_id) {
    if ancestor_id == callback_id {
      break;
    }
    match semantic.nodes().kind(ancestor_id) {
      AstKind::IfStatement(statement) => {
        let in_branch = span_contains(statement.consequent.span(), read.span)
          || statement
            .alternate
            .as_ref()
            .is_some_and(|alternate| span_contains(alternate.span(), read.span));
        if in_branch {
          push_guards_in_span(&mut guards, reads, statement.test.span());
        }
      }
      AstKind::ConditionalExpression(expression) => {
        if span_contains(expression.consequent.span(), read.span)
          || span_contains(expression.alternate.span(), read.span)
        {
          push_guards_in_span(&mut guards, reads, expression.test.span());
        }
      }
      AstKind::LogicalExpression(expression)
        if span_contains(expression.right.span(), read.span) =>
      {
        push_guards_in_span(&mut guards, reads, expression.left.span());
      }
      _ => {}
    }
  }

  guards.sort_by_key(|guard| guard.span.start);
  guards
}

fn is_after_top_level_await(
  semantic: &oxc_semantic::Semantic<'_>,
  callback_id: NodeId,
  read: &RawReactiveRead,
) -> bool {
  semantic.nodes().iter_enumerated().any(|(await_id, node)| {
    let AstKind::AwaitExpression(await_expression) = node.kind() else {
      return false;
    };
    if await_expression.span.end > read.span.start {
      return false;
    }

    for ancestor_id in semantic.nodes().ancestor_ids(await_id) {
      if ancestor_id == callback_id {
        return true;
      }
      match semantic.nodes().kind(ancestor_id) {
        AstKind::ArrowFunctionExpression(_)
        | AstKind::Function(_)
        | AstKind::IfStatement(_)
        | AstKind::ConditionalExpression(_)
        | AstKind::LogicalExpression(_) => return false,
        _ => {}
      }
    }
    false
  })
}

fn collect_effects(
  semantic: &oxc_semantic::Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  reactive_bindings: &[ReactiveBindingFact],
  sfc_source: &str,
  script_offset: usize,
) -> Vec<ReactivityEffectFact> {
  let mut effects = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = resolved_vue_callee(&call.callee, imported_bindings, ScriptKind::Script)
    else {
      continue;
    };
    if !matches!(callee.as_str(), "watchEffect" | "watchPostEffect" | "watchSyncEffect") {
      continue;
    }

    let Some(argument) = call.arguments.first() else {
      continue;
    };
    let (callback_id, body) = match argument {
      Argument::ArrowFunctionExpression(callback) => (callback.node_id.get(), &*callback.body),
      Argument::FunctionExpression(callback) => {
        let Some(body) = &callback.body else {
          continue;
        };
        (callback.node_id.get(), &**body)
      }
      _ => continue,
    };

    let raw_reads = collect_callback_reads(semantic, callback_id, reactive_bindings);
    let reads = raw_reads
      .iter()
      .map(|read| {
        let guards = path_guards(semantic, callback_id, body, &raw_reads, read);
        let kind = if is_after_top_level_await(semantic, callback_id, read) {
          ReactiveReadKind::AfterAwait
        } else if guards.is_empty() {
          ReactiveReadKind::Unconditional
        } else {
          ReactiveReadKind::Conditional
        };
        let guarded_by = guards.first().map(|guard| guard.binding.clone());
        ReactiveReadFact {
          binding: read.binding.clone(),
          property: read.property.clone(),
          kind,
          guards: guards
            .into_iter()
            .map(|guard| ReactiveGuardFact {
              binding: guard.binding,
              property: guard.property,
              span: source_span(sfc_source, script_offset, guard.span),
            })
            .collect(),
          guarded_by,
          span: source_span(sfc_source, script_offset, read.span),
        }
      })
      .collect();

    effects.push(ReactivityEffectFact {
      callee,
      span: source_span(sfc_source, script_offset, call.span),
      reads,
    });
  }

  effects
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

mod modules;

pub use modules::{ModuleLink, ModuleReactivity, ModuleSource, TraceModulesError, trace_modules};

#[cfg(test)]
mod tests;
