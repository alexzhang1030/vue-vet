//! Human- and machine-readable reactivity tracer digests for CLI reports.

use std::cmp::Reverse;

use serde::Serialize;

use crate::humanize::{
  humanize_binding_parts, humanize_edge_parts_with_property, humanize_scope,
  humanize_template_read_parts, parse_name_offset, to_path,
};

const HOTSPOT_LIMIT: usize = 5;
const DETAIL_LINE_LIMIT: usize = 12;

/// Byte range inside a module source (editor consumers map via `positionAt`).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ReactivitySpanRef {
  pub offset: usize,
  pub length: usize,
}

impl ReactivitySpanRef {
  #[must_use]
  pub const fn new(offset: usize, length: usize) -> Self {
    Self { offset, length }
  }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReactivityBindingDetail {
  pub name: String,
  pub kind: String,
  pub span: ReactivitySpanRef,
  pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReactivityScopeDetail {
  pub kind: String,
  pub callee: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub binding: Option<String>,
  pub span: ReactivitySpanRef,
  pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReactivityEdgeDetail {
  pub from: String,
  pub to: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub to_id: Option<String>,
  /// Member on `to` when the read was `bag.field` (e.g. `count` for `props.count`).
  #[serde(skip_serializing_if = "Option::is_none")]
  pub property: Option<String>,
  /// Display path `to` or `to.property`.
  pub to_path: String,
  pub kind: String,
  pub span: ReactivitySpanRef,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub to_span: Option<ReactivitySpanRef>,
  pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReactivityTemplateReadDetail {
  pub binding: String,
  pub surface: String,
  pub span: ReactivitySpanRef,
  pub label: String,
}

/// Aggregate tracer facts for the default text/JSON report footer.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ReactivityDigest {
  pub modules: usize,
  pub bindings: usize,
  pub scopes: usize,
  pub edges: usize,
  pub template_reads: usize,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,
  pub hotspots: Vec<ReactivityHotspot>,
  /// Filled when the CLI requests `--print-reactivity`.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub modules_detail: Vec<ReactivityModuleDetail>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReactivityHotspot {
  pub id: String,
  pub bindings: usize,
  pub scopes: usize,
  pub edges: usize,
  pub template_reads: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReactivityModuleDetail {
  pub id: String,
  pub bindings: Vec<String>,
  pub scopes: Vec<String>,
  pub edges: Vec<String>,
  pub template_reads: Vec<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub binding_details: Vec<ReactivityBindingDetail>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub scope_details: Vec<ReactivityScopeDetail>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub edge_details: Vec<ReactivityEdgeDetail>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub template_details: Vec<ReactivityTemplateReadDetail>,
}

/// Per-module counts and optional detail labels supplied by the CLI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReactivityModuleStats {
  pub id: String,
  pub bindings: usize,
  pub scopes: usize,
  pub edges: usize,
  pub template_reads: usize,
  pub binding_labels: Vec<String>,
  pub scope_labels: Vec<String>,
  pub edge_labels: Vec<String>,
  pub template_labels: Vec<String>,
  pub binding_details: Vec<ReactivityBindingDetail>,
  pub scope_details: Vec<ReactivityScopeDetail>,
  pub edge_details: Vec<ReactivityEdgeDetail>,
  pub template_details: Vec<ReactivityTemplateReadDetail>,
}

impl ReactivityModuleStats {
  #[must_use]
  pub fn empty(id: impl Into<String>) -> Self {
    Self {
      id: id.into(),
      bindings: 0,
      scopes: 0,
      edges: 0,
      template_reads: 0,
      binding_labels: Vec::new(),
      scope_labels: Vec::new(),
      edge_labels: Vec::new(),
      template_labels: Vec::new(),
      binding_details: Vec::new(),
      scope_details: Vec::new(),
      edge_details: Vec::new(),
      template_details: Vec::new(),
    }
  }
}

impl ReactivityDigest {
  #[must_use]
  pub fn from_modules(modules: &[ReactivityModuleStats], error: Option<String>) -> Self {
    let mut modules = modules.to_vec();
    modules.sort_by(|left, right| left.id.cmp(&right.id));
    let bindings = modules.iter().map(|module| module.bindings).sum();
    let scopes = modules.iter().map(|module| module.scopes).sum();
    let edges = modules.iter().map(|module| module.edges).sum();
    let template_reads = modules.iter().map(|module| module.template_reads).sum();
    let mut ranked = modules
      .iter()
      .map(|module| {
        let weight = module
          .bindings
          .saturating_add(module.scopes)
          .saturating_add(module.edges)
          .saturating_add(module.template_reads);
        (weight, module)
      })
      .filter(|(weight, _)| *weight > 0)
      .collect::<Vec<_>>();
    ranked.sort_by_key(|(weight, module)| (Reverse(*weight), module.id.as_str()));
    let hotspots = ranked
      .into_iter()
      .take(HOTSPOT_LIMIT)
      .map(|(_, module)| ReactivityHotspot {
        id: module.id.clone(),
        bindings: module.bindings,
        scopes: module.scopes,
        edges: module.edges,
        template_reads: module.template_reads,
      })
      .collect();
    Self {
      modules: modules.len(),
      bindings,
      scopes,
      edges,
      template_reads,
      error,
      hotspots,
      modules_detail: Vec::new(),
    }
  }

  #[must_use]
  pub fn with_modules_detail(mut self, modules: &[ReactivityModuleStats]) -> Self {
    let mut details = modules
      .iter()
      .map(|module| ReactivityModuleDetail {
        id: module.id.clone(),
        bindings: module.binding_labels.clone(),
        scopes: module.scope_labels.clone(),
        edges: module.edge_labels.clone(),
        template_reads: module.template_labels.clone(),
        binding_details: module.binding_details.clone(),
        scope_details: module.scope_details.clone(),
        edge_details: module.edge_details.clone(),
        template_details: module.template_details.clone(),
      })
      .collect::<Vec<_>>();
    details.sort_by(|left, right| left.id.cmp(&right.id));
    self.modules_detail = details;
    self
  }
}

/// Build a humanized edge detail from graph facts (shared by CLI fill path / tests).
#[must_use]
pub fn edge_detail(
  from: impl Into<String>,
  to: impl Into<String>,
  to_id: Option<String>,
  property: Option<String>,
  kind: impl Into<String>,
  span: ReactivitySpanRef,
  to_span: Option<ReactivitySpanRef>,
) -> ReactivityEdgeDetail {
  let from = from.into();
  let to = to.into();
  let to_path = to_path(&to, property.as_deref());
  let label = humanize_edge_parts_with_property(&from, &to, property.as_deref());
  ReactivityEdgeDetail {
    from,
    to,
    to_id,
    property,
    to_path,
    kind: kind.into(),
    span,
    to_span,
    label,
  }
}

#[must_use]
pub fn binding_detail(
  name: impl Into<String>,
  kind: impl Into<String>,
  span: ReactivitySpanRef,
) -> ReactivityBindingDetail {
  let name = name.into();
  let kind = kind.into();
  let label = humanize_binding_parts(&name, &kind);
  ReactivityBindingDetail { name, kind, span, label }
}

#[must_use]
pub fn scope_detail(
  kind: impl Into<String>,
  callee: impl Into<String>,
  binding: Option<String>,
  span: ReactivitySpanRef,
) -> ReactivityScopeDetail {
  let kind = kind.into();
  let callee = callee.into();
  let machine =
    binding.as_ref().map_or_else(|| format!("{kind}({callee})"), |name| format!("{kind}({name})"));
  let label = humanize_scope(&machine);
  ReactivityScopeDetail { kind, callee, binding, span, label }
}

#[must_use]
pub fn template_read_detail(
  binding: impl Into<String>,
  surface: impl Into<String>,
  span: ReactivitySpanRef,
) -> ReactivityTemplateReadDetail {
  let binding = binding.into();
  let surface = surface.into();
  let label = humanize_template_read_parts(&binding, &surface);
  ReactivityTemplateReadDetail { binding, surface, span, label }
}

/// Resolve `to_span` from a span-qualified `to_id` and optional binding length lookup.
#[must_use]
pub fn to_span_from_identity(
  to_id: Option<&str>,
  binding_length: impl FnOnce(&str) -> Option<usize>,
) -> Option<ReactivitySpanRef> {
  let identity = to_id?;
  let (name, offset) = parse_name_offset(identity)?;
  let length = binding_length(name).unwrap_or_else(|| name.len().max(1));
  Some(ReactivitySpanRef::new(offset, length))
}

#[must_use]
#[expect(clippy::format_push_string, reason = "report footer builds a small owned buffer")]
pub fn render_reactivity_footer(digest: &ReactivityDigest) -> String {
  let mut output = String::from("\n\nReactivity\n");
  if let Some(error) = &digest.error {
    output.push_str("  unavailable: ");
    output.push_str(error);
    output.push('\n');
    return output;
  }
  output.push_str(&format!(
    "  traced {} module(s) · {} bindings · {} scopes · {} edges · {} template reads\n",
    digest.modules, digest.bindings, digest.scopes, digest.edges, digest.template_reads
  ));
  let facts = digest
    .bindings
    .saturating_add(digest.scopes)
    .saturating_add(digest.edges)
    .saturating_add(digest.template_reads);
  if digest.modules > 0 && facts == 0 {
    output.push_str(
      "  tracer ran; no reactive facts in scanned scripts (empty ≠ fully reactive — often missing imports / Nuxt auto-import gaps)\n",
    );
    return output;
  }
  if digest.hotspots.is_empty() {
    return output;
  }
  output.push_str("  busiest\n");
  let width = digest.hotspots.iter().map(|hotspot| hotspot.id.len()).max().unwrap_or(0);
  for hotspot in &digest.hotspots {
    output.push_str(&format!(
      "    {:width$}  {}b  {}s  {}e  {}t\n",
      hotspot.id,
      hotspot.bindings,
      hotspot.scopes,
      hotspot.edges,
      hotspot.template_reads,
      width = width
    ));
  }
  output
}

#[must_use]
pub fn render_reactivity_detail(digest: &ReactivityDigest) -> String {
  let mut output = String::from("\nReactivity detail\n");
  if let Some(error) = &digest.error {
    output.push_str("  unavailable: ");
    output.push_str(error);
    output.push('\n');
    return output;
  }
  if digest.modules_detail.is_empty() {
    output.push_str("  (no modules traced)\n");
    return output;
  }
  for module in &digest.modules_detail {
    output.push_str("  ");
    output.push_str(&module.id);
    output.push('\n');
    append_detail_section(&mut output, "bindings", &module.bindings);
    append_detail_section(&mut output, "scopes", &module.scopes);
    append_detail_section(&mut output, "edges", &module.edges);
    append_detail_section(&mut output, "template", &module.template_reads);
  }
  output
}

#[expect(clippy::format_push_string, reason = "detail section builds a small owned buffer")]
fn append_detail_section(output: &mut String, label: &str, lines: &[String]) {
  if lines.is_empty() {
    output.push_str("    ");
    output.push_str(label);
    output.push_str(": (none)\n");
    return;
  }
  output.push_str("    ");
  output.push_str(label);
  output.push_str(": ");
  let shown = lines.iter().take(DETAIL_LINE_LIMIT).cloned().collect::<Vec<_>>();
  output.push_str(&shown.join(", "));
  if lines.len() > DETAIL_LINE_LIMIT {
    output.push_str(&format!(" … +{} more", lines.len() - DETAIL_LINE_LIMIT));
  }
  output.push('\n');
}

#[cfg(test)]
mod tests {
  use super::*;

  fn module(
    id: &str,
    bindings: usize,
    scopes: usize,
    edges: usize,
    reads: usize,
  ) -> ReactivityModuleStats {
    let mut stats = ReactivityModuleStats::empty(id);
    stats.bindings = bindings;
    stats.scopes = scopes;
    stats.edges = edges;
    stats.template_reads = reads;
    stats
  }

  #[test]
  fn footer_lists_hotspots_and_totals() {
    let digest = ReactivityDigest::from_modules(
      &[
        module("pages/index.vue", 4, 2, 3, 1),
        module("components/Hero.vue", 1, 0, 0, 0),
        module("empty.ts", 0, 0, 0, 0),
      ],
      None,
    );
    let rendered = render_reactivity_footer(&digest);
    assert!(
      rendered.contains("traced 3 module(s) · 5 bindings · 2 scopes · 3 edges · 1 template reads")
    );
    assert!(rendered.contains("pages/index.vue"));
    assert!(rendered.contains("4b  2s  3e  1t"));
    assert!(!rendered.contains("empty.ts"));
  }

  #[test]
  fn footer_explains_empty_facts() {
    let digest = ReactivityDigest::from_modules(&[module("a.ts", 0, 0, 0, 0)], None);
    let rendered = render_reactivity_footer(&digest);
    assert!(rendered.contains("empty ≠ fully reactive"));
  }

  #[test]
  fn footer_surfaces_errors() {
    let digest = ReactivityDigest::from_modules(&[], Some("boom".into()));
    let rendered = render_reactivity_footer(&digest);
    assert!(rendered.contains("unavailable: boom"));
  }

  #[test]
  #[expect(clippy::expect_used, reason = "unit test asserts serialize succeeds")]
  fn modules_detail_carries_structured_spans() {
    let mut stats = ReactivityModuleStats::empty("App.vue");
    stats.bindings = 1;
    stats.edges = 1;
    stats.binding_labels = vec!["error:ref".into()];
    stats.edge_labels = vec!["template:if@10 -> error".into()];
    stats.binding_details = vec![binding_detail("error", "ref", ReactivitySpanRef::new(4, 5))];
    stats.edge_details = vec![edge_detail(
      "template:if@10",
      "error",
      Some("error@4".into()),
      None,
      "template",
      ReactivitySpanRef::new(10, 2),
      Some(ReactivitySpanRef::new(4, 5)),
    )];
    let modules = [stats];
    let digest = ReactivityDigest::from_modules(&modules, None).with_modules_detail(&modules);
    let detail = digest.modules_detail.first();
    assert_eq!(
      detail.and_then(|module| module.edge_details.first()).map(|edge| edge.label.as_str()),
      Some("v-if  →  error")
    );
    assert_eq!(
      detail.and_then(|module| module.edge_details.first()).map(|edge| edge.to_path.as_str()),
      Some("error")
    );
    assert_eq!(
      detail.and_then(|module| module.edge_details.first()).map(|edge| edge.span.offset),
      Some(10)
    );
    assert_eq!(
      detail.and_then(|module| module.binding_details.first()).map(|binding| binding.span.offset),
      Some(4)
    );
    let json = serde_json::to_string(&digest).expect("digest must serialize");
    assert!(json.contains("\"edge_details\""));
    assert!(json.contains("\"binding_details\""));
  }
}
