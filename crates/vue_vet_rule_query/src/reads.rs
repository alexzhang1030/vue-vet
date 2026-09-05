//! Read / write path formatting and control-flow classification over facts.

use std::fmt;

use vue_vet_core::{
  ReactiveBindingFact, ReactiveBindingKind, ReactiveGuardFact, ReactiveReadFact, ReactiveReadKind,
  ReactiveWriteFact, TrackingScopeFact, TrackingScopeKind,
};

/// `binding` or `binding.property`, borrowed from the fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemberPath<'a> {
  binding: &'a str,
  property: Option<&'a str>,
}

impl MemberPath<'_> {
  fn push_to(self, out: &mut String) {
    out.push_str(self.binding);
    if let Some(property) = self.property {
      out.push('.');
      out.push_str(property);
    }
  }
}

impl fmt::Display for MemberPath<'_> {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(self.binding)?;
    if let Some(property) = self.property {
      formatter.write_str(".")?;
      formatter.write_str(property)?;
    }
    Ok(())
  }
}

impl PartialEq<str> for MemberPath<'_> {
  fn eq(&self, other: &str) -> bool {
    self.property.map_or_else(
      || self.binding == other,
      |property| {
        other.len() == self.binding.len() + 1 + property.len()
          && other.starts_with(self.binding)
          && other.as_bytes().get(self.binding.len()) == Some(&b'.')
          && other.ends_with(property)
      },
    )
  }
}

impl PartialEq<&str> for MemberPath<'_> {
  fn eq(&self, other: &&str) -> bool {
    *self == **other
  }
}

#[must_use]
pub const fn member_path<'a>(binding: &'a str, property: Option<&'a str>) -> MemberPath<'a> {
  MemberPath { binding, property }
}

#[must_use]
pub fn binding_path(read: &ReactiveReadFact) -> MemberPath<'_> {
  member_path(&read.binding, read.property.as_deref())
}

#[must_use]
pub fn write_path(write: &ReactiveWriteFact) -> MemberPath<'_> {
  member_path(&write.binding, write.property.as_deref())
}

#[must_use]
pub fn guard_path(guard: &ReactiveGuardFact) -> MemberPath<'_> {
  member_path(&guard.binding, guard.property.as_deref())
}

/// Join borrowed paths into one owned string (for messages that list several).
#[must_use]
pub fn join_member_paths<'a>(paths: impl IntoIterator<Item = MemberPath<'a>>, sep: &str) -> String {
  let mut out = String::new();
  for (index, path) in paths.into_iter().enumerate() {
    if index > 0 {
      out.push_str(sep);
    }
    path.push_to(&mut out);
  }
  out
}

#[must_use]
pub fn same_target(read: &ReactiveReadFact, write: &ReactiveWriteFact) -> bool {
  read.binding == write.binding && read.property == write.property
}

/// Root name for `const alias = known` (`alias_of`), otherwise `name`.
#[must_use]
pub fn alias_root<'a>(bindings: &'a [ReactiveBindingFact], name: &'a str) -> &'a str {
  bindings
    .iter()
    .find(|binding| binding.name == name)
    .and_then(|binding| binding.alias_of.as_deref())
    .unwrap_or(name)
}

/// Same reactive source after alias resolution (`const alias = count`).
#[must_use]
pub fn same_reactive_target(
  bindings: &[ReactiveBindingFact],
  read: &ReactiveReadFact,
  write: &ReactiveWriteFact,
) -> bool {
  alias_root(bindings, &read.binding) == alias_root(bindings, &write.binding)
    && read.property == write.property
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

pub fn unconditional_self_triggers(
  scope: &TrackingScopeFact,
) -> impl Iterator<Item = &ReactiveReadFact> {
  scope.reads.iter().filter(|read| {
    read.kind == ReactiveReadKind::Unconditional
      && scope.writes.iter().any(|write| same_target(read, write))
  })
}

#[must_use]
pub const fn effect_family(kind: TrackingScopeKind) -> bool {
  kind.is_effect_family()
}

#[must_use]
pub const fn is_readonly_kind(kind: ReactiveBindingKind) -> bool {
  matches!(kind, ReactiveBindingKind::Readonly | ReactiveBindingKind::ShallowReadonly)
}
