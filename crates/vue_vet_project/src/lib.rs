//! Vue Vet project graph — thin façade over an explicit analysis pipeline.
//!
//! Modules:
//! - [`context`] — `ConventionsLoad` ([`ProjectContext`])
//! - [`structural`] — `StructuralLink` (import/component edges + Nuxt seed hook)
//! - [`passes`] — enrichment (`ExternalSummaryLoad` / `SummaryMerge`)
//! - [`pipeline`] — orchestration + retained state
//! - [`layers`] — post-trace template / prop layers
//! - [`rules`] — project diagnostics
//! - [`model`] — stable DTOs
//! - [`resolve`] / [`conventions`] — resolver + Nuxt maps

mod context;
mod conventions;
mod layers;
mod model;
mod passes;
mod pipeline;
mod resolve;
mod rules;
mod state;
mod structural;

pub use context::{
  ContextChangeKind, ContextEpochs, ProjectContext, context_change_kind_for,
  project_context_from_inputs,
};
pub use conventions::NuxtImportTarget;
pub use model::{
  CONVENTIONS_VERSION, EdgeKind, GraphEdge, GraphNode, NodeKind, PROJECT_RULE_IDS, ProjectFile,
  ProjectGraph, ReactivityIssue,
};
pub use passes::{
  ENRICHMENT_STEPS, EXTERNAL_COMPANION_MAX_BYTES, EnrichmentStage, EnrichmentStepMeta,
  ExternalSummaryLoadPass, NuxtImportsSeedPass, ProvisionalFactoryMergePass,
};
pub use pipeline::{
  build_project_graph, build_project_graph_incremental_with_options,
  build_project_graph_with_options,
};
pub use resolve::{OXC_RESOLVER_VERSION, normalize_project_root, resolver_config_inputs};
pub use state::{ProjectGraphState, ProjectGraphStats};
