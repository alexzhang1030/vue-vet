//! Composable / factory return-shape analysis over Oxc function bodies.
//!
//! One pass per function collects object bags, nested value bags, scalar
//! factories, forwards, and generic-parameter returns into [`ComposableReturn`].

use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use oxc_ast::{
  AstKind,
  ast::{Argument, BindingPattern, Expression, ObjectPropertyKind},
};
use oxc_semantic::NodeId;
use vue_vet_core::{ReactiveBindingKind, ReactivityGraph, ScriptKind};

use super::super::follow::local_function_id;
use super::super::kinds::{
  collect_binding_identifiers, collect_imported_bindings, reactive_binding_kind,
  reference_resolves_to_binding, resolved_vue_callee,
};
use super::declared_types::{
  composable_shape_from_type_assertion, declared_return_for_arrow, declared_return_for_function,
  generic_param_index_from_assertion,
};
use super::model::{
  ComposableReturn, ComposableShape, DeclaredReturn, PendingValueBagField, ValueBag, ValueBagEntry,
};

/// One-pass index: owning function/arrow → return statement node ids.
///
/// Built once per semantic so composable shape extraction is O(returns) total
/// instead of O(functions × nodes).
#[must_use]
pub fn build_returns_by_function(
  semantic: &oxc_semantic::Semantic<'_>,
) -> BTreeMap<NodeId, Vec<NodeId>> {
  let mut returns_by_function: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
  for (return_id, node) in semantic.nodes().iter_enumerated() {
    let AstKind::ReturnStatement(_) = node.kind() else {
      continue;
    };
    let Some(owner) = semantic.nodes().ancestor_ids(return_id).find(|ancestor_id| {
      matches!(
        semantic.nodes().kind(*ancestor_id),
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
      )
    }) else {
      continue;
    };
    returns_by_function.entry(owner).or_default().push(return_id);
  }
  returns_by_function
}

/// `api.maps.useX` → root `api` plus path segments `maps` / `useX`.
pub fn static_member_call_path(callee: &Expression<'_>) -> Option<(String, Vec<String>)> {
  let mut path = Vec::new();
  let mut current = callee;
  loop {
    match current {
      Expression::StaticMemberExpression(member) => {
        path.push(member.property.name.to_string());
        current = &member.object;
      }
      Expression::ChainExpression(chain) => match &chain.expression {
        oxc_ast::ast::ChainElement::StaticMemberExpression(member) => {
          path.push(member.property.name.to_string());
          current = &member.object;
        }
        _ => return None,
      },
      Expression::Identifier(identifier) => {
        if path.is_empty() {
          return None;
        }
        path.reverse();
        return Some((identifier.name.to_string(), path));
      }
      _ => return None,
    }
  }
}

#[expect(
  clippy::struct_excessive_bools,
  reason = "return-kind accumulator tracks independent under-approx signals"
)]
struct ReturnKindAccum {
  shape: BTreeMap<String, ReactiveBindingKind>,
  open_reactive_spread: bool,
  ambiguous: BTreeSet<String>,
  pending_value_bag_fields: BTreeMap<String, PendingValueBagField>,
  value_bag: ValueBag,
  factory_kind: Option<ReactiveBindingKind>,
  factory_conflict: bool,
  saw_object_return: bool,
  saw_scalar_return: bool,
  /// `return <call>(...).value` — provisional until paired with a plain object declaration.
  saw_unwrapped_state: bool,
  /// Sole unresolved `return callee(...)` forward target.
  forward_callee: Option<String>,
  forward_conflict: bool,
  /// Sole `return expr as T` where `T` is an enclosing type parameter index.
  generic_param: Option<u8>,
  generic_param_conflict: bool,
}

impl ReturnKindAccum {
  fn absorb_object_shape(&mut self, shape: ComposableShape) {
    self.saw_object_return = true;
    self.open_reactive_spread = self.open_reactive_spread || shape.open_reactive_spread;
    for (field, kind) in shape.fields {
      merge_shape_field(&mut self.shape, &mut self.ambiguous, field, kind);
    }
    for (field, pending) in shape.pending_value_bag_fields {
      self.pending_value_bag_fields.entry(field).or_insert(pending);
    }
  }

