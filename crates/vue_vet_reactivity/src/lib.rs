//! Static reactivity tracing for Vue Vet.
//!
//! Default consumers use [`ModuleSource`] plus [`trace_modules`] /
//! [`prepare_standalone_module_source`], then [`explain_tracking_scope`].
//! Oxc `Semantic` / AST helpers live under [`oxc`].

mod explain;
mod prop_flow;
mod trace;

pub use explain::{
  explain_tracking_scope, module_id_matches, query_module_prefix, scope_covering_span,
  select_tracking_scopes,
};
pub use prop_flow::{PropFlowSite, join_prop_flows};
pub use trace::{
  ComposableShape, DEEP_WATCH_PROPERTY, ModuleLink, ModuleReactivity, ModuleSource, ModuleSummary,
  ModuleTraceState, NamedApiBag, PreparedModuleTrace, TraceConfig, TraceModulesError,
  TraceModulesOptions, TraceModulesReport, TraceModulesStats, TracerPlugin, ValueBag,
  ValueBagEntry, flatten_named_api_bags, is_named_api_bag_callee,
  merge_declaration_implementation_summary, named_api_bag, prepare_standalone_module_source,
  trace_modules, trace_modules_incremental_from_arcs, trace_modules_incremental_from_refs,
  trace_modules_incremental_with_options, trace_modules_with_options,
};

/// Oxc `Semantic` / AST / `Span` / `NodeId` entry points and helpers.
///
/// Pin Oxc crates to the same versions as this package's `oxc_*` dependencies.
/// Product adapters (`vue_vet_oxc`) should import from here. Ordinary source /
/// module consumers do not need this namespace.
pub mod oxc {
  pub use crate::trace::{
    arrow_return_type_kind, arrow_return_type_shape, build_returns_by_function,
    composable_factory_kind_with_index, composable_return_shape,
    composable_return_shape_with_index, composable_value_bag_with_index, function_return_type_kind,
    function_return_type_shape, prepare_module_summary, prepare_module_summary_with_config,
    prepare_module_trace, trace_reactivity, trace_reactivity_with_config,
  };
}

#[doc(hidden)]
pub use oxc::{
  arrow_return_type_kind, arrow_return_type_shape, build_returns_by_function,
  composable_factory_kind_with_index, composable_return_shape, composable_return_shape_with_index,
  composable_value_bag_with_index, function_return_type_kind, function_return_type_shape,
  prepare_module_summary, prepare_module_summary_with_config, prepare_module_trace,
  trace_reactivity, trace_reactivity_with_config,
};

#[cfg(test)]
pub(crate) use trace::{TraceSeeds, trace_reactivity_seeded};

#[cfg(test)]
mod oracle;
#[cfg(test)]
mod tests;
