//! Pure A6 [`ExportState`] lattice operations (no AST).
//!
//! Contract: [reactivity tracer PCR](../../../../../../.agents/docs/reactivity-tracer.md)
//! — seedable vs provisional, local merge, publish barrier.

use super::ExportState;

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
}
