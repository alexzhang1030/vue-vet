//! Options-object callback bag slots: `defineFormProps({ setup({ values }) })`.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::{
  AstKind,
  ast::{BindingPattern, Expression, ObjectPropertyKind, PropertyKey},
};
use oxc_semantic::Semantic;
use vue_vet_core::{ReactiveBindingFact, ReactiveBindingKind};

use super::super::expr::{is_nested_in_function, peel_parens};
use super::ComposableShape;

/// Call-argument index → object-property name → first-param bag of that callback.
///
/// Example: `defineFormProps({ setup({ values }) {…} })` where the callee declares
/// `props: { setup?: (ctx: { values: Ref<T> }) => … }` publishes
/// `{ 0: { "setup": shape(values→Ref) } }`.
pub type OptionsCallbackSlots = BTreeMap<u32, BTreeMap<String, ComposableShape>>;

#[derive(Clone)]
struct InterfaceDecl<'a> {
  members: &'a [oxc_ast::ast::TSSignature<'a>],
  extends: Vec<&'a str>,
}

struct TypeDeclIndex<'a> {
  interfaces: BTreeMap<&'a str, InterfaceDecl<'a>>,
  aliases: BTreeMap<&'a str, &'a oxc_ast::ast::TSType<'a>>,
}

impl<'a> TypeDeclIndex<'a> {
  fn build(semantic: &'a Semantic<'a>) -> Self {
    let mut interfaces = BTreeMap::new();
    let mut aliases = BTreeMap::new();
    for node in semantic.nodes() {
      match node.kind() {
        AstKind::TSInterfaceDeclaration(interface) => {
          let extends = interface
            .extends
            .iter()
            .filter_map(|heritage| match &heritage.expression {
              Expression::Identifier(identifier) => Some(identifier.name.as_str()),
              _ => None,
            })
            .collect();
          interfaces.insert(
            interface.id.name.as_str(),
            InterfaceDecl { members: interface.body.body.as_slice(), extends },
          );
        }
        AstKind::TSTypeAliasDeclaration(alias) => {
          aliases.insert(alias.id.name.as_str(), &alias.type_annotation);
        }
        _ => {}
      }
    }
    Self { interfaces, aliases }
  }
}

/// Named locals whose options-object params carry typed callback bags.
pub fn collect_local_options_callback_slots(
  semantic: &Semantic<'_>,
) -> BTreeMap<String, OptionsCallbackSlots> {
  let mut out = BTreeMap::new();
  for node in semantic.nodes() {
    match node.kind() {
      AstKind::Function(function) => {
        let Some(identifier) = &function.id else {
          continue;
        };
        if let Some(slots) = options_callback_slots_from_params(
          semantic,
          &function.params,
          function.type_parameters.as_deref(),
        ) {
          out.insert(identifier.name.to_string(), slots);
        }
      }
      AstKind::VariableDeclarator(declarator) => {
        if is_nested_in_function(semantic, node.id()) {
          continue;
        }
        let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
          continue;
        };
        let name = identifier.name.to_string();
        match &declarator.init {
          Some(Expression::ArrowFunctionExpression(arrow)) => {
            if let Some(slots) = options_callback_slots_from_params(
              semantic,
              &arrow.params,
              arrow.type_parameters.as_deref(),
            ) {
              out.insert(name, slots);
            }
          }
          Some(Expression::FunctionExpression(function)) => {
            if let Some(slots) = options_callback_slots_from_params(
              semantic,
              &function.params,
              function.type_parameters.as_deref(),
            ) {
              out.insert(name, slots);
            }
          }
          None => {
            if let Some(annotation) = declarator.type_annotation.as_ref()
              && let Some(params) = function_type_formal_params(&annotation.type_annotation)
              && let Some(slots) = options_callback_slots_from_params(semantic, params, None)
            {
              out.insert(name, slots);
            }
          }
          _ => {}
        }
      }
      _ => {}
    }
  }
  out
}

