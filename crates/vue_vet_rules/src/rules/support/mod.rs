//! Shared detection helpers for built-in rules (not rules themselves).

use vue_vet_core::{
  ReactiveBindingKind, ReactiveReadFact, ReactiveReadKind, ReactiveWriteFact, TrackingScopeFact,
  TrackingScopeKind,
};

#[must_use]
pub const fn effect_family(kind: TrackingScopeKind) -> bool {
  kind.is_effect_family()
}

#[must_use]
pub fn binding_path(read: &ReactiveReadFact) -> String {
  read
    .property
    .as_ref()
    .map_or_else(|| read.binding.clone(), |property| format!("{}.{property}", read.binding))
}

#[must_use]
pub fn write_path(write: &ReactiveWriteFact) -> String {
  write
    .property
    .as_ref()
    .map_or_else(|| write.binding.clone(), |property| format!("{}.{property}", write.binding))
}

#[must_use]
pub fn same_target(read: &ReactiveReadFact, write: &ReactiveWriteFact) -> bool {
  read.binding == write.binding && read.property == write.property
}

#[must_use]
pub fn unconditional_self_triggers(scope: &TrackingScopeFact) -> Vec<&ReactiveReadFact> {
  let mut hits = Vec::new();
  for read in &scope.reads {
    if read.kind != ReactiveReadKind::Unconditional {
      continue;
    }
    if scope.writes.iter().any(|write| same_target(read, write)) {
      hits.push(read);
    }
  }
  hits
}

#[must_use]
pub const fn is_readonly_kind(kind: ReactiveBindingKind) -> bool {
  matches!(kind, ReactiveBindingKind::Readonly | ReactiveBindingKind::ShallowReadonly)
}
