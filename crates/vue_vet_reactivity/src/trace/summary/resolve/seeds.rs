//! Worker-side seed materialization: attach SFC-absolute spans to the
//! coordinator's [`ModuleSeedPlan`] from the module's live parse.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::{
  AstKind,
  ast::{BindingPattern, Expression, IdentifierReference},
};
use oxc_semantic::Semantic;
use oxc_span::Span;
use vue_vet_core::ReactiveBindingFact;

use super::super::super::kinds::{
  collect_binding_identifiers, collect_imported_bindings, source_span,
};
use super::super::super::{TraceSeeds, collect_inject_sites};
use super::super::{
  ExportState, ImportSummary, ModuleSource, ValueBag, ValueBagEntry, collect_imports,
  export_lattice, seed_options_callback_params_at_calls, seed_typed_callback_params_at_calls,
  static_member_call_path,
};
use super::{ImportSeedPlan, ModuleSeedPlan};

/// `const { field: local } = useX()` — one destructured field of a call.
#[derive(Debug, Eq, PartialEq)]
struct DestructuredCallBinding {
  imported_local: String,
  property: String,
  local: String,
  span: Span,
}

/// `const bag = useFoo()` — whole-object composable call used via member access.
#[derive(Debug, Eq, PartialEq)]
struct InstanceCallBinding {
  imported_local: String,
  local: String,
  span: Span,
}

/// Call sites whose callee resolved to a seed-plan key, each list sorted by span.
#[derive(Default)]
struct CallSites {
  destructured: Vec<DestructuredCallBinding>,
  instances: Vec<InstanceCallBinding>,
}

impl CallSites {
  fn destructured_of<'s>(
    &'s self,
    local: &'s str,
  ) -> impl Iterator<Item = &'s DestructuredCallBinding> {
    self.destructured.iter().filter(move |call| call.imported_local == local)
  }

  fn instances_of<'s>(&'s self, local: &'s str) -> impl Iterator<Item = &'s InstanceCallBinding> {
    self.instances.iter().filter(move |call| call.imported_local == local)
  }
}

/// Callee is an import local whose reference binds to that import declaration.
fn resolve_imported_callee<'a>(
  semantic: &Semantic<'_>,
  callee: &IdentifierReference<'_>,
  imports: &'a [ImportSummary],
) -> Option<&'a ImportSummary> {
  imports.iter().find(|import| {
    if import.local != callee.name.as_str() {
      return false;
    }
    let Some(reference_id) = callee.reference_id.get() else {
      return false;
    };
    semantic
      .scoping()
      .get_reference(reference_id)
      .symbol_id()
      .is_some_and(|symbol_id| semantic.scoping().symbol_span(symbol_id) == import.span)
  })
}

/// Callee is an unresolved identifier (bare Nuxt / Vite auto-import) named in the plan.
fn resolve_bare_callee(
  semantic: &Semantic<'_>,
  callee: &IdentifierReference<'_>,
  plan: &ImportSeedPlan,
) -> Option<String> {
  if !plan.contains_key(callee.name.as_str()) {
    return None;
  }
  let reference_id = callee.reference_id.get()?;
  if semantic.scoping().get_reference(reference_id).symbol_id().is_some() {
    return None;
  }
  Some(callee.name.to_string())
}

