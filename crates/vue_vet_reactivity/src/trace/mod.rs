//! Static Vue reactivity dependency tracing.
//!
//! Internal stages live in [`summary`] (prepare / return shapes) and
//! [`summary::link`] (cross-module seeds / incremental).
//!
//! Builds a serializable [`vue_vet_core::ReactivityGraph`] from an Oxc semantic
//! model (single script) or a resolved module graph ([`trace_modules`]).
//!
//! # Charter
//!
//! - **Static only** — does not execute Vue effects or Proxies.
//! - **Under-approximation** — missing edges are acceptable; invented edges are
//!   bugs.
//! - **Stable facts** — returned types live in `vue_vet_core`; Oxc AST nodes do
//!   not cross this boundary.
//!
//! # Single-file entry
//!
//! [`trace_reactivity`] records bindings and tracking scopes for one Oxc
//! semantic model. Pass the original file text and script byte offset so spans
//! map back to the SFC or module source.
//!
//! # Cross-module entry
//!
//! [`trace_modules`] consumes prepared phase-one summaries when the Oxc adapter
//! supplies them, links composable/export seeds across [`ModuleLink`] edges,
//! and reparses only seeded consumers. Both phases use a bounded worker pool.
//! Callers supply already-resolved links.

use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

use oxc_semantic::Semantic;
use oxc_span::Span;
use vue_vet_core::{ReactiveBindingFact, ReactiveBindingKind, ReactivityGraph, ScriptKind};

mod bindings;
mod branch_hygiene;
mod context;
mod expr;
mod follow;
mod inject;
mod kinds;
mod local;
mod plugin;
mod reads;
mod render;
mod scopes;
mod summary;
mod uncertain;
mod writes;

use bindings::{
  CollectedBindings, collect_component_props_bindings, collect_reactive_bindings,
  collect_typed_reactive_bindings, extend_with_reactive_aliases,
};
use kinds::{
  clear_trace_line_index, collect_imported_bindings, install_trace_line_index,
  push_binding_by_span, source_may_have_component_props_factory,
  source_may_have_typed_ref_annotations,
};
use local::collect_local_composable_usage;
use scopes::{collect_render_scopes, collect_tracking_scopes};

pub use inject::{
  InjectSite, InjectionKey, ProvideOffer, ProvideSite, collect_inject_sites, collect_provide_sites,
  provide_offer_index, resolve_inject_links, resolve_inject_offer,
};
pub use kinds::DEEP_WATCH_PROPERTY;
pub use plugin::{
  NamedApiBag, TraceConfig, TracerPlugin, flatten_named_api_bags, is_named_api_bag_callee,
  named_api_bag,
};
pub use summary::{
  ComposableShape, ModuleLink, ModuleReactivity, ModuleSource, ModuleSummary, ModuleTraceState,
  PreparedModuleTrace, TraceModulesError, TraceModulesOptions, TraceModulesReport,
  TraceModulesStats, ValueBag, ValueBagEntry, arrow_return_type_kind, arrow_return_type_shape,
  build_returns_by_function, composable_factory_kind_with_index, composable_return_shape,
  composable_return_shape_with_index, composable_value_bag_with_index, function_return_type_kind,
  function_return_type_shape, merge_declaration_implementation_summary, prepare_module_summary,
  prepare_module_summary_with_config, prepare_module_trace, prepare_standalone_module_source,
  trace_modules, trace_modules_incremental_with_options, trace_modules_with_options,
};

/// Trace Vue reactive bindings and tracking-scope dependencies from an Oxc semantic model.
///
/// The returned graph contains only Vue Vet-owned serializable facts. Oxc nodes
/// remain an implementation detail of this crate.
///
/// `sfc_source` is the original file used for absolute line/column mapping;
/// `script_offset` is the byte offset of the analyzed script within that file
/// (use `0` for a standalone module).
#[must_use]
pub fn trace_reactivity(
  semantic: &Semantic<'_>,
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
) -> ReactivityGraph {
  trace_reactivity_with_config(
    semantic,
    sfc_source,
    script_offset,
    script_kind,
    &TraceConfig::empty(),
  )
}

/// Trace with an explicit plugin catalog ([`TraceConfig::named_api_bags`]).
#[must_use]
pub fn trace_reactivity_with_config(
  semantic: &Semantic<'_>,
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
  config: &TraceConfig<'_>,
) -> ReactivityGraph {
  trace_reactivity_seeded(
    semantic,
    sfc_source,
    script_offset,
    script_kind,
    &TraceSeeds::default(),
    config,
  )
}

