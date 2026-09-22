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
mod model_demand;
mod passes;
mod pipeline;
mod resolve;
mod rules;
mod state;
mod structural;
mod vapor_migration;

pub use context::{
  ContextChangeKind, ContextEpochs, ProjectContext, context_change_kind_for, layer_input_relatives,
  project_context_from_inputs,
};
pub use conventions::NuxtImportTarget;
pub use model::{
  CONVENTIONS_VERSION, EdgeKind, GraphEdge, GraphNode, NodeKind, PROJECT_GRAPH_SCHEMA_VERSION,
  PROJECT_RULE_IDS, ProjectFile, ProjectGraph, ReactivityIssue,
};
pub use model_demand::{ModelDemandStats, join_model_demand_facts};
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
pub use vapor_migration::{
  AUDITED_CORE_COMMIT, AUDITED_PLUGIN_VUE_COMMIT, AUDITED_PLUGIN_VUE_VERSION, AUDITED_VUE_VERSION,
  VAPOR_MIGRATION_RULE_IDS, vapor_migration_diagnostics,
};
