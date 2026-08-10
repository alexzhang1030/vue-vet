//! Pure A6 [`ExportState`] lattice operations (no AST).
//!
//! Contract: [reactivity tracer PCR](../../../../../../.agents/docs/reactivity-tracer.md)
//! — seedable vs provisional, local merge, name resolve, publish barrier.

use std::collections::BTreeMap;

use vue_vet_core::ModuleId;

use super::ExportState;

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
}
