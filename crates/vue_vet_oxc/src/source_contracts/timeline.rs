//! Sorted byte-offset timelines and the one split algorithm behind every
//! "first after / last before / between" query in the source-contract lanes.
//!
//! Every site index is sorted by offset once in `Indexes::build`; queries are
//! `partition_point` splits, never linear scans. `after` / `from` / `before` /
//! `through` are the four half-open cuts; `between` is exclusive on both
//! ends. Duplicated offsets are allowed and keep their relative order.

use super::stats::WorkCounter;

/// A fact that sits at one byte offset in source order.
pub(super) trait Timed {
  fn at(&self) -> usize;
}

impl Timed for usize {
  fn at(&self) -> usize {
    *self
  }
}

/// Index of the first item with offset `> pos`.
fn split_after<T: Timed>(work: &WorkCounter, items: &[T], pos: usize) -> usize {
  work.partition_point(items, |item| item.at() <= pos)
}

/// Index of the first item with offset `>= pos`.
fn split_at<T: Timed>(work: &WorkCounter, items: &[T], pos: usize) -> usize {
  work.partition_point(items, |item| item.at() < pos)
}

/// Items with offset `> pos`.
pub(super) fn after<'a, T: Timed>(work: &WorkCounter, items: &'a [T], pos: usize) -> &'a [T] {
  items.get(split_after(work, items, pos)..).unwrap_or(&[])
}

/// Items with offset `>= pos`.
pub(super) fn from<'a, T: Timed>(work: &WorkCounter, items: &'a [T], pos: usize) -> &'a [T] {
  items.get(split_at(work, items, pos)..).unwrap_or(&[])
}

/// Items with offset `< pos`.
pub(super) fn before<'a, T: Timed>(work: &WorkCounter, items: &'a [T], pos: usize) -> &'a [T] {
  items.get(..split_at(work, items, pos)).unwrap_or(&[])
}

/// Items with offset `<= pos`.
pub(super) fn through<'a, T: Timed>(work: &WorkCounter, items: &'a [T], pos: usize) -> &'a [T] {
  items.get(..split_after(work, items, pos)).unwrap_or(&[])
}

/// Items with `lo < offset < hi`. Empty when `hi <= lo`.
pub(super) fn between<'a, T: Timed>(
  work: &WorkCounter,
  items: &'a [T],
  lo: usize,
  hi: usize,
) -> &'a [T] {
  if hi <= lo {
    work.add_queries(1);
    return &[];
  }
  let start = split_after(work, items, lo);
  let end = split_at(work, items, hi);
  items.get(start..end).unwrap_or(&[])
}

/// First item with offset `> pos`.
pub(super) fn first_after<'a, T: Timed>(
  work: &WorkCounter,
  items: &'a [T],
  pos: usize,
) -> Option<&'a T> {
  after(work, items, pos).first()
}

/// Last item with offset `< pos`.
pub(super) fn last_before<'a, T: Timed>(
  work: &WorkCounter,
  items: &'a [T],
  pos: usize,
) -> Option<&'a T> {
  before(work, items, pos).last()
}

/// Sorted, deduplicated byte offsets. Appended unsorted during the scan and
/// sealed once by [`Timeline::sort`]; queries assume the sorted invariant.
#[derive(Clone, Debug, Default)]
pub(super) struct Timeline {
  offsets: Vec<usize>,
}

impl Timeline {
  pub(super) const fn new() -> Self {
    Self { offsets: Vec::new() }
  }

  pub(super) fn push(&mut self, offset: usize) {
    self.offsets.push(offset);
  }

  pub(super) fn extend_from(&mut self, other: &Self) {
    self.offsets.extend_from_slice(&other.offsets);
  }

  pub(super) fn reserve(&mut self, additional: usize) {
    self.offsets.reserve(additional);
  }

  pub(super) const fn len(&self) -> usize {
    self.offsets.len()
  }

