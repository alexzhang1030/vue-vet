//! Domain dirty planning for incremental session scans.
//!
//! Dirty parse is scheduled from content changes. Context epochs refresh
//! resolution, environments, and indexes without forcing re-parse of unchanged
//! source bytes. See `.agents/docs/architecture.md` (`Post-#107 locality gap`).

use std::collections::BTreeSet;

use vue_vet_core::{FileId, ModuleId};
use vue_vet_project::ContextEpochs;

use crate::discovery::{SourceInput, SourceKind};

/// Which analysis artifacts a consumer needs published.
///
/// Internal linking still runs so rules stay correct; only the published
/// [`vue_vet_project::ProjectGraph`] DTO is trimmed for lighter products.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AnalysisProduct {
  /// Diagnostics + coverage only. LSP / incremental editor path.
  DiagnosticsOnly,
  /// Diagnostics plus structural nodes/edges for navigation.
  DiagnosticsAndNavigation,
  /// Full report including module reactivity graphs.
  #[default]
  FullReport,
}

/// How broadly import / package resolution must be refreshed.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum ResolutionScope {
  #[default]
  None,
  /// Reserved for package-subtree epochs (monorepo scoping).
  PackageSubtree,
  Workspace,
}

/// Semantic impact of pending input / context changes before stage scheduling.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChangeImpact {
  pub parse: BTreeSet<FileId>,
  pub environment: BTreeSet<FileId>,
  pub resolution: ResolutionScope,
  pub component_index: bool,
  pub membership: bool,
}

/// Per-stage dirty partitions consumed by the analysis pipeline.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DirtyPlan {
  pub parse_files: BTreeSet<FileId>,
  pub structural_files: BTreeSet<FileId>,
  pub module_summaries: BTreeSet<ModuleId>,
  /// Seed-plan dirty set from the last A6 pass (`TraceModulesReport::seed_plan_dirty`).
  /// Empty on a warm linking-cache hit. The linker still computes this set.
  /// The pipeline passes source-dirty modules as a subset; the tracer merges
  /// cached summaries and pulls seed-dirty consumers that were not in the input.
  pub export_closure: BTreeSet<ModuleId>,
  pub rule_files: BTreeSet<FileId>,
  pub diagnostic_files: BTreeSet<FileId>,
}

/// Real work performed by one scan (not merely dirty-set cardinality).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScanWorkCounters {
  pub files_visited: u64,
  pub files_parsed: u64,
  pub files_reused: u64,
  pub structural_partitions_rebuilt: u64,
  pub module_summaries_visited: u64,
  pub seed_plans_recomputed: u64,
  pub export_resolve_ran: bool,
  pub seeded_reparses: u64,
  pub graph_cow_clones: u64,
  pub rules_rerun: u64,
  pub diagnostics_finalized: u64,
}

/// Build [`ChangeImpact`] from dirty content ids, cold-start force, and epoch deltas.
#[must_use]
pub fn change_impact_from(
  dirty_files: &BTreeSet<FileId>,
  force_full_parse: bool,
  previous_epochs: &ContextEpochs,
  current_epochs: &ContextEpochs,
  sources: &[SourceInput],
  previously_analyzed: &BTreeSet<FileId>,
) -> ChangeImpact {
  let all_ids = sources.iter().map(|source| source.file_id.clone()).collect::<BTreeSet<_>>();
  let vue_ids = sources
    .iter()
    .filter(|source| matches!(source.kind, SourceKind::Vue))
    .map(|source| source.file_id.clone())
    .collect::<BTreeSet<_>>();

  if force_full_parse {
    return ChangeImpact {
      parse: all_ids,
      environment: vue_ids,
      resolution: ResolutionScope::Workspace,
      component_index: true,
      membership: true,
    };
  }

  let mut impact = ChangeImpact::default();
  for file in dirty_files {
    if all_ids.contains(file) {
      impact.parse.insert(file.clone());
    }
  }
  for source in sources {
    if !previously_analyzed.contains(&source.file_id) {
      impact.parse.insert(source.file_id.clone());
    }
  }

  if previous_epochs.package_manifest != current_epochs.package_manifest {
    impact.resolution = ResolutionScope::Workspace;
    impact.environment.extend(vue_ids.iter().cloned());
  }
  if previous_epochs.lockfile != current_epochs.lockfile
    || previous_epochs.tsconfig != current_epochs.tsconfig
  {
    impact.resolution = ResolutionScope::Workspace;
  }
  if previous_epochs.source_membership != current_epochs.source_membership {
    impact.resolution = ResolutionScope::Workspace;
    impact.membership = true;
  }
  if previous_epochs.nuxt_declarations != current_epochs.nuxt_declarations {
    impact.component_index = true;
  }

  impact
}

