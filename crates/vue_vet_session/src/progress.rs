//! Coarse scan-stage progress for interactive CLI runs.

use std::sync::Arc;

/// One barrier in the analysis pipeline (stderr streaming; not per-file).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProgressEvent {
  Discovering,
  Parsing { pending: usize, reused: usize },
  BuildingGraph,
  LoadingExternalSeeds { roots: usize },
  RunningRules { files: usize },
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
      "loading external seeds (2 root(s), prefer .d.ts)"
    );
  }
}
