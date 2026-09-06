use serde::{Deserialize, Serialize};

use crate::diagnostics::SourceSpan;

/// Vue watcher API whose callback/cleanup contract is shared across aliases.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WatcherApiKind {
  Watch,
  WatchEffect,
  WatchPostEffect,
  WatchSyncEffect,
}

impl WatcherApiKind {
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Watch => "watch",
      Self::WatchEffect => "watchEffect",
      Self::WatchPostEffect => "watchPostEffect",
      Self::WatchSyncEffect => "watchSyncEffect",
    }
  }
}

/// Why `onWatcherCleanup` lost the active watcher.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LateWatcherCleanupKind {
  Await,
  DeferredCallback,
}

/// A watcher callback that returns a function Vue will not register as cleanup.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReturnedWatcherCleanupFact {
  pub api: WatcherApiKind,
  /// The returned function expression or identifier.
  pub returned_span: SourceSpan,
  pub callback_span: SourceSpan,
  pub registration_span: SourceSpan,
  #[serde(default, skip_serializing_if = "is_false")]
  pub async_callback: bool,
}

/// `onWatcherCleanup` after a proven await or deferred callback.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LateWatcherCleanupFact {
  pub api: WatcherApiKind,
  pub kind: LateWatcherCleanupKind,
  pub cleanup_span: SourceSpan,
  pub callback_span: SourceSpan,
  pub registration_span: SourceSpan,
  pub boundary_span: SourceSpan,
}

/// Watcher created after `await` inside a proven `effectScope().run` callback
/// whose stop handle is unused.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OrphanedScopeWatcherFact {
  pub api: WatcherApiKind,
  pub watcher_span: SourceSpan,
  pub run_span: SourceSpan,
  pub await_span: SourceSpan,
  pub owner_span: SourceSpan,
}

/// `onScopeDispose` after `await` in a proven async `effectScope().run` callback.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[expect(clippy::struct_field_names, reason = "each field is a distinct causal span")]
pub struct LateScopeDisposeFact {
  pub dispose_span: SourceSpan,
  pub callback_span: SourceSpan,
  pub run_span: SourceSpan,
  pub await_span: SourceSpan,
  pub owner_span: SourceSpan,
}

/// Domain facts for watcher/effect-scope lifetime contracts.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReactivityLifetimeFacts {
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub returned_watcher_cleanups: Vec<ReturnedWatcherCleanupFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub late_watcher_cleanups: Vec<LateWatcherCleanupFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub orphaned_scope_watchers: Vec<OrphanedScopeWatcherFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub late_scope_disposes: Vec<LateScopeDisposeFact>,
}

impl ReactivityLifetimeFacts {
  #[must_use]
  pub const fn is_empty(&self) -> bool {
    self.returned_watcher_cleanups.is_empty()
      && self.late_watcher_cleanups.is_empty()
      && self.orphaned_scope_watchers.is_empty()
      && self.late_scope_disposes.is_empty()
  }

  pub fn sort_by_source_order(&mut self) {
    self.returned_watcher_cleanups.sort_by_key(|fact| fact.returned_span.offset);
    self.late_watcher_cleanups.sort_by_key(|fact| fact.cleanup_span.offset);
    self.orphaned_scope_watchers.sort_by_key(|fact| fact.watcher_span.offset);
    self.late_scope_disposes.sort_by_key(|fact| fact.dispose_span.offset);
  }
}

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde skip_serializing_if takes &T")]
const fn is_false(value: &bool) -> bool {
  !*value
}