/// Expand rule / diagnostic consumers after parse reuse and reverse-dep growth.
#[must_use]
pub fn dirty_plan_from(
  impact: &ChangeImpact,
  parse_files: BTreeSet<FileId>,
  last_affected: &BTreeSet<FileId>,
  sources: &[SourceInput],
  export_closure: BTreeSet<ModuleId>,
) -> DirtyPlan {
  let all_ids = sources.iter().map(|source| source.file_id.clone()).collect::<BTreeSet<_>>();
  let vue_ids = sources
    .iter()
    .filter(|source| matches!(source.kind, SourceKind::Vue))
    .map(|source| source.file_id.clone())
    .collect::<BTreeSet<_>>();

  let mut structural_files = last_affected.clone();
  if impact.resolution != ResolutionScope::None || impact.membership || impact.component_index {
    // Linking / indexes still rebuild broadly until Batch 2 partitions land.
    structural_files.extend(all_ids.iter().cloned());
  }

  let mut rule_files =
    last_affected.iter().filter(|&id| vue_ids.contains(id)).cloned().collect::<BTreeSet<_>>();
  rule_files.extend(impact.environment.iter().cloned());
  if impact.component_index {
    rule_files.extend(vue_ids.iter().cloned());
  }

  let module_summaries = structural_files
    .iter()
    .flat_map(|file| [ModuleId::primary(file), ModuleId::ordinary(file)])
    .collect::<BTreeSet<_>>();

  DirtyPlan {
    parse_files,
    diagnostic_files: rule_files.clone(),
    rule_files,
    export_closure,
    module_summaries,
    structural_files,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::{path::PathBuf, sync::Arc};

  use vue_vet_core::PhysicalPath;

  use crate::discovery::SourceKind;

  fn source(id: &str, kind: SourceKind) -> SourceInput {
    SourceInput {
      physical_path: PhysicalPath::new(PathBuf::from(id)),
      file_id: FileId::from(id),
      source: Arc::from(""),
      kind,
    }
  }

  #[test]
  fn tsconfig_epoch_does_not_force_parse() {
    let sources = vec![
      source("App.vue", SourceKind::Vue),
      source("a.ts", SourceKind::Script { language: "ts".into() }),
    ];
    let previous = ContextEpochs::default();
    let mut current = previous;
    current.tsconfig = 1;
    let analyzed = sources.iter().map(|source| source.file_id.clone()).collect();
    let impact =
      change_impact_from(&BTreeSet::new(), false, &previous, &current, &sources, &analyzed);
    assert!(impact.parse.is_empty());
    assert_eq!(impact.resolution, ResolutionScope::Workspace);
  }

  #[test]
  fn package_epoch_marks_environment_without_parse() {
    let sources = vec![source("App.vue", SourceKind::Vue)];
    let previous = ContextEpochs::default();
    let mut current = previous;
    current.package_manifest = 1;
    let analyzed = BTreeSet::from([FileId::from("App.vue")]);
    let impact =
      change_impact_from(&BTreeSet::new(), false, &previous, &current, &sources, &analyzed);
    assert!(impact.parse.is_empty());
    assert!(impact.environment.contains(&FileId::from("App.vue")));
    assert_eq!(impact.resolution, ResolutionScope::Workspace);
  }

  #[test]
  fn content_dirty_file_is_parse_impact() {
    let sources = vec![source("leaf.ts", SourceKind::Script { language: "ts".into() })];
    let analyzed = BTreeSet::from([FileId::from("leaf.ts")]);
    let dirty = BTreeSet::from([FileId::from("leaf.ts")]);
    let impact = change_impact_from(
      &dirty,
      false,
      &ContextEpochs::default(),
      &ContextEpochs::default(),
      &sources,
      &analyzed,
    );
    assert_eq!(impact.parse, dirty);
  }

  #[test]
  fn dirty_plan_keeps_export_closure_from_linker() {
    let sources = vec![source("leaf.ts", SourceKind::Script { language: "ts".into() })];
    let impact =
      ChangeImpact { parse: BTreeSet::from([FileId::from("leaf.ts")]), ..ChangeImpact::default() };
    let parse_files = BTreeSet::from([FileId::from("leaf.ts")]);
    let last_affected = parse_files.clone();
    let plan = dirty_plan_from(&impact, parse_files, &last_affected, &sources, BTreeSet::new());
    assert!(!plan.module_summaries.is_empty(), "parse dirty still expands to module summary ids");
    assert!(
      plan.export_closure.is_empty(),
      "export_closure is the linker seed-dirty set, not a clone of module_summaries"
    );
  }
}