/// Instance bag fields: field name → reactive kind (no open-spread flag).
type InstanceShape = BTreeMap<String, ReactiveBindingKind>;
/// Map of bag/composable name → return field kinds.
type ComposableShapeMap = BTreeMap<String, InstanceShape>;

/// Same-file composable/factory export classification.
#[derive(Debug, Eq, PartialEq)]
pub enum LocalComposableExport {
  /// `return { field: ref(0) }` object bag (may include open reactive spreads).
  Bag(summary::ComposableShape),
  /// `return ref(0)` / declared `(): Ref<T>` scalar factory.
  Factory(ReactiveBindingKind),
  /// `return { maps: { useX } }` nested method bag factory.
  ValueFactory(summary::ValueBag),
}

/// Same-file composable defs: name → (definition span, return classification).
/// Span is required so call sites resolve like instance-bag seeding (no name-only invent).
type LocalComposableDefs = BTreeMap<String, (Span, LocalComposableExport)>;

#[derive(Debug, Default)]
pub struct TraceSeeds {
  bindings: Vec<ReactiveBindingFact>,
  /// `const bag = useFoo()` locals mapped to composable return field kinds.
  composable_instances: ComposableShapeMap,
  /// `const api = createApi()` nested method bags for member-call seeding.
  value_bags: BTreeMap<String, summary::ValueBag>,
  /// Import locals that wrap Vue `defineComponent` (cross-module `ComponentFactory`).
  component_factories: BTreeSet<String>,
}

pub fn trace_reactivity_seeded(
  semantic: &Semantic<'_>,
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
  seeds: &TraceSeeds,
  config: &TraceConfig<'_>,
) -> ReactivityGraph {
  // Index only — do not `SourceContext::new(&str)` here; that copies the whole
  // buffer into `Arc<str>` on every module and regresses cold `trace_*` benches.
  let line_index = Arc::new(vue_vet_core::LineIndex::new(sfc_source));
  install_trace_line_index(line_index);
  let graph =
    trace_reactivity_seeded_inner(semantic, sfc_source, script_offset, script_kind, seeds, config);
  clear_trace_line_index();
  graph
}

