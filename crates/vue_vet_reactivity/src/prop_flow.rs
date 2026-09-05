//! Cross-file parent `:prop` → child `props.prop` edges (under-approx).

use std::{collections::BTreeMap, sync::Arc};

use vue_vet_core::{
  ReactiveBindingKind, ReactiveDependencyEdge, ReactiveDependencyKind, ReactivityGraph, SourceSpan,
  TemplateElementFact, TemplateFacts, qualify_dependency_to_id,
};

use crate::ModuleReactivity;

/// One resolved component usage in a parent template that targets `child_module`.
pub struct PropFlowSite<'a> {
  pub element_span: SourceSpan,
  pub parent_template: &'a TemplateFacts,
  pub parent_graph: &'a ReactivityGraph,
  pub child_module: &'a str,
}

/// Append static prop-flow edges onto each child graph.
///
/// Links `:foo="bar"` / `v-bind:foo="bar"` / `v-model` when the parent expression
/// is a bare identifier or a static member chain rooted at a parent binding
/// (`ident`, `ident.value`, `ident.member`, `ident.a.b`, `ident?.a?.b`), and the
/// child has a `props` reactive bag. Whole-object `v-bind="obj"`, calls, and
/// computed brackets stay quiet.
pub fn join_prop_flows(children: &mut [Arc<ModuleReactivity>], sites: &[PropFlowSite<'_>]) {
  let child_index = children
    .iter()
    .enumerate()
    .map(|(index, child)| (child.id.as_str(), index))
    .collect::<BTreeMap<_, _>>();
  let mut elements_by_template: BTreeMap<
    *const TemplateFacts,
    BTreeMap<usize, &TemplateElementFact>,
  > = BTreeMap::new();
  let mut pending: BTreeMap<usize, Vec<ReactiveDependencyEdge>> = BTreeMap::new();
  for site in sites {
    let template = std::ptr::from_ref(site.parent_template);
    let elements = elements_by_template.entry(template).or_default();
    if elements.is_empty() {
      for element in &site.parent_template.elements {
        elements.entry(element.span.offset).or_insert(element);
      }
    }
    let Some(element) = elements.get(&site.element_span.offset).copied() else {
      continue;
    };
    let Some(&child_idx) = child_index.get(site.child_module) else {
      continue;
    };
    let Some(child) = children.get(child_idx) else {
      continue;
    };
    if !child_has_props_bag(&child.graph) {
      continue;
    }
    let new_edges = collect_prop_edges(element, site.parent_graph);
    if new_edges.is_empty() {
      continue;
    }
    pending.entry(child_idx).or_default().extend(new_edges);
  }
  for (child_idx, mut new_edges) in pending {
    let Some(child) = children.get_mut(child_idx) else {
      continue;
    };
    let child = Arc::make_mut(child);
    let graph = Arc::make_mut(&mut child.graph);
    graph.edges.append(&mut new_edges);
    graph.edges.sort_by(|left, right| {
      (left.kind, left.from.as_str(), left.to.as_str(), left.property.as_deref(), left.span.offset)
        .cmp(&(
          right.kind,
          right.from.as_str(),
          right.to.as_str(),
          right.property.as_deref(),
          right.span.offset,
        ))
    });
    graph.edges.dedup_by(|left, right| {
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
  let parent_bindings = parent
    .bindings
    .iter()
    .map(|binding| (binding.name.as_str(), binding))
    .collect::<BTreeMap<_, _>>();
  for directive in &element.directives {
    let prop_name = match directive.name.as_str() {
      "bind" => {
        let Some(name) = directive.argument.as_deref().filter(|name| !name.is_empty()) else {
          // `v-bind="obj"` — quiet (would invent many props).
          continue;
        };
        name
      }
      "model" => {
        directive.argument.as_deref().filter(|name| !name.is_empty()).unwrap_or("modelValue")
      }
      _ => continue,
    };
    let Some(expression) = directive.expression.as_deref() else {
      continue;
    };
    let Some(binding) = parse_parent_binding_root(expression) else {
      continue;
    };
    let Some(parent_binding) = parent_bindings.get(binding) else {
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
      span: directive.span,
    });
  }
  edges
}

/// Bare `foo` or a static member chain `foo.bar.baz` / `foo?.bar?.baz` → root `foo`.
///
/// Under-approx: only the root binding is joined; nested keys are not invented on
/// the child. Optional chaining is normalized to dots. Rejects empty segments,
/// non-idents, calls, and computed brackets.
fn parse_parent_binding_root(expression: &str) -> Option<&str> {
  let trimmed = expression.trim();
  if trimmed.is_empty() {
    return None;
  }
  // `?.` → `.` so `bag?.nested?.name` matches the same root as `bag.nested.name`.
  let normalized = trimmed.replace("?.", ".");
  let mut parts = normalized.split('.');
  let root = parts.next()?;
  if !is_ident_segment(root) {
    return None;
  }
  for part in parts {
    if part.is_empty() || !is_ident_segment(part) {
      return None;
    }
  }
  // Root is always a leading prefix of the original trimmed expression.
  trimmed.get(..root.len()).filter(|&slice| slice == root)
}

fn is_ident_segment(value: &str) -> bool {
  let mut chars = value.chars();
  let Some(first) = chars.next() else {
    return false;
  };
  (first.is_ascii_alphabetic() || first == '_' || first == '$')
    && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
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
        has_accessible_content: false,
        has_labelable_descendant: false,
        has_label_ancestor: false,
        has_accessible_name_ancestor: false,
      }],
      expressions: Vec::new(),
    };
    let mut parent_graph = ReactivityGraph {
      bindings: vec![ReactiveBindingFact {
        name: "label".into(),
        kind: ReactiveBindingKind::Ref,
        initialized_with_null: false,
        alias_of: None,
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
        alias_of: None,
        span: span(2),
      }],
      ..ReactivityGraph::default()
    };
    let mut children =
      vec![Arc::new(ModuleReactivity { id: "Child.vue".into(), graph: Arc::new(child_graph) })];
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
  #[expect(clippy::panic, reason = "fixture construction failures must fail the unit test")]
  fn duplicate_element_spans_keep_the_first_prop_site() {
    let first = TemplateElementFact {
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
      has_accessible_content: false,
      has_labelable_descendant: false,
      has_label_ancestor: false,
      has_accessible_name_ancestor: false,
    };
    let second = TemplateElementFact {
      tag: "Child".into(),
      span: span(10),
      attributes: Vec::new(),
      directives: vec![TemplateDirectiveFact {
        name: "bind".into(),
        raw_name: ":title".into(),
        argument: Some("title".into()),
        expression: Some("other".into()),
        modifiers: Vec::new(),
        span: span(13),
      }],
      has_children: false,
      has_accessible_content: false,
      has_labelable_descendant: false,
      has_label_ancestor: false,
      has_accessible_name_ancestor: false,
    };
    let parent_template = TemplateFacts { elements: vec![first, second], expressions: Vec::new() };
    let mut parent_graph = ReactivityGraph {
      bindings: vec![
        ReactiveBindingFact {
          name: "label".into(),
          kind: ReactiveBindingKind::Ref,
          initialized_with_null: false,
          alias_of: None,
          span: span(1),
        },
        ReactiveBindingFact {
          name: "other".into(),
          kind: ReactiveBindingKind::Ref,
          initialized_with_null: false,
          alias_of: None,
          span: span(2),
        },
      ],
      ..ReactivityGraph::default()
    };
    parent_graph.set_module_id("Parent.vue");
    let child_graph = ReactivityGraph {
      bindings: vec![ReactiveBindingFact {
        name: "props".into(),
        kind: ReactiveBindingKind::Reactive,
        initialized_with_null: false,
        alias_of: None,
        span: span(3),
      }],
      ..ReactivityGraph::default()
    };
    let mut children =
      vec![Arc::new(ModuleReactivity { id: "Child.vue".into(), graph: Arc::new(child_graph) })];
    join_prop_flows(
      &mut children,
      &[PropFlowSite {
        element_span: span(10),
        parent_template: &parent_template,
        parent_graph: &parent_graph,
        child_module: "Child.vue",
      }],
    );
    let child = children.first().unwrap_or_else(|| panic!("child missing"));
    let prop_tos: Vec<&str> = child
      .graph
      .edges
      .iter()
      .filter(|edge| edge.kind == ReactiveDependencyKind::Prop)
      .map(|edge| edge.to.as_str())
      .collect();
    assert_eq!(
      prop_tos,
      ["label"],
      "duplicate spans must keep the first element: {:?}",
      child.graph.edges
    );
  }

  #[test]
  fn joins_v_model_and_member_access_onto_child_props() {
    let parent_template = TemplateFacts {
      elements: vec![TemplateElementFact {
        tag: "Child".into(),
        span: span(10),
        attributes: Vec::new(),
        directives: vec![
          TemplateDirectiveFact {
            name: "model".into(),
            raw_name: "v-model".into(),
            argument: None,
            expression: Some("msg".into()),
            modifiers: Vec::new(),
            span: span(12),
          },
          TemplateDirectiveFact {
            name: "bind".into(),
            raw_name: ":title".into(),
            argument: Some("title".into()),
            expression: Some("bag.name".into()),
            modifiers: Vec::new(),
            span: span(14),
          },
          TemplateDirectiveFact {
            name: "bind".into(),
            raw_name: ":count".into(),
            argument: Some("count".into()),
            expression: Some("msg.value".into()),
            modifiers: Vec::new(),
            span: span(16),
          },
        ],
        has_children: false,
        has_accessible_content: false,
        has_labelable_descendant: false,
        has_label_ancestor: false,
        has_accessible_name_ancestor: false,
      }],
      expressions: Vec::new(),
    };
    let mut parent_graph = ReactivityGraph {
      bindings: vec![
        ReactiveBindingFact {
          name: "msg".into(),
          kind: ReactiveBindingKind::Ref,
          initialized_with_null: false,
          alias_of: None,
          span: span(1),
        },
        ReactiveBindingFact {
          name: "bag".into(),
          kind: ReactiveBindingKind::Reactive,
          initialized_with_null: false,
          alias_of: None,
          span: span(2),
        },
      ],
      ..ReactivityGraph::default()
    };
    parent_graph.set_module_id("Parent.vue");
    let child_graph = ReactivityGraph {
      bindings: vec![ReactiveBindingFact {
        name: "props".into(),
        kind: ReactiveBindingKind::Reactive,
        initialized_with_null: false,
        alias_of: None,
        span: span(3),
      }],
      ..ReactivityGraph::default()
    };
    let mut children =
      vec![Arc::new(ModuleReactivity { id: "Child.vue".into(), graph: Arc::new(child_graph) })];
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
      let props: Vec<_> = child
        .graph
        .edges
        .iter()
        .filter(|edge| edge.kind == ReactiveDependencyKind::Prop)
        .map(|edge| (edge.property.as_deref(), edge.to.as_str()))
        .collect();
      assert!(
        props.contains(&(Some("modelValue"), "msg"))
          && props.contains(&(Some("title"), "bag"))
          && props.contains(&(Some("count"), "msg")),
        "expected v-model/member/.value prop edges; got {props:?}"
      );
    }
  }

  #[test]
  fn joins_multi_hop_static_member_chain_onto_child_props() {
    let parent_template = TemplateFacts {
      elements: vec![TemplateElementFact {
        tag: "Child".into(),
        span: span(10),
        attributes: Vec::new(),
        directives: vec![TemplateDirectiveFact {
          name: "bind".into(),
          raw_name: ":subtitle".into(),
          argument: Some("subtitle".into()),
          expression: Some("bag.nested.name".into()),
          modifiers: Vec::new(),
          span: span(12),
        }],
        has_children: false,
        has_accessible_content: false,
        has_labelable_descendant: false,
        has_label_ancestor: false,
        has_accessible_name_ancestor: false,
      }],
      expressions: Vec::new(),
    };
    let mut parent_graph = ReactivityGraph {
      bindings: vec![ReactiveBindingFact {
        name: "bag".into(),
        kind: ReactiveBindingKind::Reactive,
        initialized_with_null: false,
        alias_of: None,
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
        alias_of: None,
        span: span(2),
      }],
      ..ReactivityGraph::default()
    };
    let mut children =
      vec![Arc::new(ModuleReactivity { id: "Child.vue".into(), graph: Arc::new(child_graph) })];
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
            && edge.to == "bag"
            && edge.property.as_deref() == Some("subtitle")
            && edge.to_id.as_deref().is_some_and(|id| id.starts_with("Parent.vue:bag@"))
        }),
        "expected multi-hop prop flow to root binding; got {:?}",
        child.graph.edges
      );
    }
  }

  #[test]
  fn joins_optional_chain_static_member_onto_child_props() {
    let parent_template = TemplateFacts {
      elements: vec![TemplateElementFact {
        tag: "Child".into(),
        span: span(10),
        attributes: Vec::new(),
        directives: vec![TemplateDirectiveFact {
          name: "bind".into(),
          raw_name: ":subtitle".into(),
          argument: Some("subtitle".into()),
          expression: Some("bag?.nested?.name".into()),
          modifiers: Vec::new(),
          span: span(12),
        }],
        has_children: false,
        has_accessible_content: false,
        has_labelable_descendant: false,
        has_label_ancestor: false,
        has_accessible_name_ancestor: false,
      }],
      expressions: Vec::new(),
    };
    let mut parent_graph = ReactivityGraph {
      bindings: vec![ReactiveBindingFact {
        name: "bag".into(),
        kind: ReactiveBindingKind::Reactive,
        initialized_with_null: false,
        alias_of: None,
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
        alias_of: None,
        span: span(2),
      }],
      ..ReactivityGraph::default()
    };
    let mut children =
      vec![Arc::new(ModuleReactivity { id: "Child.vue".into(), graph: Arc::new(child_graph) })];
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
        edge.is_some_and(|edge| edge.from == "props" && edge.to == "bag"),
        "expected optional-chain prop flow to root binding; got {:?}",
        child.graph.edges
      );
    }
  }

  #[test]
  fn stays_quiet_for_non_ident_member_chains() {
    assert_eq!(parse_parent_binding_root("bag.nested.name"), Some("bag"));
    assert_eq!(parse_parent_binding_root("bag?.name"), Some("bag"));
    assert_eq!(parse_parent_binding_root("bag?.nested?.name"), Some("bag"));
    assert_eq!(parse_parent_binding_root("bag[name]"), None);
    assert_eq!(parse_parent_binding_root("foo().bar"), None);
    assert_eq!(parse_parent_binding_root("bag..name"), None);
    assert_eq!(parse_parent_binding_root("bag?.()"), None);
    assert_eq!(parse_parent_binding_root("a?.[b]"), None);
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
        has_accessible_content: false,
        has_labelable_descendant: false,
        has_label_ancestor: false,
        has_accessible_name_ancestor: false,
      }],
      expressions: Vec::new(),
    };
    let mut parent_graph = ReactivityGraph {
      bindings: vec![ReactiveBindingFact {
        name: "bag".into(),
        kind: ReactiveBindingKind::Reactive,
        initialized_with_null: false,
        alias_of: None,
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
        alias_of: None,
        span: span(2),
      }],
      ..ReactivityGraph::default()
    };
    let mut children =
      vec![Arc::new(ModuleReactivity { id: "Child.vue".into(), graph: Arc::new(child_graph) })];
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