/// One walk over every `callee(...)` declarator initializer. `resolve_callee`
/// maps the callee identifier to its seed-plan key, or `None` to skip the site.
///
/// `bare_plan` switches on the bare auto-import extensions for instance
/// bindings: `const x = cond ? ref(false) : useX()` when both arms are ref-like,
/// and first declaration wins per local name.
fn collect_call_sites(
  semantic: &Semantic<'_>,
  resolve_callee: impl Fn(&IdentifierReference<'_>) -> Option<String>,
  bare_plan: Option<&ImportSeedPlan>,
) -> CallSites {
  let mut sites = CallSites::default();
  let mut seen_locals = BTreeSet::new();
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some(callee) = call.callee.get_identifier_reference() else {
      continue;
    };
    let Some(imported_local) = resolve_callee(callee) else {
      continue;
    };
    let call_id = call.node_id.get();
    let (declarator, ternary_plan) = match semantic.nodes().parent_kind(call_id) {
      AstKind::VariableDeclarator(declarator) => (declarator, None),
      AstKind::ConditionalExpression(_) if bare_plan.is_some() => {
        // call → conditional → declarator
        let cond_id = semantic.nodes().parent_id(call_id);
        match semantic.nodes().parent_kind(cond_id) {
          AstKind::VariableDeclarator(declarator) => (declarator, bare_plan),
          _ => continue,
        }
      }
      _ => continue,
    };
    match &declarator.id {
      BindingPattern::ObjectPattern(pattern) if ternary_plan.is_none() => {
        for property in &pattern.properties {
          let Some(exported) = property.key.static_name() else {
            continue;
          };
          let mut identifiers = Vec::new();
          collect_binding_identifiers(&property.value, &mut identifiers);
          for (local, span) in identifiers {
            sites.destructured.push(DestructuredCallBinding {
              imported_local: imported_local.clone(),
              property: exported.to_string(),
              local,
              span,
            });
          }
        }
      }
      BindingPattern::BindingIdentifier(identifier) => {
        if let Some(plan) = ternary_plan {
          let Some(Expression::ConditionalExpression(cond)) = &declarator.init else {
            continue;
          };
          if !conditional_arms_ref_like_with_plan(cond, plan) {
            continue;
          }
        }
        let local = identifier.name.to_string();
        if bare_plan.is_some() && !seen_locals.insert(local.clone()) {
          continue;
        }
        sites.instances.push(InstanceCallBinding { imported_local, local, span: identifier.span });
      }
      _ => {}
    }
  }
  sites.destructured.sort_by_key(|call| call.span.start);
  sites.instances.sort_by_key(|call| call.span.start);
  sites
}

