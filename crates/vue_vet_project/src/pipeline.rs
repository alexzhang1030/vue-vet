//! Project graph pipeline orchestrator.
//!
//! Stage order (deterministic):
//! ```text
//! Context → StructuralLink → Enrichment(ExternalSummary + SummaryMerge)
//!   → Trace(reactivity) → Layers → ProjectRules → ProjectGraph
//! ```

use std::{
  collections::{BTreeMap, BTreeSet},
  path::Path,
  sync::Arc,
};

use vue_vet_core::ModuleId;
use vue_vet_plugins::{default_trace_modules_options, ensure_default_plugins};
use vue_vet_reactivity::{
  ModuleSource, ModuleTraceState, TraceModulesOptions, trace_modules_incremental_from_refs,
};

use crate::context::ProjectContext;
use crate::conventions::convention_component_name;
use crate::layers::apply_template_prop_layers;
use crate::model::{CONVENTIONS_VERSION, NodeKind, ProjectFile, ProjectGraph, ReactivityIssue};
use crate::passes::ExternalSummaryLoadPass;
use crate::resolve::{ProjectResolver, normalize_project_root, normalized_path};
use crate::rules::unused_component_diagnostics;
use crate::state::{ProjectGraphState, ProjectGraphStats};
use crate::structural::{
  StructuralContextKey, StructuralFileCache, analyze_structural_file, file_node,
  insert_component_name,
};

#[cfg(test)]
#[path = "pipeline_tests/mod.rs"]
mod tests;

#[must_use]
pub fn build_project_graph(root: &Path, files: &[ProjectFile]) -> ProjectGraph {
  let options = default_trace_modules_options();
  build_project_graph_with_options(root, files, &options)
}

#[must_use]
pub fn build_project_graph_with_options(
  root: &Path,
  files: &[ProjectFile],
  trace_options: &TraceModulesOptions,
) -> ProjectGraph {
  let root = normalize_project_root(root);
  let known =
    files.iter().map(|file| normalized_path(file.path.as_path())).collect::<BTreeSet<_>>();
  let context = ProjectContext::from_filesystem(&root, &known);
  // Clone so we can ensure_default_plugins without requiring &mut from callers.
  let options = ensure_default_plugins(trace_options.clone());
  build_project_graph_incremental_with_options(
    &root,
    files,
    &options,
    &context,
    &mut ProjectGraphState::default(),
    None,
  )
}