fn trace_reactivity_seeded_inner(
  semantic: &Semantic<'_>,
  sfc_source: &str,
  script_offset: usize,
  script_kind: ScriptKind,
  seeds: &TraceSeeds,
  config: &TraceConfig<'_>,
) -> ReactivityGraph {
  let imported_bindings = collect_imported_bindings(semantic);
  let named_api_bags = config.named_api_bags;
  // Include function-local refs when resolving `return { signal }` shapes and when
  // classifying nested tracking scopes. Do not publish them as top-level graph
  // bindings (they would collide with `const { signal } = useX()` seeds by name).
  let CollectedBindings { bindings: mut scope_bindings, ambient_call_handles } =
    collect_reactive_bindings(
      semantic,
      &imported_bindings,
      sfc_source,
      script_offset,
      script_kind,
      true,
      named_api_bags,
    );
  let CollectedBindings { mut bindings, .. } = collect_reactive_bindings(
    semantic,
    &imported_bindings,
    sfc_source,
    script_offset,
    script_kind,
    false,
    named_api_bags,
  );
  // `type: ComputedRef<T>` / `const x: Ref<T> = …` — typed parameters & declarators.
  // Cheap source gate: skip the AST walk when no Ref-like annotation text exists
  // (keeps `trace_1k_modules` / plain `ref()` modules off this path).
  if source_may_have_typed_ref_annotations(sfc_source) {
    for binding in collect_typed_reactive_bindings(semantic, sfc_source, script_offset) {
      push_binding_by_span(&mut scope_bindings, binding);
    }
  }
  // Same-file `defineFormProps({ setup({ values }) })` options-object callback bags.
  // Collect is cheap when empty; do not require local `Ref` text (slots come from types).
  let options_slots = summary::collect_local_options_callback_slots(semantic);
  if !options_slots.is_empty() {
    let mut options_bindings = Vec::new();
    summary::seed_options_callback_params_at_calls(
      semantic,
      &options_slots,
      sfc_source,
      script_offset,
      &mut options_bindings,
    );
    for binding in options_bindings {
      push_binding_by_span(&mut scope_bindings, binding.clone());
      if !bindings.iter().any(|local| local.name == binding.name) {
        bindings.push(binding);
      }
    }
  }
  // Same-file `useX(init, (params: ComputedRef<T>) => …)` typed function callbacks.
  let typed_callback_slots = summary::collect_local_typed_callback_param_slots(semantic);
  if !typed_callback_slots.is_empty() {
    let mut typed_bindings = Vec::new();
    summary::seed_typed_callback_params_at_calls(
      semantic,
      &typed_callback_slots,
      sfc_source,
      script_offset,
      &mut typed_bindings,
    );
    for binding in typed_bindings {
      push_binding_by_span(&mut scope_bindings, binding.clone());
      if !bindings.iter().any(|local| local.name == binding.name) {
        bindings.push(binding);
      }
    }
  }
  // `defineComponent` / `setup(props)` — props bag is reactive.
  // Custom wrappers seed when they forward to `defineComponent` (same-file or
  // cross-module `ExportState::ComponentFactory` via seeds).
  if source_may_have_component_props_factory(sfc_source) || !seeds.component_factories.is_empty() {
    for binding in collect_component_props_bindings(
      semantic,
      &imported_bindings,
      sfc_source,
      script_offset,
      &seeds.component_factories,
    ) {
      push_binding_by_span(&mut scope_bindings, binding.clone());
      if !bindings.iter().any(|local| local.name == binding.name) {
        bindings.push(binding);
      }
    }
  }
  for binding in &seeds.bindings {
    if !bindings.iter().any(|local| local.name == binding.name) {
      bindings.push(binding.clone());
    }
    push_binding_by_span(&mut scope_bindings, binding.clone());
  }

  // Same-file composables: `function useX()` / `const useX = () => …` shapes, then
  // `const bag = useX()` / `const { field } = useX()` seeds. Cross-module seeds win
  // on name conflict (already linked by the module graph).
  let shape_graph =
    ReactivityGraph { bindings: scope_bindings.clone(), ..ReactivityGraph::default() };
  let (local_instances, local_destructured, local_composable_shapes) =
    collect_local_composable_usage(semantic, &shape_graph, sfc_source, script_offset);
  for binding in local_destructured {
    push_binding_by_span(&mut scope_bindings, binding.clone());
    if !bindings.iter().any(|local| local.name == binding.name) {
      bindings.push(binding);
    }
  }
  let mut composable_instances = local_instances;
  for (bag, shape) in &seeds.composable_instances {
    composable_instances.insert(bag.clone(), shape.clone());
  }
  // Same-file provide → inject after instance bags exist (provide(api) where api = useX()).
  let provides = collect_provide_sites(
    semantic,
    &imported_bindings,
    &bindings,
    &composable_instances,
    &local_composable_shapes,
    script_kind,
  );
  let injects = collect_inject_sites(semantic, &imported_bindings, &bindings, script_kind);
  let resolved = resolve_inject_links(&provides, &injects, sfc_source, script_offset);
  for binding in resolved.bindings {
    push_binding_by_span(&mut scope_bindings, binding.clone());
    if !bindings.iter().any(|local| local.name == binding.name) {
      bindings.push(binding);
    }
  }
  for (bag, shape) in resolved.instances {
    composable_instances.entry(bag).or_insert(shape);
  }

  // `const alias = knownRef` — same object identity; seed before scopes so `.value` tracks.
  extend_with_reactive_aliases(semantic, &mut bindings, sfc_source, script_offset);
  extend_with_reactive_aliases(semantic, &mut scope_bindings, sfc_source, script_offset);

  // Classify scopes with function-local + typed bindings; publish top-level bindings only.
  let mut scopes = collect_tracking_scopes(
    semantic,
    &imported_bindings,
    &scope_bindings,
    &composable_instances,
    &ambient_call_handles,
    sfc_source,
    script_offset,
  );
  scopes.extend(collect_render_scopes(
    semantic,
    &imported_bindings,
    &scope_bindings,
    &composable_instances,
    &ambient_call_handles,
    sfc_source,
    script_offset,
  ));
  // Seed/merge order is not source order; stabilize before publishing the graph.
  bindings.sort_by_key(|fact| fact.span.offset);
  scopes.sort_by_key(|fact| fact.span.offset);
  let mut graph = ReactivityGraph {
    version: vue_vet_core::REACTIVITY_GRAPH_VERSION,
    module_id: String::new(),
    bindings,
    scopes,
    effects: Vec::new(),
    edges: Vec::new(),
    template_reads: Vec::new(),
    // Retain instance bags so template joins can resolve `bag.field` after tracing.
    composable_instances,
  };
  graph.project_effects_from_scopes();
  graph
}
