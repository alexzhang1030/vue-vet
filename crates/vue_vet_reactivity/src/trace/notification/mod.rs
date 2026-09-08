//! Lost-notification source/view/path collection (one pass per file).

mod joins;
mod paths;
mod provenance;
mod uses;

use std::cell::RefCell;
use std::collections::BTreeMap;

use oxc_semantic::Semantic;
use vue_vet_core::{ReactivityGraph, ScriptKind};

use super::follow::FileTraceIndex;
pub use uses::NotificationWork;
use uses::build_owner_index;

thread_local! {
  static LAST_WORK: RefCell<NotificationWork> = const { RefCell::new(NotificationWork::zero()) };
}

#[cfg(test)]
pub fn last_notification_work() -> NotificationWork {
  LAST_WORK.with(|slot| *slot.borrow())
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
  let mut work = NotificationWork::zero();
  let owners = build_owner_index(semantic, imported_bindings, script_kind, &mut work);
  let mut provenance = provenance::collect_provenance(
    semantic,
    imported_bindings,
    &owners,
    sfc_source,
    script_offset,
    script_kind,
    &mut work,
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
    &mut work,
  );
  LAST_WORK.with(|slot| {
    *slot.borrow_mut() = work;
  });
}
