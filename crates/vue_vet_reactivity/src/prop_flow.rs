//! Cross-file parent `:prop` → child `props.prop` edges (under-approx).

use vue_vet_core::{
  ReactiveBindingKind, ReactiveDependencyEdge, ReactiveDependencyKind, ReactivityGraph, SourceSpan,
  TemplateElementFact, TemplateFacts, qualify_dependency_to_id,
};

use crate::modules::ModuleReactivity;

/// One resolved component usage in a parent template that targets `child_module`.
pub struct PropFlowSite<'a> {
  pub element_span: SourceSpan,
  pub parent_template: &'a TemplateFacts,
  pub parent_graph: &'a ReactivityGraph,
  pub child_module: &'a str,
}

/// Append static prop-flow edges onto each child graph.
///
/// Links `:foo="bar"` / `v-bind:foo="bar"` when `bar` is a parent reactive binding
/// and the child has a `props` reactive bag. Whole-object `v-bind="obj"` stays quiet.
pub fn join_prop_flows(children: &mut [ModuleReactivity], sites: &[PropFlowSite<'_>]) {
  for site in sites {
    let Some(element) = site
      .parent_template
      .elements
      .iter()
      .find(|element| element.span.offset == site.element_span.offset)
    else {
      continue;
    };
    let Some(child) = children.iter_mut().find(|module| module.id == site.child_module) else {
      continue;
    };
    if !child_has_props_bag(&child.graph) {
      continue;
    }
    let mut new_edges = collect_prop_edges(element, site.parent_graph);
    if new_edges.is_empty() {
      continue;
    }
    child.graph.edges.append(&mut new_edges);
    child.graph.edges.sort_by(|left, right| {
      (left.kind, left.from.as_str(), left.to.as_str(), left.property.as_deref(), left.span.offset)
        .cmp(&(
          right.kind,
          right.from.as_str(),
          right.to.as_str(),
          right.property.as_deref(),
          right.span.offset,
        ))
    });
    child.graph.edges.dedup_by(|left, right| {
      left.from == right.from
        && left.to == right.to
        && left.property == right.property
        && left.kind == right.kind
        && left.span.offset == right.span.offset
    });
  }
}

fn child_has_props_bag(graph: &ReactivityGraph) -> bool {
  graph
    .bindings
    .iter()
    .any(|binding| binding.name == "props" && matches!(binding.kind, ReactiveBindingKind::Reactive))
}

fn collect_prop_edges(
  element: &TemplateElementFact,
  parent: &ReactivityGraph,
) -> Vec<ReactiveDependencyEdge> {
  let mut edges = Vec::new();
  for directive in &element.directives {
    if directive.name != "bind" {
      continue;
    }
    let Some(prop_name) = directive.argument.as_deref().filter(|name| !name.is_empty()) else {
      // `v-bind="obj"` — quiet (would invent many props).
      continue;
    };
    let Some(expression) = directive.expression.as_deref() else {
      continue;
    };
    let Some(binding) = parse_bare_identifier(expression) else {
      continue;
    };
    let Some(parent_binding) = parent.bindings.iter().find(|item| item.name == binding) else {
      continue;
    };
    edges.push(ReactiveDependencyEdge {
      from: "props".into(),
      to: binding.to_owned(),
      to_id: Some(qualify_dependency_to_id(
        &parent.module_id,
        &parent_binding.name,
        parent_binding.span.offset,
      )),
      property: Some(prop_name.to_owned()),
      kind: ReactiveDependencyKind::Prop,
      span: directive.span.clone(),
    });
  }
  edges
}

fn parse_bare_identifier(expression: &str) -> Option<&str> {
  let trimmed = expression.trim();
  if trimmed.is_empty() {
    return None;
  }
  let mut chars = trimmed.chars();
  let first = chars.next()?;
  if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
    return None;
  }
  if chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$') {
    Some(trimmed)
  } else {
    None
  }
}

