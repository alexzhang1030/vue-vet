//! Per-module binding inspect index.
//!
//! Folds `edge_details` + `template_details` once so TUI / VS Code can look up
//! "who reads this" / "what does this depend on" without scanning labels.
//! Same match rules as the previous linear consumers:
//! - bag inspect (`props`): `to_path == binding` or `to_path` starts with `binding.`
//! - member pick (`props.count`): exact `to_path` only
//! - template joins attach to the bare binding, not member picks
//! - member picks are inbound-only (no outbound, no scope summaries)

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::reactivity::{ReactivityEdgeDetail, ReactivitySpanRef, ReactivityTemplateReadDetail};

/// Compact inspect index for one module. Omitted from JSON when empty.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct BindingNav {
  /// Inspect target (`count`, `props`, `props.count`) → inbound readers.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub inbound: BTreeMap<String, Vec<BindingNavReader>>,
  /// Bare binding → outbound dependencies. Member picks are absent.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub outbound: BTreeMap<String, Vec<BindingNavDep>>,
  /// Reactive bag → first-level member names (`props` → `["count", "mode"]`).
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub properties: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingNavSource {
  Edge,
  Template,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BindingNavReader {
  pub source: BindingNavSource,
  pub from: String,
  pub to_path: String,
  pub kind: String,
  pub span: ReactivitySpanRef,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub to_span: Option<ReactivitySpanRef>,
  pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BindingNavDep {
  pub from: String,
  pub to_path: String,
  pub kind: String,
  pub span: ReactivitySpanRef,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub to_span: Option<ReactivitySpanRef>,
  pub label: String,
}

impl BindingNav {
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.inbound.is_empty() && self.outbound.is_empty() && self.properties.is_empty()
  }

  #[must_use]
  pub fn inbound_for(&self, binding: &str, property: Option<&str>) -> &[BindingNavReader] {
    self.inbound.get(&inspect_key(binding, property)).map_or(&[], Vec::as_slice)
  }

  #[must_use]
  pub fn outbound_for(&self, binding: &str, property: Option<&str>) -> &[BindingNavDep] {
    if property.is_some() {
      return &[];
    }
    self.outbound.get(binding).map_or(&[], Vec::as_slice)
  }

  #[must_use]
  pub fn properties_for(&self, bag: &str) -> &[String] {
    self.properties.get(bag).map_or(&[], Vec::as_slice)
  }
}

/// Fold structured edge + template details into a deterministic inspect index.
#[must_use]
pub fn binding_nav_from_details(
  edges: &[ReactivityEdgeDetail],
  templates: &[ReactivityTemplateReadDetail],
) -> BindingNav {
  let mut inbound_exact: BTreeMap<String, Vec<BindingNavReader>> = BTreeMap::new();
  let mut inbound_members: BTreeMap<String, Vec<BindingNavReader>> = BTreeMap::new();
  let mut outbound: BTreeMap<String, Vec<BindingNavDep>> = BTreeMap::new();
  let mut properties: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

  for edge in edges {
    let reader = reader_from_edge(edge);
    inbound_exact.entry(edge.to_path.clone()).or_default().push(reader.clone());
    if let Some((bag, rest)) = edge.to_path.split_once('.') {
      inbound_members.entry(bag.to_owned()).or_default().push(reader);
      let property = rest.split('.').next().unwrap_or(rest);
      if !property.is_empty() {
        properties.entry(bag.to_owned()).or_default().insert(property.to_owned());
      }
    }
    outbound.entry(outbound_binding_key(&edge.from)).or_default().push(dep_from_edge(edge));
  }

  for read in templates {
    inbound_exact.entry(read.binding.clone()).or_default().push(reader_from_template(read));
  }

  let mut inbound = inbound_exact;
  for (bag, members) in inbound_members {
    inbound.entry(bag).or_default().extend(members);
  }
  for readers in inbound.values_mut() {
    sort_readers(readers);
  }
  for deps in outbound.values_mut() {
    sort_deps(deps);
  }

  BindingNav {
    inbound,
    outbound,
    properties: properties
      .into_iter()
      .map(|(bag, names)| (bag, names.into_iter().collect()))
      .collect(),
  }
}

fn inspect_key(binding: &str, property: Option<&str>) -> String {
  property.map_or_else(|| binding.to_owned(), |property| format!("{binding}.{property}"))
}

fn outbound_binding_key(from: &str) -> String {
  let head = from.split_once('@').map_or(from, |(head, _)| head);
  head.rsplit_once(':').map_or_else(|| head.to_owned(), |(_, name)| name.to_owned())
}

fn reader_from_edge(edge: &ReactivityEdgeDetail) -> BindingNavReader {
  BindingNavReader {
    source: BindingNavSource::Edge,
    from: edge.from.clone(),
    to_path: edge.to_path.clone(),
    kind: edge.kind.clone(),
    span: edge.span,
    to_span: edge.to_span,
    label: edge.label.clone(),
  }
}

fn reader_from_template(read: &ReactivityTemplateReadDetail) -> BindingNavReader {
  BindingNavReader {
    source: BindingNavSource::Template,
    from: read.surface.clone(),
    to_path: read.binding.clone(),
    kind: read.surface.clone(),
    span: read.span,
    to_span: None,
    label: read.label.clone(),
  }
}

fn dep_from_edge(edge: &ReactivityEdgeDetail) -> BindingNavDep {
  BindingNavDep {
    from: edge.from.clone(),
    to_path: edge.to_path.clone(),
    kind: edge.kind.clone(),
    span: edge.span,
    to_span: edge.to_span,
    label: edge.label.clone(),
  }
}

fn sort_readers(readers: &mut [BindingNavReader]) {
  readers.sort_by(|left, right| {
    (
      source_rank(left.source),
      left.from.as_str(),
      left.to_path.as_str(),
      left.kind.as_str(),
      left.span.offset,
      left.label.as_str(),
    )
      .cmp(&(
        source_rank(right.source),
        right.from.as_str(),
        right.to_path.as_str(),
        right.kind.as_str(),
        right.span.offset,
        right.label.as_str(),
      ))
  });
}

fn sort_deps(deps: &mut [BindingNavDep]) {
  deps.sort_by(|left, right| {
    (left.from.as_str(), left.to_path.as_str(), left.kind.as_str(), left.span.offset).cmp(&(
      right.from.as_str(),
      right.to_path.as_str(),
      right.kind.as_str(),
      right.span.offset,
    ))
  });
}

const fn source_rank(source: BindingNavSource) -> u8 {
  match source {
    BindingNavSource::Edge => 0,
    BindingNavSource::Template => 1,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::reactivity::{edge_detail, template_read_detail};

  fn fixture_edges() -> Vec<ReactivityEdgeDetail> {
    vec![
      edge_detail(
        "label",
        "props",
        None,
        Some("count".into()),
        "computed",
        ReactivitySpanRef::new(30, 5),
        Some(ReactivitySpanRef::new(4, 5)),
      ),
      edge_detail(
        "watch_sources:watch@40",
        "props",
        None,
        Some("mode".into()),
        "effect",
        ReactivitySpanRef::new(40, 4),
        None,
      ),
      edge_detail(
        "template:if@50",
        "props",
        None,
        None,
        "template",
        ReactivitySpanRef::new(50, 2),
        None,
      ),
      edge_detail(
        "double",
        "count",
        None,
        None,
        "computed",
        ReactivitySpanRef::new(60, 6),
        Some(ReactivitySpanRef::new(10, 5)),
      ),
    ]
  }

  fn fixture_templates() -> Vec<ReactivityTemplateReadDetail> {
    vec![template_read_detail("props", "if", ReactivitySpanRef::new(50, 2))]
  }

  fn edge_to_matches(to_path: &str, binding: &str, property: Option<&str>) -> bool {
    property.map_or_else(
      || to_path == binding || to_path.starts_with(&format!("{binding}.")),
      |property| to_path == format!("{binding}.{property}"),
    )
  }

  fn scan_inbound(
    edges: &[ReactivityEdgeDetail],
    templates: &[ReactivityTemplateReadDetail],
    binding: &str,
    property: Option<&str>,
  ) -> Vec<BindingNavReader> {
    let mut readers = Vec::new();
    for edge in edges {
      if edge_to_matches(&edge.to_path, binding, property) {
        readers.push(reader_from_edge(edge));
      }
    }
    if property.is_none() {
      for read in templates {
        if read.binding == binding {
          readers.push(reader_from_template(read));
        }
      }
    }
    sort_readers(&mut readers);
    readers
  }

  fn scan_outbound(edges: &[ReactivityEdgeDetail], binding: &str) -> Vec<BindingNavDep> {
    let mut deps = Vec::new();
    for edge in edges {
      if outbound_binding_key(&edge.from) == binding {
        deps.push(dep_from_edge(edge));
      }
    }
    sort_deps(&mut deps);
    deps
  }

  fn scan_properties(edges: &[ReactivityEdgeDetail], bag: &str) -> Vec<String> {
    let prefix = format!("{bag}.");
    let mut names = BTreeSet::new();
    for edge in edges {
      if let Some(rest) = edge.to_path.strip_prefix(&prefix) {
        let property = rest.split('.').next().unwrap_or(rest);
        if !property.is_empty() {
          names.insert(property.to_owned());
        }
      }
    }
    names.into_iter().collect()
  }

  #[test]
  fn fold_matches_scan_for_bag_member_and_empty() {
    let edges = fixture_edges();
    let templates = fixture_templates();
    let nav = binding_nav_from_details(&edges, &templates);

    assert_eq!(nav.inbound_for("props", None), scan_inbound(&edges, &templates, "props", None));
    assert_eq!(
      nav.inbound_for("props", Some("count")),
      scan_inbound(&edges, &templates, "props", Some("count"))
    );
    assert_eq!(
      nav.inbound_for("props", Some("mode")),
      scan_inbound(&edges, &templates, "props", Some("mode"))
    );
    assert_eq!(nav.inbound_for("count", None), scan_inbound(&edges, &templates, "count", None));
    assert!(nav.inbound_for("missing", None).is_empty());
    assert_eq!(nav.inbound_for("props", None).len(), 4);
    assert_eq!(nav.inbound_for("props", Some("count")).len(), 1);
    assert!(
      !nav
        .inbound_for("props", Some("count"))
        .iter()
        .any(|reader| reader.source == BindingNavSource::Template)
    );

    assert_eq!(nav.outbound_for("label", None), scan_outbound(&edges, "label"));
    assert_eq!(nav.outbound_for("double", None), scan_outbound(&edges, "double"));
    assert_eq!(nav.outbound_for("watch", None), scan_outbound(&edges, "watch"));
    assert!(nav.outbound_for("label", Some("count")).is_empty());
    assert!(nav.outbound_for("props", None).is_empty());

    assert_eq!(nav.properties_for("props"), scan_properties(&edges, "props"));
    assert_eq!(nav.properties_for("props"), ["count", "mode"]);
    assert!(nav.properties_for("count").is_empty());
  }

  #[test]
  fn empty_details_omit_the_index() {
    assert!(binding_nav_from_details(&[], &[]).is_empty());
  }

  #[test]
  fn nested_member_pick_is_exact_only() {
    let edges = [edge_detail(
      "label",
      "props",
      None,
      Some("foo.bar".into()),
      "computed",
      ReactivitySpanRef::new(8, 3),
      None,
    )];
    let nav = binding_nav_from_details(&edges, &[]);
    assert_eq!(nav.inbound_for("props", Some("foo.bar")).len(), 1);
    assert!(nav.inbound_for("props", Some("foo")).is_empty());
    assert_eq!(nav.properties_for("props"), ["foo"]);
    assert_eq!(nav.inbound_for("props", None).len(), 1);
  }
}
