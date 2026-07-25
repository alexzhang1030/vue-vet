//! Human- and machine-readable reactivity tracer digests for CLI reports.

use std::cmp::Reverse;

use serde::Serialize;

const HOTSPOT_LIMIT: usize = 5;
const DETAIL_LINE_LIMIT: usize = 12;

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
      })
      .collect::<Vec<_>>();
    details.sort_by(|left, right| left.id.cmp(&right.id));
    self.modules_detail = details;
    self
  }
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
    ReactivityModuleStats {
      id: id.into(),
      bindings,
      scopes,
      edges,
      template_reads: reads,
      binding_labels: Vec::new(),
      scope_labels: Vec::new(),
      edge_labels: Vec::new(),
      template_labels: Vec::new(),
    }
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
}