  #[expect(
    clippy::too_many_arguments,
    reason = "return-kind classification needs semantic + graph + import/param context"
  )]
  fn consider(
    &mut self,
    semantic: &oxc_semantic::Semantic<'_>,
    expression: &Expression<'_>,
    graph: &ReactivityGraph,
    imported_bindings: &BTreeMap<String, (String, String)>,
    param_names: &BTreeSet<String>,
    script_offset: usize,
    function_id: NodeId,
    returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
    visiting: &mut BTreeSet<NodeId>,
  ) {
    let expression = match expression {
      Expression::ParenthesizedExpression(paren) => &paren.expression,
      other => other,
    };
    // `return inject(key) as Ctx` / `return expr as { mapId: Ref<…> }` — peel the
    // asserted object-bag type before walking the inner expression.
    if let Some(shape) = composable_shape_from_type_assertion(semantic, expression) {
      self.absorb_object_shape(shape);
      return;
    }
    // `return value as T` where `T` is an enclosing type parameter (context factories).
    if let Some(index) = generic_param_index_from_assertion(semantic, function_id, expression) {
      match self.generic_param {
        None => self.generic_param = Some(index),
        Some(existing) if existing == index => {}
        Some(_) => self.generic_param_conflict = true,
      }
      return;
    }
    let expression = match expression {
      Expression::TSAsExpression(assertion) => &assertion.expression,
      Expression::TSTypeAssertion(assertion) => &assertion.expression,
      other => other,
    };
    if matches!(expression, Expression::ObjectExpression(_)) {
      self.saw_object_return = true;
      let opened = merge_return_object_into_shape(
        semantic,
        expression,
        graph,
        imported_bindings,
        param_names,
        script_offset,
        function_id,
        &mut self.shape,
        &mut self.ambiguous,
        &mut self.pending_value_bag_fields,
      );
      self.open_reactive_spread = self.open_reactive_spread || opened;
      // Value-bag walk is for method nests (`{ maps: { useX } }`). Skip when this
      // return is already a reactive field bag — the common composable path.
      if self.shape.is_empty()
        && !self.open_reactive_spread
        && self.pending_value_bag_fields.is_empty()
      {
        merge_return_object_into_value_bag(
          semantic,
          expression,
          graph,
          imported_bindings,
          script_offset,
          returns_by_function,
          visiting,
          &mut self.value_bag,
        );
      }
      return;
    }
    if is_to_refs_call(semantic, expression, imported_bindings) {
      self.saw_object_return = true;
      self.open_reactive_spread = true;
      return;
    }
    if let Expression::Identifier(identifier) = expression {
      if let Some(shape) = composable_shape_from_identifier_assertion_init(semantic, identifier) {
        self.absorb_object_shape(shape);
        return;
      }
      if identifier_initialized_with_to_refs(semantic, function_id, identifier, imported_bindings) {
        self.saw_object_return = true;
        self.open_reactive_spread = true;
        return;
      }
      // `const storage = useX(); return storage` — forward to `useX`'s export kind at
      // link time (same as `return useX()`). Covers storage helpers that return a
      // local binding without a declared return type on the wrapper.
      if let Some(callee) = initializer_call_callee_name(semantic, function_id, identifier) {
        match &self.forward_callee {
          None => self.forward_callee = Some(callee),
          Some(existing) if existing == &callee => {}
          Some(_) => self.forward_conflict = true,
        }
        return;
      }
    }
    if let Expression::CallExpression(call) = expression
      && let Some(forwarded) = resolve_call_return_forward(
        semantic,
        call,
        graph,
        imported_bindings,
        script_offset,
        returns_by_function,
        visiting,
      )
    {
      match forwarded {
        ComposableReturn::Object(shape) => {
          self.saw_object_return = true;
          self.open_reactive_spread = self.open_reactive_spread || shape.open_reactive_spread;
          for (field, kind) in shape.fields {
            merge_shape_field(&mut self.shape, &mut self.ambiguous, field, kind);
          }
          for (field, pending) in shape.pending_value_bag_fields {
            self.pending_value_bag_fields.entry(field).or_insert(pending);
          }
        }
        ComposableReturn::ValueBag(bag) => {
          self.saw_object_return = true;
          merge_value_bag(&mut self.value_bag, bag);
        }
        ComposableReturn::Factory(kind) => {
          self.saw_scalar_return = true;
          match self.factory_kind {
            None => self.factory_kind = Some(kind),
            Some(existing) if existing == kind => {}
            Some(_) => self.factory_conflict = true,
          }
        }
        ComposableReturn::Forward(name) => match &self.forward_callee {
          None => self.forward_callee = Some(name),
          Some(existing) if existing == &name => {}
          Some(_) => self.forward_conflict = true,
        },
        ComposableReturn::UnwrappedState => {
          self.saw_scalar_return = true;
          self.saw_unwrapped_state = true;
        }
        ComposableReturn::GenericParam(index) => match self.generic_param {
          None => self.generic_param = Some(index),
          Some(existing) if existing == index => {}
          Some(_) => self.generic_param_conflict = true,
        },
      }
      return;
    }
    if is_unwrapped_call_return(semantic, expression, imported_bindings) {
      self.saw_scalar_return = true;
      self.saw_unwrapped_state = true;
      return;
    }
    self.saw_scalar_return = true;
    let Some(kind) = reactive_return_kind(
      semantic,
      expression,
      graph,
      imported_bindings,
      param_names,
      script_offset,
    ) else {
      self.factory_conflict = true;
      return;
    };
    match self.factory_kind {
      None => self.factory_kind = Some(kind),
      Some(existing) if existing == kind => {}
      Some(_) => self.factory_conflict = true,
    }
  }

  fn finish(self) -> Option<ComposableReturn> {
    if self.saw_object_return && self.saw_scalar_return {
      return None;
    }
    if self.saw_object_return {
      if !self.shape.is_empty()
        || self.open_reactive_spread
        || !self.pending_value_bag_fields.is_empty()
      {
        return Some(ComposableReturn::Object(ComposableShape {
          fields: self.shape,
          open_reactive_spread: self.open_reactive_spread,
          pending_value_bag_fields: self.pending_value_bag_fields,
        }));
      }
      if !self.value_bag.is_empty() {
        return Some(ComposableReturn::ValueBag(self.value_bag));
      }
    }
    if self.saw_scalar_return && !self.factory_conflict {
      if let Some(kind) = self.factory_kind {
        return Some(ComposableReturn::Factory(kind));
      }
      if self.saw_unwrapped_state {
        return Some(ComposableReturn::UnwrappedState);
      }
    }
    if !self.saw_object_return
      && !self.saw_scalar_return
      && !self.forward_conflict
      && let Some(callee) = self.forward_callee
    {
      return Some(ComposableReturn::Forward(callee));
    }
    if !self.saw_object_return
      && !self.saw_scalar_return
      && !self.generic_param_conflict
      && let Some(index) = self.generic_param
    {
      return Some(ComposableReturn::GenericParam(index));
    }
    None
  }
}

