//! Scan-stage progress events for host surfaces (CLI, tests).

use std::sync::Arc;

/// Pipeline progress events. Hosts choose how to present them.
///
/// Stage barriers mark coarse phases. [`Self::FileRules`] reports a completion
/// count for eligible rule files (parallel completion order). Callers must keep
/// displayed counts monotonic and must not treat this as a global percentage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProgressEvent {
  Discovering,
  CheckingCache,
  CacheHit,
  SavingCache,
  Parsing {
    pending: usize,
    reused: usize,
  },
  BuildingGraph,
  LoadingExternalSeeds {
    roots: usize,
  },
  RunningRules {
    files: usize,
  },
  /// One eligible rule file finished (`done` of `total`, completion order).
  FileRules {
    done: usize,
    total: usize,
  },
  WritingReport,
}

impl ProgressEvent {
  /// Human-readable stage line (without the `vue-vet:` prefix).
  #[must_use]
  pub fn message(&self) -> String {
    match self {
      Self::Discovering => "discovering workspace".into(),
      Self::CheckingCache => "checking cache".into(),
      Self::CacheHit => "cache hit".into(),
      Self::SavingCache => "saving cache".into(),
      Self::Parsing { pending, reused } => {
        format!("parsing {pending} file(s) ({reused} reused)")
      }
      Self::BuildingGraph => "building project graph".into(),
      Self::LoadingExternalSeeds { roots } => {
        format!("resolving dependencies ({roots} root(s))")
      }
      Self::RunningRules { files } => {
        format!("checking rules (0/{files} eligible files)")
      }
      Self::FileRules { done, total } => {
        format!("checking rules ({done}/{total} eligible files)")
      }
      Self::WritingReport => "writing report".into(),
    }
  }
}

/// Callback sink for [`ProgressEvent`] (CLI stderr, tests, etc.).
#[derive(Clone)]
pub struct ProgressReporter {
  sink: Arc<dyn Fn(&ProgressEvent) + Send + Sync>,
}

impl ProgressReporter {
  #[must_use]
  pub fn new(sink: impl Fn(&ProgressEvent) + Send + Sync + 'static) -> Self {
    Self { sink: Arc::new(sink) }
  }

  pub fn emit(&self, event: &ProgressEvent) {
    (self.sink)(event);
  }
}

impl std::fmt::Debug for ProgressReporter {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str("ProgressReporter(..)")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn progress_event_messages_are_stable() {
    assert_eq!(ProgressEvent::Discovering.message(), "discovering workspace");
    assert_eq!(
      ProgressEvent::Parsing { pending: 3, reused: 1 }.message(),
      "parsing 3 file(s) (1 reused)"
    );
    assert_eq!(
      ProgressEvent::LoadingExternalSeeds { roots: 2 }.message(),
      "resolving dependencies (2 root(s))"
    );
    assert_eq!(
      ProgressEvent::FileRules { done: 1, total: 2 }.message(),
      "checking rules (1/2 eligible files)"
    );
    assert_eq!(ProgressEvent::CacheHit.message(), "cache hit");
  }
}
