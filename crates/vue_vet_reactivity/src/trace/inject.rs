//! Provide / inject site collection and unique-key seed resolution.
//!
//! Unique known provide wins; ambiguous keys stay quiet (under-approx).
//! Shared by the single-file tracer and cross-module link.

use std::collections::BTreeMap;

use oxc_ast::{
  AstKind,
  ast::{Argument, BindingPattern, Expression},
};
use oxc_semantic::{NodeId, Semantic};
use oxc_span::Span;
use vue_vet_core::{ReactiveBindingFact, ReactiveBindingKind, ScriptKind};

use super::kinds::{
  reactive_binding_kind, reference_resolves_to_span, resolved_vue_callee, source_span,
};
use super::{
  ComposableShapeMap, InstanceShape, LocalComposableDefs, LocalComposableExport, summary,
};

/// Injection key identity for provide/inject linking (under-approx).
///
/// - [`InjectionKey::String`]: exact string / cooked template key.
/// - [`InjectionKey::Imported`]: named import used as key (`import { ThemeKey } from '…'`).
/// - [`InjectionKey::Local`]: file-local binding (e.g. `const ThemeKey = Symbol()`), keyed by
///   definition span so two `Symbol()` locals never collapse across files.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum InjectionKey {
  String(String),
  Imported { specifier: String, imported: String },
  Local { name: String, def_start: u32 },
}

/// One `provide` site's offered value shape (scalar kind and/or composable bag).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProvideOffer {
  pub kind: Option<ReactiveBindingKind>,
  pub instance_shape: Option<InstanceShape>,
}

impl ProvideOffer {
  const fn is_known(&self) -> bool {
    self.kind.is_some() || self.instance_shape.is_some()
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProvideSite {
  pub key: InjectionKey,
  pub offer: ProvideOffer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InjectSite {
  pub local: String,
  pub key: InjectionKey,
  pub span: Span,
  pub default_kind: Option<ReactiveBindingKind>,
  pub default_instance_shape: Option<InstanceShape>,
}

/// Resolved inject seeds for one file/module.
#[derive(Debug, Default)]
pub struct ResolvedInjectLinks {
  pub bindings: Vec<ReactiveBindingFact>,
  pub instances: ComposableShapeMap,
}

/// `provide(key, value)` and `*.provide(key, value)` with a known-or-unknown value shape.
pub fn collect_provide_sites(
  semantic: &Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  local_composable_defs: &LocalComposableDefs,
  script_kind: ScriptKind,
) -> Vec<ProvideSite> {
  let mut sites = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    if !is_provide_call(semantic, &call.callee, imported_bindings, script_kind) {
      continue;
    }
    let Some(key_expr) = call.arguments.first().and_then(Argument::as_expression) else {
      continue;
    };
    let Some(key) = injection_key(semantic, key_expr, imported_bindings) else {
      continue;
    };
    let offer = call.arguments.get(1).and_then(Argument::as_expression).map_or(
      ProvideOffer { kind: None, instance_shape: None },
      |value| {
        expression_provide_offer(
          semantic,
          value,
          imported_bindings,
          reactive_bindings,
          composable_instances,
          local_composable_defs,
          script_kind,
        )
      },
    );
    sites.push(ProvideSite { key, offer });
  }
  sites
}

/// `const local = inject(key)` / `inject(key, default)` / `inject(key) as Ctx`.
pub fn collect_inject_sites(
  semantic: &Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  reactive_bindings: &[ReactiveBindingFact],
  script_kind: ScriptKind,
) -> Vec<InjectSite> {
  let mut sites = Vec::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = resolved_vue_callee(semantic, &call.callee, imported_bindings, script_kind)
    else {
      continue;
    };
    if callee != "inject" {
      continue;
    }
    let Some((declarator, asserted_type)) =
      inject_declarator_and_assertion(semantic, call.node_id.get())
    else {
      continue;
    };
    let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
      continue;
    };
    let Some(key_expr) = call.arguments.first().and_then(Argument::as_expression) else {
      continue;
    };
    let Some(key) = injection_key(semantic, key_expr, imported_bindings) else {
      continue;
    };
    let (default_kind, mut default_instance_shape) =
      call.arguments.get(1).and_then(Argument::as_expression).map_or((None, None), |value| {
        if matches!(
          value,
          Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
        ) {
          (None, None)
        } else {
          let offer = expression_provide_offer(
            semantic,
            value,
            imported_bindings,
            reactive_bindings,
            &BTreeMap::new(),
            &BTreeMap::new(),
            script_kind,
          );
          (offer.kind, offer.instance_shape)
        }
      });
    // `inject(key) as Ctx` — same-file interface/type with Ref fields seeds the
    // inject local when no unique known provide exists (common for provide helpers
    // that spread a typed parameter into the bag).
    if let Some(ts_type) = asserted_type {
      let shape = summary::composable_shape_from_ts_type(semantic, ts_type);
      if !shape.fields.is_empty() {
        default_instance_shape = Some(shape.fields);
      }
    }
    sites.push(InjectSite {
      local: identifier.name.to_string(),
      key,
      span: identifier.span,
      default_kind,
      default_instance_shape,
    });
  }
  sites
}