/// Object bag / value bag / scalar factory return for a function/arrow (under-approx).
///
/// Single-pass — callers should prefer this over calling shape + value-bag + factory
/// helpers separately (each would re-walk returns). Standalone entry builds one
/// canonical import index and delegates to the borrowed-index implementation.
pub fn composable_return_with_index(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  graph: &ReactivityGraph,
  script_offset: usize,
  returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
) -> Option<ComposableReturn> {
  let imported_bindings = collect_imported_bindings(semantic);
  composable_return_with_borrowed_index(
    semantic,
    function_id,
    graph,
    script_offset,
    returns_by_function,
    &imported_bindings,
  )
}

pub(super) fn composable_return_with_borrowed_index(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  graph: &ReactivityGraph,
  script_offset: usize,
  returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> Option<ComposableReturn> {
  let mut visiting = BTreeSet::new();
  composable_return_with_index_visiting(
    semantic,
    function_id,
    graph,
    script_offset,
    returns_by_function,
    imported_bindings,
    &mut visiting,
  )
}

fn composable_return_with_index_visiting(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  graph: &ReactivityGraph,
  script_offset: usize,
  returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  visiting: &mut BTreeSet<NodeId>,
) -> Option<ComposableReturn> {
  if !visiting.insert(function_id) {
    return None;
  }
  let param_names = function_param_names(semantic, function_id);
  let mut accum = ReturnKindAccum {
    shape: BTreeMap::new(),
    open_reactive_spread: false,
    ambiguous: BTreeSet::new(),
    pending_value_bag_fields: BTreeMap::new(),
    value_bag: ValueBag::default(),
    factory_kind: None,
    factory_conflict: false,
    saw_object_return: false,
    saw_scalar_return: false,
    saw_unwrapped_state: false,
    forward_callee: None,
    forward_conflict: false,
    generic_param: None,
    generic_param_conflict: false,
  };

  // `() => ({ field: ref(0) })` / `() => ref(0)` expression body — no ReturnStatement.
  if let AstKind::ArrowFunctionExpression(arrow) = semantic.nodes().kind(function_id)
    && arrow.expression
    && let Some(statement) = arrow.body.statements.first()
    && let oxc_ast::ast::Statement::ExpressionStatement(expression) = statement
  {
    accum.consider(
      semantic,
      &expression.expression,
      graph,
      imported_bindings,
      &param_names,
      script_offset,
      function_id,
      returns_by_function,
      visiting,
    );
  }

  if let Some(return_ids) = returns_by_function.get(&function_id) {
    for &return_id in return_ids {
      let AstKind::ReturnStatement(statement) = semantic.nodes().kind(return_id) else {
        continue;
      };
      let Some(argument) = &statement.argument else {
        accum.factory_conflict = true;
        continue;
      };
      accum.consider(
        semantic,
        argument,
        graph,
        imported_bindings,
        &param_names,
        script_offset,
        function_id,
        returns_by_function,
        visiting,
      );
    }
  }

  visiting.remove(&function_id);
  accum.finish()
}

/// `return <call>(...).value` where callee is unresolved or imported from `#imports`.
///
/// Name-agnostic: pairs with a declared plain-object return to yield `Factory(Reactive)`.
fn is_unwrapped_call_return(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> bool {
  let Expression::StaticMemberExpression(member) = expression else {
    return false;
  };
  if member.property.name.as_str() != "value" {
    return false;
  }
  let Expression::CallExpression(call) = &member.object else {
    return false;
  };
  let Some(callee) = call.callee.get_identifier_reference() else {
    return false;
  };
  if let Some((source, _)) = imported_bindings.get(callee.name.as_str()) {
    return source == "#imports";
  }
  let Some(reference_id) = callee.reference_id.get() else {
    return false;
  };
  semantic.scoping().get_reference(reference_id).symbol_id().is_none()
}

/// `const ctx = … as Ctx; return ctx` — one-hop init assertion → bag shape.
fn composable_shape_from_identifier_assertion_init<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  identifier: &oxc_ast::ast::IdentifierReference<'_>,
) -> Option<ComposableShape> {
  let reference_id = identifier.reference_id.get()?;
  let symbol_id = semantic.scoping().get_reference(reference_id).symbol_id()?;
  let decl = semantic.symbol_declaration(symbol_id);
  let AstKind::VariableDeclarator(declarator) = decl.kind() else {
    return None;
  };
  let init = declarator.init.as_ref()?;
  composable_shape_from_type_assertion(semantic, init)
}

fn is_to_refs_call(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> bool {
  let Expression::CallExpression(call) = expression else {
    return false;
  };
  resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Script)
    .is_some_and(|callee| callee == "toRefs")
}

fn identifier_initialized_with_to_refs(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  identifier: &oxc_ast::ast::IdentifierReference<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> bool {
  let Some(init) = identifier_initializer_expression(semantic, function_id, identifier) else {
    return false;
  };
  is_to_refs_call(semantic, init, imported_bindings)
}

/// `const local = callee(...)` / `const local = await callee(...)` owned by `function_id`.
///
/// Returns the bare callee name for export forwarding (`return local` ≡ `return callee()`).
fn initializer_call_callee_name(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  identifier: &oxc_ast::ast::IdentifierReference<'_>,
) -> Option<String> {
  let init = identifier_initializer_expression(semantic, function_id, identifier)?;
  let call = call_expression_from_init(init)?;
  let callee = call.callee.get_identifier_reference()?;
  // Skip Vue primitives — those stay on the scalar factory path via graph seeds.
  if reactive_binding_kind(callee.name.as_str()).is_some() {
    return None;
  }
  Some(callee.name.to_string())
}

fn identifier_initializer_expression<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  function_id: NodeId,
  identifier: &oxc_ast::ast::IdentifierReference<'_>,
) -> Option<&'a Expression<'a>> {
  let reference_id = identifier.reference_id.get()?;
  let symbol_id = semantic.scoping().get_reference(reference_id).symbol_id()?;
  let decl = semantic.symbol_declaration(symbol_id);
  let AstKind::VariableDeclarator(declarator) = decl.kind() else {
    return None;
  };
  let owned_here = semantic.nodes().ancestor_ids(decl.id()).any(|ancestor| ancestor == function_id);
  if !owned_here {
    return None;
  }
  declarator.init.as_ref()
}

/// Peel `await` / TS assertions / non-null to reach an underlying call expression.
fn call_expression_from_init<'a>(
  expression: &'a Expression<'a>,
) -> Option<&'a oxc_ast::ast::CallExpression<'a>> {
  let mut current = expression;
  for _ in 0..4 {
    match current {
      Expression::CallExpression(call) => return Some(call),
      Expression::AwaitExpression(await_expr) => current = &await_expr.argument,
      Expression::TSAsExpression(assertion) => current = &assertion.expression,
      Expression::TSTypeAssertion(assertion) => current = &assertion.expression,
      Expression::TSNonNullExpression(non_null) => current = &non_null.expression,
      Expression::ParenthesizedExpression(paren) => current = &paren.expression,
      _ => return None,
    }
  }
  None
}

