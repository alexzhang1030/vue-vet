//! TypeScript type surface → reactive kinds and composable bag shapes (under-approx).
//!
//! Used for declared returns (`.d.ts` / annotated source), typed parameters and
//! declarators, `inject(key) as Ctx` defaults, and `return x as Ctx` assertions.

use std::collections::BTreeMap;

use oxc_ast::{AstKind, ast::Expression};
use oxc_semantic::NodeId;
use vue_vet_core::ReactiveBindingKind;

use super::model::{ComposableShape, DeclaredReturn, ExportState};

/// Declared TypeScript return type on a function (`.d.ts` / annotated source).
#[must_use]
pub fn function_return_type_kind(
  function: &oxc_ast::ast::Function<'_>,
) -> Option<ReactiveBindingKind> {
  function
    .return_type
    .as_ref()
    .and_then(|annotation| ts_type_reactive_kind(&annotation.type_annotation))
}

/// Declared object-bag return shape on a function (`.d.ts` / annotated source).
///
/// Kept out of line so the `const x = ref(0)` module-export cold path does not
/// pay for TypeScript shape machinery in instruction cache.
#[must_use]
#[inline(never)]
pub fn function_return_type_shape(
  semantic: &oxc_semantic::Semantic<'_>,
  function: &oxc_ast::ast::Function<'_>,
) -> ComposableShape {
  let Some(annotation) = function.return_type.as_ref() else {
    return ComposableShape::default();
  };
  let mut index = None;
  ts_type_composable_shape(semantic, &annotation.type_annotation, 0, &mut index)
}

/// Declared TypeScript return type on an arrow function.
#[must_use]
pub fn arrow_return_type_kind(
  arrow: &oxc_ast::ast::ArrowFunctionExpression<'_>,
) -> Option<ReactiveBindingKind> {
  arrow
    .return_type
    .as_ref()
    .and_then(|annotation| ts_type_reactive_kind(&annotation.type_annotation))
}

/// Declared object-bag return shape on an arrow function.
#[must_use]
#[inline(never)]
pub fn arrow_return_type_shape(
  semantic: &oxc_semantic::Semantic<'_>,
  arrow: &oxc_ast::ast::ArrowFunctionExpression<'_>,
) -> ComposableShape {
  let Some(annotation) = arrow.return_type.as_ref() else {
    return ComposableShape::default();
  };
  let mut index = None;
  ts_type_composable_shape(semantic, &annotation.type_annotation, 0, &mut index)
}

pub(super) fn declared_return_for_function(
  semantic: &oxc_semantic::Semantic<'_>,
  function: &oxc_ast::ast::Function<'_>,
) -> Option<DeclaredReturn> {
  if let Some(kind) = function_return_type_kind(function) {
    return Some(DeclaredReturn::Factory(kind));
  }
  let annotation = function.return_type.as_ref()?;
  classify_declared_return_type(semantic, &annotation.type_annotation)
}

pub(super) fn declared_return_for_arrow(
  semantic: &oxc_semantic::Semantic<'_>,
  arrow: &oxc_ast::ast::ArrowFunctionExpression<'_>,
) -> Option<DeclaredReturn> {
  if let Some(kind) = arrow_return_type_kind(arrow) {
    return Some(DeclaredReturn::Factory(kind));
  }
  let annotation = arrow.return_type.as_ref()?;
  classify_declared_return_type(semantic, &annotation.type_annotation)
}

/// `export declare const useX: () => T` — function type on the declarator.
#[inline(never)]
pub(super) fn declared_return_from_declarator_annotation(
  semantic: &oxc_semantic::Semantic<'_>,
  declarator: &oxc_ast::ast::VariableDeclarator<'_>,
) -> Option<DeclaredReturn> {
  use oxc_ast::ast::TSType;
  let annotation = declarator.type_annotation.as_ref()?;
  let ts_type = match &annotation.type_annotation {
    TSType::TSParenthesizedType(paren) => &paren.type_annotation,
    other => other,
  };
  let TSType::TSFunctionType(function_type) = ts_type else {
    return None;
  };
  classify_declared_return_type(semantic, &function_type.return_type.type_annotation)
}

