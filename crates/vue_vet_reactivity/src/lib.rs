//! Static reactivity tracing for Vue Vet.
//!
//! Builds a Vue Vet-owned [`vue_vet_core::ReactivityGraph`] from an Oxc semantic —
//! either a single script ([`trace_reactivity`]) or a resolved module graph
//! ([`trace_modules`]).
//!
//! Crate layout:
//! - `trace` — local effect tracing, `ModuleSummary` prepare, and cross-module link
//! - [`prop_flow`] — template prop → child props edges

mod explain;
mod prop_flow;
mod trace;

pub use explain::{explain_tracking_scope, scope_covering_span, select_tracking_scopes};
pub use prop_flow::{PropFlowSite, join_prop_flows};
pub use trace::{
  ComposableShape, DEEP_WATCH_PROPERTY, ModuleLink, ModuleReactivity, ModuleSource, ModuleSummary,
  ModuleTraceState, NamedApiBag, PreparedModuleTrace, TraceConfig, TraceModulesError,
  TraceModulesOptions, TraceModulesReport, TraceModulesStats, TracerPlugin, ValueBag,
  ValueBagEntry, arrow_return_type_kind, arrow_return_type_shape, build_returns_by_function,
  composable_factory_kind_with_index, composable_return_shape, composable_return_shape_with_index,
  composable_value_bag_with_index, flatten_named_api_bags, function_return_type_kind,
  function_return_type_shape, is_named_api_bag_callee, merge_declaration_implementation_summary,
  named_api_bag, prepare_module_summary, prepare_module_summary_with_config, prepare_module_trace,
  prepare_standalone_module_source, trace_modules, trace_modules_incremental_with_options,
  trace_modules_with_options, trace_reactivity, trace_reactivity_with_config,
};

#[cfg(test)]
pub(crate) use trace::{TraceSeeds, trace_reactivity_seeded};

#[cfg(test)]
mod oracle;
#[cfg(test)]
mod tests;
