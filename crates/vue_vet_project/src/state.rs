//! Retained incremental partitions for long-lived project graph builds.

use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use vue_vet_core::ModuleId;
use vue_vet_reactivity::{ModuleReactivity, ModuleTraceState};

use crate::resolve::ProjectResolver;
use crate::structural::StructuralProjectState;

/// Reusable project-linking state retained by a long-lived session.
///
/// Partitions are independently `Arc`-shared so a real scan can copy-on-write
/// structural caches without cloning module-trace entries (and vice versa).
#[derive(Clone, Debug, Default)]
pub struct ProjectGraphState {
  pub module_trace: Arc<ModuleTraceState>,
  pub structural: Arc<StructuralProjectState>,
  /// Final module graphs after template + prop layers (separate from base trace).
  pub layered: Arc<LayeredGraphState>,
  /// Retained while `root` + context revision are unchanged.
  pub resolver: Option<Arc<ProjectResolver>>,
  pub resolver_root: Option<PathBuf>,
  pub resolver_revision: Option<u64>,
  pub last_stats: ProjectGraphStats,
  /// Seed-plan dirty set from the last trace (`TraceModulesReport::seed_plan_dirty`).
  pub last_export_closure: BTreeSet<ModuleId>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectGraphStats {
  pub structural_files_rebuilt: usize,
  pub structural_files_reused: usize,
  pub module_graphs_reused: usize,
  pub seeded_module_reparses: usize,
  /// How many internal partition Arcs were cloned via `Arc::make_mut` this scan.
  pub partition_cow_clones: usize,
  /// Modules whose seed plans were freshly computed this scan.
  pub seed_plans_recomputed: usize,
  /// Whether export/provide fixed-point resolution ran this scan.
  pub export_resolve_ran: bool,
  /// Whether template/prop layers were recomputed (false = Arc reuse).
  pub layered_graphs_rebuilt: bool,
}

/// Cached post-trace layers: template joins + prop-flow edges.
#[derive(Clone, Debug, Default)]
pub struct LayeredGraphState {
  pub key: Option<LayeredInputKey>,
  pub modules: Arc<[ModuleReactivity]>,
}

/// Identity of base graphs + SFC facts + prop-relevant edges that feed layers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayeredInputKey {
  pub modules: Vec<ModuleLayerKey>,
  pub prop_edges: Vec<(String, String, usize)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleLayerKey {
  pub id: ModuleId,
  pub base_ptr: usize,
  pub facts_ptr: usize,
}

impl ProjectGraphState {
  #[must_use]
  pub const fn last_stats(&self) -> ProjectGraphStats {
    self.last_stats
  }

  /// Share partition Arcs with another state (refcount only — no map deep copy).
  #[must_use]
  pub fn share(&self) -> Self {
    Self {
      module_trace: Arc::clone(&self.module_trace),
      structural: Arc::clone(&self.structural),
      layered: Arc::clone(&self.layered),
      resolver: self.resolver.as_ref().map(Arc::clone),
      resolver_root: self.resolver_root.clone(),
      resolver_revision: self.resolver_revision,
      last_stats: self.last_stats,
      last_export_closure: self.last_export_closure.clone(),
    }
  }
}
