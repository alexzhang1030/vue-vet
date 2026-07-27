//! Structural component reference navigation.
//!
//! Built from project-graph `ComponentUsage` / `AutoComponent` edges. Static
//! parent `:foo` → child `props.foo` dataflow lives on reactivity
//! [`vue_vet_core::ReactiveDependencyKind::Prop`] edges, not this digest.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::reactivity::ReactivitySpanRef;

/// Compact per-file component usage index for CLI / editor hosts.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ComponentNavDigest {
  pub modules: Vec<ComponentNavModule>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ComponentNavModule {
  /// Repository-relative path (`src/pages/index.vue`), not `file:` node ids.
  pub id: String,
  pub uses: Vec<ComponentNavLink>,
  pub used_by: Vec<ComponentNavLink>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ComponentNavLink {
  /// Peer file path (normalized, no `file:` prefix).
  pub peer: String,
  /// `component_usage` or `auto_component`.
  pub kind: String,
  pub specifier: String,
  pub span: ReactivitySpanRef,
}

/// One project-graph component edge ready for digest folding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentNavEdgeInput {
  pub from: String,
  pub to: String,
  pub kind: String,
  pub specifier: String,
  pub span: ReactivitySpanRef,
}

/// Fold component edges into a deterministic per-module `uses` / `used_by` index.
#[must_use]
pub fn component_nav_from_edges(
  edges: impl IntoIterator<Item = ComponentNavEdgeInput>,
) -> ComponentNavDigest {
  let mut uses: BTreeMap<String, Vec<ComponentNavLink>> = BTreeMap::new();
  let mut used_by: BTreeMap<String, Vec<ComponentNavLink>> = BTreeMap::new();

  for edge in edges {
    let from = strip_file_prefix(&edge.from);
    let to = strip_file_prefix(&edge.to);
    uses.entry(from.clone()).or_default().push(ComponentNavLink {
      peer: to.clone(),
      kind: edge.kind.clone(),
      specifier: edge.specifier.clone(),
      span: edge.span,
    });
    used_by.entry(to).or_default().push(ComponentNavLink {
      peer: from,
      kind: edge.kind,
      specifier: edge.specifier,
      span: edge.span,
    });
  }

  let mut ids = uses.keys().cloned().chain(used_by.keys().cloned()).collect::<Vec<_>>();
  ids.sort();
  ids.dedup();

  let mut modules = ids
    .into_iter()
    .map(|id| {
      let mut module_uses = uses.remove(&id).unwrap_or_default();
      let mut module_used_by = used_by.remove(&id).unwrap_or_default();
      sort_links(&mut module_uses);
      sort_links(&mut module_used_by);
      ComponentNavModule { id, uses: module_uses, used_by: module_used_by }
    })
    .collect::<Vec<_>>();
  modules.sort_by(|left, right| left.id.cmp(&right.id));
  ComponentNavDigest { modules }
}

fn sort_links(links: &mut [ComponentNavLink]) {
  links.sort_by(|left, right| {
    (left.peer.as_str(), left.kind.as_str(), left.specifier.as_str(), left.span.offset).cmp(&(
      right.peer.as_str(),
      right.kind.as_str(),
      right.specifier.as_str(),
      right.span.offset,
    ))
  });
}

fn strip_file_prefix(id: &str) -> String {
  id.strip_prefix("file:").unwrap_or(id).replace('\\', "/")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn folds_uses_and_used_by_deterministically() {
    let digest = component_nav_from_edges([
      ComponentNavEdgeInput {
        from: "file:pages/index.vue".into(),
        to: "file:components/Demo.vue".into(),
        kind: "auto_component".into(),
        specifier: "Demo".into(),
        span: ReactivitySpanRef::new(10, 4),
      },
      ComponentNavEdgeInput {
        from: "file:pages/about.vue".into(),
        to: "file:components/Demo.vue".into(),
        kind: "component_usage".into(),
        specifier: "Demo".into(),
        span: ReactivitySpanRef::new(20, 4),
      },
    ]);
    assert_eq!(digest.modules.len(), 3);
    let demo = digest.modules.iter().find(|module| module.id == "components/Demo.vue");
    assert_eq!(demo.map(|module| module.used_by.len()), Some(2));
    assert_eq!(demo.map(|module| module.uses.len()), Some(0));
    let page = digest.modules.iter().find(|module| module.id == "pages/index.vue");
    assert_eq!(
      page.and_then(|module| module.uses.first()).map(|link| link.peer.as_str()),
      Some("components/Demo.vue")
    );
  }
}