/// Walk paren / `as` / type-assertion wrappers from `inject(...)` up to its
/// `VariableDeclarator`, capturing the outermost asserted type when present.
fn inject_declarator_and_assertion<'a>(
  semantic: &'a Semantic<'a>,
  call_id: NodeId,
) -> Option<(&'a oxc_ast::ast::VariableDeclarator<'a>, Option<&'a oxc_ast::ast::TSType<'a>>)> {
  let mut current = call_id;
  let mut asserted_type = None;
  // Bound the peel: real sources only wrap once or twice.
  for _ in 0..8 {
    let parent_id = semantic.nodes().parent_id(current);
    match semantic.nodes().kind(parent_id) {
      AstKind::ParenthesizedExpression(_) => {
        current = parent_id;
      }
      AstKind::TSAsExpression(assertion) => {
        asserted_type = Some(&assertion.type_annotation);
        current = parent_id;
      }
      AstKind::TSTypeAssertion(assertion) => {
        asserted_type = Some(&assertion.type_annotation);
        current = parent_id;
      }
      AstKind::VariableDeclarator(declarator) => {
        return Some((declarator, asserted_type));
      }
      _ => return None,
    }
  }
  None
}

/// Unique known provide → inject binding/bag, else inject default, else quiet.
pub fn resolve_inject_links(
  provides: &[ProvideSite],
  injects: &[InjectSite],
  sfc_source: &str,
  script_offset: usize,
) -> ResolvedInjectLinks {
  let index = provide_offer_index(provides);
  let mut out = ResolvedInjectLinks::default();
  for inject in injects {
    let Some(offer) = resolve_inject_offer(&index, inject) else {
      continue;
    };
    if let Some(kind) = offer.kind
      && !out.bindings.iter().any(|binding| binding.name == inject.local)
    {
      out.bindings.push(ReactiveBindingFact {
        name: inject.local.clone(),
        kind,
        initialized_with_null: false,
        span: source_span(sfc_source, script_offset, inject.span),
      });
    }
    if let Some(shape) = offer.instance_shape {
      out.instances.entry(inject.local.clone()).or_insert(shape);
    }
  }
  out
}

/// Global/same-file index: injection key → known offers (one entry per provide site).
pub fn provide_offer_index(provides: &[ProvideSite]) -> BTreeMap<InjectionKey, Vec<ProvideOffer>> {
  let mut index: BTreeMap<InjectionKey, Vec<ProvideOffer>> = BTreeMap::new();
  for site in provides {
    if !site.offer.is_known() {
      continue;
    }
    index.entry(site.key.clone()).or_default().push(site.offer.clone());
  }
  index
}