fn resolve_call_return_forward(
  semantic: &oxc_semantic::Semantic<'_>,
  call: &oxc_ast::ast::CallExpression<'_>,
  graph: &ReactivityGraph,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_offset: usize,
  returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
  visiting: &mut BTreeSet<NodeId>,
) -> Option<ComposableReturn> {
  let callee = call.callee.get_identifier_reference()?;
  let name = callee.name.as_str();
  // Vue primitives (`ref`, `computed`, …) stay on the scalar factory path.
  if resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Script)
    .is_some_and(|resolved| reactive_binding_kind(&resolved).is_some())
  {
    return None;
  }
  // Same-file function / const arrow — recurse into its return kind.
  if let Some(callee_id) = local_function_id(semantic, callee) {
    return composable_return_with_index_visiting(
      semantic,
      callee_id,
      graph,
      script_offset,
      returns_by_function,
      imported_bindings,
      visiting,
    )
    .or_else(|| {
      // Declared return on the callee when body is quiet.
      match semantic.nodes().kind(callee_id) {
        AstKind::Function(function) => {
          declared_return_for_function(semantic, function).and_then(|declared| match declared {
            DeclaredReturn::Composable(shape) => Some(ComposableReturn::Object(shape)),
            DeclaredReturn::Factory(kind) => Some(ComposableReturn::Factory(kind)),
            DeclaredReturn::PlainObject => None,
          })
        }
        AstKind::ArrowFunctionExpression(arrow) => declared_return_for_arrow(semantic, arrow)
          .and_then(|declared| match declared {
            DeclaredReturn::Composable(shape) => Some(ComposableReturn::Object(shape)),
            DeclaredReturn::Factory(kind) => Some(ComposableReturn::Factory(kind)),
            DeclaredReturn::PlainObject => None,
          }),
        _ => None,
      }
    });
  }
  // Import / unresolved — forward by local name at link time.
  if imported_bindings.contains_key(name) {
    return Some(ComposableReturn::Forward(name.to_owned()));
  }
  let reference_id = callee.reference_id.get()?;
  if semantic.scoping().get_reference(reference_id).symbol_id().is_none() {
    return Some(ComposableReturn::Forward(name.to_owned()));
  }
  None
}

