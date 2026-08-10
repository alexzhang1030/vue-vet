//! Pure A6 [`ExportState`] lattice operations (no AST).
//!
//! Contract: [reactivity tracer PCR](../../../../../../.agents/docs/reactivity-tracer.md)
//! — seedable vs provisional, local merge, declaration/implementation merge,
//! name resolve, pending fields, `MethodForward` refine, publish barrier.

use std::collections::BTreeMap;

use vue_vet_core::{ModuleId, ReactiveBindingKind};

use super::{ComposableShape, ExportState, ValueBag, ValueBagEntry};

/// Max `ForwardReturn` chain depth (depth starts at 0; `depth > this` → `None`).
pub(super) const NAME_RESOLVE_MAX_DEPTH: u8 = 8;

/// Borrowed ES import binding for pure name resolve (no AST / no `ImportSummary`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ImportBindingView<'a> {
  pub local: &'a str,
  pub source: &'a str,
  pub imported: &'a str,
}

/// Whether a state may cross the seed barrier into a consumer module.
#[must_use]
pub(super) const fn is_seedable(state: &ExportState) -> bool {
  matches!(
    state,
    ExportState::Known(_)
      | ExportState::Factory(_)
      | ExportState::Composable(_)
      | ExportState::ValueFactory(_)
      | ExportState::ValueBag(_)
      | ExportState::ComponentFactory
  )
}

/// Whether to keep `existing` when another definition offers `next` for the same name.
///
/// Lattice rules (PCR):
/// 1. `Factory` beats later `Composable` (scalar default overload vs controls bag).
/// 2. `Known` beats later `Factory` / `Composable` (graph-seeded value wins).
/// 3. Otherwise the new state replaces the old one.
#[must_use]
pub(super) const fn prefers_existing(existing: &ExportState, next: &ExportState) -> bool {
  matches!(
    (existing, next),
    (ExportState::Factory(_), ExportState::Composable(_))
      | (ExportState::Known(_), ExportState::Factory(_) | ExportState::Composable(_))
  )
}

/// Merge two successive local definitions of the same export name.
#[must_use]
pub(super) fn merge_local(existing: Option<&ExportState>, next: ExportState) -> ExportState {
  match existing {
    Some(prev) if prefers_existing(prev, &next) => prev.clone(),
    _ => next,
  }
}

/// Merge one name from a `.d.ts` declaration with its companion implementation.
///
/// Returns `Some(next)` when the implementation should overwrite / complete the
/// declaration local; `None` keeps the declaration entry unchanged.
///
/// Contract (under-approx):
/// 1. `DeclaredPlainObjectFactory` ↔ `BodyUnwrappedState` → `Factory(Reactive)`.
/// 2. Declaration plain-object + impl `Factory(Reactive)` → keep Reactive factory.
/// 3. Missing/provisional declaration + seedable impl → take impl.
/// 4. Declaration `ForwardReturn` + impl Factory/Composable/ValueFactory/ComponentFactory
///    → take impl (not Known / `ValueBag` — those stay quiet here).
/// 5. Missing declaration + provisional half alone → keep the half for later.
#[must_use]
pub(super) fn merge_declaration_implementation_local(
  declaration: Option<&ExportState>,
  implementation: &ExportState,
) -> Option<ExportState> {
  match (declaration, implementation) {
    (Some(ExportState::DeclaredPlainObjectFactory), ExportState::BodyUnwrappedState)
    | (Some(ExportState::BodyUnwrappedState), ExportState::DeclaredPlainObjectFactory) => {
      Some(ExportState::Factory(ReactiveBindingKind::Reactive))
    }
    (Some(ExportState::DeclaredPlainObjectFactory), ExportState::Factory(kind))
      if *kind == ReactiveBindingKind::Reactive =>
    {
      Some(ExportState::Factory(ReactiveBindingKind::Reactive))
    }
    (
      None | Some(ExportState::DeclaredPlainObjectFactory | ExportState::BodyUnwrappedState),
      state,
    ) if is_seedable(state) => Some(state.clone()),
    (Some(ExportState::ForwardReturn(_)), state)
      if matches!(
        state,
        ExportState::Factory(_)
          | ExportState::Composable(_)
          | ExportState::ValueFactory(_)
          | ExportState::ComponentFactory
      ) =>
    {
      Some(state.clone())
    }
    (None, ExportState::BodyUnwrappedState | ExportState::DeclaredPlainObjectFactory) => {
      Some(implementation.clone())
    }
    _ => None,
  }
}

