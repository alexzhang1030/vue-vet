//! Scan-stage progress and per-file result streaming for interactive CLI runs.

use std::sync::Arc;

use vue_vet_core::Diagnostic;

/// Pipeline progress / stream events (stderr stages; optional per-file results).
///
/// Stage barriers (`Discovering` … `WritingReport`) mark coarse phases.
/// [`Self::FileRules`] is the real **stream**: one emission per file when its
/// rule pass finishes (completion order under parallelism).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProgressEvent {
  Discovering,
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
  /// One file finished the rules stage (`done` of `total`, completion order).
  FileRules {
    path: String,
    done: usize,
    total: usize,
    /// Config + suppression finalized diagnostics for this file only.
    diagnostics: Arc<[Diagnostic]>,
  },
  WritingReport,
}

impl ProgressEvent {
  /// Human-readable stage line (without the `vue-vet:` prefix).
  #[must_use]
  pub fn message(&self) -> String {
    match self {
      Self::Discovering => "discovering workspace".into(),
      Self::Parsing { pending, reused } => {
        format!("parsing {pending} file(s) ({reused} reused)")
      }
      Self::BuildingGraph => "building project graph".into(),
      Self::LoadingExternalSeeds { roots } => {
        format!("loading external seeds ({roots} root(s), prefer .d.ts)")
      }
      Self::RunningRules { files } => format!("running rules ({files} file(s))"),
      Self::FileRules { path, done, total, .. } => {
        format!("analyzed {path} ({done}/{total})")
      }
      Self::WritingReport => "writing report".into(),
    }
  }
}

/// Callback sink for [`ProgressEvent`] (CLI stderr/stdout stream, tests, etc.).
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
      "loading external seeds (2 root(s), prefer .d.ts)"
    );
    assert_eq!(
      ProgressEvent::FileRules {
        path: "src/App.vue".into(),
        done: 1,
        total: 2,
        diagnostics: Arc::from([]),
      }
      .message(),
      "analyzed src/App.vue (1/2)"
    );
  }
}