  /// Counted sort plus dedup. Call once after the scan; queries before this
  /// are undefined.
  pub(super) fn sort(&mut self, work: &WorkCounter) {
    work.sort_by_key(&mut self.offsets, |offset| *offset);
    self.offsets.dedup();
  }

  /// True when some offset sits strictly inside `(lo, hi)`.
  pub(super) fn has_between(&self, work: &WorkCounter, lo: usize, hi: usize) -> bool {
    !between(work, &self.offsets, lo, hi).is_empty()
  }

  /// Offsets strictly inside `(lo, hi)`.
  pub(super) fn between(&self, work: &WorkCounter, lo: usize, hi: usize) -> &[usize] {
    between(work, &self.offsets, lo, hi)
  }

  pub(super) fn count_between(&self, work: &WorkCounter, lo: usize, hi: usize) -> usize {
    self.between(work, lo, hi).len()
  }

  /// First offset `> pos`.
  pub(super) fn first_after(&self, work: &WorkCounter, pos: usize) -> Option<usize> {
    first_after(work, &self.offsets, pos).copied()
  }
}

#[cfg(test)]
mod tests {
  use super::{Timeline, after, before, between, first_after, from, last_before, through};
  use crate::source_contracts::stats::WorkCounter;

  fn timeline(offsets: &[usize]) -> Timeline {
    let mut timeline = Timeline::new();
    for offset in offsets {
      timeline.push(*offset);
    }
    timeline.sort(&WorkCounter::default());
    timeline
  }

  #[test]
  fn empty_timeline_answers_nothing() {
    let work = WorkCounter::default();
    let empty = timeline(&[]);
    assert!(!empty.has_between(&work, 0, usize::MAX));
    assert_eq!(empty.first_after(&work, 0), None);
    assert_eq!(last_before(&work, &[] as &[usize], usize::MAX), None);
    assert_eq!(empty.count_between(&work, 0, 10), 0);
  }

  #[test]
  fn single_offset_boundaries_are_exclusive() {
    let work = WorkCounter::default();
    let one = timeline(&[5]);
    assert!(one.has_between(&work, 4, 6));
    assert!(!one.has_between(&work, 5, 6), "lo is exclusive");
    assert!(!one.has_between(&work, 4, 5), "hi is exclusive");
    assert!(!one.has_between(&work, 6, 4), "inverted window is empty");
    assert_eq!(one.first_after(&work, 4), Some(5));
    assert_eq!(one.first_after(&work, 5), None);
    assert_eq!(last_before(&work, &[5usize], 6), Some(&5));
    assert_eq!(last_before(&work, &[5usize], 5), None);
  }

  #[test]
  fn unsorted_input_with_duplicates_is_sorted_and_deduped() {
    let work = WorkCounter::default();
    let many = timeline(&[9, 3, 7, 3, 9, 1]);
    assert_eq!(many.len(), 4);
    assert_eq!(many.between(&work, 1, 9), &[3, 7]);
    assert_eq!(many.count_between(&work, 0, 10), 4);
    assert_eq!(many.first_after(&work, 3), Some(7));
    assert_eq!(many.first_after(&work, 0), Some(1));
    assert_eq!(many.first_after(&work, 9), None);
  }

  #[test]
  fn slice_cuts_keep_partition_point_semantics_with_duplicates() {
    let work = WorkCounter::default();
    let items = [1usize, 3, 3, 5];
    assert_eq!(after(&work, &items, 3), &[5]);
    assert_eq!(from(&work, &items, 3), &[3, 3, 5]);
    assert_eq!(before(&work, &items, 3), &[1]);
    assert_eq!(through(&work, &items, 3), &[1, 3, 3]);
    assert_eq!(between(&work, &items, 1, 5), &[3, 3]);
    assert_eq!(between(&work, &items, 3, 3), &[] as &[usize]);
    assert_eq!(first_after(&work, &items, 5), None);
    assert_eq!(last_before(&work, &items, 1), None);
    assert_eq!(after(&work, &items, 100), &[] as &[usize]);
    assert_eq!(before(&work, &items, 0), &[] as &[usize]);
  }
}
