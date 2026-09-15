//! Read / write path formatting and control-flow classification over facts.

use std::fmt;

use vue_vet_core::{
  ReactiveBindingFact, ReactiveBindingKind, ReactiveReadFact, ReactiveWriteFact, TrackingScopeKind,
};

/// `binding` or `binding.property`, borrowed from the fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemberPath<'a> {
  binding: &'a str,
  property: Option<&'a str>,
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

/// Root name for `const alias = known` (`alias_of`), otherwise `name`.
#[must_use]
pub fn alias_root<'a>(bindings: &'a [ReactiveBindingFact], name: &'a str) -> &'a str {
  bindings
    .iter()
    .find(|binding| binding.name == name)
    .and_then(|binding| binding.alias_of.as_deref())
    .unwrap_or(name)
}

/// Canonical writer identity: alias writes join the Oxc-resolved root span.
/// Same-name locals in different functions stay distinct via `binding_span`.
#[must_use]
pub fn canonical_write_identity<'a>(
  bindings: &'a [ReactiveBindingFact],
  write: &'a ReactiveWriteFact,
) -> (usize, &'a str, Option<&'a str>) {
  let written = bindings.iter().find(|binding| {
    write.binding_span.map_or(binding.name == write.binding, |span| {
      binding.name == write.binding && binding.span.offset == span.offset
    })
  });
  let root =
    written.and_then(|binding| binding.alias_of.as_deref()).unwrap_or(write.binding.as_str());
  let identity = written
    .and_then(|binding| {
      binding
        .alias_of_span
        .map(|span| span.offset)
        .or_else(|| unique_root_span(bindings, binding.alias_of.as_deref()))
        .or(Some(binding.span.offset))
    })
    .or_else(|| write.binding_span.map(|span| span.offset))
    .unwrap_or(write.span.offset);
  (identity, root, write.property.as_deref())
}

fn unique_root_span(bindings: &[ReactiveBindingFact], root_name: Option<&str>) -> Option<usize> {
  let root_name = root_name?;
  let mut matches =
    bindings.iter().filter(|candidate| candidate.name == root_name && candidate.alias_of.is_none());
  let first = matches.next()?;
  matches.next().is_none().then_some(first.span.offset)
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

#[must_use]
pub const fn effect_family(kind: TrackingScopeKind) -> bool {
  kind.is_effect_family()
}

#[must_use]
pub const fn is_readonly_kind(kind: ReactiveBindingKind) -> bool {
  matches!(kind, ReactiveBindingKind::Readonly | ReactiveBindingKind::ShallowReadonly)
}