/// Resolve a name to an [`ExportState`] (PCR Name resolve order, under-approx).
///
/// 1. Working `locals` — recurse nested `ForwardReturn`; return seedable as-is;
///    fall through on provisional / missing.
/// 2. First ES import whose `local` matches → `link(source)` →
///    `resolved_export(target, imported)`. Missing link/export fails closed
///    (does not try later imports or bare).
/// 3. Bare auto-import → `link("#nuxt-imports:{name}")` →
///    `resolved_export(target, name)`.
///
/// `link` and `resolved_export` are pure views over the caller's link table and
/// resolved export map — no AST, no graph mutation.
pub(super) fn resolve_name_export_state<L, R>(
  name: &str,
  locals: &BTreeMap<String, ExportState>,
  imports: &[ImportBindingView<'_>],
  mut link: L,
  mut resolved_export: R,
  depth: u8,
) -> Option<ExportState>
where
  L: FnMut(&str) -> Option<ModuleId>,
  R: FnMut(&ModuleId, &str) -> Option<ExportState>,
{
  if depth > NAME_RESOLVE_MAX_DEPTH {
    return None;
  }
  if let Some(state) = locals.get(name) {
    if let ExportState::ForwardReturn(callee) = state {
      return resolve_name_export_state(
        callee,
        locals,
        imports,
        link,
        resolved_export,
        depth.saturating_add(1),
      );
    }
    if is_seedable(state) {
      return Some(state.clone());
    }
  }
  for import in imports {
    if import.local != name {
      continue;
    }
    let target = link(import.source)?;
    return resolved_export(&target, import.imported);
  }
  let bare = format!("{}{name}", super::NUXT_IMPORTS_SPECIFIER_PREFIX);
  if let Some(target) = link(&bare) {
    return resolved_export(&target, name);
  }
  None
}

/// Outcome of publishing `next` over an already-published `existing` export.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PublishMerge {
  /// Same state, or existing is already [`ExportState::Ambiguous`] — map unchanged.
  Unchanged,
  /// Same seedable class (`ValueFactory` / `ValueBag` / Composable) — replace with refinement.
  Replace,
  /// Conflicting seedable evidence — publish [`ExportState::Ambiguous`].
  Ambiguous,
}

/// How two successive seedable publishes of the same export name combine.
///
/// Same-class bag refinements replace; anything else becomes Ambiguous.
/// Sticky Ambiguous is never overwritten (fail closed).
#[must_use]
pub(super) fn merge_published(existing: &ExportState, next: &ExportState) -> PublishMerge {
  if existing == next || *existing == ExportState::Ambiguous {
    return PublishMerge::Unchanged;
  }
  match (existing, next) {
    (ExportState::ValueFactory(_), ExportState::ValueFactory(_))
    | (ExportState::ValueBag(_), ExportState::ValueBag(_))
    | (ExportState::Composable(_), ExportState::Composable(_)) => PublishMerge::Replace,
    _ => PublishMerge::Ambiguous,
  }
}

/// Resolve a pending bag field against the already-resolved root export state.
///
/// - Empty `path`: `const { field } = useX()` → [`ExportState::Composable`] field lookup.
/// - Non-empty path: `ValueBag` walk then method / `MethodFactory` leaf.
///
/// Under-approx: wrong root class, unresolved path, or `MethodForward` leaf → `None`.
#[must_use]
pub(super) fn resolve_pending_field(
  root: &ExportState,
  path: &[String],
  field: &str,
) -> Option<ReactiveBindingKind> {
  if path.is_empty() {
    let ExportState::Composable(shape) = root else {
      return None;
    };
    return shape.fields.get(field).copied();
  }
  let (ExportState::ValueBag(bag) | ExportState::ValueFactory(bag)) = root else {
    return None;
  };
  match bag.resolve_path(path)? {
    ValueBagEntry::Method(method_shape) => method_shape.kind_for_destructure(field),
    ValueBagEntry::MethodFactory(kind) => Some(*kind),
    ValueBagEntry::MethodForward(_)
    | ValueBagEntry::MethodGeneric(_)
    | ValueBagEntry::Nested(_) => None,
  }
}

/// Refine a [`ValueBagEntry::MethodForward`] once its callee export is known.
///
/// Unresolved / non-matching callee keeps the forward marker (under-approx).
#[must_use]
pub(super) fn refine_method_forward(resolved: &ExportState, callee: String) -> ValueBagEntry {
  match resolved {
    ExportState::Composable(shape) => ValueBagEntry::Method(shape.clone()),
    ExportState::Factory(kind) => ValueBagEntry::MethodFactory(*kind),
    ExportState::ValueFactory(nested) | ExportState::ValueBag(nested) => {
      ValueBagEntry::Nested(nested.clone())
    }
    _ => ValueBagEntry::MethodForward(callee),
  }
}