fn merge_value_bag(into: &mut ValueBag, from: ValueBag) {
  for (key, entry) in from.entries {
    match (into.entries.get_mut(&key), entry) {
      (Some(ValueBagEntry::Nested(existing)), ValueBagEntry::Nested(incoming)) => {
        merge_value_bag(existing, incoming);
      }
      (None, entry) => {
        into.entries.insert(key, entry);
      }
      (Some(_), _) => {
        // Conflicting entry kinds stay with the first under-approx winner.
      }
    }
  }
}

#[expect(clippy::too_many_arguments, reason = "value-bag merge mirrors object-shape helper arity")]
fn merge_return_object_into_value_bag(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  graph: &ReactivityGraph,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_offset: usize,
  returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
  visiting: &mut BTreeSet<NodeId>,
  bag: &mut ValueBag,
) {
  let expression = match expression {
    Expression::ParenthesizedExpression(paren) => &paren.expression,
    other => other,
  };
  let Expression::ObjectExpression(object) = expression else {
    return;
  };
  for property in &object.properties {
    let ObjectPropertyKind::ObjectProperty(property) = property else {
      continue;
    };
    let Some(exported) = property.key.static_name() else {
      continue;
    };
    let key = exported.into_owned();
    if let Some(entry) = value_bag_entry_from_expression(
      semantic,
      &property.value,
      graph,
      imported_bindings,
      script_offset,
      returns_by_function,
      visiting,
    ) {
      bag.entries.entry(key).or_insert(entry);
    }
  }
}

