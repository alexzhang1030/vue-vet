//! Read / write path formatting and control-flow classification over facts.

use vue_vet_core::{
  ReactiveBindingKind, ReactiveGuardFact, ReactiveReadFact, ReactiveReadKind, ReactiveWriteFact,
  TrackingScopeFact, TrackingScopeKind,
};

/// `binding` or `binding.property`.
#[must_use]
pub fn member_path(binding: &str, property: Option<&str>) -> String {
  property.map_or_else(|| binding.to_owned(), |property| format!("{binding}.{property}"))
}

#[must_use]
pub fn binding_path(read: &ReactiveReadFact) -> String {
  member_path(&read.binding, read.property.as_deref())
}

#[must_use]
pub fn write_path(write: &ReactiveWriteFact) -> String {
  member_path(&write.binding, write.property.as_deref())
}

#[must_use]
pub fn guard_path(guard: &ReactiveGuardFact) -> String {
  member_path(&guard.binding, guard.property.as_deref())
}

#[must_use]
pub fn same_target(read: &ReactiveReadFact, write: &ReactiveWriteFact) -> bool {
  read.binding == write.binding && read.property == write.property
}

/// Earlier unconditional read of the same `(binding, property)` in `reads`.
///
/// Conditional-dependency rules skip a Conditional read when this is true so
/// an earlier hard read already established the dependency.
#[must_use]
pub fn has_prior_unconditional_read(reads: &[ReactiveReadFact], read: &ReactiveReadFact) -> bool {
  reads.iter().any(|candidate| {
    candidate.kind == ReactiveReadKind::Unconditional
      && candidate.span.offset < read.span.offset
      && candidate.binding == read.binding
      && candidate.property == read.property
  })
}

/// Conditional reads that are not preceded by an unconditional same-target read.
pub fn unguarded_conditional_reads(
  reads: &[ReactiveReadFact],
) -> impl Iterator<Item = &ReactiveReadFact> {
  reads.iter().filter(|read| {
    read.kind == ReactiveReadKind::Conditional && !has_prior_unconditional_read(reads, read)
  })
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
pub const fn effect_family(kind: TrackingScopeKind) -> bool {
  kind.is_effect_family()
}

#[must_use]
pub const fn is_readonly_kind(kind: ReactiveBindingKind) -> bool {
  matches!(kind, ReactiveBindingKind::Readonly | ReactiveBindingKind::ShallowReadonly)
}