#[cfg(test)]
mod tests {
  use vue_vet_core::{
    ReactiveBindingFact, ReactiveBindingKind, ReactivityGraph, SourceSpan, TemplateDirectiveFact,
    TemplateElementFact, TemplateFacts,
  };

  use super::*;

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 4, line: 1, column: offset.saturating_add(1) }
  }

  #[test]
  fn joins_static_prop_identifier_onto_child_props() {
    let parent_template = TemplateFacts {
      elements: vec![TemplateElementFact {
        tag: "Child".into(),
        span: span(10),
        attributes: Vec::new(),
        directives: vec![TemplateDirectiveFact {
          name: "bind".into(),
          raw_name: ":title".into(),
          argument: Some("title".into()),
          expression: Some("label".into()),
          modifiers: Vec::new(),
          span: span(12),
        }],
        has_children: false,
      }],
      expressions: Vec::new(),
    };
    let mut parent_graph = ReactivityGraph {
      bindings: vec![ReactiveBindingFact {
        name: "label".into(),
        kind: ReactiveBindingKind::Ref,
        initialized_with_null: false,
        span: span(1),
      }],
      ..ReactivityGraph::default()
    };
    parent_graph.set_module_id("Parent.vue");
    let child_graph = ReactivityGraph {
      bindings: vec![ReactiveBindingFact {
        name: "props".into(),
        kind: ReactiveBindingKind::Reactive,
        initialized_with_null: false,
        span: span(2),
      }],
      ..ReactivityGraph::default()
    };
    let mut children = vec![ModuleReactivity { id: "Child.vue".into(), graph: child_graph }];
    join_prop_flows(
      &mut children,
      &[PropFlowSite {
        element_span: span(10),
        parent_template: &parent_template,
        parent_graph: &parent_graph,
        child_module: "Child.vue",
      }],
    );
    assert_eq!(children.len(), 1);
    if let Some(child) = children.first() {
      let edge = child.graph.edges.iter().find(|edge| edge.kind == ReactiveDependencyKind::Prop);
      assert!(
        edge.is_some_and(|edge| {
          edge.from == "props"
            && edge.to == "label"
            && edge.property.as_deref() == Some("title")
            && edge.to_id.as_deref().is_some_and(|id| id.starts_with("Parent.vue:label@"))
        }),
        "expected prop flow edge; got {:?}",
        child.graph.edges
      );
    }
  }

  #[test]
  fn stays_quiet_for_object_v_bind() {
    let parent_template = TemplateFacts {
      elements: vec![TemplateElementFact {
        tag: "Child".into(),
        span: span(10),
        attributes: Vec::new(),
        directives: vec![TemplateDirectiveFact {
          name: "bind".into(),
          raw_name: "v-bind".into(),
          argument: None,
          expression: Some("bag".into()),
          modifiers: Vec::new(),
          span: span(12),
        }],
        has_children: false,
      }],
      expressions: Vec::new(),
    };
    let mut parent_graph = ReactivityGraph {
      bindings: vec![ReactiveBindingFact {
        name: "bag".into(),
        kind: ReactiveBindingKind::Reactive,
        initialized_with_null: false,
        span: span(1),
      }],
      ..ReactivityGraph::default()
    };
    parent_graph.set_module_id("Parent.vue");
    let child_graph = ReactivityGraph {
      bindings: vec![ReactiveBindingFact {
        name: "props".into(),
        kind: ReactiveBindingKind::Reactive,
        initialized_with_null: false,
        span: span(2),
      }],
      ..ReactivityGraph::default()
    };
    let mut children = vec![ModuleReactivity { id: "Child.vue".into(), graph: child_graph }];
    join_prop_flows(
      &mut children,
      &[PropFlowSite {
        element_span: span(10),
        parent_template: &parent_template,
        parent_graph: &parent_graph,
        child_module: "Child.vue",
      }],
    );
    assert_eq!(children.len(), 1);
    if let Some(child) = children.first() {
      assert!(child.graph.edges.is_empty());
    }
  }
}