/// Worker-side: attach SFC-absolute spans from the live parse (no second parse).
pub(super) fn materialize_seeds(
  module: &ModuleSource,
  semantic: &Semantic<'_>,
  plan: &ModuleSeedPlan,
) -> TraceSeeds {
  if plan.is_empty() {
    return TraceSeeds::default();
  }
  let imports = collect_imports(semantic);
  let imported = collect_call_sites(
    semantic,
    |callee| resolve_imported_callee(semantic, callee, &imports).map(|import| import.local.clone()),
    None,
  );
  let bare = collect_call_sites(
    semantic,
    |callee| resolve_bare_callee(semantic, callee, &plan.imports),
    Some(&plan.imports),
  );
  let span_source = module.span_origin();
  let span_base = module.source_offset;
  let mut seeds = TraceSeeds::default();
  for (local, state) in &plan.imports {
    match state {
      ExportState::Known(kind) => {
        // Prefer the import binding span; bare Nuxt/Vite auto-imports of exported
        // `const currentUser = computed(...)` have no import — use the first
        // unresolved identifier reference as the span.
        let span = if let Some(import) = imports.iter().find(|import| import.local == *local) {
          source_span(span_source, span_base, import.span)
        } else if let Some(reference_span) = first_bare_identifier_span(semantic, local) {
          source_span(span_source, span_base, reference_span)
        } else {
          continue;
        };
        if seeds.bindings.iter().any(|binding| binding.name == *local) {
          continue;
        }
        seeds.bindings.push(ReactiveBindingFact {
          name: local.clone(),
          kind: *kind,
          initialized_with_null: false,
          alias_of: None,
          alias_of_span: None,
          span,
        });
      }
      ExportState::Factory(kind) => {
        for call in imported.instances_of(local).chain(bare.instances_of(local)) {
          if seeds.bindings.iter().any(|binding| binding.name == call.local) {
            continue;
          }
          seeds.bindings.push(ReactiveBindingFact {
            name: call.local.clone(),
            kind: *kind,
            initialized_with_null: false,
            alias_of: None,
            alias_of_span: None,
            span: source_span(span_source, span_base, call.span),
          });
        }
      }
      ExportState::Composable(shape) => {
        for call in imported.destructured_of(local).chain(bare.destructured_of(local)) {
          let Some(kind) = shape.kind_for_destructure(&call.property) else {
            continue;
          };
          seeds.bindings.push(ReactiveBindingFact {
            name: call.local.clone(),
            kind,
            initialized_with_null: false,
            alias_of: None,
            alias_of_span: None,
            span: source_span(span_source, span_base, call.span),
          });
        }
        for call in imported.instances_of(local).chain(bare.instances_of(local)) {
          seeds.composable_instances.insert(call.local.clone(), shape.fields.clone());
        }
      }
      ExportState::ValueFactory(bag) => {
        // `const api = createApi()` — calling a value factory yields a value bag.
        for call in imported.instances_of(local).chain(bare.instances_of(local)) {
          seed_value_bag_binding(&mut seeds, &call.local, bag);
        }
      }
      ExportState::ValueBag(bag) => {
        seed_value_bag_binding(&mut seeds, local, bag);
      }
      ExportState::ComponentFactory => {
        // Import local is a defineComponent setup wrapper — seed props at call sites.
        seeds.component_factories.insert(local.clone());
      }
      // Provisional / non-seedable (`!is_seedable`) — never invent consumer seeds.
      // New seedable variants without an arm also fall here (fail closed).
      _ => {}
    }
  }
  // Member-call destructures against seeded value bags (`api.maps.useX()`).
  let bags = seeds.value_bags.clone();
  seed_member_calls_from_value_bags(semantic, &bags, span_source, span_base, &mut seeds);
  // `defineFormProps({ setup({ values }) {…} })` — seed options-object callback bags.
  seed_options_callback_params_at_calls(
    semantic,
    &plan.options_callback_slots,
    span_source,
    span_base,
    &mut seeds.bindings,
  );
  // `useX(init, (state: ComputedRef<T>) => …)` — seed typed function-callback formals.
  seed_typed_callback_params_at_calls(
    semantic,
    &plan.typed_callback_param_slots,
    span_source,
    span_base,
    &mut seeds.bindings,
  );
  // Inject locals: re-read sites for exact spans; offers from the coordinator plan.
  if !plan.injects.is_empty() {
    let imported_bindings = collect_imported_bindings(semantic);
    let injects = collect_inject_sites(semantic, &imported_bindings, &[], module.kind);
    for inject in injects {
      let Some(offer) = plan.injects.get(&inject.local) else {
        continue;
      };
      if let Some(kind) = offer.kind
        && !seeds.bindings.iter().any(|binding| binding.name == inject.local)
      {
        seeds.bindings.push(ReactiveBindingFact {
          name: inject.local.clone(),
          kind,
          initialized_with_null: false,
          alias_of: None,
          alias_of_span: None,
          span: source_span(span_source, span_base, inject.span),
        });
      }
      if let Some(shape) = &offer.instance_shape {
        seeds.composable_instances.entry(inject.local).or_insert_with(|| shape.clone());
      }
    }
  }
  seeds
}

/// First unresolved `IdentifierReference` for `name` (bare auto-import value use).
fn first_bare_identifier_span(semantic: &oxc_semantic::Semantic<'_>, name: &str) -> Option<Span> {
  let mut best: Option<Span> = None;
  for node in semantic.nodes() {
    let AstKind::IdentifierReference(identifier) = node.kind() else {
      continue;
    };
    if identifier.name.as_str() != name {
      continue;
    }
    let Some(reference_id) = identifier.reference_id.get() else {
      continue;
    };
    if semantic.scoping().get_reference(reference_id).symbol_id().is_some() {
      continue;
    }
    let span = identifier.span;
    if best.is_none_or(|current| span.start < current.start) {
      best = Some(span);
    }
  }
  best
}