fn value_bag_entry_from_expression(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  graph: &ReactivityGraph,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_offset: usize,
  returns_by_function: &BTreeMap<NodeId, Vec<NodeId>>,
  visiting: &mut BTreeSet<NodeId>,
) -> Option<ValueBagEntry> {
  let expression = match expression {
    Expression::ParenthesizedExpression(paren) => &paren.expression,
    other => other,
  };
  match expression {
    Expression::Identifier(identifier) => {
      let callee_id = local_function_id(semantic, identifier)?;
      match composable_return_with_index_visiting(
        semantic,
        callee_id,
        graph,
        script_offset,
        returns_by_function,
        imported_bindings,
        visiting,
      )? {
        ComposableReturn::Object(shape) => Some(ValueBagEntry::Method(shape)),
        ComposableReturn::Factory(kind) => Some(ValueBagEntry::MethodFactory(kind)),
        ComposableReturn::ValueBag(nested) => Some(ValueBagEntry::Nested(nested)),
        ComposableReturn::Forward(callee) => Some(ValueBagEntry::MethodForward(callee)),
        ComposableReturn::GenericParam(index) => Some(ValueBagEntry::MethodGeneric(index)),
        ComposableReturn::UnwrappedState => None,
      }
    }
    Expression::CallExpression(call) => match resolve_call_return_forward(
      semantic,
      call,
      graph,
      imported_bindings,
      script_offset,
      returns_by_function,
      visiting,
    )? {
      ComposableReturn::ValueBag(nested) => Some(ValueBagEntry::Nested(nested)),
      ComposableReturn::Object(shape) => Some(ValueBagEntry::Method(shape)),
      ComposableReturn::Factory(kind) => Some(ValueBagEntry::MethodFactory(kind)),
      ComposableReturn::Forward(callee) => Some(ValueBagEntry::MethodForward(callee)),
      ComposableReturn::GenericParam(index) => Some(ValueBagEntry::MethodGeneric(index)),
      ComposableReturn::UnwrappedState => None,
    },
    Expression::ObjectExpression(_) => {
      let mut nested = ValueBag::default();
      merge_return_object_into_value_bag(
        semantic,
        expression,
        graph,
        imported_bindings,
        script_offset,
        returns_by_function,
        visiting,
        &mut nested,
      );
      (!nested.is_empty()).then_some(ValueBagEntry::Nested(nested))
    }
    Expression::FunctionExpression(function) => {
      match composable_return_with_index_visiting(
        semantic,
        function.node_id.get(),
        graph,
        script_offset,
        returns_by_function,
        imported_bindings,
        visiting,
      )? {
        ComposableReturn::Object(shape) => Some(ValueBagEntry::Method(shape)),
        ComposableReturn::Factory(kind) => Some(ValueBagEntry::MethodFactory(kind)),
        ComposableReturn::ValueBag(nested) => Some(ValueBagEntry::Nested(nested)),
        ComposableReturn::Forward(callee) => Some(ValueBagEntry::MethodForward(callee)),
        ComposableReturn::GenericParam(index) => Some(ValueBagEntry::MethodGeneric(index)),
        ComposableReturn::UnwrappedState => None,
      }
    }
    Expression::ArrowFunctionExpression(arrow) => {
      match composable_return_with_index_visiting(
        semantic,
        arrow.node_id.get(),
        graph,
        script_offset,
        returns_by_function,
        imported_bindings,
        visiting,
      )? {
        ComposableReturn::Object(shape) => Some(ValueBagEntry::Method(shape)),
        ComposableReturn::Factory(kind) => Some(ValueBagEntry::MethodFactory(kind)),
        ComposableReturn::ValueBag(nested) => Some(ValueBagEntry::Nested(nested)),
        ComposableReturn::Forward(callee) => Some(ValueBagEntry::MethodForward(callee)),
        ComposableReturn::GenericParam(index) => Some(ValueBagEntry::MethodGeneric(index)),
        ComposableReturn::UnwrappedState => None,
      }
    }
    _ => None,
  }
}

