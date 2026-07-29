//! Shared IR types produced by enrichment passes.

use std::path::PathBuf;

use vue_vet_core::ModuleId;

/// One resolved external import that should be summarized for reactivity seeds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalReactivityRoot {
  pub from: ModuleId,
  pub specifier: String,
  pub resolved_path: PathBuf,
}