/// Both ternary arms are ref-like: Vue `ref`/`computed`/… or seed-plan Factory/Known.
fn conditional_arms_ref_like_with_plan(
  cond: &oxc_ast::ast::ConditionalExpression<'_>,
  plan: &ImportSeedPlan,
) -> bool {
  arm_is_ref_like_with_plan(&cond.consequent, plan)
    && arm_is_ref_like_with_plan(&cond.alternate, plan)
}

fn arm_is_ref_like_with_plan(
  expression: &oxc_ast::ast::Expression<'_>,
  plan: &ImportSeedPlan,
) -> bool {
  let mut current = expression;
  for _ in 0..4 {
    match current {
      Expression::ParenthesizedExpression(paren) => current = &paren.expression,
      Expression::TSAsExpression(assertion) => current = &assertion.expression,
      Expression::TSTypeAssertion(assertion) => current = &assertion.expression,
      Expression::TSNonNullExpression(non_null) => current = &non_null.expression,
      Expression::CallExpression(call) => {
        let Some(callee) = call.callee.get_identifier_reference() else {
          return false;
        };
        let name = callee.name.as_str();
        // Vue primitive allowlist (bare or imported).
        if matches!(
          name,
          "ref"
            | "shallowRef"
            | "computed"
            | "customRef"
            | "toRef"
            | "useTemplateRef"
            | "defineModel"
        ) {
          return true;
        }
        return plan.get(name).and_then(export_lattice::ref_like_kind_from_export).is_some();
      }
      _ => return false,
    }
  }
  false
}

fn seed_value_bag_binding(seeds: &mut TraceSeeds, local: &str, bag: &ValueBag) {
  seeds.value_bags.insert(local.to_owned(), bag.clone());
}

fn seed_member_calls_from_value_bags(
  semantic: &Semantic<'_>,
  bags: &BTreeMap<String, ValueBag>,
  span_source: &str,
  span_base: usize,
  seeds: &mut TraceSeeds,
) {
  if bags.is_empty() {
    return;
  }
  for node in semantic.nodes() {
    let AstKind::CallExpression(call) = node.kind() else {
      continue;
    };
    let Some((root, path)) = static_member_call_path(&call.callee) else {
      continue;
    };
    let Some(bag) = bags.get(&root) else {
      continue;
    };
    let Some(entry) = bag.resolve_path(&path) else {
      continue;
    };
    let AstKind::VariableDeclarator(declarator) = semantic.nodes().parent_kind(call.node_id.get())
    else {
      continue;
    };
    match (entry, &declarator.id) {
      (ValueBagEntry::Method(shape), BindingPattern::ObjectPattern(pattern)) => {
        for property in &pattern.properties {
          let Some(exported) = property.key.static_name() else {
            continue;
          };
          let Some(kind) = shape.kind_for_destructure(exported.as_ref()) else {
            continue;
          };
          let mut identifiers = Vec::new();
          collect_binding_identifiers(&property.value, &mut identifiers);
          for (local, span) in identifiers {
            if seeds.bindings.iter().any(|binding| binding.name == local) {
              continue;
            }
            seeds.bindings.push(ReactiveBindingFact {
              name: local,
              kind,
              initialized_with_null: false,
              alias_of: None,
              alias_of_span: None,
              span: source_span(span_source, span_base, span),
            });
          }
        }
      }
      (ValueBagEntry::Method(shape), BindingPattern::BindingIdentifier(identifier)) => {
        seeds.composable_instances.insert(identifier.name.to_string(), shape.fields.clone());
      }
      (ValueBagEntry::MethodFactory(kind), BindingPattern::BindingIdentifier(identifier)) => {
        if seeds.bindings.iter().any(|binding| binding.name == identifier.name.as_str()) {
          continue;
        }
        seeds.bindings.push(ReactiveBindingFact {
          name: identifier.name.to_string(),
          kind: *kind,
          initialized_with_null: false,
          alias_of: None,
          alias_of_span: None,
          span: source_span(span_source, span_base, identifier.span),
        });
      }
      _ => {}
    }
  }
}