/// Refine [`ExportState::GenericMethodInstantiate`] against the callee bag.
///
/// - Callee is ValueFactory/ValueBag with `MethodGeneric(i)` at `property` and a
///   non-empty type-arg shape → [`ExportState::Composable`].
/// - Callee bag present but property/index miss → [`ExportState::Ambiguous`].
/// - Callee not yet a bag → keep the instantiate marker.
#[must_use]
pub(super) fn refine_generic_method_instantiate(
  callee_state: Option<&ExportState>,
  property: &str,
  type_arg_shapes: &[ComposableShape],
  keep: ExportState,
) -> ExportState {
  match callee_state {
    Some(ExportState::ValueFactory(bag) | ExportState::ValueBag(bag)) => {
      match bag.entries.get(property) {
        Some(ValueBagEntry::MethodGeneric(index)) => type_arg_shapes
          .get(*index as usize)
          .filter(|shape| !shape.is_empty())
          .map_or(ExportState::Ambiguous, |shape| ExportState::Composable(shape.clone())),
        _ => ExportState::Ambiguous,
      }
    }
    _ => keep,
  }
}

/// Resolve nested [`ValueBagEntry::MethodForward`] entries via `resolve(name)`.
///
/// Recurses into nested bags. Unresolved forwards stay markers (under-approx).
#[must_use]
pub(super) fn refine_value_bag(
  bag: ValueBag,
  mut resolve: impl FnMut(&str) -> Option<ExportState>,
) -> ValueBag {
  // Dyn object avoids infinitely nested `impl FnMut` types on nested bags.
  fn refine_with(bag: ValueBag, resolve: &mut dyn FnMut(&str) -> Option<ExportState>) -> ValueBag {
    let mut entries = BTreeMap::new();
    for (key, entry) in bag.entries {
      let next = match entry {
        ValueBagEntry::Nested(nested) => ValueBagEntry::Nested(refine_with(nested, resolve)),
        ValueBagEntry::MethodForward(callee) => match resolve(&callee) {
          Some(state) => refine_method_forward(&state, callee),
          None => ValueBagEntry::MethodForward(callee),
        },
        other => other,
      };
      entries.insert(key, next);
    }
    ValueBag { entries }
  }
  refine_with(bag, &mut resolve)
}

/// Materialize pending bag fields on a composable shape via `resolve_root(name)`.
///
/// Already-present fields win; unresolved pendings are retained for later rounds.
#[must_use]
pub(super) fn refine_composable_pending(
  mut shape: ComposableShape,
  mut resolve_root: impl FnMut(&str) -> Option<ExportState>,
) -> ComposableShape {
  let pending = std::mem::take(&mut shape.pending_value_bag_fields);
  for (key, pref) in pending {
    if shape.fields.contains_key(&key) {
      continue;
    }
    match resolve_root(&pref.root)
      .and_then(|root| resolve_pending_field(&root, &pref.path, &pref.field))
    {
      Some(kind) => {
        shape.fields.insert(key, kind);
      }
      None => {
        shape.pending_value_bag_fields.insert(key, pref);
      }
    }
  }
  shape
}

/// Whether a refined local may attempt publish this round (before seedable gate).
///
/// - [`ExportState::ValueFactoryCall`]: only when caller supplies a materialized
///   [`ExportState::ValueBag`] (from a finished `ValueFactory` callee).
/// - [`ExportState::GenericMethodInstantiate`] / [`ExportState::Ambiguous`]: never.
/// - otherwise: clone (insert still applies [`is_seedable`]).
#[must_use]
pub(super) fn as_publishable(
  state: &ExportState,
  materialized_value_factory_call: Option<ExportState>,
) -> Option<ExportState> {
  match state {
    ExportState::ValueFactoryCall(_) => match materialized_value_factory_call {
      some @ Some(ExportState::ValueBag(_)) => some,
      _ => None,
    },
    ExportState::GenericMethodInstantiate { .. } | ExportState::Ambiguous => None,
    other => Some(other.clone()),
  }
}

