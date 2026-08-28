//! Reactivity-graph lookups shared by unused-binding and operand rules.

use std::collections::BTreeSet;

use vue_vet_core::{
  ReactiveBindingFact, ReactivityGraph, ScriptBindingFact, ScriptBlockFacts, TemplateElementFact,
};

/// Binding / write / edge-target names that count as a use of a reactive local.
///
/// Matches the historical walk in `no-unused-reactive-binding` and
/// `no-unused-computed-binding` (template reads, scope reads and writes,
/// edge `to`). Callers may insert extra names (static template `ref="…"`).
#[must_use]
pub fn used_reactive_names(graph: &ReactivityGraph) -> BTreeSet<&str> {
  let mut used = BTreeSet::new();
  for read in &graph.template_reads {
    used.insert(read.binding.as_str());
  }
  for scope in &graph.scopes {
    for read in &scope.reads {
      used.insert(read.binding.as_str());
    }
    for write in &scope.writes {
      used.insert(write.binding.as_str());
    }
  }
  for edge in &graph.edges {
    used.insert(edge.to.as_str());
  }
  used
}

/// Static `ref="…"` attribute values in source order.
pub fn static_template_ref_names(elements: &[TemplateElementFact]) -> impl Iterator<Item = &str> {
  elements
    .iter()
    .filter_map(|element| element.attribute("ref"))
    .filter_map(|attribute| attribute.value.as_deref())
}

#[must_use]
pub fn reactive_binding<'a>(
  block: &'a ScriptBlockFacts,
  name: &str,
) -> Option<&'a ReactiveBindingFact> {
  block.reactivity_graph.bindings.iter().find(|binding| binding.name == name)
}

#[must_use]
pub fn script_binding<'a>(
  block: &'a ScriptBlockFacts,
  name: &str,
) -> Option<&'a ScriptBindingFact> {
  block.bindings.iter().find(|binding| binding.name == name)
}
