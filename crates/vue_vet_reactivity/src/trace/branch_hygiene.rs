//! Pure A4 branch hygiene (no AST).
//!
//! Contract: [reactivity tracer PCR](../../../../.agents/docs/reactivity-tracer.md)
//! — all-path same-identity branch reads are **not** Conditional / `BranchTest`.
//! Under-approx: missing arm coverage stays Conditional; never invent Unconditional.

/// Borrowed view of one reactive read for branch-pair coverage (no AST).
pub(super) trait BranchReadView {
  fn binding(&self) -> &str;
  /// Static member path segment (`Some("value")`, …) or bare root `None`.
  fn property(&self) -> Option<&str>;
  fn span_start(&self) -> u32;
  fn span_end(&self) -> u32;
  /// Outside-tracking reads never prove all-path coverage.
  fn outside_tracking(&self) -> bool;
}

/// Inclusive-ish byte span using Oxc's half-open `[start, end)` convention.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SpanRange {
  pub start: u32,
  pub end: u32,
}

/// Whether `outer` fully contains `inner` (same rule as local span helpers).
#[must_use]
pub(super) const fn span_contains(outer: SpanRange, inner: SpanRange) -> bool {
  outer.start <= inner.start && outer.end >= inner.end
}

/// True when both arms of a branch pair contain a same-identity in-tracking read.
///
/// Used so `cond ? x.value : x.value` / `if (c) x.value; else x.value` do not
/// attach [`vue_vet_core::ReactiveGuardRole::BranchTest`] — every path still tracks `x`.
///
/// Under-approx:
/// - Missing `right` arm (if-without-else) → `false`
/// - Outside-tracking occurrences do not count
/// - Binding **and** property must match (`x` ≠ `x.value`)
#[must_use]
pub(super) fn branch_pair_covers_read<R: BranchReadView>(
  reads: &[R],
  target_binding: &str,
  target_property: Option<&str>,
  left: SpanRange,
  right: Option<SpanRange>,
) -> bool {
  let Some(right) = right else {
    return false;
  };
  let matches_id = |candidate: &R| {
    !candidate.outside_tracking()
      && candidate.binding() == target_binding
      && candidate.property() == target_property
  };
  let occurrence_span =
    |candidate: &R| SpanRange { start: candidate.span_start(), end: candidate.span_end() };
  let in_left = reads
    .iter()
    .any(|candidate| matches_id(candidate) && span_contains(left, occurrence_span(candidate)));
  let in_right = reads
    .iter()
    .any(|candidate| matches_id(candidate) && span_contains(right, occurrence_span(candidate)));
  in_left && in_right
}

#[cfg(test)]
mod tests {
  use super::*;

  #[derive(Clone, Copy)]
  struct TestRead {
    binding: &'static str,
    property: Option<&'static str>,
    start: u32,
    end: u32,
    outside: bool,
  }

  impl BranchReadView for TestRead {
    fn binding(&self) -> &str {
      self.binding
    }
    fn property(&self) -> Option<&str> {
      self.property
    }
    fn span_start(&self) -> u32 {
      self.start
    }
    fn span_end(&self) -> u32 {
      self.end
    }
    fn outside_tracking(&self) -> bool {
      self.outside
    }
  }

  fn read(
    binding: &'static str,
    property: Option<&'static str>,
    start: u32,
    end: u32,
    outside: bool,
  ) -> TestRead {
    TestRead { binding, property, start, end, outside }
  }

  #[test]
  fn span_contains_half_open_semantics() {
    let outer = SpanRange { start: 0, end: 20 };
    assert!(span_contains(outer, SpanRange { start: 0, end: 20 }));
    assert!(span_contains(outer, SpanRange { start: 5, end: 10 }));
    assert!(!span_contains(outer, SpanRange { start: 0, end: 21 }));
    assert!(!span_contains(outer, SpanRange { start: 18, end: 25 }));
  }

  #[test]
  fn both_arms_same_identity_covers() {
    let reads = [read("x", Some("value"), 10, 17, false), read("x", Some("value"), 30, 37, false)];
    assert!(branch_pair_covers_read(
      &reads,
      "x",
      Some("value"),
      SpanRange { start: 5, end: 20 },
      Some(SpanRange { start: 25, end: 40 }),
    ));
  }

  #[test]
  fn missing_else_arm_does_not_cover() {
    let reads = [read("x", Some("value"), 10, 17, false)];
    assert!(!branch_pair_covers_read(
      &reads,
      "x",
      Some("value"),
      SpanRange { start: 5, end: 20 },
      None,
    ));
  }

  #[test]
  fn only_one_arm_has_read_does_not_cover() {
    let reads = [read("x", Some("value"), 10, 17, false)];
    assert!(!branch_pair_covers_read(
      &reads,
      "x",
      Some("value"),
      SpanRange { start: 5, end: 20 },
      Some(SpanRange { start: 25, end: 40 }),
    ));
  }

  #[test]
  fn different_property_is_not_same_identity() {
    // Bare `x` on one arm and `x.value` on the other must not invent coverage.
    let reads = [read("x", None, 10, 11, false), read("x", Some("value"), 30, 37, false)];
    assert!(!branch_pair_covers_read(
      &reads,
      "x",
      Some("value"),
      SpanRange { start: 5, end: 20 },
      Some(SpanRange { start: 25, end: 40 }),
    ));
    assert!(!branch_pair_covers_read(
      &reads,
      "x",
      None,
      SpanRange { start: 5, end: 20 },
      Some(SpanRange { start: 25, end: 40 }),
    ));
  }

  #[test]
  fn different_binding_does_not_cover() {
    let reads = [read("a", Some("value"), 10, 17, false), read("b", Some("value"), 30, 37, false)];
    assert!(!branch_pair_covers_read(
      &reads,
      "a",
      Some("value"),
      SpanRange { start: 5, end: 20 },
      Some(SpanRange { start: 25, end: 40 }),
    ));
  }

  #[test]
  fn outside_tracking_reads_do_not_prove_coverage() {
    let reads = [read("x", Some("value"), 10, 17, true), read("x", Some("value"), 30, 37, false)];
    assert!(!branch_pair_covers_read(
      &reads,
      "x",
      Some("value"),
      SpanRange { start: 5, end: 20 },
      Some(SpanRange { start: 25, end: 40 }),
    ));
  }

  #[test]
  fn bare_root_reads_match_only_bare() {
    let reads = [read("flag", None, 10, 14, false), read("flag", None, 30, 34, false)];
    assert!(branch_pair_covers_read(
      &reads,
      "flag",
      None,
      SpanRange { start: 5, end: 20 },
      Some(SpanRange { start: 25, end: 40 }),
    ));
  }
}
