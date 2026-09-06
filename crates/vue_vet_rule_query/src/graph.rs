//! Reactivity-graph lookups shared by unused-binding and operand rules.

use std::collections::BTreeSet;

use vue_vet_core::{
  ReactiveBindingFact, ReactivityGraph, ScriptBindingFact, ScriptBlockFacts, ScriptOperandFact,
  TemplateElementFact,
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

/// Reactive binding for an operand identifier.
///
/// Resolved identifiers match by Oxc declaration span so callback parameters,
/// nested locals, and other functions do not inherit an outer ref. Unresolved
/// identifiers (bare auto-imported seeds) match a unique proven graph binding
/// of the same name only when this module has no local symbol of that name.
#[must_use]
pub fn reactive_binding_for_operand<'a>(
  block: &'a ScriptBlockFacts,
  operand: &ScriptOperandFact,
) -> Option<&'a ReactiveBindingFact> {
  if let Some(binding_span) = operand.binding_span {
    return block
      .reactivity_graph
      .bindings
      .iter()
      .find(|binding| binding.name == operand.name && binding.span.offset == binding_span.offset);
  }
  if block.bindings.iter().any(|binding| binding.name == operand.name) {
    return None;
  }
  let mut matches =
    block.reactivity_graph.bindings.iter().filter(|binding| binding.name == operand.name);
  let first = matches.next()?;
  matches.next().is_none().then_some(first)
}

#[must_use]
pub fn script_binding<'a>(
  block: &'a ScriptBlockFacts,
  name: &str,
) -> Option<&'a ScriptBindingFact> {
  block.bindings.iter().find(|binding| binding.name == name)
}

/// Script symbol whose declaration span matches a reactive binding.
///
/// Same-name inner locals must not inherit an outer `exported` flag.
#[must_use]
pub fn script_binding_at<'a>(
  block: &'a ScriptBlockFacts,
  name: &str,
  span: vue_vet_core::SourceSpan,
) -> Option<&'a ScriptBindingFact> {
  block.bindings.iter().find(|binding| binding.name == name && binding.span.offset == span.offset)
}