#[must_use]
pub fn build_project_graph_incremental_with_options<'a>(
  root: &Path,
  files: impl IntoIterator<Item = &'a ProjectFile>,
  trace_options: &TraceModulesOptions,
  project_context: &ProjectContext,
  state: &mut ProjectGraphState,
  on_external_seeds: Option<&dyn Fn(usize)>,
) -> ProjectGraph {
  let mut trace_options = ensure_default_plugins(trace_options.clone());
  state.last_stats = ProjectGraphStats::default();
  state.last_export_closure.clear();
  let root = normalize_project_root(root);
  let mut ordered = files.into_iter().collect::<Vec<_>>();
  ordered.sort_by_key(|file| normalized_path(file.path.as_path()));
  let known =
    ordered.iter().map(|file| normalized_path(file.path.as_path())).collect::<BTreeSet<_>>();
  let resolver = retain_project_resolver(state, &root, project_context.revision);

  // --- StructuralLink: nodes, per-file edges, NuxtImportsSeedPass ---
  let mut nodes = ordered.iter().map(|file| file_node(file)).collect::<Vec<_>>();
  let node_by_path =
    nodes.iter().map(|node| (node.path.clone(), node.id.clone())).collect::<BTreeMap<_, _>>();
  let dts_names = &project_context.nuxt_component_names;
  for node in &mut nodes {
    if node.kind != NodeKind::Component {
      continue;
    }
    if let Some(name) = dts_names
      .iter()
      .find_map(|(name, path)| (path == &node.path).then_some(name.clone()))
      .or_else(|| convention_component_name(&node.path))
    {
      node.name = name;
    }
  }
  let mut component_by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
  for node in nodes.iter().filter(|node| node.kind == NodeKind::Component) {
    insert_component_name(&mut component_by_name, &node.name, &node.id);
  }
  for (name, path) in dts_names {
    if let Some(id) = node_by_path.get(path) {
      insert_component_name(&mut component_by_name, name, id);
    }
  }
  let composable_by_name = nodes
    .iter()
    .filter(|node| node.kind == NodeKind::Composable)
    .map(|node| (node.name.clone(), node.id.clone()))
    .collect::<BTreeMap<_, _>>();
  let persist_cache = trace_options.persist_linking_cache;
  let subset_input = persist_cache && state.module_trace.has_cached_modules();
  let mut module_ids = BTreeSet::new();
  let mut borrowed_sources = Vec::new();
  let mut owned_sources = Vec::new();
  for file in &ordered {
    take_workspace_source(
      file.module_source.as_ref(),
      ModuleId::primary(&file.path),
      &state.module_trace,
      subset_input,
      &mut module_ids,
      &mut borrowed_sources,
      &mut owned_sources,
    );
    take_workspace_source(
      file.ordinary_module_source.as_ref(),
      ModuleId::ordinary(&file.path),
      &state.module_trace,
      subset_input,
      &mut module_ids,
      &mut borrowed_sources,
      &mut owned_sources,
    );
  }
  if Arc::strong_count(&state.structural) > 1 {
    state.last_stats.partition_cow_clones += 1;
  }
  let structural = Arc::make_mut(&mut state.structural);
  if !structural_context_matches(
    structural.context.as_ref(),
    &root,
    project_context.revision,
    &nodes,
    &module_ids,
    &project_context.nuxt_import_names,
  ) {
    structural.files.clear();
    structural.context = Some(StructuralContextKey {
      root: root.clone(),
      revision: project_context.revision,
      nodes: nodes.clone(),
      module_ids: module_ids.clone(),
      nuxt_import_names: project_context.nuxt_import_names.clone(),
    });
  }
  let present = ordered.iter().map(|file| &file.path).collect::<BTreeSet<_>>();
  structural.files.retain(|file_id, _| present.contains(file_id));
  for file in &ordered {
    let reuse = structural.files.get(&file.path).is_some_and(|cached| cached.facts == file.facts);
    if reuse {
      state.last_stats.structural_files_reused += 1;
      continue;
    }
    state.last_stats.structural_files_rebuilt += 1;
    let output = Arc::new(analyze_structural_file(
      file,
      resolver.as_ref(),
      &known,
      &node_by_path,
      &component_by_name,
      &composable_by_name,
      &module_ids,
      &project_context.nuxt_import_names,
    ));
    structural
      .files
      .insert(file.path.clone(), StructuralFileCache { facts: Arc::clone(&file.facts), output });
  }
  let mut module_links = Vec::new();
  let mut external_nodes = BTreeMap::new();
  let mut edges = Vec::new();
  let mut diagnostics = Vec::new();
  let mut external_roots = Vec::new();
  for file in &ordered {
    let Some(cached) = structural.files.get(&file.path) else {
      continue;
    };
    let output = cached.output.as_ref();
    for node in &output.external_nodes {
      external_nodes.entry(node.id.clone()).or_insert_with(|| node.clone());
    }
    edges.extend(output.edges.iter().cloned());
    diagnostics.extend(output.diagnostics.iter().cloned());
    module_links.extend(output.module_links.iter().cloned());
    external_roots.extend(output.external_reactivity_roots.iter().cloned());
  }
  nodes.extend(external_nodes.into_values());
  nodes.sort();
  edges.sort();
  edges.dedup();

  // --- ProjectRules (unused-component; unresolved already in structural) ---
  diagnostics.extend(unused_component_diagnostics(&ordered, &nodes, &edges));
  diagnostics.sort_by(|left, right| {
    (&left.file, left.span.offset, &left.rule_id).cmp(&(
      &right.file,
      right.span.offset,
      &right.rule_id,
    ))
  });

  // --- Enrichment: ExternalSummaryLoad (+ per-module SummaryMerge) ---
  let (external_sources, external_links) =
    ExternalSummaryLoadPass::run(&root, resolver.as_ref(), &external_roots, on_external_seeds);
  let live_externals = external_sources.iter().map(|module| &module.id).collect::<BTreeSet<_>>();
  if persist_cache && subset_input {
    trace_options.retain_cached_modules = true;
    trace_options.drop_module_ids =
      dropped_cached_ids(&state.module_trace, &module_ids, &live_externals);
  }
  let mut borrowed_externals = Vec::new();
  for external in &external_sources {
    let keep = !subset_input
      || state.module_trace.cached_source(&external.id).is_none_or(|cached| cached != external);
    if keep {
      borrowed_externals.push(external);
    }
  }
  module_links.extend(external_links);
  let mut module_sources = borrowed_sources;
  module_sources.extend(owned_sources.iter());
  module_sources.extend(borrowed_externals);

  // --- Trace (reactivity seed fixed point) ---
  if Arc::strong_count(&state.module_trace) > 1 {
    state.last_stats.partition_cow_clones += 1;
  }
  let module_trace = Arc::make_mut(&mut state.module_trace);
  let mut trace_report = trace_modules_incremental_from_refs(
    &module_sources,
    &module_links,
    &trace_options,
    module_trace,
  );
  // External package summaries seed consumers only — never lint surfaces.
  trace_report.modules.retain(|module| module_ids.contains(&module.id));
  state.last_stats.module_graphs_reused = trace_report.stats.reused_graphs;
  state.last_stats.seeded_module_reparses = trace_report.stats.seeded_reparses;
  state.last_stats.seed_plans_recomputed = trace_report.stats.seed_plans_recomputed;
  state.last_stats.export_resolve_ran = trace_report.stats.export_resolve_ran;
  state.last_stats.module_summaries_visited =
    trace_report.stats.phase_one_succeeded.saturating_add(trace_report.stats.phase_one_failed);
  state.last_export_closure = trace_report.seed_plan_dirty;
  let reactivity_issues = trace_report
    .issues
    .into_iter()
    .filter(|error| error.module_id().is_none_or(|id| module_ids.contains(id)))
    .map(|error| ReactivityIssue { module: error.module_id().cloned(), message: error.to_string() })
    .collect::<Vec<_>>();
  let reactivity_error = (!reactivity_issues.is_empty()).then(|| {
    reactivity_issues.iter().map(|issue| issue.message.as_str()).collect::<Vec<_>>().join("; ")
  });

  // --- Layers: template joins + prop flow ---
  let module_reactivity =
    apply_template_prop_layers(state, &ordered, &edges, trace_report.modules, &module_ids);
  let mut invalidation_inputs = known.into_iter().collect::<Vec<_>>();
  invalidation_inputs.extend(project_context.invalidation_inputs.iter().cloned());
  invalidation_inputs.sort();
  invalidation_inputs.dedup();
  ProjectGraph {
    conventions_version: CONVENTIONS_VERSION,
    nodes,
    edges,
    diagnostics,
    invalidation_inputs,
    module_reactivity,
    reactivity_issues,
    reactivity_error,
  }
}