/// Seed `callee({ setup({ values }) {…} })` from declared options-callback slots.
pub fn seed_options_callback_params_at_calls(
  semantic: &Semantic<'_>,
  slots_by_callee: &BTreeMap<String, OptionsCallbackSlots>,
  span_source: &str,
  span_base: usize,
  into: &mut Vec<ReactiveBindingFact>,
) {
  if slots_by_callee.is_empty() {
    return;
  }
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = call.callee.get_identifier_reference() else {
      continue;
    };
    let Some(arg_slots) = slots_by_callee.get(callee.name.as_str()) else {
      continue;
    };
    for (arg_index, prop_slots) in arg_slots {
      let Some(argument) = call.arguments.get(usize::try_from(*arg_index).unwrap_or(usize::MAX))
      else {
        continue;
      };
      let Some(expression) = argument.as_expression() else {
        continue;
      };
      let Expression::ObjectExpression(object) = peel_parens(expression) else {
        continue;
      };
      for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
          continue;
        };
        let prop_name = match &property.key {
          PropertyKey::StaticIdentifier(identifier) => identifier.name.as_str(),
          PropertyKey::StringLiteral(literal) => literal.value.as_str(),
          _ => continue,
        };
        let Some(shape) = prop_slots.get(prop_name) else {
          continue;
        };
        let Some(callback_params) = callback_params_from_expression(&property.value) else {
          continue;
        };
        push_options_callback_pattern_bindings(
          callback_params,
          shape,
          span_source,
          span_base,
          into,
        );
      }
    }
  }
}

fn options_callback_slots_from_params(
  semantic: &Semantic<'_>,
  params: &oxc_ast::ast::FormalParameters<'_>,
  type_params: Option<&oxc_ast::ast::TSTypeParameterDeclaration<'_>>,
) -> Option<OptionsCallbackSlots> {
  let mut slots = OptionsCallbackSlots::new();
  let mut index = None;
  let type_param_constraints = type_param_constraint_map(type_params);
  for (arg_index, parameter) in params.items.iter().enumerate() {
    let Some(annotation) = parameter.type_annotation.as_ref() else {
      continue;
    };
    let prop_slots = options_callback_props_from_ts_type(
      semantic,
      &annotation.type_annotation,
      0,
      &mut index,
      &mut BTreeSet::new(),
      &type_param_constraints,
    );
    if !prop_slots.is_empty() {
      slots.insert(u32::try_from(arg_index).unwrap_or(u32::MAX), prop_slots);
    }
  }
  (!slots.is_empty()).then_some(slots)
}

fn type_param_constraint_map<'a>(
  type_params: Option<&'a oxc_ast::ast::TSTypeParameterDeclaration<'a>>,
) -> BTreeMap<&'a str, &'a oxc_ast::ast::TSType<'a>> {
  let mut out = BTreeMap::new();
  let Some(type_params) = type_params else {
    return out;
  };
  for param in &type_params.params {
    if let Some(constraint) = param.constraint.as_ref() {
      out.insert(param.name.name.as_str(), constraint);
    }
  }
  out
}

fn options_callback_props_from_ts_type<'a>(
  semantic: &'a Semantic<'a>,
  ts_type: &'a oxc_ast::ast::TSType<'a>,
  depth: u8,
  index: &mut Option<TypeDeclIndex<'a>>,
  visiting: &mut BTreeSet<&'a str>,
  type_param_constraints: &BTreeMap<&'a str, &'a oxc_ast::ast::TSType<'a>>,
) -> BTreeMap<String, ComposableShape> {
  use oxc_ast::ast::{TSType, TSTypeName, TSTypeOperatorOperator};
  if depth > 4 {
    return BTreeMap::new();
  }
  match ts_type {
    TSType::TSParenthesizedType(paren) => options_callback_props_from_ts_type(
      semantic,
      &paren.type_annotation,
      depth,
      index,
      visiting,
      type_param_constraints,
    ),
    TSType::TSTypeOperatorType(operator)
      if operator.operator == TSTypeOperatorOperator::Readonly =>
    {
      options_callback_props_from_ts_type(
        semantic,
        &operator.type_annotation,
        depth,
        index,
        visiting,
        type_param_constraints,
      )
    }
    TSType::TSIntersectionType(intersection) => {
      let mut merged = BTreeMap::new();
      for part in &intersection.types {
        for (name, shape) in options_callback_props_from_ts_type(
          semantic,
          part,
          depth.saturating_add(1),
          index,
          visiting,
          type_param_constraints,
        ) {
          merged.entry(name).or_insert(shape);
        }
      }
      merged
    }
    TSType::TSTypeLiteral(literal) => options_callback_props_from_signatures(
      semantic,
      &literal.members,
      depth,
      index,
      type_param_constraints,
    ),
    TSType::TSTypeReference(reference) => {
      let name = match &reference.type_name {
        TSTypeName::IdentifierReference(identifier) => identifier.name.as_str(),
        TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => return BTreeMap::new(),
      };
      if !visiting.insert(name) {
        return BTreeMap::new();
      }
      let resolved = {
        let decls = index.get_or_insert_with(|| TypeDeclIndex::build(semantic));
        if let Some(interface) = decls.interfaces.get(name).cloned() {
          let props = options_callback_props_from_interface(
            semantic,
            &interface,
            depth.saturating_add(1),
            index,
            visiting,
            type_param_constraints,
          );
          visiting.remove(name);
          return props;
        }
        decls.aliases.get(name).copied().or_else(|| type_param_constraints.get(name).copied())
      };
      let props = resolved.map_or_else(BTreeMap::new, |alias| {
        options_callback_props_from_ts_type(
          semantic,
          alias,
          depth.saturating_add(1),
          index,
          visiting,
          type_param_constraints,
        )
      });
      visiting.remove(name);
      props
    }
    _ => BTreeMap::new(),
  }
}

