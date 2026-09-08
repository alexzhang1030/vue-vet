//! Lost-notification source/view/path collection (one pass per file).

mod joins;
mod paths;
mod provenance;
mod uses;

#[cfg(test)]
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use oxc_semantic::Semantic;
use vue_vet_core::{ReactivityGraph, ScriptKind};

use super::follow::FileTraceIndex;
#[cfg(test)]
pub use uses::NotificationWork;
use uses::{WorkCounter, build_owner_index, module_has_notification_source};

#[cfg(test)]
thread_local! {
  static LAST_WORK: RefCell<NotificationWork> = const { RefCell::new(NotificationWork::zero()) };
  static FORCE_FULL: Cell<bool> = const { Cell::new(false) };
}

#[cfg(test)]
pub fn last_notification_work() -> NotificationWork {
  LAST_WORK.with(|slot| *slot.borrow())
}

#[cfg(test)]
pub fn with_forced_full_notification<R>(f: impl FnOnce() -> R) -> R {
  struct Reset;
  impl Drop for Reset {
    fn drop(&mut self) {
      FORCE_FULL.with(|slot| slot.set(false));
    }
  }
  let _reset = Reset;
  FORCE_FULL.with(|slot| slot.set(true));
  f()
}

#[cfg(test)]
fn force_full_notification() -> bool {
  FORCE_FULL.with(Cell::get)
}

#[cfg(not(test))]
const fn force_full_notification() -> bool {
  false
}

#[cfg(test)]
fn store_last_work(work: &WorkCounter) {
  LAST_WORK.with(|slot| {
    *slot.borrow_mut() = work.snapshot();
  });
}

pub(super) fn collect_notification_facts(
  semantic: &Semantic<'_>,
  imported_bindings: &BTreeMap<String, (String, String)>,
  file_index: &FileTraceIndex,
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
  graph: &mut ReactivityGraph,
) {
  let work = WorkCounter::default();
  if !force_full_notification()
    && !module_has_notification_source(semantic, imported_bindings, &work)
  {
    #[cfg(test)]
    store_last_work(&work);
    return;
  }
  let owners = build_owner_index(semantic, imported_bindings, script_kind, &work);
  let mut provenance = provenance::collect_provenance(
    semantic,
    imported_bindings,
    &owners,
    sfc_source,
    script_offset,
    script_kind,
    &work,
  );
  graph.source_views = provenance.records.iter().map(provenance::SourceRecord::to_fact).collect();
  graph.source_views.sort_by_key(|fact| fact.binding_span.offset);
  graph.notification_bypasses = joins::join_bypasses(
    semantic,
    imported_bindings,
    &mut provenance,
    &owners,
    file_index,
    &graph.scopes,
    sfc_source,
    script_offset,
    script_kind,
    &work,
  );
  #[cfg(test)]
  store_last_work(&work);
}
