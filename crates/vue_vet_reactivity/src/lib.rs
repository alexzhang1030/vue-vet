//! Static reactivity tracing for Vue Vet.
//!
//! Builds a Vue Vet-owned [`vue_vet_core::ReactivityGraph`] from an Oxc semantic —
//! either a single script ([`trace_reactivity`]) or a resolved module graph
//! ([`trace_modules`]).
//!
//! Crate layout:
//! - `trace` — local effect tracing, `ModuleSummary` prepare, and cross-module link
//! - [`prop_flow`] — template prop → child props edges

mod prop_flow;
mod trace;

pub use prop_flow::{PropFlowSite, join_prop_flows};
pub use trace::{
  ComposableShape, DEEP_WATCH_PROPERTY, ModuleLink, ModuleReactivity, ModuleSource, ModuleSummary,
  ModuleTraceState, PreparedModuleTrace, TraceModulesError, TraceModulesOptions,
  TraceModulesReport, TraceModulesStats, arrow_return_type_kind, arrow_return_type_shape,
  build_returns_by_function, composable_factory_kind_with_index, composable_return_shape,
  composable_return_shape_with_index, function_return_type_kind, function_return_type_shape,
  merge_declaration_implementation_summary, prepare_module_summary, prepare_module_trace,
  prepare_standalone_module_source, trace_modules, trace_modules_incremental_with_options,
  trace_modules_with_options, trace_reactivity,
};

#[cfg(test)]
pub(crate) use trace::{TraceSeeds, trace_reactivity_seeded};

#[cfg(test)]
mod oracle;
#[cfg(test)]
mod tests;
