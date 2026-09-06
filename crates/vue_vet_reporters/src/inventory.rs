//! Text rendering for `--list-rules` inventory.

use std::fmt::Write;

use vue_vet_core::{RuleGroupId, RuleInventory, Severity};

/// Dense ID / category / group / severity columns.
#[must_use]
#[expect(clippy::expect_used, reason = "formatting into String is infallible")]
pub fn render_rule_inventory_text(inventory: &RuleInventory) -> String {
  let id_width = inventory.rules.iter().map(|row| row.id.len()).max().unwrap_or(2).max(2);
  let category_width =
    inventory.rules.iter().map(|row| row.category.len()).max().unwrap_or(8).max(8);
  let group_width = inventory
    .rules
    .iter()
    .map(|row| row.group.map_or("-", RuleGroupId::slug).len())
    .max()
    .unwrap_or(5)
    .max(5);
  let mut output = String::new();
  writeln!(
    output,
    "{:<id_width$}  {:<category_width$}  {:<group_width$}  SEVERITY",
    "ID", "CATEGORY", "GROUP"
  )
  .expect("formatting into String is infallible");
  for row in &inventory.rules {
    let group = row.group.map_or("-", RuleGroupId::slug);
    writeln!(
      output,
      "{:<id_width$}  {:<category_width$}  {:<group_width$}  {}",
      row.id,
      row.category,
      group,
      severity_label(row.severity)
    )
    .expect("formatting into String is infallible");
  }
  output
}

const fn severity_label(severity: Severity) -> &'static str {
  match severity {
    Severity::Info => "info",
    Severity::Warning => "warning",
    Severity::Error => "error",
  }
}