/// `export declare const useX: typeof useY` — forward the named binding at link time.
///
/// Name-agnostic: only the `typeof` identifier matters (packages re-export
/// `typeof` aliases without repeating the return shape).
pub(super) fn typeof_forward_from_declarator(
  declarator: &oxc_ast::ast::VariableDeclarator<'_>,
) -> Option<ExportState> {
  use oxc_ast::ast::{TSType, TSTypeQueryExprName};
  let annotation = declarator.type_annotation.as_ref()?;
  let ts_type = match &annotation.type_annotation {
    TSType::TSParenthesizedType(paren) => &paren.type_annotation,
    other => other,
  };
  let TSType::TSTypeQuery(query) = ts_type else {
    return None;
  };
  let name = match &query.expr_name {
    TSTypeQueryExprName::IdentifierReference(identifier) => identifier.name.as_str(),
    TSTypeQueryExprName::QualifiedName(_)
    | TSTypeQueryExprName::ThisExpression(_)
    | TSTypeQueryExprName::TSImportType(_) => return None,
  };
  if name.is_empty() {
    return None;
  }
  Some(ExportState::ForwardReturn(name.to_owned()))
}

#[inline(never)]
fn classify_declared_return_type(
  semantic: &oxc_semantic::Semantic<'_>,
  ts_type: &oxc_ast::ast::TSType<'_>,
) -> Option<DeclaredReturn> {
  if let Some(kind) = ts_type_reactive_kind(ts_type) {
    return Some(DeclaredReturn::Factory(kind));
  }
  let mut index = None;
  let shape = ts_type_composable_shape(semantic, ts_type, 0, &mut index);
  if !shape.is_empty() {
    return Some(DeclaredReturn::Composable(shape));
  }
  if ts_type_is_plain_object_shaped(semantic, ts_type, 0, &mut index) {
    return Some(DeclaredReturn::PlainObject);
  }
  None
}

/// Object-shaped type: ≥1 property and no Ref-like field types (under-approx).
#[inline(never)]
fn ts_type_is_plain_object_shaped<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  ts_type: &'a oxc_ast::ast::TSType<'a>,
  depth: u8,
  index: &mut Option<TypeDeclIndex<'a>>,
) -> bool {
  use oxc_ast::ast::{TSType, TSTypeName, TSTypeOperatorOperator};
  if depth > 4 {
    return false;
  }
  if ts_type_reactive_kind(ts_type).is_some() {
    return false;
  }
  match ts_type {
    TSType::TSParenthesizedType(paren) => {
      ts_type_is_plain_object_shaped(semantic, &paren.type_annotation, depth, index)
    }
    TSType::TSTypeOperatorType(operator)
      if operator.operator == TSTypeOperatorOperator::Readonly =>
    {
      ts_type_is_plain_object_shaped(semantic, &operator.type_annotation, depth, index)
    }
    TSType::TSTypeLiteral(literal) => signatures_are_plain_object_shaped(&literal.members),
    TSType::TSTypeReference(reference) => {
      let Some(name) = (match &reference.type_name {
        TSTypeName::IdentifierReference(identifier) => Some(identifier.name.as_str()),
        TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => None,
      }) else {
        return false;
      };
      let alias = {
        let decls = index.get_or_insert_with(|| TypeDeclIndex::build(semantic));
        if let Some(members) = decls.interfaces.get(name).copied() {
          return signatures_are_plain_object_shaped(members);
        }
        decls.aliases.get(name).copied()
      };
      let Some(alias) = alias else {
        return false;
      };
      ts_type_is_plain_object_shaped(semantic, alias, depth.saturating_add(1), index)
    }
    _ => false,
  }
}

