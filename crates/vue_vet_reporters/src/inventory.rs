//! Text rendering for `--list-rules` inventory and the markdown rule catalog.

use std::collections::BTreeMap;
use std::fmt::Write;

use vue_vet_core::{RuleGroupId, RuleInventory, RuleMeta, Severity};

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

const INFALLIBLE: &str = "formatting into String is infallible";

/// Human catalog (`docs/rules/README.md`) generated from file-rule `RuleMeta`.
#[must_use]
#[expect(clippy::expect_used, reason = "formatting into String is infallible")]
pub fn render_rule_catalog_markdown(
  file_rules: &[&RuleMeta],
  project_ids: &[&str],
  migration_ids: &[&str],
) -> String {
  let mut by_category: BTreeMap<&str, Vec<&RuleMeta>> = BTreeMap::new();
  let mut tiers: BTreeMap<&str, usize> = BTreeMap::new();
  for meta in file_rules {
    *tiers.entry(catalog_tier(meta.id)).or_insert(0) += 1;
    by_category.entry(meta.id.split('/').nth(1).unwrap_or("unknown")).or_default().push(meta);
  }
  let tier = |name: &str| tiers.get(name).copied().unwrap_or(0);
  let mut category_counts = String::new();
  let mut sections = String::new();
  for (category, metas) in &by_category {
    writeln!(category_counts, "| `{category}` | {} |", metas.len()).expect(INFALLIBLE);
    writeln!(sections, "## {category}\n").expect(INFALLIBLE);
    for meta in metas {
      let rel = meta.documentation.strip_prefix("rules/").unwrap_or(meta.documentation);
      writeln!(sections, "- [`{}`](./{rel}.md) `{}`", meta.id, catalog_tier(meta.id))
        .expect(INFALLIBLE);
    }
    sections.push('\n');
  }
  let mut project = String::new();
  for id in project_ids {
    writeln!(project, "- `{id}`").expect(INFALLIBLE);
  }
  let mut migration = String::new();
  for id in migration_ids {
    let name = id.rsplit('/').next().unwrap_or(id);
    writeln!(migration, "- [`{id}`](./migration/{name}.md)").expect(INFALLIBLE);
  }
  format!(
    "# Rule catalog\n\n\
Generated from `RuleMeta` documentation keys. Regenerate with\n\
`vue-vet --list-rules --format markdown` (`just rules-catalog`).\n\n\
## Differentiation tiers\n\n\
Vue Vet ships a large built-in set. **Differentiation is the reactivity tracer**,\n\
not Essential/a11y parity with `eslint-plugin-vue`.\n\n\
| Tier | Meaning | Count |\n\
| --- | --- | ---: |\n\
| `tracer` | Needs `vue_vet_reactivity` graph facts (read kinds, guards, scopes, prop edges, binding kinds) | {tracer} |\n\
| `parity` | Template Essential / a11y / macros / after-await registrars — open-box completeness | {parity} |\n\
| `practice` | Ecosystem suggestions (`category: practice`); excluded from score by default | {practice} |\n\n\
Total registered **file** rules (builtins + practice): **{total}**.\n\n\
Project-graph IDs are listed separately below and are not in this file-ID set.\n\n\
| Category | Count |\n\
| --- | ---: |\n\
{category_counts}\n\
Per-rule pages live under `docs/rules/<category>/<name>.md`.\n\n\
{sections}\
## How to read a rule\n\n\
- CLI: `vue-vet --explain <rule-id>`\n\
- Fixtures: `fixtures/rules/<name>/{{invalid,valid}}/`\n\
- Practice suggestions do not affect score by default\n\
- Prefer tracer-tier findings when evaluating Vue Vet against other doctors\n\
- Removed historical IDs: [`docs/rules/removed-ids.md`](./removed-ids.md)\n\n\
## Project-graph rules\n\n\
These IDs belong to the project registry and are listed separately from file rules.\n\n\
{project}\n\
## Migration assessment rules\n\n\
These IDs belong to the opt-in `vapor-migration` group (`category: migration`) and are listed separately from file rules.\n\n\
{migration}",
    tracer = tier("tracer"),
    parity = tier("parity"),
    practice = tier("practice"),
    total = file_rules.len(),
  )
}

fn catalog_tier(rid: &str) -> &'static str {
  const TRACER_FORCE: &[&str] = &[
    "vue-vet/reactivity/prefer-computed",
    "vue-vet/reactivity/no-after-await-watch-effect-dependency",
    "vue-vet/reactivity/no-unused-reactive-binding",
    "vue-vet/reactivity/no-stale-prop-flow",
    "vue-vet/reactivity/no-model-default-unsynced-parent-demand",
    "vue-vet/reactivity/no-shared-default-cross-instance-demand",
    "vue-vet/reactivity/no-nonreactive-props-destructure",
    "vue-vet/correctness/no-mutating-props",
  ];
  const PRACTICE_FORCE: &[&str] = &["vue-vet/reactivity/prefer-use-template-ref"];
  const PARITY_FORCE: &[&str] =
    &["vue-vet/security/no-v-html", "vue-vet/maintainability/no-redundant-role"];
  if TRACER_FORCE.contains(&rid) {
    return "tracer";
  }
  if PRACTICE_FORCE.contains(&rid) {
    return "practice";
  }
  if PARITY_FORCE.contains(&rid) {
    return "parity";
  }
  let category = rid.split('/').nth(1).unwrap_or("");
  if category == "practice" || rid.starts_with("vue-vet/practice/") {
    return "practice";
  }
  if category == "accessibility" || category == "correctness" {
    return "parity";
  }
  if category == "reactivity" {
    return "tracer";
  }
  "parity"
}

const fn severity_label(severity: Severity) -> &'static str {
  match severity {
    Severity::Info => "info",
    Severity::Warning => "warning",
    Severity::Error => "error",
  }
}