#[expect(
  clippy::too_many_arguments,
  reason = "shape merge is a pure helper; packing args would obscure the call sites"
)]
fn merge_return_object_into_shape(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  graph: &ReactivityGraph,
  imported_bindings: &BTreeMap<String, (String, String)>,
  param_names: &BTreeSet<String>,
  script_offset: usize,
  function_id: NodeId,
  shape: &mut BTreeMap<String, ReactiveBindingKind>,
  ambiguous: &mut BTreeSet<String>,
  pending_value_bag_fields: &mut BTreeMap<String, PendingValueBagField>,
) -> bool {
  // `() => ({ field })` wraps the object in parentheses.
  let expression = match expression {
    Expression::ParenthesizedExpression(paren) => &paren.expression,
    other => other,
  };
  // `return toRefs(param)` — every static key is ToRef when the argument is a parameter.
  if let Expression::CallExpression(call) = expression
    && resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Script)
      .is_some_and(|callee| callee == "toRefs")
    && call
      .arguments
      .first()
      .and_then(Argument::as_expression)
      .and_then(Expression::get_identifier_reference)
      .is_some_and(|identifier| param_names.contains(identifier.name.as_str()))
  {
    // Without an object shape we cannot invent keys; leave quiet.
    return false;
  }
  let Expression::ObjectExpression(object) = expression else {
    return false;
  };
  let mut open_reactive_spread = false;
  for property in &object.properties {
    match property {
      ObjectPropertyKind::SpreadProperty(spread) => {
        let Some(ident) = spread.argument.get_identifier_reference() else {
          continue;
        };
        let bag = ident.name.as_str();
        let from_members = bag_ref_fields_in_function(semantic, function_id, bag);
        if from_members.is_empty() {
          continue;
        }
        open_reactive_spread = true;
        for (exported, kind) in from_members {
          merge_shape_field(shape, ambiguous, exported, kind);
        }
      }
      ObjectPropertyKind::ObjectProperty(property) => {
        let Some(exported) = property.key.static_name() else {
          continue;
        };
        let key = exported.into_owned();
        if let Some(kind) = reactive_return_kind(
          semantic,
          &property.value,
          graph,
          imported_bindings,
          param_names,
          script_offset,
        ) {
          merge_shape_field(shape, ambiguous, key, kind);
          continue;
        }
        // `return { isLoading }` after `const { isLoading } = api.ns.useX()`.
        if let Some(reference) = property.value.get_identifier_reference()
          && let Some(pending) = pending_value_bag_field_from_binding(semantic, reference)
        {
          pending_value_bag_fields.entry(key).or_insert(pending);
        }
      }
    }
  }
  open_reactive_spread
}

/// Binding from `const { field } = root.a.b()` → pending value-bag field ref.
fn pending_value_bag_field_from_binding(
  semantic: &oxc_semantic::Semantic<'_>,
  reference: &oxc_ast::ast::IdentifierReference<'_>,
) -> Option<PendingValueBagField> {
  let reference_id = reference.reference_id.get()?;
  let symbol_id = semantic.scoping().get_reference(reference_id).symbol_id()?;
  let local_name = reference.name.as_str();
  let decl = semantic.symbol_declaration(symbol_id);
  // Oxc often reports the whole `const { a, b } = …` declarator as the symbol
  // declaration — not the inner BindingIdentifier — so handle that node first.
  let (field, call) = match decl.kind() {
    AstKind::VariableDeclarator(declarator) => {
      object_pattern_field_and_member_call(declarator, local_name)?
    }
    AstKind::BindingIdentifier(_) => {
      let mut field_name: Option<String> = None;
      let mut call_expr: Option<&oxc_ast::ast::CallExpression<'_>> = None;
      for ancestor_id in semantic.nodes().ancestor_ids(decl.id()) {
        match semantic.nodes().kind(ancestor_id) {
          AstKind::BindingProperty(property) if field_name.is_none() => {
            field_name = property.key.static_name().map(std::borrow::Cow::into_owned);
          }
          AstKind::VariableDeclarator(declarator) => {
            if let Some(Expression::CallExpression(call)) = &declarator.init {
              call_expr = Some(call);
            }
            break;
          }
          _ => {}
        }
      }
      (field_name?, call_expr?)
    }
    _ => return None,
  };
  // Member path `api.ns.useX()` → value-bag walk; bare `useX()` → composable field.
  if let Some((root, path)) = static_member_call_path(&call.callee) {
    if path.is_empty() {
      return None;
    }
    return Some(PendingValueBagField { root, path, field });
  }
  let callee = call.callee.get_identifier_reference()?;
  Some(PendingValueBagField { root: callee.name.to_string(), path: Vec::new(), field })
}