fn options_callback_props_from_interface<'a>(
  semantic: &'a Semantic<'a>,
  interface: &InterfaceDecl<'a>,
  depth: u8,
  index: &mut Option<TypeDeclIndex<'a>>,
  visiting: &mut BTreeSet<&'a str>,
  type_param_constraints: &BTreeMap<&'a str, &'a oxc_ast::ast::TSType<'a>>,
) -> BTreeMap<String, ComposableShape> {
  if depth > 4 {
    return BTreeMap::new();
  }
  let mut merged = options_callback_props_from_signatures(
    semantic,
    interface.members,
    depth,
    index,
    type_param_constraints,
  );
  for base in &interface.extends {
    if !visiting.insert(*base) {
      continue;
    }
    let base_interface = {
      let decls = index.get_or_insert_with(|| TypeDeclIndex::build(semantic));
      decls.interfaces.get(*base).cloned()
    };
    let base_props = if let Some(base_interface) = base_interface {
      options_callback_props_from_interface(
        semantic,
        &base_interface,
        depth.saturating_add(1),
        index,
        visiting,
        type_param_constraints,
      )
    } else {
      let alias = {
        let decls = index.get_or_insert_with(|| TypeDeclIndex::build(semantic));
        decls.aliases.get(*base).copied().or_else(|| type_param_constraints.get(*base).copied())
      };
      alias.map_or_else(BTreeMap::new, |alias| {
        options_callback_props_from_ts_type(
          semantic,
          alias,
          depth.saturating_add(1),
          index,
          visiting,
          type_param_constraints,
        )
      })
    };
    visiting.remove(*base);
    for (name, shape) in base_props {
      merged.entry(name).or_insert(shape);
    }
  }
  merged
}

fn options_callback_props_from_signatures<'a>(
  semantic: &'a Semantic<'a>,
  members: &'a [oxc_ast::ast::TSSignature<'a>],
  depth: u8,
  index: &mut Option<TypeDeclIndex<'a>>,
  type_param_constraints: &BTreeMap<&'a str, &'a oxc_ast::ast::TSType<'a>>,
) -> BTreeMap<String, ComposableShape> {
  use oxc_ast::ast::TSSignature;
  let mut out = BTreeMap::new();
  for member in members {
    let TSSignature::TSPropertySignature(property) = member else {
      continue;
    };
    let Some(exported) = property.key.static_name() else {
      continue;
    };
    let Some(annotation) = &property.type_annotation else {
      continue;
    };
    // `setup?: Setup` where `Setup extends StdFormGlobalSetupFn<…>` — peel constraint.
    let prop_type =
      resolve_type_param_constraint(&annotation.type_annotation, type_param_constraints);
    let Some(callback_params) =
      function_type_formal_params_resolved(semantic, prop_type, 0, index, type_param_constraints)
    else {
      continue;
    };
    let Some(first) = callback_params.items.first() else {
      continue;
    };
    let Some(first_annotation) = first.type_annotation.as_ref() else {
      continue;
    };
    let shape = composable_shape_with_extends(
      semantic,
      &first_annotation.type_annotation,
      depth.saturating_add(1),
      index,
      &mut BTreeSet::new(),
      type_param_constraints,
    );
    if !shape.is_empty() {
      out.insert(exported.into_owned(), shape);
    }
  }
  out
}

fn resolve_type_param_constraint<'a>(
  ts_type: &'a oxc_ast::ast::TSType<'a>,
  type_param_constraints: &BTreeMap<&'a str, &'a oxc_ast::ast::TSType<'a>>,
) -> &'a oxc_ast::ast::TSType<'a> {
  use oxc_ast::ast::{TSType, TSTypeName};
  let mut current = ts_type;
  for _ in 0..4 {
    current = match current {
      TSType::TSParenthesizedType(paren) => &paren.type_annotation,
      TSType::TSTypeReference(reference) => {
        let name = match &reference.type_name {
          TSTypeName::IdentifierReference(identifier) => identifier.name.as_str(),
          TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => return current,
        };
        match type_param_constraints.get(name).copied() {
          Some(constraint) => constraint,
          None => return current,
        }
      }
      _ => return current,
    };
  }
  current
}

