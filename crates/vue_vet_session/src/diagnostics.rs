use std::{collections::BTreeMap, sync::Arc};

use vue_vet_config::{Config, apply_suppressions};
use vue_vet_core::{Diagnostic, FileId, ScanSummary};

/// Applies every diagnostic policy in one deterministic final pass.
pub struct DiagnosticFinalizer<'a> {
  config: &'a Config,
  sources: BTreeMap<FileId, Arc<str>>,
}

impl<'a> DiagnosticFinalizer<'a> {
  pub fn new(config: &'a Config, sources: impl IntoIterator<Item = (FileId, Arc<str>)>) -> Self {
    Self { config, sources: sources.into_iter().collect() }
  }

  pub fn finalize(&self, files_scanned: usize, diagnostics: Vec<Diagnostic>) -> ScanSummary {
    let (analysis_issues, configurable): (Vec<_>, Vec<_>) =
      diagnostics.into_iter().partition(|diagnostic| diagnostic.category == "analysis");
    let configured = self.config.apply(configurable);
    let mut by_file = BTreeMap::<FileId, Vec<Diagnostic>>::new();
    for diagnostic in configured {
      by_file.entry(diagnostic.file.clone()).or_default().push(diagnostic);
    }

    let mut finalized = analysis_issues;
    for (file, source) in &self.sources {
      let diagnostics = by_file.remove(file).unwrap_or_default();
      finalized.extend(apply_suppressions(file.as_path(), source, diagnostics));
    }
    finalized.extend(by_file.into_values().flatten());
    finalized.sort_by(|left, right| {
      (&left.file, left.span.offset, &left.rule_id, &left.message).cmp(&(
        &right.file,
        right.span.offset,
        &right.rule_id,
        &right.message,
      ))
    });
    finalized.dedup();
    ScanSummary { files_scanned, diagnostics: finalized, score: 0 }.finish()
  }
}