fn object_pattern_field_and_member_call<'a>(
  declarator: &'a oxc_ast::ast::VariableDeclarator<'a>,
  local_name: &str,
) -> Option<(String, &'a oxc_ast::ast::CallExpression<'a>)> {
  let BindingPattern::ObjectPattern(pattern) = &declarator.id else {
    return None;
  };
  let Expression::CallExpression(call) = declarator.init.as_ref()? else {
    return None;
  };
  for property in &pattern.properties {
    let mut identifiers = Vec::new();
    collect_binding_identifiers(&property.value, &mut identifiers);
    if !identifiers.iter().any(|(name, _)| name == local_name) {
      continue;
    }
    let field = property.key.static_name().map(std::borrow::Cow::into_owned)?;
    return Some((field, call));
  }
  None
}

fn merge_shape_field(
  shape: &mut BTreeMap<String, ReactiveBindingKind>,
  ambiguous: &mut BTreeSet<String>,
  exported: String,
  kind: ReactiveBindingKind,
) {
  if ambiguous.contains(&exported) {
    return;
  }
  match shape.entry(exported.clone()) {
    Entry::Vacant(entry) => {
      entry.insert(kind);
    }
    Entry::Occupied(entry) if *entry.get() == kind => {}
    Entry::Occupied(entry) => {
      entry.remove();
      ambiguous.insert(exported);
    }
  }
}

/// `bag.field.value` reads inside `function_id` → `{ field: Ref }` (under-approx).
fn bag_ref_fields_in_function(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  bag: &str,
) -> BTreeMap<String, ReactiveBindingKind> {
  let mut fields = BTreeMap::new();
  for (node_id, node) in semantic.nodes().iter_enumerated() {
    if !semantic.nodes().ancestor_ids(node_id).any(|ancestor| ancestor == function_id) {
      continue;
    }
    let AstKind::StaticMemberExpression(outer) = node.kind() else {
      continue;
    };
    if outer.property.name.as_str() != "value" {
      continue;
    }
    let Expression::StaticMemberExpression(inner) = &outer.object else {
      continue;
    };
    let Some(root) = inner.object.get_identifier_reference() else {
      continue;
    };
    if root.name.as_str() != bag {
      continue;
    }
    fields.insert(inner.property.name.to_string(), ReactiveBindingKind::Ref);
  }
  fields
}

fn function_param_names(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
) -> BTreeSet<String> {
  let mut names = BTreeSet::new();
  let parameters = match semantic.nodes().kind(function_id) {
    AstKind::Function(function) => function.params.items.as_slice(),
    AstKind::ArrowFunctionExpression(callback) => callback.params.items.as_slice(),
    _ => return names,
  };
  for parameter in parameters {
    let mut identifiers = Vec::new();
    collect_binding_identifiers(&parameter.pattern, &mut identifiers);
    for (name, _) in identifiers {
      names.insert(name);
    }
  }
  names
}

fn reactive_return_kind(
  semantic: &oxc_semantic::Semantic<'_>,
  expression: &Expression<'_>,
  graph: &ReactivityGraph,
  imported_bindings: &BTreeMap<String, (String, String)>,
  param_names: &BTreeSet<String>,
  script_offset: usize,
) -> Option<ReactiveBindingKind> {
  if let Some(reference) = expression.get_identifier_reference() {
    if param_names.contains(reference.name.as_str()) {
      // Parametric pass-through: treat as reactive object/ref surface.
      return Some(ReactiveBindingKind::Reactive);
    }
    return graph
      .bindings
      .iter()
      .find(|binding| {
        binding.name == reference.name.as_str()
          && reference_resolves_to_binding(semantic, reference, binding, script_offset)
      })
      .map(|binding| binding.kind);
  }

  let Expression::CallExpression(call) = expression else {
    return None;
  };
  let callee = resolved_vue_callee(semantic, &call.callee, imported_bindings, ScriptKind::Script)?;
  if matches!(callee.as_str(), "toRef" | "toRefs") {
    // Parametric when first argument is a function parameter.
    if call
      .arguments
      .first()
      .and_then(Argument::as_expression)
      .and_then(Expression::get_identifier_reference)
      .is_some_and(|identifier| param_names.contains(identifier.name.as_str()))
    {
      return Some(ReactiveBindingKind::ToRef);
    }
  }
  reactive_binding_kind(&callee)
}