/// Bag shape that merges simple interface `extends` (visited + depth-bounded).
fn composable_shape_with_extends<'a>(
  semantic: &'a Semantic<'a>,
  ts_type: &'a oxc_ast::ast::TSType<'a>,
  depth: u8,
  index: &mut Option<TypeDeclIndex<'a>>,
  visiting: &mut BTreeSet<&'a str>,
  type_param_constraints: &BTreeMap<&'a str, &'a oxc_ast::ast::TSType<'a>>,
) -> ComposableShape {
  use oxc_ast::ast::{TSType, TSTypeName, TSTypeOperatorOperator};
  if depth > 4 {
    return ComposableShape::default();
  }
  let ts_type = resolve_type_param_constraint(ts_type, type_param_constraints);
  if super::ts_type_reactive_kind(ts_type).is_some() {
    return ComposableShape::default();
  }
  match ts_type {
    TSType::TSParenthesizedType(paren) => composable_shape_with_extends(
      semantic,
      &paren.type_annotation,
      depth,
      index,
      visiting,
      type_param_constraints,
    ),
    TSType::TSTypeOperatorType(operator)
      if operator.operator == TSTypeOperatorOperator::Readonly =>
    {
      composable_shape_with_extends(
        semantic,
        &operator.type_annotation,
        depth,
        index,
        visiting,
        type_param_constraints,
      )
    }
    TSType::TSIntersectionType(intersection) => {
      let mut merged = ComposableShape::default();
      for part in &intersection.types {
        let part_shape = composable_shape_with_extends(
          semantic,
          part,
          depth.saturating_add(1),
          index,
          visiting,
          type_param_constraints,
        );
        merged.open_reactive_spread =
          merged.open_reactive_spread || part_shape.open_reactive_spread;
        for (field, kind) in part_shape.fields {
          merged.fields.entry(field).or_insert(kind);
        }
      }
      merged
    }
    TSType::TSTypeLiteral(literal) => {
      ComposableShape::from_fields(shape_from_ts_signatures(&literal.members))
    }
    TSType::TSTypeReference(reference) => {
      let name = match &reference.type_name {
        TSTypeName::IdentifierReference(identifier) => identifier.name.as_str(),
        TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => {
          return ComposableShape::default();
        }
      };
      if !visiting.insert(name) {
        return ComposableShape::default();
      }
      let resolved = {
        let decls = index.get_or_insert_with(|| TypeDeclIndex::build(semantic));
        if let Some(interface) = decls.interfaces.get(name).cloned() {
          let shape = interface_shape_with_extends(
            semantic,
            &interface,
            depth.saturating_add(1),
            index,
            visiting,
            type_param_constraints,
          );
          visiting.remove(name);
          return shape;
        }
        decls.aliases.get(name).copied().or_else(|| type_param_constraints.get(name).copied())
      };
      let shape = resolved.map_or_else(ComposableShape::default, |alias| {
        composable_shape_with_extends(
          semantic,
          alias,
          depth.saturating_add(1),
          index,
          visiting,
          type_param_constraints,
        )
      });
      visiting.remove(name);
      shape
    }
    _ => ComposableShape::default(),
  }
}

fn interface_shape_with_extends<'a>(
  semantic: &'a Semantic<'a>,
  interface: &InterfaceDecl<'a>,
  depth: u8,
  index: &mut Option<TypeDeclIndex<'a>>,
  visiting: &mut BTreeSet<&'a str>,
  type_param_constraints: &BTreeMap<&'a str, &'a oxc_ast::ast::TSType<'a>>,
) -> ComposableShape {
  let mut merged = ComposableShape::from_fields(shape_from_ts_signatures(interface.members));
  if depth > 4 {
    return merged;
  }
  for base in &interface.extends {
    if !visiting.insert(*base) {
      continue;
    }
    let base_interface = {
      let decls = index.get_or_insert_with(|| TypeDeclIndex::build(semantic));
      decls.interfaces.get(*base).cloned()
    };
    let base_shape = if let Some(base_interface) = base_interface {
      interface_shape_with_extends(
        semantic,
        &base_interface,
        depth.saturating_add(1),
        index,
        visiting,
        type_param_constraints,
      )
    } else {
      let alias = {
        let decls = index.get_or_insert_with(|| TypeDeclIndex::build(semantic));
        decls.aliases.get(*base).copied().or_else(|| type_param_constraints.get(*base).copied())
      };
      alias.map_or_else(ComposableShape::default, |alias| {
        composable_shape_with_extends(
          semantic,
          alias,
          depth.saturating_add(1),
          index,
          visiting,
          type_param_constraints,
        )
      })
    };
    visiting.remove(*base);
    merged.open_reactive_spread = merged.open_reactive_spread || base_shape.open_reactive_spread;
    for (field, kind) in base_shape.fields {
      merged.fields.entry(field).or_insert(kind);
    }
  }
  merged
}

