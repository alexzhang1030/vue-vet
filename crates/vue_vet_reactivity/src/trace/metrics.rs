//! Test-only scan counters for import binding, composable usage, and summary prepare.
//!
//! Production builds keep the note functions as no-ops so call sites stay uncfg'd.

#[cfg(test)]
use std::cell::{Cell, RefCell};

/// Test-only count of the call-use walk after local composable definition collection.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ComposableUsageWork {
  pub definition_count: u64,
  pub usage_node_visits: u64,
}

/// Test-only count of the one summary-local `imported_bindings` index build.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SummaryScanWork {
  pub import_index_builds: u64,
  pub import_index_node_visits: u64,
}

#[cfg(test)]
thread_local! {
  static IMPORT_BINDING_BUILDS: Cell<u64> = const { Cell::new(0) };
  static IMPORT_BINDING_NODE_VISITS: Cell<u64> = const { Cell::new(0) };
  static LAST_COMPOSABLE_USAGE_WORK: RefCell<ComposableUsageWork> =
    const { RefCell::new(ComposableUsageWork { definition_count: 0, usage_node_visits: 0 }) };
  static LAST_SUMMARY_SCAN_WORK: RefCell<SummaryScanWork> =
    const { RefCell::new(SummaryScanWork { import_index_builds: 0, import_index_node_visits: 0 }) };
}

#[cfg(test)]
pub fn import_binding_collect_snapshot() -> (u64, u64) {
  (IMPORT_BINDING_BUILDS.with(Cell::get), IMPORT_BINDING_NODE_VISITS.with(Cell::get))
}

#[cfg(test)]
pub(super) fn note_import_binding_build() {
  IMPORT_BINDING_BUILDS.with(|slot| slot.set(slot.get().saturating_add(1)));
}

#[cfg(test)]
pub(super) fn note_import_binding_visit() {
  IMPORT_BINDING_NODE_VISITS.with(|slot| slot.set(slot.get().saturating_add(1)));
}

#[cfg(test)]
pub fn last_composable_usage_work() -> ComposableUsageWork {
  LAST_COMPOSABLE_USAGE_WORK.with(|slot| *slot.borrow())
}

#[cfg(test)]
pub(super) fn store_composable_usage_work(work: ComposableUsageWork) {
  LAST_COMPOSABLE_USAGE_WORK.with(|slot| *slot.borrow_mut() = work);
}

#[cfg(test)]
pub fn last_summary_scan_work() -> SummaryScanWork {
  LAST_SUMMARY_SCAN_WORK.with(|slot| *slot.borrow())
}

#[cfg(test)]
pub(super) fn store_summary_scan_work(work: SummaryScanWork) {
  LAST_SUMMARY_SCAN_WORK.with(|slot| *slot.borrow_mut() = work);
}