/// Extract a [`ValueFactory`] bag from a resolved callee of [`ExportState::ValueFactoryCall`].
///
/// Only unfinished call markers wait; a non-factory callee stays quiet (under-approx).
#[must_use]
pub(super) const fn value_factory_call_bag(callee: Option<&ExportState>) -> Option<&ValueBag> {
  match callee {
    Some(ExportState::ValueFactory(bag)) => Some(bag),
    _ => None,
  }
}

/// Collapse [`ExportState::ForwardReturn`] once the callee name resolves.
#[must_use]
pub(super) fn refine_forward_return(resolved: Option<ExportState>, callee: String) -> ExportState {
  resolved.unwrap_or(ExportState::ForwardReturn(callee))
}

#[cfg(test)]
mod tests {
  use super::*;
  use vue_vet_core::ReactiveBindingKind;

  fn factory_ref() -> ExportState {
    ExportState::Factory(ReactiveBindingKind::Ref)
  }

  fn factory_computed() -> ExportState {
    ExportState::Factory(ReactiveBindingKind::Computed)
  }

  fn known_ref() -> ExportState {
    ExportState::Known(ReactiveBindingKind::Ref)
  }

  fn empty_composable() -> ExportState {
    ExportState::Composable(super::super::ComposableShape::default())
  }

  fn mid(id: &str) -> ModuleId {
    ModuleId::from(id)
  }