fn signatures_are_plain_object_shaped(members: &[oxc_ast::ast::TSSignature<'_>]) -> bool {
  use oxc_ast::ast::TSSignature;
  let mut property_count = 0_usize;
  for member in members {
    let TSSignature::TSPropertySignature(property) = member else {
      continue;
    };
    property_count = property_count.saturating_add(1);
    let Some(annotation) = &property.type_annotation else {
      continue;
    };
    if ts_type_reactive_kind(&annotation.type_annotation).is_some() {
      return false;
    }
  }
  property_count > 0
}

/// Map a TypeScript type surface to a reactive binding kind (under-approx).
///
/// Recognizes Vue ref-like type names (`Ref`, `ComputedRef`, …) and a narrow
/// structural duck: a type literal whose **only** member is optional `value?`
/// (test/mock `Ref` stand-ins). Required `{ value: T }` stays quiet so plain
/// option shapes and `{ value: boolean }` factory returns are not invented.
/// Used for declared returns and for seeding typed parameters / declarators.
pub fn ts_type_reactive_kind(ts_type: &oxc_ast::ast::TSType<'_>) -> Option<ReactiveBindingKind> {
  use oxc_ast::ast::{TSType, TSTypeName, TSTypeOperatorOperator};
  match ts_type {
    TSType::TSParenthesizedType(paren) => ts_type_reactive_kind(&paren.type_annotation),
    TSType::TSTypeOperatorType(operator)
      if operator.operator == TSTypeOperatorOperator::Readonly =>
    {
      ts_type_reactive_kind(&operator.type_annotation).map(|kind| match kind {
        ReactiveBindingKind::Ref => ReactiveBindingKind::Readonly,
        ReactiveBindingKind::ShallowRef => ReactiveBindingKind::ShallowReadonly,
        other => other,
      })
    }
    TSType::TSTypeLiteral(literal) => optional_sole_value_ref_kind(&literal.members),
    TSType::TSTypeReference(reference) => {
      let name = match &reference.type_name {
        TSTypeName::IdentifierReference(identifier) => identifier.name.as_str(),
        // `vue.Ref` / `import('vue').ShallowRef` rightmost name (qualified only).
        TSTypeName::QualifiedName(qualified) => qualified.right.name.as_str(),
        TSTypeName::ThisExpression(_) => return None,
      };
      match name {
        // VueUse `RemovableRef<T> = Ref<T, …>` — storage helpers (`useLocalStorage`).
        "Ref" | "RemovableRef" => Some(ReactiveBindingKind::Ref),
        "ShallowRef" => Some(ReactiveBindingKind::ShallowRef),
        "ComputedRef" | "WritableComputedRef" => Some(ReactiveBindingKind::Computed),
        "CustomRef" => Some(ReactiveBindingKind::CustomRef),
        "ToRef" => Some(ReactiveBindingKind::ToRef),
        "Readonly" => {
          // `Readonly<Ref<T>>` — peel one type argument when present.
          let arg = reference.type_arguments.as_ref()?.params.first()?;
          ts_type_reactive_kind(arg).map(|kind| match kind {
            ReactiveBindingKind::Ref => ReactiveBindingKind::Readonly,
            ReactiveBindingKind::ShallowRef => ReactiveBindingKind::ShallowReadonly,
            other => other,
          })
        }
        _ => None,
      }
    }
    _ => None,
  }
}

/// `{ value?: T }` — sole optional `value` property, no methods/index signatures.
fn optional_sole_value_ref_kind(
  members: &[oxc_ast::ast::TSSignature<'_>],
) -> Option<ReactiveBindingKind> {
  use oxc_ast::ast::TSSignature;
  let mut saw_optional_value = false;
  for member in members {
    let TSSignature::TSPropertySignature(property) = member else {
      return None;
    };
    let name = property.key.static_name()?;
    if name.as_ref() != "value" || !property.optional || saw_optional_value {
      return None;
    }
    saw_optional_value = true;
  }
  saw_optional_value.then_some(ReactiveBindingKind::Ref)
}

/// Same-file `interface` / `type` declarations, built once per shape query.
struct TypeDeclIndex<'a> {
  interfaces: BTreeMap<&'a str, &'a [oxc_ast::ast::TSSignature<'a>]>,
  aliases: BTreeMap<&'a str, &'a oxc_ast::ast::TSType<'a>>,
}

impl<'a> TypeDeclIndex<'a> {
  fn build(semantic: &'a oxc_semantic::Semantic<'a>) -> Self {
    let mut interfaces = BTreeMap::new();
    let mut aliases = BTreeMap::new();
    for node in semantic.nodes() {
      match node.kind() {
        AstKind::TSInterfaceDeclaration(interface) => {
          interfaces.insert(interface.id.name.as_str(), interface.body.body.as_slice());
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

/// Object-bag shape from a TypeScript type surface (under-approx).
///
/// Same recognition as declared composable return types — used for
/// `inject(key) as Ctx` defaults and `return x` when `x` was asserted to a
/// Ref-field interface.
pub fn composable_shape_from_ts_type<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  ts_type: &'a oxc_ast::ast::TSType<'a>,
) -> ComposableShape {
  let mut index = None;
  ts_type_composable_shape(semantic, ts_type, 0, &mut index)
}

/// Object-bag shape from a TypeScript return type (under-approx).
///
/// Recognizes inline `{ width: Ref<number> }`, same-file `interface` / `type`
/// aliases, mapped types whose values peel to Ref (`open_reactive_spread`),
/// intersections, and a single `readonly` operator. Non-reactive fields
/// (`stop: () => void`) stay out of the shape. Depth-bounded alias follow.
fn ts_type_composable_shape<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  ts_type: &'a oxc_ast::ast::TSType<'a>,
  depth: u8,
  index: &mut Option<TypeDeclIndex<'a>>,
) -> ComposableShape {
  use oxc_ast::ast::{TSType, TSTypeName, TSTypeOperatorOperator};
  if depth > 4 {
    return ComposableShape::default();
  }
  // Scalar Ref returns are Factory, not bags.
  if ts_type_reactive_kind(ts_type).is_some() {
    return ComposableShape::default();
  }
  match ts_type {
    TSType::TSParenthesizedType(paren) => {
      ts_type_composable_shape(semantic, &paren.type_annotation, depth, index)
    }
    TSType::TSTypeOperatorType(operator)
      if operator.operator == TSTypeOperatorOperator::Readonly =>
    {
      ts_type_composable_shape(semantic, &operator.type_annotation, depth, index)
    }
    TSType::TSIntersectionType(intersection) => {
      let mut merged = ComposableShape::default();
      for part in &intersection.types {
        let part_shape = ts_type_composable_shape(semantic, part, depth.saturating_add(1), index);
        merged.open_reactive_spread =
          merged.open_reactive_spread || part_shape.open_reactive_spread;
        for (field, kind) in part_shape.fields {
          merged.fields.entry(field).or_insert(kind);
        }
        for (field, pending) in part_shape.pending_value_bag_fields {
          merged.pending_value_bag_fields.entry(field).or_insert(pending);
        }
      }
      merged
    }
    TSType::TSMappedType(mapped) => {
      let Some(annotation) = &mapped.type_annotation else {
        return ComposableShape::default();
      };
      if ts_type_has_ref_branch(annotation) {
        ComposableShape {
          fields: BTreeMap::new(),
          open_reactive_spread: true,
          pending_value_bag_fields: BTreeMap::new(),
        }
      } else {
        ComposableShape::default()
      }
    }
    TSType::TSTypeLiteral(literal) => {
      ComposableShape::from_fields(shape_from_ts_signatures(&literal.members))
    }
    TSType::TSTypeReference(reference) => {
      let Some(name) = (match &reference.type_name {
        TSTypeName::IdentifierReference(identifier) => Some(identifier.name.as_str()),
        TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => None,
      }) else {
        return ComposableShape::default();
      };
      // Resolve through a one-shot index; drop borrows before recursing into aliases.
      let alias = {
        let decls = index.get_or_insert_with(|| TypeDeclIndex::build(semantic));
        if let Some(members) = decls.interfaces.get(name).copied() {
          return ComposableShape::from_fields(shape_from_ts_signatures(members));
        }
        decls.aliases.get(name).copied()
      };
      let Some(alias) = alias else {
        return ComposableShape::default();
      };
      ts_type_composable_shape(semantic, alias, depth.saturating_add(1), index)
    }
    _ => ComposableShape::default(),
  }
}

/// Whether a type (or conditional branch) peels to a Vue Ref-like type.
fn ts_type_has_ref_branch(ts_type: &oxc_ast::ast::TSType<'_>) -> bool {
  use oxc_ast::ast::TSType;
  if ts_type_reactive_kind(ts_type).is_some() {
    return true;
  }
  match ts_type {
    TSType::TSParenthesizedType(paren) => ts_type_has_ref_branch(&paren.type_annotation),
    TSType::TSConditionalType(conditional) => {
      ts_type_has_ref_branch(&conditional.true_type)
        || ts_type_has_ref_branch(&conditional.false_type)
    }
    TSType::TSUnionType(union) => union.types.iter().any(ts_type_has_ref_branch),
    TSType::TSIntersectionType(intersection) => {
      intersection.types.iter().any(ts_type_has_ref_branch)
    }
    _ => false,
  }
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
    let Some(kind) = ts_type_reactive_kind(&annotation.type_annotation) else {
      continue;
    };
    shape.insert(exported.into_owned(), kind);
  }
  shape
}

/// Non-empty object-bag shape from `expr as Ctx` / `<Ctx>expr`.
pub(super) fn composable_shape_from_type_assertion<'a>(
  semantic: &'a oxc_semantic::Semantic<'a>,
  expression: &'a Expression<'a>,
) -> Option<ComposableShape> {
  let ts_type = match expression {
    Expression::TSAsExpression(assertion) => &assertion.type_annotation,
    Expression::TSTypeAssertion(assertion) => &assertion.type_annotation,
    _ => return None,
  };
  let shape = composable_shape_from_ts_type(semantic, ts_type);
  (!shape.is_empty()).then_some(shape)
}

/// `return expr as T` when `T` is an enclosing type parameter — index into that list.
pub(super) fn generic_param_index_from_assertion(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  expression: &Expression<'_>,
) -> Option<u8> {
  let ts_type = match expression {
    Expression::TSAsExpression(assertion) => &assertion.type_annotation,
    Expression::TSTypeAssertion(assertion) => &assertion.type_annotation,
    _ => return None,
  };
  let name = match ts_type {
    oxc_ast::ast::TSType::TSTypeReference(reference)
      if reference.type_arguments.is_none()
        && let oxc_ast::ast::TSTypeName::IdentifierReference(identifier) = &reference.type_name =>
    {
      identifier.name.as_str()
    }
    _ => return None,
  };
  enclosing_type_param_index(semantic, function_id, name)
}

fn enclosing_type_param_index(
  semantic: &oxc_semantic::Semantic<'_>,
  function_id: NodeId,
  name: &str,
) -> Option<u8> {
  for ancestor_id in std::iter::once(function_id).chain(semantic.nodes().ancestor_ids(function_id))
  {
    let params = match semantic.nodes().kind(ancestor_id) {
      AstKind::Function(function) => function.type_parameters.as_deref(),
      AstKind::ArrowFunctionExpression(arrow) => arrow.type_parameters.as_deref(),
      _ => None,
    };
    let Some(params) = params else {
      continue;
    };
    if let Some(index) = params.params.iter().position(|param| param.name.name.as_str() == name) {
      return u8::try_from(index).ok();
    }
  }
  None
}