/// Record `id` as live. Borrow `fresh` when its id already matches; clone only
/// to rewrite a mismatched id. Unchanged cached modules are not pushed.
fn take_workspace_source<'a>(
  fresh: Option<&'a ModuleSource>,
  id: ModuleId,
  state: &ModuleTraceState,
  subset: bool,
  ids: &mut BTreeSet<ModuleId>,
  borrowed: &mut Vec<&'a ModuleSource>,
  owned: &mut Vec<ModuleSource>,
) {
  let Some(fresh) = fresh else {
    return;
  };
  if subset && state.cached_source(&id).is_some_and(|cached| script_unchanged(cached, fresh, &id)) {
    ids.insert(id);
    return;
  }
  if fresh.id == id {
    borrowed.push(fresh);
    ids.insert(id);
    return;
  }
  let mut module = fresh.clone();
  module.id = id.clone();
  ids.insert(id);
  owned.push(module);
}

fn structural_context_matches(
  cached: Option<&StructuralContextKey>,
  root: &std::path::Path,
  revision: u64,
  nodes: &[crate::model::GraphNode],
  module_ids: &BTreeSet<ModuleId>,
  nuxt_import_names: &BTreeMap<String, crate::conventions::NuxtImportTarget>,
) -> bool {
  cached.is_some_and(|key| {
    key.root == root
      && key.revision == revision
      && key.nodes == nodes
      && key.module_ids == *module_ids
      && key.nuxt_import_names == *nuxt_import_names
  })
}

fn dropped_cached_ids(
  state: &ModuleTraceState,
  workspace_ids: &BTreeSet<ModuleId>,
  live_externals: &BTreeSet<&ModuleId>,
) -> BTreeSet<ModuleId> {
  state
    .cached_module_ids()
    .filter(|id| !workspace_ids.contains(*id) && !live_externals.contains(id))
    .cloned()
    .collect()
}

fn script_unchanged(cached: &ModuleSource, fresh: &ModuleSource, id: &ModuleId) -> bool {
  cached.id == *id
    && cached.source == fresh.source
    && cached.language == fresh.language
    && cached.kind == fresh.kind
    && cached.source_offset == fresh.source_offset
}

fn retain_project_resolver(
  state: &mut ProjectGraphState,
  root: &std::path::PathBuf,
  revision: u64,
) -> Arc<ProjectResolver> {
  let reusable = state.resolver.as_ref().is_some_and(|_| {
    state.resolver_root.as_ref() == Some(root) && state.resolver_revision == Some(revision)
  });
  if !reusable {
    let resolver = Arc::new(ProjectResolver::new(root));
    state.resolver = Some(Arc::clone(&resolver));
    state.resolver_root = Some(root.clone());
    state.resolver_revision = Some(revision);
    return resolver;
  }
  state.resolver.as_ref().map_or_else(|| Arc::new(ProjectResolver::new(root)), Arc::clone)
}