  fn resolve(
    name: &str,
    locals: &BTreeMap<String, ExportState>,
    imports: &[ImportBindingView<'_>],
    links: &BTreeMap<(&str, &str), ModuleId>,
    resolved: &BTreeMap<ModuleId, BTreeMap<String, ExportState>>,
  ) -> Option<ExportState> {
    resolve_name_export_state(
      name,
      locals,
      imports,
      |specifier| links.get(&("consumer", specifier)).cloned(),
      |target, export| resolved.get(target)?.get(export).cloned(),
      0,
    )
  }

  #[test]
  fn seedable_matches_publish_barrier() {
    assert!(is_seedable(&known_ref()));
    assert!(is_seedable(&factory_ref()));
    assert!(is_seedable(&empty_composable()));
    assert!(is_seedable(&ExportState::ComponentFactory));
    assert!(!is_seedable(&ExportState::ForwardReturn("useX".into())));
    assert!(!is_seedable(&ExportState::ValueFactoryCall("createApi".into())));
    assert!(!is_seedable(&ExportState::Ambiguous));
    assert!(!is_seedable(&ExportState::DeclaredPlainObjectFactory));
    assert!(!is_seedable(&ExportState::BodyUnwrappedState));
  }

  #[test]
  fn factory_beats_later_composable_overload() {
    let merged = merge_local(Some(&factory_ref()), empty_composable());
    assert_eq!(merged, factory_ref());
  }

  #[test]
  fn factory_replaces_earlier_composable() {
    let merged = merge_local(Some(&empty_composable()), factory_ref());
    assert_eq!(merged, factory_ref());
  }

  #[test]
  fn known_beats_later_factory_or_composable() {
    assert_eq!(merge_local(Some(&known_ref()), factory_ref()), known_ref());
    assert_eq!(merge_local(Some(&known_ref()), empty_composable()), known_ref());
  }

  #[test]
  fn last_write_wins_for_same_class_or_unrelated() {
    assert_eq!(merge_local(Some(&factory_ref()), factory_computed()), factory_computed());
    assert_eq!(
      merge_local(Some(&ExportState::ForwardReturn("a".into())), factory_ref()),
      factory_ref()
    );
    assert_eq!(merge_local(None, factory_ref()), factory_ref());
  }

  #[test]
  fn provisional_never_preferred_over_seedable_by_accident() {
    // ForwardReturn is not "preferred existing" when next is Factory.
    assert!(!prefers_existing(&ExportState::ForwardReturn("useX".into()), &factory_ref()));
    // Factory is not preferred when next is also Factory (different kinds last-win).
    assert!(!prefers_existing(&factory_ref(), &factory_computed()));
  }

  #[test]
  fn resolve_local_seedable_factory() {
    let locals = BTreeMap::from([("useCount".into(), factory_ref())]);
    assert_eq!(
      resolve("useCount", &locals, &[], &BTreeMap::new(), &BTreeMap::new()),
      Some(factory_ref())
    );
  }

  #[test]
  fn resolve_forward_return_chain_in_locals() {
    let locals = BTreeMap::from([
      ("storage".into(), ExportState::ForwardReturn("useX".into())),
      ("useX".into(), factory_ref()),
    ]);
    assert_eq!(
      resolve("storage", &locals, &[], &BTreeMap::new(), &BTreeMap::new()),
      Some(factory_ref())
    );
  }

  #[test]
  fn resolve_es_import_via_link_and_resolved_export() {
    let provider = mid("provider");
    let imports =
      [ImportBindingView { local: "useCount", source: "./count", imported: "useCount" }];
    let links = BTreeMap::from([(("consumer", "./count"), provider.clone())]);
    let resolved =
      BTreeMap::from([(provider, BTreeMap::from([("useCount".into(), factory_ref())]))]);
    assert_eq!(
      resolve("useCount", &BTreeMap::new(), &imports, &links, &resolved),
      Some(factory_ref())
    );
  }

  #[test]
  fn resolve_bare_nuxt_auto_import() {
    let provider = mid("auto");
    let bare = format!("{}useStorage", super::super::NUXT_IMPORTS_SPECIFIER_PREFIX);
    let links = BTreeMap::from([(("consumer", bare.as_str()), provider.clone())]);
    let resolved =
      BTreeMap::from([(provider, BTreeMap::from([("useStorage".into(), factory_ref())]))]);
    assert_eq!(
      resolve("useStorage", &BTreeMap::new(), &[], &links, &resolved),
      Some(factory_ref())
    );
  }

  #[test]
  fn resolve_forward_return_through_bare_auto_import() {
    // `const storage = useX(); return storage` → ForwardReturn("useX") where useX is bare.
    let provider = mid("auto");
    let bare = format!("{}useX", super::super::NUXT_IMPORTS_SPECIFIER_PREFIX);
    let locals = BTreeMap::from([("storage".into(), ExportState::ForwardReturn("useX".into()))]);
    let links = BTreeMap::from([(("consumer", bare.as_str()), provider.clone())]);
    let resolved = BTreeMap::from([(provider, BTreeMap::from([("useX".into(), factory_ref())]))]);
    assert_eq!(resolve("storage", &locals, &[], &links, &resolved), Some(factory_ref()));
  }

  #[test]
  fn resolve_missing_is_none_under_approx() {
    assert_eq!(resolve("missing", &BTreeMap::new(), &[], &BTreeMap::new(), &BTreeMap::new()), None);
  }

  #[test]
  fn resolve_import_link_miss_fails_closed_before_bare() {
    // Matching ES import with no link must not invent bare fallback for the same name.
    let provider = mid("auto");
    let imports = [ImportBindingView { local: "useX", source: "./missing", imported: "useX" }];
    let bare = format!("{}useX", super::super::NUXT_IMPORTS_SPECIFIER_PREFIX);
    let links = BTreeMap::from([(("consumer", bare.as_str()), provider.clone())]);
    let resolved = BTreeMap::from([(provider, BTreeMap::from([("useX".into(), factory_ref())]))]);
    assert_eq!(resolve("useX", &BTreeMap::new(), &imports, &links, &resolved), None);
  }

  #[test]
  fn resolve_provisional_local_falls_through_to_import() {
    // ValueFactoryCall is not seedable; name may still be an ES import.
    let provider = mid("provider");
    let locals = BTreeMap::from([("useX".into(), ExportState::ValueFactoryCall("create".into()))]);
    let imports = [ImportBindingView { local: "useX", source: "./x", imported: "useX" }];
    let links = BTreeMap::from([(("consumer", "./x"), provider.clone())]);
    let resolved = BTreeMap::from([(provider, BTreeMap::from([("useX".into(), factory_ref())]))]);
    assert_eq!(resolve("useX", &locals, &imports, &links, &resolved), Some(factory_ref()));
  }

  #[test]
  fn resolve_depth_cap_stops_forward_cycle() {
    let locals = BTreeMap::from([
      ("a".into(), ExportState::ForwardReturn("b".into())),
      ("b".into(), ExportState::ForwardReturn("a".into())),
    ]);
    // Start past the cap so even the first step returns None.
    assert_eq!(
      resolve_name_export_state(
        "a",
        &locals,
        &[],
        |_| None,
        |_, _| None,
        NAME_RESOLVE_MAX_DEPTH.saturating_add(1),
      ),
      None
    );
    // Long chain that exceeds the cap mid-walk.
    let mut chain = BTreeMap::new();
    for i in 0..20 {
      let next = format!("n{}", i + 1);
      chain.insert(format!("n{i}"), ExportState::ForwardReturn(next));
    }
    chain.insert("n20".into(), factory_ref());
    assert_eq!(resolve_name_export_state("n0", &chain, &[], |_| None, |_, _| None, 0), None);
  }

  fn composable_with(field: &str, kind: ReactiveBindingKind) -> ExportState {
    let mut fields = BTreeMap::new();
    fields.insert(field.into(), kind);
    ExportState::Composable(super::super::ComposableShape::from_fields(fields))
  }

  #[test]
  fn publish_merge_same_class_bags_replace() {
    use super::super::{ValueBag, ValueBagEntry};
    let a = ExportState::Composable(super::super::ComposableShape::default());
    let b = composable_with("x", ReactiveBindingKind::Ref);
    assert_eq!(merge_published(&a, &b), PublishMerge::Replace);
    let empty_factory = ExportState::ValueFactory(ValueBag::default());
    let refined_factory = ExportState::ValueFactory(ValueBag {
      entries: BTreeMap::from([(
        "useX".into(),
        ValueBagEntry::MethodFactory(ReactiveBindingKind::Ref),
      )]),
    });
    assert_eq!(merge_published(&empty_factory, &refined_factory), PublishMerge::Replace);
  }

  #[test]
  fn publish_merge_conflict_becomes_ambiguous() {
    assert_eq!(merge_published(&factory_ref(), &known_ref()), PublishMerge::Ambiguous);
  }

  #[test]
  fn publish_merge_sticky_ambiguous_and_identity() {
    assert_eq!(merge_published(&ExportState::Ambiguous, &factory_ref()), PublishMerge::Unchanged);
    assert_eq!(merge_published(&factory_ref(), &factory_ref()), PublishMerge::Unchanged);
  }

  #[test]
  fn pending_empty_path_reads_composable_field() {
    let root = composable_with("isLoading", ReactiveBindingKind::Ref);
    assert_eq!(resolve_pending_field(&root, &[], "isLoading"), Some(ReactiveBindingKind::Ref));
    assert_eq!(resolve_pending_field(&root, &[], "missing"), None);
    assert_eq!(resolve_pending_field(&factory_ref(), &[], "x"), None);
  }

  #[test]
  fn pending_path_walks_value_bag_method_factory() {
    use super::super::{ValueBag, ValueBagEntry};
    let bag = ValueBag {
      entries: BTreeMap::from([(
        "maps".into(),
        ValueBagEntry::Nested(ValueBag {
          entries: BTreeMap::from([(
            "useX".into(),
            ValueBagEntry::MethodFactory(ReactiveBindingKind::Computed),
          )]),
        }),
      )]),
    };
    let root = ExportState::ValueFactory(bag);
    let path = vec!["maps".into(), "useX".into()];
    assert_eq!(resolve_pending_field(&root, &path, "ignored"), Some(ReactiveBindingKind::Computed));
  }

  #[test]
  fn pending_path_method_shape_destructure() {
    use super::super::{ValueBag, ValueBagEntry};
    let mut fields = BTreeMap::new();
    fields.insert("count".into(), ReactiveBindingKind::Ref);
    let bag = ValueBag {
      entries: BTreeMap::from([(
        "useX".into(),
        ValueBagEntry::Method(super::super::ComposableShape::from_fields(fields)),
      )]),
    };
    let root = ExportState::ValueBag(bag);
    assert_eq!(
      resolve_pending_field(&root, &["useX".into()], "count"),
      Some(ReactiveBindingKind::Ref)
    );
  }

  #[test]
  fn pending_unresolved_forward_leaf_stays_none() {
    use super::super::{ValueBag, ValueBagEntry};
    let bag = ValueBag {
      entries: BTreeMap::from([("useX".into(), ValueBagEntry::MethodForward("other".into()))]),
    };
    assert_eq!(resolve_pending_field(&ExportState::ValueFactory(bag), &["useX".into()], "a"), None);
  }

  #[test]
  fn method_forward_refines_to_method_factory_or_nested() {
    use super::super::{ValueBag, ValueBagEntry};
    assert_eq!(
      refine_method_forward(&empty_composable(), "useX".into()),
      ValueBagEntry::Method(super::super::ComposableShape::default())
    );
    assert_eq!(
      refine_method_forward(&factory_ref(), "useX".into()),
      ValueBagEntry::MethodFactory(ReactiveBindingKind::Ref)
    );
    let nested = ValueBag::default();
    assert_eq!(
      refine_method_forward(&ExportState::ValueFactory(nested.clone()), "useX".into()),
      ValueBagEntry::Nested(nested)
    );
    assert_eq!(
      refine_method_forward(&ExportState::ForwardReturn("x".into()), "useX".into()),
      ValueBagEntry::MethodForward("useX".into())
    );
  }

  #[test]
  fn generic_instantiate_promotes_non_empty_type_arg() {
    use super::super::{ValueBag, ValueBagEntry};
    let mut fields = BTreeMap::new();
    fields.insert("state".into(), ReactiveBindingKind::Ref);
    let shape = super::super::ComposableShape::from_fields(fields);
    let bag =
      ValueBag { entries: BTreeMap::from([("useInject".into(), ValueBagEntry::MethodGeneric(0))]) };
    let keep = ExportState::GenericMethodInstantiate {
      callee: "createContext".into(),
      property: "useInject".into(),
      type_arg_shapes: vec![shape.clone()],
    };
    let promoted = refine_generic_method_instantiate(
      Some(&ExportState::ValueFactory(bag)),
      "useInject",
      std::slice::from_ref(&shape),
      keep,
    );
    assert_eq!(promoted, ExportState::Composable(shape));
  }

  #[test]
  fn generic_instantiate_keeps_marker_until_callee_is_bag() {
    let shape = super::super::ComposableShape::default();
    let keep = ExportState::GenericMethodInstantiate {
      callee: "createContext".into(),
      property: "useInject".into(),
      type_arg_shapes: vec![shape.clone()],
    };
    let still = refine_generic_method_instantiate(None, "useInject", &[shape], keep.clone());
    assert_eq!(still, keep);
  }

  #[test]
  fn generic_instantiate_empty_shape_or_miss_is_ambiguous() {
    use super::super::{ValueBag, ValueBagEntry};
    let empty = super::super::ComposableShape::default();
    let bag =
      ValueBag { entries: BTreeMap::from([("useInject".into(), ValueBagEntry::MethodGeneric(0))]) };
    let keep = ExportState::GenericMethodInstantiate {
      callee: "c".into(),
      property: "useInject".into(),
      type_arg_shapes: vec![empty.clone()],
    };
    assert_eq!(
      refine_generic_method_instantiate(
        Some(&ExportState::ValueBag(bag.clone())),
        "useInject",
        &[empty],
        keep.clone(),
      ),
      ExportState::Ambiguous
    );
    assert_eq!(
      refine_generic_method_instantiate(Some(&ExportState::ValueBag(bag)), "missing", &[], keep,),
      ExportState::Ambiguous
    );
  }

  #[test]
  fn refine_value_bag_resolves_method_forwards() {
    use super::super::{ValueBag, ValueBagEntry};
    let bag = ValueBag {
      entries: BTreeMap::from([
        ("useX".into(), ValueBagEntry::MethodForward("useCount".into())),
        (
          "nested".into(),
          ValueBagEntry::Nested(ValueBag {
            entries: BTreeMap::from([(
              "useY".into(),
              ValueBagEntry::MethodForward("useCount".into()),
            )]),
          }),
        ),
      ]),
    };
    let refined = refine_value_bag(bag, |name| (name == "useCount").then_some(factory_ref()));
    let expected = ValueBag {
      entries: BTreeMap::from([
        ("useX".into(), ValueBagEntry::MethodFactory(ReactiveBindingKind::Ref)),
        (
          "nested".into(),
          ValueBagEntry::Nested(ValueBag {
            entries: BTreeMap::from([(
              "useY".into(),
              ValueBagEntry::MethodFactory(ReactiveBindingKind::Ref),
            )]),
          }),
        ),
      ]),
    };
    assert_eq!(refined, expected);
  }

  #[test]
  fn refine_value_bag_keeps_unresolved_forwards() {
    use super::super::{ValueBag, ValueBagEntry};
    let bag = ValueBag {
      entries: BTreeMap::from([("useX".into(), ValueBagEntry::MethodForward("missing".into()))]),
    };
    let refined = refine_value_bag(bag, |_| None);
    assert_eq!(refined.entries.get("useX"), Some(&ValueBagEntry::MethodForward("missing".into())));
  }

  #[test]
  fn refine_composable_pending_materializes_and_retains() {
    use super::super::PendingValueBagField;
    let mut shape = super::super::ComposableShape::default();
    shape.pending_value_bag_fields.insert(
      "isLoading".into(),
      PendingValueBagField { root: "useQuery".into(), path: vec![], field: "isLoading".into() },
    );
    shape.pending_value_bag_fields.insert(
      "later".into(),
      PendingValueBagField { root: "missing".into(), path: vec![], field: "x".into() },
    );
    let refined = refine_composable_pending(shape, |root| {
      (root == "useQuery").then_some(composable_with("isLoading", ReactiveBindingKind::Ref))
    });
    assert_eq!(refined.fields.get("isLoading"), Some(&ReactiveBindingKind::Ref));
    assert!(refined.pending_value_bag_fields.contains_key("later"));
    assert!(!refined.pending_value_bag_fields.contains_key("isLoading"));
  }

  #[test]
  fn as_publishable_gates_value_factory_call_and_provisional() {
    let bag = ExportState::ValueBag(super::super::ValueBag::default());
    assert_eq!(
      as_publishable(&ExportState::ValueFactoryCall("create".into()), Some(bag.clone())),
      Some(bag)
    );
    assert_eq!(
      as_publishable(&ExportState::ValueFactoryCall("create".into()), Some(factory_ref())),
      None
    );
    assert_eq!(as_publishable(&ExportState::ValueFactoryCall("create".into()), None), None);
    assert_eq!(as_publishable(&ExportState::Ambiguous, None), None);
    assert_eq!(
      as_publishable(
        &ExportState::GenericMethodInstantiate {
          callee: "c".into(),
          property: "p".into(),
          type_arg_shapes: vec![],
        },
        None
      ),
      None
    );
    assert_eq!(as_publishable(&factory_ref(), None), Some(factory_ref()));
  }

  #[test]
  fn value_factory_call_bag_only_accepts_value_factory() {
    let bag = super::super::ValueBag::default();
    assert!(value_factory_call_bag(Some(&ExportState::ValueFactory(bag.clone()))).is_some());
    assert!(value_factory_call_bag(Some(&ExportState::ValueBag(bag))).is_none());
    assert!(value_factory_call_bag(Some(&factory_ref())).is_none());
    assert!(value_factory_call_bag(None).is_none());
  }

  #[test]
  fn refine_forward_return_uses_resolved_or_keeps_marker() {
    assert_eq!(refine_forward_return(Some(factory_ref()), "useX".into()), factory_ref());
    assert_eq!(
      refine_forward_return(None, "useX".into()),
      ExportState::ForwardReturn("useX".into())
    );
  }

  #[test]
  fn decl_impl_plain_object_plus_unwrapped_becomes_reactive_factory() {
    let reactive = ExportState::Factory(ReactiveBindingKind::Reactive);
    assert_eq!(
      merge_declaration_implementation_local(
        Some(&ExportState::DeclaredPlainObjectFactory),
        &ExportState::BodyUnwrappedState,
      ),
      Some(reactive.clone())
    );
    assert_eq!(
      merge_declaration_implementation_local(
        Some(&ExportState::BodyUnwrappedState),
        &ExportState::DeclaredPlainObjectFactory,
      ),
      Some(reactive.clone())
    );
    assert_eq!(
      merge_declaration_implementation_local(
        Some(&ExportState::DeclaredPlainObjectFactory),
        &reactive,
      ),
      Some(reactive)
    );
  }

  #[test]
  fn decl_impl_provisional_takes_seedable_impl() {
    assert_eq!(
      merge_declaration_implementation_local(
        Some(&ExportState::DeclaredPlainObjectFactory),
        &factory_ref(),
      ),
      Some(factory_ref())
    );
    assert_eq!(
      merge_declaration_implementation_local(None, &empty_composable()),
      Some(empty_composable())
    );
  }

  #[test]
  fn decl_impl_forward_return_takes_factory_not_known() {
    assert_eq!(
      merge_declaration_implementation_local(
        Some(&ExportState::ForwardReturn("useX".into())),
        &factory_ref(),
      ),
      Some(factory_ref())
    );
    // Known / ValueBag do not complete a ForwardReturn declaration (under-approx).
    assert_eq!(
      merge_declaration_implementation_local(
        Some(&ExportState::ForwardReturn("useX".into())),
        &known_ref(),
      ),
      None
    );
    assert_eq!(
      merge_declaration_implementation_local(
        Some(&ExportState::ForwardReturn("useX".into())),
        &ExportState::ValueBag(super::super::ValueBag::default()),
      ),
      None
    );
  }

  #[test]
  fn decl_impl_orphan_provisional_half_is_kept() {
    assert_eq!(
      merge_declaration_implementation_local(None, &ExportState::BodyUnwrappedState),
      Some(ExportState::BodyUnwrappedState)
    );
  }

  #[test]
  fn decl_impl_unrelated_pair_stays_unchanged() {
    assert_eq!(merge_declaration_implementation_local(Some(&factory_ref()), &known_ref()), None);
  }
}