fn shape_from_ts_signatures(
  members: &[oxc_ast::ast::TSSignature<'_>],
) -> BTreeMap<String, ReactiveBindingKind> {
  use oxc_ast::ast::TSSignature;
  let mut shape = BTreeMap::new();
  for member in members {
    let TSSignature::TSPropertySignature(property) = member else {
      continue;
    };
    let Some(exported) = property.key.static_name() else {
      continue;
    };
    let Some(annotation) = &property.type_annotation else {
      continue;
    };
    let Some(kind) = super::ts_type_reactive_kind(&annotation.type_annotation) else {
      continue;
    };
    shape.insert(exported.into_owned(), kind);
  }
  shape
}

fn function_type_formal_params<'a>(
  ts_type: &'a oxc_ast::ast::TSType<'a>,
) -> Option<&'a oxc_ast::ast::FormalParameters<'a>> {
  use oxc_ast::ast::TSType;
  match ts_type {
    TSType::TSParenthesizedType(paren) => function_type_formal_params(&paren.type_annotation),
    TSType::TSFunctionType(function_type) => Some(&function_type.params),
    _ => None,
  }
}

fn function_type_formal_params_resolved<'a>(
  semantic: &'a Semantic<'a>,
  ts_type: &'a oxc_ast::ast::TSType<'a>,
  depth: u8,
  index: &mut Option<TypeDeclIndex<'a>>,
  type_param_constraints: &BTreeMap<&'a str, &'a oxc_ast::ast::TSType<'a>>,
) -> Option<&'a oxc_ast::ast::FormalParameters<'a>> {
  use oxc_ast::ast::{TSType, TSTypeName};
  if depth > 4 {
    return None;
  }
  let ts_type = resolve_type_param_constraint(ts_type, type_param_constraints);
  match ts_type {
    TSType::TSParenthesizedType(paren) => function_type_formal_params_resolved(
      semantic,
      &paren.type_annotation,
      depth,
      index,
      type_param_constraints,
    ),
    TSType::TSFunctionType(function_type) => Some(&function_type.params),
    TSType::TSTypeReference(reference) => {
      let name = match &reference.type_name {
        TSTypeName::IdentifierReference(identifier) => identifier.name.as_str(),
        TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => return None,
      };
      let alias = {
        let decls = index.get_or_insert_with(|| TypeDeclIndex::build(semantic));
        decls.aliases.get(name).copied().or_else(|| type_param_constraints.get(name).copied())
      };
      function_type_formal_params_resolved(
        semantic,
        alias?,
        depth.saturating_add(1),
        index,
        type_param_constraints,
      )
    }
    _ => None,
  }
}

fn callback_params_from_expression<'a>(
  expression: &'a Expression<'a>,
) -> Option<&'a [oxc_ast::ast::FormalParameter<'a>]> {
  match peel_parens(expression) {
    Expression::ArrowFunctionExpression(arrow) => Some(arrow.params.items.as_slice()),
    Expression::FunctionExpression(function) => Some(function.params.items.as_slice()),
    _ => None,
  }
}

fn push_options_callback_pattern_bindings(
  callback_params: &[oxc_ast::ast::FormalParameter<'_>],
  shape: &ComposableShape,
  span_source: &str,
  span_base: usize,
  into: &mut Vec<ReactiveBindingFact>,
) {
  let Some(parameter) = callback_params.first() else {
    return;
  };
  if parameter.type_annotation.is_some() {
    return;
  }
  let BindingPattern::ObjectPattern(pattern) = &parameter.pattern else {
    return;
  };
  for property in &pattern.properties {
    let Some(key) = property.key.static_name() else {
      continue;
    };
    let BindingPattern::BindingIdentifier(identifier) = &property.value else {
      continue;
    };
    let Some(kind) = shape.kind_for_destructure(key.as_ref()) else {
      continue;
    };
    let span = super::super::kinds::source_span(span_source, span_base, identifier.span);
    let name = identifier.name.to_string();
    if into.iter().any(|binding| binding.name == name && binding.span.offset == span.offset) {
      continue;
    }
    into.push(ReactiveBindingFact {
      name,
      kind,
      initialized_with_null: false,
      alias_of: None,
      span,
    });
  }
}
