use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::{
  AstKind,
  ast::{
    Argument, BindingPattern, Expression, FunctionBody, IdentifierReference,
    ImportDeclarationSpecifier, ModuleExportName, Statement,
  },
};
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};
use vue_vet_core::{
  ReactiveBindingFact, ReactiveBindingKind, ReactiveGuardFact, ReactiveGuardRole, ReactiveReadFact,
  ReactiveReadKind, ReactiveWriteFact, ReactivityGraph, ScriptKind, SourceSpan, TrackingScopeFact,
  TrackingScopeKind,
};

/// Trace Vue reactive bindings and tracking-scope dependencies from an Oxc semantic model.
///
/// The returned graph contains only Vue Vet-owned serializable facts. Oxc nodes
/// remain an implementation detail of this crate.
///
/// # Panics
///
/// This function does not panic for valid Oxc semantic models.
#[must_use]
pub fn trace_reactivity(
  semantic: &Semantic<'_>,
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
) -> ReactivityGraph {
  trace_reactivity_seeded(semantic, sfc_source, script_offset, script_kind, &TraceSeeds::default())
}

#[derive(Clone, Debug, Default)]
struct TraceSeeds {
  bindings: Vec<ReactiveBindingFact>,
  /// `const bag = useFoo()` locals mapped to composable return field kinds.
  composable_instances: BTreeMap<String, BTreeMap<String, ReactiveBindingKind>>,
}