/// Exactly one known provide offer wins; otherwise a static default; multi-provide stays quiet.
pub fn resolve_inject_offer(
  index: &BTreeMap<InjectionKey, Vec<ProvideOffer>>,
  inject: &InjectSite,
) -> Option<ProvideOffer> {
  match index.get(&inject.key).map(Vec::as_slice) {
    Some([offer]) => Some(offer.clone()),
    Some([_, _, ..]) => None,
    Some([]) | None => {
      if inject.default_kind.is_none() && inject.default_instance_shape.is_none() {
        return None;
      }
      Some(ProvideOffer {
        kind: inject.default_kind,
        instance_shape: inject.default_instance_shape.clone(),
      })
    }
  }
}

fn is_provide_call(
  semantic: &Semantic<'_>,
  callee: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  script_kind: ScriptKind,
) -> bool {
  if resolved_vue_callee(semantic, callee, imported_bindings, script_kind).as_deref()
    == Some("provide")
  {
    return true;
  }
  match callee {
    Expression::StaticMemberExpression(member) => member.property.name.as_str() == "provide",
    Expression::ComputedMemberExpression(member) => {
      member.static_property_name().is_some_and(|name| name == "provide")
    }
    _ => false,
  }
}

fn injection_key(
  semantic: &Semantic<'_>,
  expression: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
) -> Option<InjectionKey> {
  match expression {
    Expression::StringLiteral(literal) => Some(InjectionKey::String(literal.value.to_string())),
    Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
      let quasi = template.quasis.first()?;
      Some(InjectionKey::String(quasi.value.cooked.as_ref()?.to_string()))
    }
    Expression::Identifier(identifier) => {
      let name = identifier.name.as_str();
      if let Some((specifier, imported)) = imported_bindings.get(name) {
        if imported == "*" {
          return None;
        }
        return Some(InjectionKey::Imported {
          specifier: specifier.clone(),
          imported: imported.clone(),
        });
      }
      let reference_id = identifier.reference_id.get()?;
      let symbol_id = semantic.scoping().get_reference(reference_id).symbol_id()?;
      let def_start = semantic.scoping().symbol_span(symbol_id).start;
      Some(InjectionKey::Local { name: name.into(), def_start })
    }
    _ => None,
  }
}

fn expression_provide_offer(
  semantic: &Semantic<'_>,
  expression: &Expression<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  reactive_bindings: &[ReactiveBindingFact],
  composable_instances: &ComposableShapeMap,
  local_composable_defs: &LocalComposableDefs,
  script_kind: ScriptKind,
) -> ProvideOffer {
  if let Some(identifier) = expression.get_identifier_reference() {
    let name = identifier.name.as_str();
    if let Some(shape) = composable_instances.get(name) {
      return ProvideOffer { kind: None, instance_shape: Some(shape.clone()) };
    }
    if let Some(binding) = reactive_bindings.iter().find(|binding| binding.name == name) {
      return ProvideOffer { kind: Some(binding.kind), instance_shape: None };
    }
    return ProvideOffer { kind: None, instance_shape: None };
  }
  if let Expression::CallExpression(call) = expression {
    // `provide('api', useCounter())` — resolve callee to the composable def span
    // (name-only would invent outer shape when a block shadows `useCounter`).
    if let Some(callee) = call.callee.get_identifier_reference()
      && let Some((_, export)) = local_composable_defs
        .get(callee.name.as_str())
        .filter(|(def_span, _)| reference_resolves_to_span(semantic, callee, *def_span))
    {
      return match export {
        LocalComposableExport::Bag(shape) if !shape.fields.is_empty() => {
          ProvideOffer { kind: None, instance_shape: Some(shape.fields.clone()) }
        }
        LocalComposableExport::Factory(kind) => {
          ProvideOffer { kind: Some(*kind), instance_shape: None }
        }
        LocalComposableExport::Bag(_) | LocalComposableExport::ValueFactory(_) => {
          ProvideOffer { kind: None, instance_shape: None }
        }
      };
    }
    if let Some(callee) =
      resolved_vue_callee(semantic, &call.callee, imported_bindings, script_kind)
      && let Some(kind) = reactive_binding_kind(&callee)
    {
      return ProvideOffer { kind: Some(kind), instance_shape: None };
    }
  }
  ProvideOffer { kind: None, instance_shape: None }
}
