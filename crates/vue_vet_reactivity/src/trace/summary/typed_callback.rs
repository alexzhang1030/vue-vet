//! Typed function-callback parameters: `useX(init, (state: ComputedRef<T>) => …)`.
//!
//! When a callee declares an argument as a function type whose formals are
//! Ref-like (`Ref` / `ComputedRef` / …), call-site arrow/function arguments seed
//! those formals so nested `computed(() => state.value)` classifies. No callee-name
//! allowlist — slots come only from TypeScript parameter types.

use std::collections::BTreeMap;

use oxc_ast::{
  AstKind,
  ast::{BindingPattern, Expression},
};
use oxc_semantic::Semantic;
use vue_vet_core::{ReactiveBindingFact, ReactiveBindingKind};

use super::super::expr::{is_nested_in_function, peel_parens};
use super::ts_type_reactive_kind;

/// Call-argument index → (callback formal index → reactive kind).
///
/// Example: `function useX(init: T, run: (state: ComputedRef<U>) => R)` publishes
/// `{ 1: { 0: Computed } }`.
pub type TypedCallbackParamSlots = BTreeMap<u32, BTreeMap<u32, ReactiveBindingKind>>;

/// Named locals whose parameters include typed Ref-like function callbacks.
pub fn collect_local_typed_callback_param_slots(
  semantic: &Semantic<'_>,
) -> BTreeMap<String, TypedCallbackParamSlots> {
  let mut out = BTreeMap::new();
  for node in semantic.nodes() {
    match node.kind() {
      AstKind::Function(function) => {
        let Some(identifier) = &function.id else {
          continue;
        };
        if let Some(slots) = slots_from_formal_params(semantic, &function.params) {
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
            if let Some(slots) = slots_from_formal_params(semantic, &arrow.params) {
              out.insert(name, slots);
            }
          }
          Some(Expression::FunctionExpression(function)) => {
            if let Some(slots) = slots_from_formal_params(semantic, &function.params) {
              out.insert(name, slots);
            }
          }
          None => {
            if let Some(annotation) = declarator.type_annotation.as_ref()
              && let Some(slots) =
                slots_from_ts_function_type(semantic, &annotation.type_annotation, 0)
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

/// Seed `callee(…, (state) => …)` formals from declared typed-callback slots.
pub fn seed_typed_callback_params_at_calls(
  semantic: &Semantic<'_>,
  slots_by_callee: &BTreeMap<String, TypedCallbackParamSlots>,
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
    for (arg_index, formal_kinds) in arg_slots {
      let Some(argument) = call.arguments.get(usize::try_from(*arg_index).unwrap_or(usize::MAX))
      else {
        continue;
      };
      let Some(expression) = argument.as_expression() else {
        continue;
      };
      let Some(callback_params) = callback_formals_from_expression(expression) else {
        continue;
      };
      for (formal_index, kind) in formal_kinds {
        let Some(parameter) =
          callback_params.get(usize::try_from(*formal_index).unwrap_or(usize::MAX))
        else {
          continue;
        };
        // Explicit annotations on the callback win via `collect_typed_reactive_bindings`.
        if parameter.type_annotation.is_some() {
          continue;
        }
        let BindingPattern::BindingIdentifier(identifier) = &parameter.pattern else {
          continue;
        };
        let span = super::super::source_span(span_source, span_base, identifier.span);
        let name = identifier.name.to_string();
        if into.iter().any(|binding| binding.name == name && binding.span.offset == span.offset) {
          continue;
        }
        into.push(ReactiveBindingFact { name, kind: *kind, initialized_with_null: false, span });
      }
    }
  }
}

fn slots_from_formal_params(
  semantic: &Semantic<'_>,
  params: &oxc_ast::ast::FormalParameters<'_>,
) -> Option<TypedCallbackParamSlots> {
  let mut slots = TypedCallbackParamSlots::new();
  for (arg_index, parameter) in params.items.iter().enumerate() {
    let Some(annotation) = parameter.type_annotation.as_ref() else {
      continue;
    };
    let Some(formal_kinds) =
      reactive_kinds_from_function_type(semantic, &annotation.type_annotation, 0)
    else {
      continue;
    };
    if !formal_kinds.is_empty() {
      slots.insert(u32::try_from(arg_index).unwrap_or(u32::MAX), formal_kinds);
    }
  }
  (!slots.is_empty()).then_some(slots)
}

/// `const useX: (init: T, run: (state: ComputedRef<U>) => R) => void`
fn slots_from_ts_function_type<'a>(
  semantic: &'a Semantic<'a>,
  ts_type: &'a oxc_ast::ast::TSType<'a>,
  depth: u8,
) -> Option<TypedCallbackParamSlots> {
  let params = function_type_params(semantic, ts_type, depth)?;
  let mut slots = TypedCallbackParamSlots::new();
  for (arg_index, parameter) in params.items.iter().enumerate() {
    let Some(annotation) = parameter.type_annotation.as_ref() else {
      continue;
    };
    let Some(formal_kinds) = reactive_kinds_from_function_type(
      semantic,
      &annotation.type_annotation,
      depth.saturating_add(1),
    ) else {
      continue;
    };
    if !formal_kinds.is_empty() {
      slots.insert(u32::try_from(arg_index).unwrap_or(u32::MAX), formal_kinds);
    }
  }
  (!slots.is_empty()).then_some(slots)
}

fn reactive_kinds_from_function_type<'a>(
  semantic: &'a Semantic<'a>,
  ts_type: &'a oxc_ast::ast::TSType<'a>,
  depth: u8,
) -> Option<BTreeMap<u32, ReactiveBindingKind>> {
  let params = function_type_params(semantic, ts_type, depth)?;
  let mut kinds = BTreeMap::new();
  for (formal_index, parameter) in params.items.iter().enumerate() {
    let Some(annotation) = parameter.type_annotation.as_ref() else {
      continue;
    };
    let Some(kind) = ts_type_reactive_kind(&annotation.type_annotation) else {
      continue;
    };
    kinds.insert(u32::try_from(formal_index).unwrap_or(u32::MAX), kind);
  }
  (!kinds.is_empty()).then_some(kinds)
}

fn function_type_params<'a>(
  semantic: &'a Semantic<'a>,
  ts_type: &'a oxc_ast::ast::TSType<'a>,
  depth: u8,
) -> Option<&'a oxc_ast::ast::FormalParameters<'a>> {
  use oxc_ast::ast::{TSType, TSTypeName};
  if depth > 4 {
    return None;
  }
  match ts_type {
    TSType::TSParenthesizedType(paren) => {
      function_type_params(semantic, &paren.type_annotation, depth)
    }
    TSType::TSFunctionType(function_type) => Some(&function_type.params),
    TSType::TSTypeReference(reference) => {
      let name = match &reference.type_name {
        TSTypeName::IdentifierReference(identifier) => identifier.name.as_str(),
        TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => return None,
      };
      let alias = type_alias_annotation(semantic, name)?;
      function_type_params(semantic, alias, depth.saturating_add(1))
    }
    _ => None,
  }
}

fn type_alias_annotation<'a>(
  semantic: &'a Semantic<'a>,
  name: &str,
) -> Option<&'a oxc_ast::ast::TSType<'a>> {
  for node in semantic.nodes() {
    if let AstKind::TSTypeAliasDeclaration(alias) = node.kind()
      && alias.id.name.as_str() == name
    {
      return Some(&alias.type_annotation);
    }
  }
  None
}

fn callback_formals_from_expression<'a>(
  expression: &'a Expression<'a>,
) -> Option<&'a [oxc_ast::ast::FormalParameter<'a>]> {
  match peel_parens(expression) {
    Expression::ArrowFunctionExpression(arrow) => Some(arrow.params.items.as_slice()),
    Expression::FunctionExpression(function) => Some(function.params.items.as_slice()),
    _ => None,
  }
}