fn trace_reactivity_seeded(
  semantic: &Semantic<'_>,
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
  seeds: &TraceSeeds,
) -> ReactivityGraph {
  let imported_bindings = collect_imported_bindings(semantic);
  let mut bindings =
    collect_reactive_bindings(semantic, &imported_bindings, sfc_source, script_offset, script_kind);
  for binding in &seeds.bindings {
    if !bindings.iter().any(|local| local.name == binding.name) {
      bindings.push(binding.clone());
    }
  }
  let mut scopes = collect_tracking_scopes(
    semantic,
    &imported_bindings,
    &bindings,
    &seeds.composable_instances,
    sfc_source,
    script_offset,
  );
  bindings.sort_by_key(|fact| fact.span.offset);
  scopes.sort_by_key(|fact| fact.span.offset);
  let mut graph = ReactivityGraph {
    version: vue_vet_core::REACTIVITY_GRAPH_VERSION,
    bindings,
    scopes,
    effects: Vec::new(),
    edges: Vec::new(),
    template_reads: Vec::new(),
  };
  graph.project_effects_from_scopes();
  graph
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

/// Locals assigned from `effectScope()` (Vue import / `#imports` / namespace).
fn effect_scope_instance_locals(
  semantic: &oxc_semantic::Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> BTreeSet<String> {
  let mut locals = BTreeSet::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = resolved_vue_callee(&call.callee, imported_bindings, ScriptKind::Script)
    else {
      continue;
    };
    if callee != "effectScope" {
      continue;
    }
    if let Some(name) = assigned_binding_name(semantic, call.node_id.get()) {
      locals.insert(name);
    }
  }
  locals
}

fn resolved_vue_callee(
  callee: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  kind: ScriptKind,
) -> Option<String> {
  if let Some(identifier) = callee.get_identifier_reference() {
    let local = identifier.name.as_str();
    if matches!(local, "defineModel" | "defineProps")
      && kind == ScriptKind::Setup
      && !imported_bindings.contains_key(local)
    {
      return Some(local.into());
    }
    return imported_bindings.get(local).and_then(|(source, imported)| {
      known_reactivity_export(source, imported).then(|| imported.clone())
    });
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
  imported_bindings.get(namespace).and_then(|(source, imported)| {
    if imported == "*" && matches!(source.as_str(), "vue" | "#imports") {
      known_reactivity_export("vue", &property).then_some(property)
    } else {
      None
    }
  })
}

/// Packages/exports the tracer treats as reactivity APIs (under-approx allowlist).
fn known_reactivity_export(source: &str, imported: &str) -> bool {
  match source {
    "vue" | "#imports" => {
      reactive_binding_kind(imported).is_some()
        || TrackingScopeKind::from_vue_callee(imported).is_some()
        || matches!(
          imported,
          "storeToRefs"
            | "useRoute"
            | "useRouter"
            | "pauseTracking"
            | "enableTracking"
            | "resetTracking"
        )
    }
    "pinia" => matches!(imported, "storeToRefs"),
    "vue-router" => matches!(imported, "useRoute" | "useRouter"),
    _ => false,
  }
}

fn reactive_binding_kind(callee: &str) -> Option<ReactiveBindingKind> {
  match callee {
    "ref" => Some(ReactiveBindingKind::Ref),
    "shallowRef" => Some(ReactiveBindingKind::ShallowRef),
    "computed" => Some(ReactiveBindingKind::Computed),
    // defineProps / useRoute / useRouter expose reactive objects (member reads, not .value).
    "reactive" | "defineProps" | "useRoute" | "useRouter" => Some(ReactiveBindingKind::Reactive),
    "shallowReactive" => Some(ReactiveBindingKind::ShallowReactive),
    "readonly" => Some(ReactiveBindingKind::Readonly),
    "shallowReadonly" => Some(ReactiveBindingKind::ShallowReadonly),
    "customRef" => Some(ReactiveBindingKind::CustomRef),
    "toRef" | "toRefs" | "storeToRefs" => Some(ReactiveBindingKind::ToRef),
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
    if matches!(callee.as_str(), "toRefs" | "storeToRefs") {
      // `const { count, name } = storeToRefs(store)` / `toRefs(obj)` → each local is ref-like.
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
  outside_tracking: bool,
}

#[derive(Clone, Debug)]
struct RawGuard {
  read: RawReactiveRead,
  role: ReactiveGuardRole,
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

fn reference_resolves_to_binding(
  semantic: &oxc_semantic::Semantic<'_>,
  reference: &IdentifierReference<'_>,
  binding: &ReactiveBindingFact,
  script_offset: usize,
) -> bool {
  let Some(reference_id) = reference.reference_id.get() else {
    return false;
  };
  let Some(symbol_id) = semantic.scoping().get_reference(reference_id).symbol_id() else {
    return false;
  };
  if semantic.scoping().symbol_name(symbol_id) != binding.name {
    return false;
  }
  let symbol_span = semantic.scoping().symbol_span(symbol_id);
  let relative = usize::try_from(symbol_span.start).unwrap_or(usize::MAX);
  let absolute = script_offset.saturating_add(relative);
  // Exact absolute match (local facts and correctly offset seeds).
  if absolute == binding.span.offset {
    return true;
  }
  // Seeds historically/occasionally store script-relative spans even when the
  // module re-trace uses a non-zero SFC offset — accept the relative match too.
  script_offset > 0 && relative == binding.span.offset
}

/// True when `function_id` is a callback argument to a known **synchronously**
/// invoked higher-order method (Array extras, etc.).
fn is_sync_hof_callback(semantic: &oxc_semantic::Semantic<'_>, function_id: NodeId) -> bool {
  let function_span = match semantic.nodes().kind(function_id) {
    AstKind::ArrowFunctionExpression(callback) => callback.span,
    AstKind::Function(function) => function.span,
    _ => return false,
  };
  for ancestor_id in semantic.nodes().ancestor_ids(function_id) {
    let AstKind::CallExpression(call) = semantic.nodes().kind(ancestor_id) else {
      if matches!(
        semantic.nodes().kind(ancestor_id),
        AstKind::ArrowFunctionExpression(_) | AstKind::Function(_)
      ) {
        return false;
      }
      continue;
    };
    let is_argument = call.arguments.iter().any(|argument| match argument {
      Argument::ArrowFunctionExpression(callback) => callback.span == function_span,
      Argument::FunctionExpression(function) => function.span == function_span,
      _ => false,
    });
    if !is_argument {
      continue;
    }
    return is_sync_hof_callee(&call.callee);
  }
  false
}

fn is_sync_hof_callee(callee: &Expression<'_>) -> bool {
  const METHODS: &[&str] = &[
    "filter",
    "map",
    "forEach",
    "reduce",
    "reduceRight",
    "some",
    "every",
    "find",
    "findIndex",
    "findLast",
    "findLastIndex",
    "flatMap",
    "toSorted",
    "toSpliced",
  ];
  match callee {
    Expression::StaticMemberExpression(member) => METHODS.contains(&member.property.name.as_str()),
    Expression::ComputedMemberExpression(member) => {
      member.static_property_name().is_some_and(|name| METHODS.contains(&name.as_str()))
    }
    _ => false,
  }
}

fn is_deferred_callback_container(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
) -> bool {
  let function_span = match semantic.nodes().kind(function_id) {
    AstKind::ArrowFunctionExpression(callback) => callback.span,
    AstKind::Function(function) => function.span,
    _ => return false,
  };
  for ancestor_id in semantic.nodes().ancestor_ids(function_id) {
    let AstKind::CallExpression(call) = semantic.nodes().kind(ancestor_id) else {
      if matches!(
        semantic.nodes().kind(ancestor_id),
        AstKind::ArrowFunctionExpression(_) | AstKind::Function(_)
      ) {
        return false;
      }
      continue;
    };
    let is_argument = call.arguments.iter().any(|argument| match argument {
      Argument::ArrowFunctionExpression(callback) => callback.span == function_span,
      Argument::FunctionExpression(function) => function.span == function_span,
      _ => false,
    });
    if !is_argument {
      continue;
    }
    return match &call.callee {
      Expression::StaticMemberExpression(member) => {
        matches!(member.property.name.as_str(), "then" | "catch" | "finally" | "nextTick")
      }
      Expression::Identifier(identifier) => {
        matches!(identifier.name.as_str(), "nextTick" | "queueMicrotask" | "setTimeout")
      }
      _ => false,
    };
  }
  false
}

fn scope_context(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  member_id: NodeId,
  member_span: Span,
) -> Option<(bool, bool)> {
  let mut reached_scope = false;
  let mut outside_tracking = false;
  let mut write_only = false;
  for ancestor_id in semantic.nodes().ancestor_ids(member_id) {
    if ancestor_id == scope_id {
      reached_scope = true;
      break;
    }
    match semantic.nodes().kind(ancestor_id) {
      AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
        if is_deferred_callback_container(semantic, ancestor_id) {
          outside_tracking = true;
          continue;
        }
        // Sync higher-order callbacks (Array#filter/map/…) run during the parent
        // tracking flush, so Vue still tracks their reactive reads.
        if is_sync_hof_callback(semantic, ancestor_id) {
          continue;
        }
        return None;
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
  if !reached_scope || write_only {
    return None;
  }
  Some((reached_scope, outside_tracking))
}

fn collect_scope_reads(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &BTreeMap<String, BTreeMap<String, ReactiveBindingKind>>,
  script_offset: usize,
) -> Vec<RawReactiveRead> {
  let mut reads = semantic
    .nodes()
    .iter_enumerated()
    .filter_map(|(member_id, member_node)| {
      // Nested composable instance: bag.field.value
      if let AstKind::StaticMemberExpression(outer) = member_node.kind()
        && outer.property.name.as_str() == "value"
        && let Expression::StaticMemberExpression(inner) = &outer.object
        && let Some(instance) = inner.object.get_identifier_reference()
        && let Some(shape) = composable_instances.get(instance.name.as_str())
        && let Some(kind) = shape.get(inner.property.name.as_str())
        && is_ref_like(*kind)
      {
        let (_, outside_tracking) = scope_context(semantic, scope_id, member_id, outer.span)?;
        return Some(RawReactiveRead {
          node_id: member_id,
          binding: inner.property.name.to_string(),
          property: Some("value".into()),
          span: outer.span,
          outside_tracking,
        });
      }

      // Nested composable instance: bag.field for non-ref-like kinds
      if let AstKind::StaticMemberExpression(member) = member_node.kind()
        && let Some(instance) = member.object.get_identifier_reference()
        && let Some(shape) = composable_instances.get(instance.name.as_str())
        && let Some(kind) = shape.get(member.property.name.as_str())
        && !is_ref_like(*kind)
      {
        let (_, outside_tracking) = scope_context(semantic, scope_id, member_id, member.span)?;
        return Some(RawReactiveRead {
          node_id: member_id,
          binding: member.property.name.to_string(),
          property: Some(member.property.name.to_string()),
          span: member.span,
          outside_tracking,
        });
      }

      let (object, property, member_span) = match member_node.kind() {
        AstKind::StaticMemberExpression(member) => (
          member.object.get_identifier_reference()?,
          Some(member.property.name.to_string()),
          member.span,
        ),
        AstKind::ComputedMemberExpression(member) => (
          member.object.get_identifier_reference()?,
          member.static_property_name().map(|name| name.to_string()),
          member.span,
        ),
        _ => return None,
      };

      let (_, outside_tracking) = scope_context(semantic, scope_id, member_id, member_span)?;

      let binding = reactive_bindings.iter().find(|binding| {
        binding.name == object.name.as_str()
          && reference_resolves_to_binding(semantic, object, binding, script_offset)
          && (!is_ref_like(binding.kind) || property.as_deref() == Some("value"))
      })?;
      Some(RawReactiveRead {
        node_id: member_id,
        binding: binding.name.clone(),
        property,
        span: member_span,
        outside_tracking,
      })
    })
    .collect::<Vec<_>>();
  reads.sort_by_key(|read| read.span.start);
  reads
}

fn push_guards_in_span(
  guards: &mut Vec<RawGuard>,
  reads: &[RawReactiveRead],
  span: Span,
  role: ReactiveGuardRole,
) {
  for read in reads.iter().filter(|read| span_contains(span, read.span) && !read.outside_tracking) {
    if !guards.iter().any(|guard| {
      guard.read.binding == read.binding
        && guard.read.property == read.property
        && guard.read.span.start == read.span.start
        && guard.read.span.end == read.span.end
    }) {
      guards.push(RawGuard { read: read.clone(), role });
    }
  }
}

fn is_early_return(statement: &Statement<'_>) -> bool {
  match statement {
    Statement::ReturnStatement(_) | Statement::ThrowStatement(_) => true,
    Statement::BlockStatement(block) => match block.body.as_slice() {
      [only] => is_early_return(only),
      _ => false,
    },
    _ => false,
  }
}

fn collect_prefix_early_exits(
  statements: &[Statement<'_>],
  read_start: u32,
  reads: &[RawReactiveRead],
  guards: &mut Vec<RawGuard>,
) {
  for statement in statements {
    if statement.span().start >= read_start {
      break;
    }
    match statement {
      // Only statements fully before the read can guard it. Reads inside the
      // `if` test itself remain unconditional relative to that early exit.
      Statement::IfStatement(guard)
        if guard.span.end <= read_start
          && guard.alternate.is_none()
          && is_early_return(&guard.consequent) =>
      {
        push_guards_in_span(guards, reads, guard.test.span(), ReactiveGuardRole::EarlyExit);
      }
      Statement::BlockStatement(block) => {
        collect_prefix_early_exits(&block.body, read_start, reads, guards);
      }
      Statement::TryStatement(try_statement) => {
        collect_prefix_early_exits(&try_statement.block.body, read_start, reads, guards);
        if let Some(handler) = &try_statement.handler {
          collect_prefix_early_exits(&handler.body.body, read_start, reads, guards);
        }
        if let Some(finalizer) = &try_statement.finalizer {
          collect_prefix_early_exits(&finalizer.body, read_start, reads, guards);
        }
      }
      _ => {}
    }
  }
}

fn path_guards(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  body: Option<&FunctionBody<'_>>,
  reads: &[RawReactiveRead],
  read: &RawReactiveRead,
) -> Vec<RawGuard> {
  let mut guards = Vec::new();

  if let Some(body) = body {
    collect_prefix_early_exits(&body.statements, read.span.start, reads, &mut guards);
  }

  for ancestor_id in semantic.nodes().ancestor_ids(read.node_id) {
    if ancestor_id == scope_id {
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
          push_guards_in_span(
            &mut guards,
            reads,
            statement.test.span(),
            ReactiveGuardRole::BranchTest,
          );
        }
      }
      AstKind::ConditionalExpression(expression) => {
        if span_contains(expression.consequent.span(), read.span)
          || span_contains(expression.alternate.span(), read.span)
        {
          push_guards_in_span(
            &mut guards,
            reads,
            expression.test.span(),
            ReactiveGuardRole::BranchTest,
          );
        }
      }
      AstKind::LogicalExpression(expression)
        if span_contains(expression.right.span(), read.span) =>
      {
        push_guards_in_span(
          &mut guards,
          reads,
          expression.left.span(),
          ReactiveGuardRole::ShortCircuit,
        );
      }
      AstKind::SwitchCase(case) if span_contains(case.span, read.span) => {
        let switch_id = semantic.nodes().parent_id(ancestor_id);
        if let AstKind::SwitchStatement(switch_statement) = semantic.nodes().kind(switch_id) {
          push_guards_in_span(
            &mut guards,
            reads,
            switch_statement.discriminant.span(),
            ReactiveGuardRole::SwitchDiscriminant,
          );
        }
      }
      _ => {}
    }
  }

  guards.sort_by_key(|guard| guard.read.span.start);
  guards
}

fn is_after_top_level_await(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
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
      if ancestor_id == scope_id {
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

fn is_pause_tracking_call(
  call: &oxc_ast::ast::CallExpression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> bool {
  resolved_vue_callee(&call.callee, imported_bindings, ScriptKind::Script)
    .is_some_and(|callee| matches!(callee.as_str(), "pauseTracking"))
}

fn is_resume_tracking_call(
  call: &oxc_ast::ast::CallExpression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> bool {
  resolved_vue_callee(&call.callee, imported_bindings, ScriptKind::Script)
    .is_some_and(|callee| matches!(callee.as_str(), "enableTracking" | "resetTracking"))
}

/// True when a top-level `pauseTracking()` in the scope precedes the read without a resume.
fn is_after_pause_tracking(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  read: &RawReactiveRead,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> bool {
  let mut paused = false;
  let mut events = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let call_id = call.node_id.get();
    let mut owned = false;
    for ancestor_id in semantic.nodes().ancestor_ids(call_id) {
      if ancestor_id == scope_id {
        owned = true;
        break;
      }
      if matches!(
        semantic.nodes().kind(ancestor_id),
        AstKind::ArrowFunctionExpression(_) | AstKind::Function(_)
      ) {
        break;
      }
    }
    if !owned {
      continue;
    }
    if is_pause_tracking_call(call, imported_bindings) {
      events.push((call.span.end, true));
    } else if is_resume_tracking_call(call, imported_bindings) {
      events.push((call.span.end, false));
    }
  }
  events.sort_by_key(|(end, _)| *end);
  for (end, is_pause) in events {
    if end > read.span.start {
      break;
    }
    paused = is_pause;
  }
  paused
}

struct ClassifyRead<'a> {
  semantic: &'a oxc_semantic::Semantic<'a>,
  scope_id: NodeId,
  body: Option<&'a FunctionBody<'a>>,
  raw_reads: &'a [RawReactiveRead],
  read: &'a RawReactiveRead,
  sfc_source: &'a str,
  script_offset: usize,
  imported_bindings: &'a BTreeMap<String, (String, String)>,
}

fn classify_read(input: &ClassifyRead<'_>) -> ReactiveReadFact {
  let outside = input.read.outside_tracking
    || is_after_pause_tracking(input.semantic, input.scope_id, input.read, input.imported_bindings);
  let guards = if outside {
    Vec::new()
  } else {
    path_guards(input.semantic, input.scope_id, input.body, input.raw_reads, input.read)
  };
  let kind = if outside {
    ReactiveReadKind::OutsideTracking
  } else if is_after_top_level_await(input.semantic, input.scope_id, input.read) {
    ReactiveReadKind::AfterAwait
  } else if guards.is_empty() {
    ReactiveReadKind::Unconditional
  } else {
    ReactiveReadKind::Conditional
  };
  let guarded_by = guards.first().map(|guard| guard.read.binding.clone());
  ReactiveReadFact {
    binding: input.read.binding.clone(),
    property: input.read.property.clone(),
    kind,
    guards: guards
      .into_iter()
      .map(|guard| ReactiveGuardFact {
        binding: guard.read.binding,
        property: guard.read.property,
        span: source_span(input.sfc_source, input.script_offset, guard.read.span),
        role: guard.role,
      })
      .collect(),
    guarded_by,
    span: source_span(input.sfc_source, input.script_offset, input.read.span),
  }
}

fn callback_parts<'a>(
  argument: &'a Argument<'a>,
) -> Option<(NodeId, Option<&'a FunctionBody<'a>>)> {
  match argument {
    Argument::ArrowFunctionExpression(callback) => {
      Some((callback.node_id.get(), Some(&*callback.body)))
    }
    Argument::FunctionExpression(callback) => {
      Some((callback.node_id.get(), callback.body.as_deref()))
    }
    _ => None,
  }
}

fn is_assignment_only_body(body: Option<&FunctionBody<'_>>) -> bool {
  let Some(body) = body else {
    return false;
  };
  if body.statements.is_empty() {
    return false;
  }
  body.statements.iter().all(|statement| match statement {
    Statement::ExpressionStatement(expression) => {
      matches!(expression.expression, Expression::AssignmentExpression(_))
    }
    Statement::EmptyStatement(_) => true,
    _ => false,
  })
}

fn collect_scope_writes(
  semantic: &oxc_semantic::Semantic<'_>,
  scope_id: NodeId,
  reactive_bindings: &[ReactiveBindingFact],
  sfc_source: &str,
  script_offset: usize,
) -> Vec<ReactiveWriteFact> {
  let mut writes = Vec::new();
  for node in semantic.nodes() {
    let AstKind::AssignmentExpression(assignment) = node.kind() else {
      continue;
    };
    if !assignment.operator.is_assign() {
      continue;
    }
    let (object, property, write_span) = match &assignment.left {
      oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) => (
        member.object.get_identifier_reference(),
        Some(member.property.name.to_string()),
        member.span,
      ),
      oxc_ast::ast::AssignmentTarget::ComputedMemberExpression(member) => (
        member.object.get_identifier_reference(),
        member.static_property_name().map(|name| name.to_string()),
        member.span,
      ),
      _ => continue,
    };
    let Some(object) = object else {
      continue;
    };

    let mut reached_scope = false;
    let mut nested_function = false;
    for ancestor_id in semantic.nodes().ancestor_ids(node.id()) {
      if ancestor_id == scope_id {
        reached_scope = true;
        break;
      }
      if matches!(
        semantic.nodes().kind(ancestor_id),
        AstKind::ArrowFunctionExpression(_) | AstKind::Function(_)
      ) {
        nested_function = true;
        break;
      }
    }
    if !reached_scope || nested_function {
      continue;
    }

    let Some(binding) = reactive_bindings.iter().find(|binding| {
      binding.name == object.name.as_str()
        && reference_resolves_to_binding(semantic, object, binding, script_offset)
        && (!is_ref_like(binding.kind) || property.as_deref() == Some("value"))
    }) else {
      continue;
    };
    writes.push(ReactiveWriteFact {
      binding: binding.name.clone(),
      property,
      span: source_span(sfc_source, script_offset, write_span),
    });
  }
  writes.sort_by_key(|write| write.span.offset);
  writes
}

struct ScopeBuild<'a> {
  kind: TrackingScopeKind,
  callee: String,
  span: vue_vet_core::SourceSpan,
  reads: Vec<ReactiveReadFact>,
  binding: Option<String>,
  semantic: &'a oxc_semantic::Semantic<'a>,
  scope_id: NodeId,
  body: Option<&'a FunctionBody<'a>>,
  reactive_bindings: &'a [ReactiveBindingFact],
  sfc_source: &'a str,
  script_offset: usize,
}

fn finish_scope(build: ScopeBuild<'_>) -> TrackingScopeFact {
  let writes = collect_scope_writes(
    build.semantic,
    build.scope_id,
    build.reactive_bindings,
    build.sfc_source,
    build.script_offset,
  );
  TrackingScopeFact {
    kind: build.kind,
    callee: build.callee,
    span: build.span,
    reads: build.reads,
    writes,
    assignment_only: is_assignment_only_body(build.body),
    binding: build.binding,
  }
}

#[expect(
  clippy::too_many_lines,
  reason = "scope dispatch covers all Vue tracking APIs in one pass"
)]
fn collect_tracking_scopes(
  semantic: &oxc_semantic::Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &BTreeMap<String, BTreeMap<String, ReactiveBindingKind>>,
  sfc_source: &str,
  script_offset: usize,
) -> Vec<TrackingScopeFact> {
  // Only treat `.run(cb)` as an effect-scope body when the receiver was assigned
  // from Vue's `effectScope()` — never invent edges for arbitrary `.run` APIs.
  let effect_scope_locals = effect_scope_instance_locals(semantic, imported_bindings);
  let mut scopes = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };

    if let Expression::StaticMemberExpression(member) = &call.callee
      && member.property.name.as_str() == "run"
      && let Some(receiver) = member.object.get_identifier_reference()
      && effect_scope_locals.contains(receiver.name.as_str())
      && let Some(argument) = call.arguments.first()
      && let Some((scope_id, body)) = callback_parts(argument)
    {
      let raw_reads = collect_scope_reads(
        semantic,
        scope_id,
        reactive_bindings,
        composable_instances,
        script_offset,
      );
      let reads = raw_reads
        .iter()
        .map(|read| {
          classify_read(&ClassifyRead {
            semantic,
            scope_id,
            body,
            raw_reads: &raw_reads,
            read,
            sfc_source,
            script_offset,
            imported_bindings,
          })
        })
        .collect();
      scopes.push(finish_scope(ScopeBuild {
        kind: TrackingScopeKind::EffectScope,
        callee: "effectScope.run".into(),
        span: source_span(sfc_source, script_offset, call.span),
        reads,
        binding: None,
        semantic,
        scope_id,
        body,
        reactive_bindings,
        sfc_source,
        script_offset,
      }));
      continue;
    }

    let Some(callee) = resolved_vue_callee(&call.callee, imported_bindings, ScriptKind::Script)
    else {
      continue;
    };
    let Some(scope_kind) = TrackingScopeKind::from_vue_callee(&callee) else {
      continue;
    };

    match scope_kind {
      TrackingScopeKind::WatchEffect
      | TrackingScopeKind::WatchPostEffect
      | TrackingScopeKind::WatchSyncEffect
      | TrackingScopeKind::Computed
      | TrackingScopeKind::OnScopeDispose => {
        let Some(argument) = call.arguments.first() else {
          continue;
        };
        let Some((scope_id, body)) = callback_parts(argument) else {
          continue;
        };
        let raw_reads = collect_scope_reads(
          semantic,
          scope_id,
          reactive_bindings,
          composable_instances,
          script_offset,
        );
        let mut reads = raw_reads
          .iter()
          .map(|read| {
            classify_read(&ClassifyRead {
              semantic,
              scope_id,
              body,
              raw_reads: &raw_reads,
              read,
              sfc_source,
              script_offset,
              imported_bindings,
            })
          })
          .collect::<Vec<_>>();
        if scope_kind == TrackingScopeKind::OnScopeDispose {
          for read in &mut reads {
            read.kind = ReactiveReadKind::OutsideTracking;
            read.guards.clear();
            read.guarded_by = None;
          }
        }
        let binding = if scope_kind == TrackingScopeKind::Computed {
          assigned_binding_name(semantic, call.node_id.get())
        } else {
          None
        };
        scopes.push(finish_scope(ScopeBuild {
          kind: scope_kind,
          callee,
          span: source_span(sfc_source, script_offset, call.span),
          reads,
          binding,
          semantic,
          scope_id,
          body,
          reactive_bindings,
          sfc_source,
          script_offset,
        }));
      }
      TrackingScopeKind::EffectScope => {
        // effectScope(fn) or const s = effectScope(); s.run(fn)
        if let Some(argument) = call.arguments.first()
          && let Some((scope_id, body)) = callback_parts(argument)
        {
          let raw_reads = collect_scope_reads(
            semantic,
            scope_id,
            reactive_bindings,
            composable_instances,
            script_offset,
          );
          let reads = raw_reads
            .iter()
            .map(|read| {
              classify_read(&ClassifyRead {
                semantic,
                scope_id,
                body,
                raw_reads: &raw_reads,
                read,
                sfc_source,
                script_offset,
                imported_bindings,
              })
            })
            .collect();
          scopes.push(finish_scope(ScopeBuild {
            kind: TrackingScopeKind::EffectScope,
            callee: callee.clone(),
            span: source_span(sfc_source, script_offset, call.span),
            reads,
            binding: assigned_binding_name(semantic, call.node_id.get()),
            semantic,
            scope_id,
            body,
            reactive_bindings,
            sfc_source,
            script_offset,
          }));
        }
        // Also capture `.run(callback)` on effectScope instances via member call below.
      }
      TrackingScopeKind::WatchSources => {
        let Some(source_argument) = call.arguments.first() else {
          continue;
        };
        let call_span = source_span(sfc_source, script_offset, call.span);
        let reads = collect_watch_source_reads(
          semantic,
          source_argument,
          reactive_bindings,
          composable_instances,
          imported_bindings,
          sfc_source,
          script_offset,
        );
        scopes.push(TrackingScopeFact {
          kind: TrackingScopeKind::WatchSources,
          callee: callee.clone(),
          span: call_span.clone(),
          reads,
          writes: Vec::new(),
          assignment_only: false,
          binding: None,
        });

        if let Some(callback_argument) = call.arguments.get(1)
          && let Some((scope_id, body)) = callback_parts(callback_argument)
        {
          let raw_reads = collect_scope_reads(
            semantic,
            scope_id,
            reactive_bindings,
            composable_instances,
            script_offset,
          );
          let reads = raw_reads
            .iter()
            .map(|read| {
              let mut fact = classify_read(&ClassifyRead {
                semantic,
                scope_id,
                body,
                raw_reads: &raw_reads,
                read,
                sfc_source,
                script_offset,
                imported_bindings,
              });
              fact.kind = ReactiveReadKind::OutsideTracking;
              fact.guards.clear();
              fact.guarded_by = None;
              fact
            })
            .collect();
          scopes.push(finish_scope(ScopeBuild {
            kind: TrackingScopeKind::WatchCallback,
            callee,
            span: call_span,
            reads,
            binding: None,
            semantic,
            scope_id,
            body,
            reactive_bindings,
            sfc_source,
            script_offset,
          }));
        }
      }
      TrackingScopeKind::WatchCallback => {}
    }
  }
  scopes
}

fn assigned_binding_name(semantic: &oxc_semantic::Semantic<'_>, call_id: NodeId) -> Option<String> {
  let AstKind::VariableDeclarator(declarator) = semantic.nodes().parent_kind(call_id) else {
    return None;
  };
  match &declarator.id {
    BindingPattern::BindingIdentifier(identifier) => Some(identifier.name.to_string()),
    _ => None,
  }
}

fn collect_watch_source_reads(
  semantic: &oxc_semantic::Semantic<'_>,
  argument: &Argument<'_>,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &BTreeMap<String, BTreeMap<String, ReactiveBindingKind>>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  sfc_source: &str,
  script_offset: usize,
) -> Vec<ReactiveReadFact> {
  match argument {
    Argument::ArrowFunctionExpression(callback) => {
      let scope_id = callback.node_id.get();
      let body = Some(&*callback.body);
      let raw_reads = collect_scope_reads(
        semantic,
        scope_id,
        reactive_bindings,
        composable_instances,
        script_offset,
      );
      raw_reads
        .iter()
        .map(|read| {
          classify_read(&ClassifyRead {
            semantic,
            scope_id,
            body,
            raw_reads: &raw_reads,
            read,
            sfc_source,
            script_offset,
            imported_bindings,
          })
        })
        .collect()
    }
    Argument::FunctionExpression(callback) => {
      let scope_id = callback.node_id.get();
      let body = callback.body.as_deref();
      let raw_reads = collect_scope_reads(
        semantic,
        scope_id,
        reactive_bindings,
        composable_instances,
        script_offset,
      );
      raw_reads
        .iter()
        .map(|read| {
          classify_read(&ClassifyRead {
            semantic,
            scope_id,
            body,
            raw_reads: &raw_reads,
            read,
            sfc_source,
            script_offset,
            imported_bindings,
          })
        })
        .collect()
    }
    Argument::ArrayExpression(array) => {
      let mut reads = Vec::new();
      for element in &array.elements {
        let Some(expression) = element.as_expression() else {
          continue;
        };
        collect_expression_source_reads(
          semantic,
          expression,
          reactive_bindings,
          sfc_source,
          script_offset,
          &mut reads,
        );
      }
      reads.sort_by_key(|read| read.span.offset);
      reads
    }
    argument => {
      let mut reads = Vec::new();
      if let Some(expression) = argument.as_expression() {
        collect_expression_source_reads(
          semantic,
          expression,
          reactive_bindings,
          sfc_source,
          script_offset,
          &mut reads,
        );
      }
      reads
    }
  }
}

fn collect_expression_source_reads(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  reactive_bindings: &[ReactiveBindingFact],
  sfc_source: &str,
  script_offset: usize,
  reads: &mut Vec<ReactiveReadFact>,
) {
  match expression {
    Expression::Identifier(identifier) => {
      if let Some(binding) = reactive_bindings.iter().find(|binding| {
        binding.name == identifier.name.as_str()
          && reference_resolves_to_binding(semantic, identifier, binding, script_offset)
      }) {
        // Vue's `watch(ref)` / `watch([ref])` tracks the ref's `.value` dep key.
        // Bare reactive objects deep-track many keys — stay quiet rather than invent
        // a property-less edge that is not a runtime onTrack identity.
        if is_ref_like(binding.kind) {
          reads.push(ReactiveReadFact {
            binding: binding.name.clone(),
            property: Some("value".into()),
            kind: ReactiveReadKind::Unconditional,
            guards: Vec::new(),
            guarded_by: None,
            span: source_span(sfc_source, script_offset, identifier.span),
          });
        }
      }
    }
    Expression::StaticMemberExpression(member) => {
      if let Some(object) = member.object.get_identifier_reference()
        && let Some(binding) = reactive_bindings.iter().find(|binding| {
          binding.name == object.name.as_str()
            && reference_resolves_to_binding(semantic, object, binding, script_offset)
            && (!is_ref_like(binding.kind) || member.property.name.as_str() == "value")
        })
      {
        reads.push(ReactiveReadFact {
          binding: binding.name.clone(),
          property: Some(member.property.name.to_string()),
          kind: ReactiveReadKind::Unconditional,
          guards: Vec::new(),
          guarded_by: None,
          span: source_span(sfc_source, script_offset, member.span),
        });
      }
    }
    Expression::ComputedMemberExpression(member) => {
      if let Some(object) = member.object.get_identifier_reference()
        && let Some(property) = member.static_property_name()
        && let Some(binding) = reactive_bindings.iter().find(|binding| {
          binding.name == object.name.as_str()
            && reference_resolves_to_binding(semantic, object, binding, script_offset)
            && (!is_ref_like(binding.kind) || property == "value")
        })
      {
        reads.push(ReactiveReadFact {
          binding: binding.name.clone(),
          property: Some(property.to_string()),
          kind: ReactiveReadKind::Unconditional,
          guards: Vec::new(),
          guarded_by: None,
          span: source_span(sfc_source, script_offset, member.span),
        });
      }
    }
    Expression::ArrayExpression(array) => {
      for element in &array.elements {
        if let Some(inner) = element.as_expression() {
          collect_expression_source_reads(
            semantic,
            inner,
            reactive_bindings,
            sfc_source,
            script_offset,
            reads,
          );
        }
      }
    }
    _ => {}
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
mod oracle;
#[cfg(test)]
mod tests;
